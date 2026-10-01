//! Signal analysis: [`PitchTracker`], a pitch, gate and level tracker.
//!
//! `PitchTracker` turns a monophonic audio signal (a voice, an instrument, a captured
//! input) into the three signals that play a patch like a keyboard: a V/Oct
//! pitch, a gate and a level. Pitch is estimated with YIN (de Cheveigné and
//! Kawahara, *YIN, a fundamental frequency estimator for speech and music*,
//! JASA 111(4), 2002).
//!
//! # Real-time cost
//!
//! YIN's difference function costs `W · τmax` multiply-adds per estimate. Computed
//! in one tick that is a burst of tens of thousands of operations every few
//! milliseconds. `PitchTracker` instead spreads it evenly: it low-passes and decimates the
//! input to an analysis rate chosen per [`PitchRange`] (so every band has the same
//! lag count, about 170), snapshots a frame every 5 ms, and computes a fixed
//! number of lags per tick until the next snapshot. The per-tick cost is bounded:
//! at most one or two lags of a window (`W` = 150–480 analysis samples) per tick,
//! plus one frame copy and one estimate per hop. All buffers are sized in
//! `new`/`set_sample_rate` for every band, so neither `tick` nor `set_range`
//! allocates.

use super::common::{env_coef, flush_denorm, sanitize_audio, C4_HZ, GATE_HIGH_V};
use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use alloc::vec;
use alloc::vec::Vec;
use libm::Libm;

/// The pitch band a [`PitchTracker`] searches.
///
/// Each band is about 3.6–3.8 octaves wide and is analysed at its own decimated
/// rate, so the lowest pitch spans the same number of analysis samples in every
/// band and the cost does not depend on the band.
///
/// Non-exhaustive: more bands may come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PitchRange {
    /// 40–500 Hz: bass instruments, low voices.
    Low,
    /// 70–1000 Hz: voices, guitar, most melodic instruments.
    #[default]
    Mid,
    /// 140–2000 Hz: high voices, flute, whistling, lead lines.
    High,
}

impl PitchRange {
    /// Lowest and highest pitch searched, in Hz.
    pub fn bounds(self) -> (f64, f64) {
        match self {
            PitchRange::Low => (40.0, 500.0),
            PitchRange::Mid => (70.0, 1000.0),
            PitchRange::High => (140.0, 2000.0),
        }
    }

    /// Target analysis rate: the input is decimated to roughly this rate.
    fn analysis_rate(self) -> f64 {
        match self {
            PitchRange::Low => 6_000.0,
            PitchRange::Mid => 12_000.0,
            PitchRange::High => 24_000.0,
        }
    }

    /// The `range` parameter value: `0` low, `1` mid, `2` high.
    pub fn index(self) -> usize {
        match self {
            PitchRange::Low => 0,
            PitchRange::Mid => 1,
            PitchRange::High => 2,
        }
    }

    /// Inverse of [`index`](Self::index); rounds, and is `None` out of range or not
    /// finite.
    pub fn from_index(value: f64) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        match libm::round(value) as i64 {
            0 => Some(PitchRange::Low),
            1 => Some(PitchRange::Mid),
            2 => Some(PitchRange::High),
            _ => None,
        }
    }

    const ALL: [PitchRange; 3] = [PitchRange::Low, PitchRange::Mid, PitchRange::High];
}

/// Analysis geometry for one band at one sample rate, in analysis samples.
#[derive(Debug, Clone, Copy)]
struct Geometry {
    /// Decimation factor: engine samples per analysis sample.
    decim: usize,
    /// Analysis rate in Hz (`sample_rate / decim`).
    rate: f64,
    /// Shortest lag searched (highest pitch).
    tau_min: usize,
    /// Longest lag computed (lowest pitch, plus one for interpolation).
    tau_max: usize,
    /// Integration window `W`.
    window: usize,
    /// Analysis samples between snapshots.
    hop: usize,
    /// Lags computed per engine tick, so an estimate finishes within one hop.
    lags_per_tick: usize,
    /// Consecutive unpitched estimates after which the gate closes: a frame and a hop,
    /// so the aperiodic frames that straddle a legato pitch change do not close it.
    unpitched_hold: usize,
}

impl Geometry {
    fn new(range: PitchRange, sample_rate: f64) -> Self {
        let (f_min, f_max) = range.bounds();
        let decim = (Libm::<f64>::round(sample_rate / range.analysis_rate()) as usize).max(1);
        let rate = sample_rate / decim as f64;
        let tau_max = Libm::<f64>::ceil(rate / f_min) as usize + 1;
        let tau_min = ((rate / f_max) as usize).max(2);
        // At least one longest period, and at least 20 ms: YIN's bias near an amplitude
        // edge and its variance in noise both shrink as the window grows.
        let window =
            tau_max.max(Libm::<f64>::round(rate * PitchTracker::MIN_WINDOW_SECONDS) as usize);
        let hop = (Libm::<f64>::round(rate * PitchTracker::HOP_SECONDS) as usize).max(1);
        let ticks_per_hop = hop * decim;
        Self {
            decim,
            rate,
            tau_min,
            tau_max,
            window,
            hop,
            lags_per_tick: tau_max.div_ceil(ticks_per_hop).max(1),
            unpitched_hold: (window + tau_max).div_ceil(hop) + 1,
        }
    }

    /// Analysis samples in one frame: the window plus the longest lag.
    fn frame_len(&self) -> usize {
        self.window + self.tau_max
    }
}

/// Direct-form-II-transposed biquad, used as the decimation low-pass.
#[derive(Debug, Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// RBJ low-pass at `cutoff` Hz with quality `q`.
    fn lowpass(cutoff: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * core::f64::consts::PI * cutoff / sample_rate;
        let (sin, cos) = (Libm::<f64>::sin(w0), Libm::<f64>::cos(w0));
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            b2: (1.0 - cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = flush_denorm(self.b1 * x - self.a1 * y + self.z2);
        self.z2 = flush_denorm(self.b2 * x - self.a2 * y);
        y
    }
}

/// Pitch, gate and level tracker.
///
/// Feed it a monophonic audio signal; it outputs the signal's pitch as V/Oct, a
/// gate that is high while a pitched sound is present, and its level — the three
/// signals that play a patch like a keyboard.
///
/// # Ports
///
/// | Port | Kind | |
/// |---|---|---|
/// | `in` | Audio | The signal to track (±5 V full scale) |
/// | `threshold` | CV unipolar | Gate threshold on the `level` scale, default 0.25 V (≈ −32 dB below full scale) |
/// | `voct` | V/Oct | Pitch, 0 V = C4; holds the last pitched estimate while the gate is low |
/// | `gate` | Gate | 5 V while the input is pitched and at or above `threshold` |
/// | `level` | CV unipolar | RMS level (two-pole, 10 ms), scaled so a full-scale ±5 V sine reads 10 V |
///
/// The `range` parameter (an introspection `select`: `0` low, `1` mid, `2` high,
/// default mid) picks the band searched; see [`PitchRange`].
///
/// # Behaviour
///
/// Measured at 48 kHz and pinned by the tests:
///
/// - **Pitch** is a YIN estimate (absolute threshold [`YIN_THRESHOLD`](Self::YIN_THRESHOLD),
///   parabolic interpolation on the raw difference), updated every 5 ms. A steady sine
///   anywhere in its band reads within ±1 cent; with full-band white noise 20 dB below it,
///   within ±10 cents; 10 dB below (mid band), under 10 cents RMS. A harmonic-rich tone (a
///   band-limited sawtooth) reads within ±6 cents, worst near the band's top. A tone with
///   no fundamental at all reads right mid-band, but in the top third of an octave of a
///   band a weak or missing fundamental can read **an octave low**: pick the band so the
///   notes sit below its top.
/// - **Decays and swells.** Plucked and struck notes, which decay from their first sample,
///   open the gate as promptly as held ones and read within 5 cents. A decay faster than
///   about three periods per e-fold reads a little sharp (about `(P/T)²/4π²`: 11–13 cents
///   for a 55 Hz pluck with a 50 ms decay). A swelling onset opens the gate at most 25 ms
///   after an abrupt one (a 100 ms linear attack: 20, 5 and 5 ms later).
/// - **Edges.** YIN's dip shifts once a frame holds the start or end of a sound, so an
///   estimate is used only when its frame held no silence before the onset (the level
///   clock) and the next frame shows the sound carried on past it (see `conclude`). The
///   pitch is therefore right (±1 cent) when the gate rises, and holds within ±5 cents
///   after it falls (±10 after a slow release, which is itself a fast decay), for notes
///   two semitones or more above the band's floor. Within the bottom two semitones a stop
///   can leave the held pitch further off: up to ~40 cents after an abrupt stop in the low
///   band, ~25 after a fast release in the high band.
/// - **Gate** is decided at each estimate from the analysis frame's RMS (on the `level`
///   scale, so it does not ripple with the waveform). It opens with the first confirmed
///   estimate whose frame is at or above `threshold`, so `voct` already holds the new note
///   when it rises. It closes when the frame falls below half the threshold, or after a
///   frame's worth of unpitched estimates (long enough that a legato pitch change, whose
///   frames are briefly aperiodic, keeps it open). Broadband (white) noise does not open
///   it; brown or heavily low-passed noise, whose slow wander can look periodic within one
///   frame, now and then can.
/// - **Latency**: the gate opens within a frame and three hops of an onset (63.4, 48.0 and
///   40.8 ms in the low, mid and high bands) and closes within a window and three hops of
///   the end (28.4, 28.0 and 25.8 ms); a legato change lands within a frame and three hops
///   (43 ms, mid band).
/// - **Cost**: 100, 160 and 300 ns per tick on average (low, mid, high; 0.5–1.4 % of one
///   core at 48 kHz, release build, Apple M-series). There is deliberately no
///   `tick_masked` shortcut: the analysis feeds every output (the gate needs the pitch
///   estimates, the pitch needs the level clock), so it must run whichever outputs are
///   cabled.
pub struct PitchTracker {
    sample_rate: f64,
    range: PitchRange,
    geometry: Geometry,
    /// Two cascaded biquads: a 4th-order Butterworth anti-alias low-pass at 80 % of the
    /// analysis Nyquist. Bypassed when there is no decimation.
    lowpass: [Biquad; 2],
    /// Engine samples since the last analysis sample.
    decim_phase: usize,
    /// Analysis-rate history ring.
    history: Vec<f64>,
    /// Next write position in `history`.
    history_pos: usize,
    /// Analysis samples written since reset (saturates at the frame length).
    filled: usize,
    /// Analysis samples since the last snapshot.
    since_snapshot: usize,
    /// Snapshot being analysed, oldest sample first.
    frame: Vec<f64>,
    /// RMS of the snapshot's newest window, on the `level` scale.
    frame_level: f64,
    /// The newest pitched estimate (V/Oct, Hz), waiting for the next frame to show the
    /// sound carried on past its frame (see [`conclude`](Self::conclude)).
    pending: Option<(f64, f64, f64)>,
    /// Whole-period power ratio of the last settled, pitched frame: the reference a
    /// pending estimate's confirmation is measured against.
    last_ratio: Option<f64>,
    /// Engine samples since the level last rose out of silence (a tenth of the gate
    /// threshold); saturates.
    sounding_for: usize,
    /// Difference function `d(τ)`.
    raw: Vec<f64>,
    /// Cumulative-mean-normalised difference `d'(τ)`.
    diff: Vec<f64>,
    /// Next lag to compute, or 0 when no estimate is in progress.
    next_lag: usize,
    /// Running `Σ d(1..=τ)` for the normalisation.
    diff_sum: f64,
    /// Output pitch, V/Oct.
    voct: f64,
    gate: bool,
    /// Consecutive estimates without a pitch.
    unpitched_run: usize,
    /// Two-pole mean-square smoother for `level`.
    mean_square: [f64; 2],
    smoothing: f64,
    spec: PortSpec,
}

impl PitchTracker {
    /// YIN's absolute threshold on the cumulative-mean-normalised difference: an
    /// estimate is pitched when its aperiodicity is below this.
    pub const YIN_THRESHOLD: f64 = 0.15;

    /// Time between estimates.
    pub const HOP_SECONDS: f64 = 0.005;

    /// Shortest integration window.
    const MIN_WINDOW_SECONDS: f64 = 0.020;

    /// An estimate is used only if its frame's newest whole periods hold between these
    /// multiples of its oldest periods' power. Wide on purpose: plucks (a power ratio of
    /// 0.3–0.5 across a frame) and swells must pass, and YIN reads a decay only slightly
    /// sharp. What it rejects is a release or attack of a few milliseconds, where the
    /// bias reaches tens of cents.
    const STEADY: (f64, f64) = (0.25, 4.0);

    /// RMS → `level` scale: a full-scale (5 V peak) sine reads 10 V.
    const LEVEL_PER_RMS: f64 = 2.0 * core::f64::consts::SQRT_2;

    /// Create a tracker for a graph running at `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        let sample_rate = if sample_rate > 0.0 {
            sample_rate
        } else {
            44_100.0
        };
        let range = PitchRange::default();
        let mut track = Self {
            sample_rate,
            range,
            geometry: Geometry::new(range, sample_rate),
            lowpass: [Biquad::default(); 2],
            decim_phase: 0,
            history: Vec::new(),
            history_pos: 0,
            filled: 0,
            since_snapshot: 0,
            frame: Vec::new(),
            frame_level: 0.0,
            pending: None,
            last_ratio: None,
            sounding_for: 0,
            raw: Vec::new(),
            diff: Vec::new(),
            next_lag: 0,
            diff_sum: 0.0,
            voct: 0.0,
            gate: false,
            unpitched_run: 0,
            mean_square: [0.0; 2],
            smoothing: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "threshold", SignalKind::CvUnipolar).with_default(0.25),
                ],
                outputs: vec![
                    PortDef::new(10, "voct", SignalKind::VoltPerOctave),
                    PortDef::new(11, "gate", SignalKind::Gate),
                    PortDef::new(12, "level", SignalKind::CvUnipolar),
                ],
            },
        };
        track.configure();
        track
    }

    /// Builder form of [`set_range`](Self::set_range).
    pub fn with_range(mut self, range: PitchRange) -> Self {
        self.set_range(range);
        self
    }

    /// Choose the pitch band. Restarts the analysis (the history no longer matches the
    /// new analysis rate) but keeps the held pitch; never allocates.
    pub fn set_range(&mut self, range: PitchRange) {
        if range != self.range {
            self.range = range;
            self.geometry = Geometry::new(range, self.sample_rate);
            self.design_lowpass();
            self.restart_analysis();
        }
    }

    /// The pitch band searched.
    pub fn range(&self) -> PitchRange {
        self.range
    }

    /// Size buffers for every band at the current rate (so `set_range` never
    /// allocates), and derive the filters and the level smoother.
    fn configure(&mut self) {
        let geometries = PitchRange::ALL.map(|r| Geometry::new(r, self.sample_rate));
        let largest_frame = geometries
            .iter()
            .map(Geometry::frame_len)
            .max()
            .unwrap_or(0);
        let largest_lag = geometries.iter().map(|g| g.tau_max).max().unwrap_or(0);
        self.geometry = Geometry::new(self.range, self.sample_rate);
        self.history = vec![0.0; largest_frame];
        self.frame = vec![0.0; largest_frame];
        self.raw = vec![0.0; largest_lag + 1];
        self.diff = vec![0.0; largest_lag + 1];
        self.smoothing = env_coef(0.010, self.sample_rate);
        self.design_lowpass();
        self.restart_analysis();
    }

    fn design_lowpass(&mut self) {
        let g = self.geometry;
        self.lowpass = if g.decim > 1 {
            let cutoff = 0.4 * g.rate;
            [
                Biquad::lowpass(cutoff, 0.541_196_100_146_197, self.sample_rate),
                Biquad::lowpass(cutoff, 1.306_562_964_876_376_7, self.sample_rate),
            ]
        } else {
            [Biquad::default(); 2]
        };
    }

    /// Forget the analysis history and any estimate in progress.
    fn restart_analysis(&mut self) {
        self.history.fill(0.0);
        self.history_pos = 0;
        self.filled = 0;
        self.since_snapshot = 0;
        self.decim_phase = 0;
        self.next_lag = 0;
        self.diff_sum = 0.0;
        self.frame_level = 0.0;
        self.pending = None;
        self.last_ratio = None;
        self.sounding_for = 0;
        for stage in &mut self.lowpass {
            stage.z1 = 0.0;
            stage.z2 = 0.0;
        }
    }

    /// Copy the newest frame out of the history ring, oldest first, and measure the RMS
    /// of its newest window.
    fn snapshot(&mut self) {
        let g = self.geometry;
        let len = g.frame_len();
        let cap = self.history.len();
        let start = (self.history_pos + cap - len) % cap;
        for (i, slot) in self.frame[..len].iter_mut().enumerate() {
            *slot = self.history[(start + i) % cap];
        }
        let newest = self.frame[len - g.window..len]
            .iter()
            .map(|x| x * x)
            .sum::<f64>();
        self.frame_level = Libm::<f64>::sqrt(newest / g.window as f64) * Self::LEVEL_PER_RMS;
        self.next_lag = 1;
        self.diff_sum = 0.0;
        self.diff[0] = 1.0;
        self.raw[0] = 0.0;
    }

    /// Compute `d(τ)` for the next lag and fold it into the normalisation.
    fn compute_lag(&mut self) {
        let tau = self.next_lag;
        let w = self.geometry.window;
        let frame = &self.frame;
        let mut d = 0.0;
        for j in 0..w {
            let delta = frame[j] - frame[j + tau];
            d += delta * delta;
        }
        self.raw[tau] = d;
        self.diff_sum += d;
        // Cumulative-mean-normalised difference d'(τ) = d(τ) · τ / Σ d(1..=τ).
        self.diff[tau] = if self.diff_sum > 0.0 {
            d * tau as f64 / self.diff_sum
        } else {
            1.0
        };
        self.next_lag += 1;
    }

    /// Pick the period from the finished `d'(τ)`: `Some(Hz)` when pitched.
    fn estimate(&self) -> Option<f64> {
        let g = self.geometry;
        let d = &self.diff[..=g.tau_max];
        let last = g.tau_max - 1;
        // First dip under the absolute threshold, followed down to its local minimum.
        let mut tau = (g.tau_min..=last).find(|&t| d[t] < Self::YIN_THRESHOLD)?;
        while tau < last && d[tau + 1] < d[tau] {
            tau += 1;
        }
        // Parabolic interpolation around the minimum, on the raw difference: the
        // normalisation tilts d'(τ) and biases the vertex (several cents on clean
        // sines), while d(τ) is a clean cosine dip there.
        let (a, b, c) = (self.raw[tau - 1], self.raw[tau], self.raw[tau + 1]);
        let curvature = a - 2.0 * b + c;
        let shift = if curvature > 0.0 {
            (0.5 * (a - c) / curvature).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        Some(g.rate / (tau as f64 + shift))
    }

    /// Power of the frame's newest whole periods over its oldest, measured over as many
    /// whole periods of `hz` as fit the window. Whole periods make the ratio exact (1)
    /// for any steady periodic waveform, pulse-like voices included.
    fn period_ratio(&self, hz: f64) -> (f64, usize) {
        let g = self.geometry;
        let len = g.frame_len();
        let period = (Libm::<f64>::round(g.rate / hz) as usize).clamp(1, g.window);
        let span = (g.window / period) * period;
        let power = |w: &[f64]| w.iter().map(|x| x * x).sum::<f64>();
        let (newest, oldest) = (
            power(&self.frame[len - span..len]),
            power(&self.frame[..span]),
        );
        let ratio = if oldest > 0.0 { newest / oldest } else { 0.0 };
        (ratio, span)
    }

    /// Finish an estimate: confirm or drop the pending one, then update the gate.
    ///
    /// Once a sound starts or stops inside a frame, the difference function's dip
    /// shifts — by tens of cents in a band's bottom half-octave — however little of the
    /// frame is silent, so a frame that straddles an edge must not set the pitch.
    ///
    /// - **Onsets**: a frame is used only once its oldest sample postdates the moment
    ///   the level rose out of silence (`sounding_for`), so it holds no leading silence.
    ///   This keys on the level, not the waveform; a swell's rising frames also have to
    ///   pass [`STEADY`](Self::STEADY), so it opens the gate up to 25 ms later than an
    ///   abrupt onset.
    /// - **Stops** cannot be seen in the frame that straddles them, but the next frame
    ///   holds exactly one hop more of whatever followed. So a pitched estimate waits one
    ///   hop and is used only if the next frame's power ratio (newest whole periods over
    ///   oldest) is at least `1 − hop/span` (less 1 %) of the ratio of the frame *before*
    ///   the estimate's, capped at 1. Measuring against the earlier frame lets a steady
    ///   decay or swell, whose ratio repeats frame to frame, through, while a stop, which
    ///   costs the ratio one more hop of silence each frame, is caught even when the
    ///   estimate's own frame already straddles it.
    ///
    /// Costs one hop (5 ms) of latency; buys a pitch that is right when the gate rises
    /// and right after it falls.
    fn conclude(&mut self, threshold: f64) {
        let g = self.geometry;
        if let Some((voct, hz, reference)) = self.pending.take() {
            let (ratio, span) = self.period_ratio(hz);
            if ratio >= reference * (1.0 - g.hop as f64 / span as f64) - 0.01 {
                self.voct = voct;
                self.unpitched_run = 0;
                if self.frame_level >= threshold {
                    self.gate = true;
                }
            }
        }

        let audible = self.frame_level >= 0.5 * threshold && self.frame_level > 0.0;
        let settled = self.sounding_for >= (g.frame_len() + g.hop) * g.decim;
        match self.estimate().filter(|_| audible) {
            Some(hz) if settled => {
                let (ratio, _) = self.period_ratio(hz);
                if (Self::STEADY.0..=Self::STEADY.1).contains(&ratio) {
                    let reference = self.last_ratio.unwrap_or(ratio).min(1.0);
                    self.pending = Some((Libm::<f64>::log2(hz / C4_HZ), hz, reference));
                }
                self.last_ratio = Some(ratio);
            }
            Some(_) => self.last_ratio = None,
            None => {
                self.unpitched_run += 1;
                self.last_ratio = None;
            }
        }
        if self.gate
            && (self.frame_level < 0.5 * threshold || self.unpitched_run >= g.unpitched_hold)
        {
            self.gate = false;
        }
    }
}

impl Default for PitchTracker {
    fn default() -> Self {
        Self::new(44_100.0)
    }
}

impl GraphModule for PitchTracker {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let x = sanitize_audio(inputs.get_or(0, 0.0));
        let threshold = sanitize_audio(inputs.get_or(1, 0.25)).clamp(0.0, 10.0);

        // Level: RMS through two cascaded one-pole smoothers.
        let a = self.smoothing;
        let [first, second] = &mut self.mean_square;
        *first = flush_denorm(a * *first + (1.0 - a) * x * x);
        *second = flush_denorm(a * *second + (1.0 - a) * *first);
        let level = (Libm::<f64>::sqrt(*second) * Self::LEVEL_PER_RMS).min(10.0);
        self.sounding_for = if level > 0.1 * threshold {
            self.sounding_for.saturating_add(1)
        } else {
            0
        };

        // Decimate into the analysis history; snapshot a frame every hop.
        let g = self.geometry;
        let filtered = if g.decim > 1 {
            let y = self.lowpass[0].process(x);
            self.lowpass[1].process(y)
        } else {
            x
        };
        self.decim_phase += 1;
        if self.decim_phase >= g.decim {
            self.decim_phase = 0;
            let cap = self.history.len();
            self.history[self.history_pos] = filtered;
            self.history_pos = (self.history_pos + 1) % cap;
            self.filled = (self.filled + 1).min(g.frame_len());
            self.since_snapshot += 1;
            if self.next_lag == 0 && self.filled >= g.frame_len() && self.since_snapshot >= g.hop {
                self.since_snapshot = 0;
                self.snapshot();
            }
        }

        // Spread the difference function over the ticks of one hop.
        if self.next_lag > 0 {
            for _ in 0..g.lags_per_tick {
                self.compute_lag();
                if self.next_lag > g.tau_max {
                    self.next_lag = 0;
                    self.conclude(threshold);
                    break;
                }
            }
        }

        outputs.set(10, self.voct);
        outputs.set(11, if self.gate { GATE_HIGH_V } else { 0.0 });
        outputs.set(12, level);
    }

    fn reset(&mut self) {
        self.restart_analysis();
        self.voct = 0.0;
        self.gate = false;
        self.unpitched_run = 0;
        self.mean_square = [0.0; 2];
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        if sample_rate > 0.0 && sample_rate != self.sample_rate {
            self.sample_rate = sample_rate;
            self.configure();
        }
    }

    fn type_id(&self) -> &'static str {
        "pitch_tracker"
    }

    crate::impl_introspect!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    const SR: f64 = 48_000.0;

    /// Feed `samples` and return the `[voct, gate, level]` after each.
    fn run(track: &mut PitchTracker, samples: impl IntoIterator<Item = f64>) -> Vec<[f64; 3]> {
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        samples
            .into_iter()
            .map(|x| {
                inputs.set(0, x);
                track.tick(&inputs, &mut outputs);
                [
                    outputs.get(10).unwrap(),
                    outputs.get(11).unwrap(),
                    outputs.get(12).unwrap(),
                ]
            })
            .collect()
    }

    fn sine(hz: f64, amplitude: f64, seconds: f64) -> impl Iterator<Item = f64> {
        let n = (seconds * SR) as usize;
        (0..n).map(move |i| {
            amplitude * Libm::<f64>::sin(2.0 * core::f64::consts::PI * hz * i as f64 / SR)
        })
    }

    fn silence(seconds: f64) -> impl Iterator<Item = f64> {
        core::iter::repeat(0.0).take((seconds * SR) as usize)
    }

    fn cents(voct: f64, hz: f64) -> f64 {
        (voct - Libm::<f64>::log2(hz / C4_HZ)) * 1200.0
    }

    /// Peak amplitude of uniform white noise whose RMS is `snr_db` below a 2.5 V sine's.
    fn noise_peak(snr_db: f64) -> f64 {
        let tone_rms = 2.5 / core::f64::consts::SQRT_2;
        tone_rms * Libm::<f64>::pow(10.0, -snr_db / 20.0) * Libm::<f64>::sqrt(3.0)
    }

    /// Pitch error over the last 200 ms of a 400 ms, 2.5 V tone plus white noise of
    /// peak `noise`: `(worst, rms)` in cents. Asserts the gate holds throughout.
    fn pitch_error(range: PitchRange, hz: f64, noise: f64) -> (f64, f64) {
        let mut track = PitchTracker::new(SR).with_range(range);
        let mut rng = Rng::from_seed(7);
        let frames = run(
            &mut track,
            sine(hz, 2.5, 0.4).map(|s| s + noise * (2.0 * rng.next_f64() - 1.0)),
        );
        let tail = &frames[frames.len() - (0.2 * SR) as usize..];
        assert!(
            tail.iter().all(|f| f[1] == GATE_HIGH_V),
            "{range:?} {hz} Hz: gate not held"
        );
        let errors: Vec<f64> = tail.iter().map(|f| cents(f[0], hz)).collect();
        let worst = errors.iter().fold(0.0f64, |m, e| m.max(e.abs()));
        let rms =
            Libm::<f64>::sqrt(errors.iter().map(|e| e * e).sum::<f64>() / errors.len() as f64);
        (worst, rms)
    }

    const BANDS: [(PitchRange, &[f64]); 3] = [
        (
            PitchRange::Low,
            &[41.2, 55.0, 82.4, 110.0, 196.0, 330.0, 480.0],
        ),
        (
            PitchRange::Mid,
            &[73.4, 110.0, 196.0, 261.6, 440.0, 659.3, 950.0],
        ),
        (
            PitchRange::High,
            &[146.8, 261.6, 440.0, 880.0, 1318.5, 1900.0],
        ),
    ];

    #[test]
    fn tracker_reads_clean_sines_within_one_cent_in_every_band() {
        for (range, freqs) in BANDS {
            for &hz in freqs {
                let (worst, _) = pitch_error(range, hz, 0.0);
                assert!(worst < 1.0, "{range:?} {hz} Hz: {worst:.3} cents");
            }
        }
    }

    #[test]
    fn tracker_holds_pitch_in_noise() {
        // Full-band white noise 20 dB below the tone: within ±10 cents in every band.
        for (range, freqs) in BANDS {
            for &hz in freqs {
                let (worst, _) = pitch_error(range, hz, noise_peak(20.0));
                assert!(worst < 10.0, "{range:?} {hz} Hz at 20 dB: {worst:.2} cents");
            }
        }
        // 10 dB below, mid band: the gate holds and the RMS error stays under 10 cents.
        for &hz in BANDS[1].1 {
            let (_, rms) = pitch_error(PitchRange::Mid, hz, noise_peak(10.0));
            assert!(rms < 10.0, "{hz} Hz at 10 dB: {rms:.2} cents RMS");
        }
    }

    #[test]
    fn tracker_ignores_white_noise() {
        for range in PitchRange::ALL {
            let mut track = PitchTracker::new(SR).with_range(range);
            let mut rng = Rng::from_seed(11);
            let frames = run(
                &mut track,
                (0..SR as usize).map(|_| 4.0 * (2.0 * rng.next_f64() - 1.0)),
            );
            assert!(
                frames.iter().all(|f| f[1] == 0.0),
                "{range:?}: noise opened the gate"
            );
            assert!(
                frames.last().unwrap()[2] > 1.0,
                "the level still follows noise"
            );
        }
    }

    #[test]
    fn tracker_gate_follows_a_tone_burst() {
        // 100 ms silence, 500 ms tone, 300 ms silence, in every band.
        // (band, Hz, documented open ms, documented close ms): the numbers in the docs.
        for (range, hz, doc_on, doc_off) in [
            (PitchRange::Low, 82.4, 63.4, 28.4),
            (PitchRange::Mid, 330.0, 48.0, 28.0),
            (PitchRange::High, 880.0, 40.8, 25.8),
        ] {
            let mut track = PitchTracker::new(SR).with_range(range);
            let frames = run(
                &mut track,
                silence(0.1).chain(sine(hz, 2.5, 0.5)).chain(silence(0.3)),
            );
            let gate: Vec<bool> = frames.iter().map(|f| f[1] > 2.5).collect();
            let edges: Vec<usize> = (1..gate.len())
                .filter(|&i| gate[i] != gate[i - 1])
                .collect();
            assert_eq!(
                edges.len(),
                2,
                "{range:?}: one clean on/off, no chatter: {edges:?}"
            );
            // Opens within a frame and three hops of the onset (the frame must hold no
            // silence, then one hop to compute and one to confirm); closes within a
            // window and three hops of the end. And matches the documented figures, so
            // the docs cannot go stale.
            let g = track.geometry;
            let ms = |samples: usize| samples as f64 / g.rate * 1000.0;
            let hop_ms = PitchTracker::HOP_SECONDS * 1000.0;
            let on_ms = (edges[0] as f64 / SR - 0.1) * 1000.0;
            let off_ms = (edges[1] as f64 / SR - 0.6) * 1000.0;
            assert!(
                (0.0..=ms(g.frame_len()) + 3.0 * hop_ms).contains(&on_ms),
                "{range:?}: opened {on_ms:.1} ms after onset"
            );
            assert!(
                (0.0..=ms(g.window) + 3.0 * hop_ms).contains(&off_ms),
                "{range:?}: closed {off_ms:.1} ms after offset"
            );
            assert!(
                (on_ms - doc_on).abs() < 1.0 && (off_ms - doc_off).abs() < 1.0,
                "{range:?}: {on_ms:.1}/{off_ms:.1} ms; the docs say {doc_on}/{doc_off}"
            );
            // The pitch is right when the gate rises, and held after it falls.
            let at_rise = cents(frames[edges[0]][0], hz);
            assert!(
                at_rise.abs() < 1.0,
                "{range:?}: {at_rise:.1} cents at the gate"
            );
            assert!(
                cents(frames.last().unwrap()[0], hz).abs() < 1.0,
                "{range:?}: held pitch"
            );
        }
    }

    /// Documented legato settling time, mid band (milliseconds).
    const LEGATO_MS: f64 = 43.0;

    #[test]
    fn tracker_follows_a_legato_pitch_change() {
        // 220 Hz into 330 Hz with no gap: the gate stays high and the pitch moves.
        let mut track = PitchTracker::new(SR);
        let frames = run(
            &mut track,
            sine(220.0, 2.5, 0.3).chain(sine(330.0, 2.5, 0.3)),
        );
        let second = &frames[(0.3 * SR) as usize..];
        assert!(
            second.iter().all(|f| f[1] == GATE_HIGH_V),
            "gate dropped on a legato change"
        );
        // The new pitch lands within a frame and three hops (and as documented, ±1 ms).
        let g = track.geometry;
        let bound = g.frame_len() as f64 / g.rate + 3.0 * PitchTracker::HOP_SECONDS;
        let settled = second
            .iter()
            .position(|f| cents(f[0], 330.0).abs() < 5.0)
            .unwrap();
        let settled_ms = settled as f64 / SR * 1000.0;
        assert!(settled_ms < bound * 1000.0, "took {settled_ms} ms");
        assert!(
            (settled_ms - LEGATO_MS).abs() < 1.0,
            "{settled_ms} ms; the docs say {LEGATO_MS}"
        );
        assert!(cents(second.last().unwrap()[0], 330.0).abs() < 1.0);
    }

    #[test]
    fn tracker_threshold_and_level() {
        // Level is RMS-based: a sine reads 2√2 × RMS = 2 × peak, and a low note does not
        // ripple with its waveform.
        let mut track = PitchTracker::new(SR);
        let frames = run(&mut track, sine(55.0, 2.5, 0.4));
        let tail: Vec<f64> = frames[frames.len() - 4800..].iter().map(|f| f[2]).collect();
        let (lo, hi) = tail
            .iter()
            .fold((f64::MAX, 0.0f64), |(l, h), &v| (l.min(v), h.max(v)));
        assert!(
            lo > 4.75 && hi < 5.25,
            "55 Hz level ripples: {lo:.3}..{hi:.3}"
        );
        let mut track = PitchTracker::new(SR);
        let frames = run(&mut track, sine(220.0, 5.0, 0.2));
        assert!(
            (frames.last().unwrap()[2] - 10.0).abs() < 0.1,
            "full scale reads 10 V"
        );

        // A tone under the threshold never opens the gate; lowering the threshold does.
        let quiet = 0.05; // level ≈ 0.1 V, under the 0.25 V default
        let mut track = PitchTracker::new(SR);
        assert!(run(&mut track, sine(220.0, quiet, 0.3))
            .iter()
            .all(|f| f[1] == 0.0));
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 0.05);
        let mut opened = false;
        for s in sine(220.0, quiet, 0.3) {
            inputs.set(0, s);
            track.tick(&inputs, &mut outputs);
            opened |= outputs.get(11).unwrap() > 0.0;
        }
        assert!(opened, "a lower threshold opens the gate");
        assert!(cents(outputs.get(10).unwrap(), 220.0).abs() < 1.0);
    }

    #[test]
    fn tracker_range_reset_and_sample_rate() {
        let mut track = PitchTracker::new(SR);
        assert_eq!(track.range(), PitchRange::Mid);
        // 1500 Hz is above the mid band; the high band reads it.
        let frames = run(&mut track, sine(1500.0, 2.5, 0.3));
        assert!(cents(frames.last().unwrap()[0], 1500.0).abs() > 50.0);
        track.set_range(PitchRange::High);
        let frames = run(&mut track, sine(1500.0, 2.5, 0.3));
        assert!(cents(frames.last().unwrap()[0], 1500.0).abs() < 1.0);

        // reset() returns to the fresh state: the same input renders the same output.
        track.reset();
        let a = run(&mut track, sine(440.0, 2.5, 0.2));
        let mut fresh = PitchTracker::new(SR).with_range(PitchRange::High);
        assert_eq!(a, run(&mut fresh, sine(440.0, 2.5, 0.2)));

        // Other sample rates keep the accuracy.
        for sr in [22_050.0, 44_100.0, 96_000.0] {
            let mut track = PitchTracker::new(SR);
            track.set_sample_rate(sr);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            for i in 0..(0.4 * sr) as usize {
                let t = i as f64 / sr;
                inputs.set(
                    0,
                    2.5 * Libm::<f64>::sin(2.0 * core::f64::consts::PI * 440.0 * t),
                );
                track.tick(&inputs, &mut outputs);
            }
            assert!(
                cents(outputs.get(10).unwrap(), 440.0).abs() < 1.0,
                "{sr} Hz"
            );
            assert_eq!(outputs.get(11), Some(GATE_HIGH_V));
        }
        assert_eq!(track.type_id(), "pitch_tracker");
        assert_eq!(PitchRange::from_index(2.2), Some(PitchRange::High));
        assert_eq!(PitchRange::from_index(-1.0), None);
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(PitchRange::from_index(bad), None, "{bad}");
        }
        #[cfg(feature = "alloc")]
        {
            use crate::introspection::ModuleIntrospection;
            let mut tracker = PitchTracker::new(SR).with_range(PitchRange::High);
            assert!(
                !tracker.set_param_by_id("range", f64::NAN),
                "NaN does not pick low"
            );
            assert_eq!(tracker.range(), PitchRange::High);
        }
    }

    /// Edges are where YIN goes wrong: once a frame holds a start or a stop, its dip
    /// shifts. Across stop alignments, abrupt and natural releases, and notes from two
    /// semitones above each band's floor, the pitch is right when the gate rises (±1
    /// cent) and after it falls (±5 cents; ±10 after a 30 ms release).
    #[test]
    fn tracker_pitch_is_clean_at_note_edges() {
        for range in PitchRange::ALL {
            let f_min = range.bounds().0;
            for semitones in [2.0, 7.0, 19.0] {
                let hz = f_min * Libm::<f64>::pow(2.0, semitones / 12.0);
                for release in [0.0, 0.010, 0.030] {
                    for shift in 0..3 {
                        let tone = (0.2 * SR) as usize + shift * 337;
                        let tail = release * SR;
                        let signal =
                            (0..(0.1 * SR) as usize + tone + (0.15 * SR) as usize).map(|i| {
                                let k = i as f64 - 0.1 * SR;
                                let env = if k < 0.0 {
                                    0.0
                                } else if k < tone as f64 {
                                    1.0
                                } else if tail > 0.0 {
                                    Libm::<f64>::exp(-(k - tone as f64) / tail)
                                } else {
                                    0.0
                                };
                                let phase = 2.0 * core::f64::consts::PI * hz * k / SR;
                                env * 2.5 * Libm::<f64>::sin(phase)
                            });
                        let mut track = PitchTracker::new(SR).with_range(range);
                        let frames = run(&mut track, signal);
                        let rise = frames.windows(2).position(|w| w[1][1] > w[0][1]).unwrap() + 1;
                        let at_rise = cents(frames[rise][0], hz);
                        let held = cents(frames.last().unwrap()[0], hz);
                        let case =
                            format!("{range:?} {hz:.1} Hz, release {release} s, shift {shift}");
                        assert!(
                            at_rise.abs() < 1.0,
                            "{case}: {at_rise:.2} cents at the gate"
                        );
                        // A slow release is a fast decay, and YIN reads a fast decay a
                        // little sharp ((P/T)^2 / 4 pi^2): 6.8 cents measured at 78.6 Hz.
                        let tolerance = if release > 0.02 { 10.0 } else { 5.0 };
                        assert!(held.abs() < tolerance, "{case}: held {held:.2} cents");
                        assert_eq!(frames.last().unwrap()[1], 0.0, "{case}: gate closed");
                    }
                }
            }
        }
    }

    /// Render a tone with amplitude envelope `env(t)` (0 before `t = 0`) after 50 ms of
    /// silence; return the gate-open time (ms after the onset), the cents at the gate's
    /// rise, and the worst cents while the gate is high.
    fn envelope_probe(range: PitchRange, hz: f64, env: &dyn Fn(f64) -> f64) -> (f64, f64, f64) {
        let pad = 0.05 * SR;
        let signal = (0..(0.65 * SR) as usize).map(|i| {
            let t = (i as f64 - pad) / SR;
            if t < 0.0 {
                0.0
            } else {
                env(t) * Libm::<f64>::sin(2.0 * core::f64::consts::PI * hz * t)
            }
        });
        let frames = run(&mut PitchTracker::new(SR).with_range(range), signal);
        let rise = frames
            .windows(2)
            .position(|w| w[1][1] > w[0][1])
            .expect("the gate opened")
            + 1;
        let open_ms = (rise as f64 - pad) / SR * 1000.0;
        let worst = frames
            .iter()
            .filter(|f| f[1] > 0.0)
            .map(|f| cents(f[0], hz).abs())
            .fold(0.0, f64::max);
        (open_ms, cents(frames[rise][0], hz), worst)
    }

    /// Plucked and struck notes decay from their first sample. They open the gate as
    /// promptly as a held note, read within 5 cents, and a fast low pluck stays within
    /// the documented decay bias.
    #[test]
    fn tracker_gates_decaying_plucks() {
        for (range, hz, tau) in [
            (PitchRange::Low, 110.0, 0.05),
            (PitchRange::Low, 110.0, 0.15),
            (PitchRange::Mid, 220.0, 0.03),
            (PitchRange::Mid, 220.0, 0.05),
            (PitchRange::High, 440.0, 0.02),
            (PitchRange::High, 440.0, 0.05),
        ] {
            let g = PitchTracker::new(SR).with_range(range).geometry;
            let bound = (g.frame_len() as f64 / g.rate + 3.0 * PitchTracker::HOP_SECONDS) * 1000.0;
            let (open, rise, worst) =
                envelope_probe(range, hz, &|t| 2.5 * Libm::<f64>::exp(-t / tau));
            let case = format!("{range:?} {hz} Hz, tau {tau} s");
            assert!(open <= bound, "{case}: opened at {open:.1} ms");
            assert!(
                rise.abs() < 5.0 && worst < 5.0,
                "{case}: {rise:.2} / {worst:.2} cents"
            );
        }
        // A fast decay near a band's floor reads sharp by about (P/T)^2 / 4 pi^2.
        let (_, _, worst) = envelope_probe(PitchRange::Low, 55.0, &|t| {
            2.5 * Libm::<f64>::exp(-t / 0.05)
        });
        assert!(worst < 15.0, "55 Hz, tau 50 ms: {worst:.2} cents");
    }

    /// A swelling onset opens the gate at most 25 ms after an abrupt one, on a pitch
    /// within 5 cents.
    #[test]
    fn tracker_swells_open_promptly() {
        for (range, hz) in [
            (PitchRange::Low, 110.0),
            (PitchRange::Mid, 220.0),
            (PitchRange::High, 440.0),
        ] {
            let (abrupt, _, _) = envelope_probe(range, hz, &|_| 2.5);
            for attack in [0.05, 0.1, 0.2] {
                let (open, rise, _) = envelope_probe(range, hz, &|t| 2.5 * (t / attack).min(1.0));
                let case = format!("{range:?} {hz} Hz, {attack} s attack");
                assert!(
                    open - abrupt <= 25.0,
                    "{case}: {open:.1} ms against {abrupt:.1}"
                );
                assert!(rise.abs() < 5.0, "{case}: {rise:.2} cents");
            }
        }
    }

    /// Harmonic-rich tones (a band-limited sawtooth) read within 6 cents across each band,
    /// worst near its top; a tone with no fundamental at all reads right mid-band.
    #[test]
    fn tracker_reads_harmonic_rich_tones() {
        use core::f64::consts::PI;
        for (range, freqs) in [
            (PitchRange::Low, &[41.2, 110.0, 250.0, 480.0][..]),
            (PitchRange::Mid, &[73.4, 220.0, 500.0, 950.0][..]),
            (PitchRange::High, &[146.8, 440.0, 1000.0, 1900.0][..]),
        ] {
            for &hz in freqs {
                let saw = (0..(0.4 * SR) as usize).map(|i| {
                    let t = i as f64 / SR;
                    let mut s = 0.0;
                    let mut n = 1.0;
                    while n * hz < 8_000.0 {
                        s += Libm::<f64>::sin(2.0 * PI * n * hz * t) / n;
                        n += 1.0;
                    }
                    1.6 * s
                });
                let frames = run(&mut PitchTracker::new(SR).with_range(range), saw);
                let tail = &frames[frames.len() - (0.2 * SR) as usize..];
                assert!(
                    tail.iter().all(|f| f[1] == GATE_HIGH_V),
                    "{range:?} {hz} Hz"
                );
                let worst = tail
                    .iter()
                    .map(|f| cents(f[0], hz).abs())
                    .fold(0.0, f64::max);
                assert!(worst < 6.0, "{range:?} {hz} Hz sawtooth: {worst:.2} cents");
            }
        }
        for hz in [110.0, 220.0, 500.0] {
            let missing = (0..(0.4 * SR) as usize).map(|i| {
                let t = i as f64 / SR;
                (2..=6)
                    .map(|n| Libm::<f64>::sin(2.0 * PI * n as f64 * hz * t))
                    .sum::<f64>()
                    * 0.8
            });
            let frames = run(&mut PitchTracker::new(SR), missing);
            let c = cents(frames.last().unwrap()[0], hz);
            assert!(
                c.abs() < 5.0,
                "{hz} Hz without its fundamental: {c:.1} cents"
            );
        }
    }
}
