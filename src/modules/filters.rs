//! Filter modules.

use crate::modules::common::{flush_denorm, sanitize_audio};
use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use alloc::vec;
use core::f64::consts::{PI, TAU};
use libm::Libm;

/// State Variable Filter (SVF)
///
/// A versatile 12dB/oct filter with simultaneous lowpass, bandpass,
/// highpass, and notch outputs. Features cutoff, resonance, FM, and
/// keyboard tracking inputs.
///
/// Implemented as a Zavalishin topology-preserving-transform (TPT / zero-delay
/// feedback) SVF. The prewarped coefficient `g = tan(π·fc/fs)` keeps the cutoff
/// correctly tuned all the way toward Nyquist (unlike the older Chamberlin core
/// whose `2·sin(π·fc/fs)` coefficient froze above ~fs/6), and the trapezoidal
/// integrator states are bounded by a soft nonlinearity so high resonance
/// self-oscillates stably instead of diverging.
///
/// Phase 3 features:
/// - Self-oscillation at high resonance values
/// - Keyboard tracking for filter-follows-pitch
pub struct Svf {
    /// First trapezoidal integrator state (TPT `ic1eq`).
    ic1eq: f64,
    /// Second trapezoidal integrator state (TPT `ic2eq`).
    ic2eq: f64,
    sample_rate: f64,
    spec: PortSpec,
}

/// Minimum damping factor `k` (`= 1/Q`). Floored strictly positive so the
/// linear TPT core keeps its poles inside the unit circle (never diverges);
/// at `res = 1` this leaves a near-lossless resonator that sustains a long,
/// bounded self-oscillation.
const SVF_K_MIN: f64 = 1e-5;

/// Soft-clip limit (volts) applied to the SVF integrator states. Chosen well
/// above the nominal ±5 V audio range so ordinary signals pass through
/// linearly, while still bounding runaway energy under heavy drive at extreme
/// resonance.
const SVF_STATE_LIMIT: f64 = 8.0;

/// Bounded nonlinearity for the SVF integrator states: identity within
/// `±SVF_STATE_LIMIT`, tanh-limited beyond. Keeps self-oscillation and hard
/// drive finite without distorting normal-level audio.
#[inline]
fn svf_soft_clip(x: f64) -> f64 {
    if Libm::<f64>::fabs(x) <= SVF_STATE_LIMIT {
        x
    } else {
        SVF_STATE_LIMIT * Libm::<f64>::tanh(x / SVF_STATE_LIMIT)
    }
}

impl Svf {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            ic1eq: 0.0,
            ic2eq: 0.0,
            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "cutoff", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "res", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(3, "fm", SignalKind::CvBipolar).with_attenuverter(),
                    // Phase 3: Keyboard tracking input
                    PortDef::new(4, "keytrack", SignalKind::VoltPerOctave),
                    // Phase 3: Keyboard tracking amount (0-1)
                    PortDef::new(5, "keytrack_amt", SignalKind::CvUnipolar).with_default(0.0),
                ],
                outputs: vec![
                    PortDef::new(10, "lp", SignalKind::Audio),
                    PortDef::new(11, "bp", SignalKind::Audio),
                    PortDef::new(12, "hp", SignalKind::Audio),
                    PortDef::new(13, "notch", SignalKind::Audio),
                ],
            },
        }
    }
}

impl Default for Svf {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Svf {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize so a non-finite input can never poison the resonant
        // TPT integrator state (which would otherwise latch NaN forever).
        let input = sanitize_audio(inputs.get_or(0, 0.0));
        let cutoff_cv = inputs.get_or(1, 0.5) + inputs.get_or(3, 0.0);
        let res = inputs.get_or(2, 0.0).clamp(0.0, 1.0);

        // Phase 3: Keyboard tracking
        let keytrack_voct = inputs.get_or(4, 0.0);
        let keytrack_amt = inputs.get_or(5, 0.0).clamp(0.0, 1.0);

        // Calculate base cutoff frequency
        let base_cutoff_hz = 20.0 * Libm::<f64>::pow(1000.0, cutoff_cv.clamp(0.0, 1.0));

        // Apply keyboard tracking: each octave of V/Oct doubles the cutoff
        let keytrack_multiplier = Libm::<f64>::pow(2.0, keytrack_voct * keytrack_amt);
        let cutoff_hz = (base_cutoff_hz * keytrack_multiplier).clamp(20.0, 20000.0);

        // TPT prewarp: g = tan(π·fc/fs). Valid all the way toward Nyquist, so the
        // cutoff stays correctly tuned across the whole advertised range. Clamp fc
        // just below Nyquist (0.49·fs) so tan() never blows up near π/2.
        let max_fc = 0.49 * self.sample_rate;
        let fc = Libm::<f64>::fmin(cutoff_hz, max_fc);
        let g = Libm::<f64>::tan(PI * fc / self.sample_rate);

        // Damping k = 1/Q, parameterized as k = 2 - 2·res (res 0 → k=2 / Q=0.5,
        // res 1 → k≈0 / near-infinite Q). Floored strictly positive so the linear
        // core's poles stay inside the unit circle and can never diverge.
        let k = Libm::<f64>::fmax(2.0 - 2.0 * res, SVF_K_MIN);

        // Zero-delay-feedback (Cytomic) coefficients.
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        // Resolve the loop for this sample (no unit delay in the feedback path).
        let v0 = input;
        let v3 = v0 - self.ic2eq;
        let v1 = a1 * self.ic1eq + a2 * v3;
        let v2 = self.ic2eq + a2 * self.ic1eq + a3 * v3;

        // Trapezoidal integrator update s = 2·v - s_old, with the bounded
        // nonlinearity keeping self-oscillation and hard drive finite, and
        // denormal flushing to dodge CPU denormal penalties.
        self.ic1eq = flush_denorm(svf_soft_clip(2.0 * v1 - self.ic1eq));
        self.ic2eq = flush_denorm(svf_soft_clip(2.0 * v2 - self.ic2eq));

        let low = v2;
        let band = v1;
        let high = v0 - k * v1 - v2;
        let notch = low + high; // = v0 - k·v1

        outputs.set(10, low); // LP
        outputs.set(11, band); // BP
        outputs.set(12, high); // HP
        outputs.set(13, notch); // Notch
    }

    fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "svf"
    }
}

/// Diode Ladder Filter
///
/// A 24dB/oct (4-pole) lowpass filter modeled after the classic TB-303 / Moog
/// diode ladder topology. Features:
/// - Characteristic "squelchy" resonance
/// - Keyboard tracking
/// - Self-oscillation at high resonance
/// - Non-linear diode saturation at each stage
///
/// This is a Phase 3 addition.
pub struct DiodeLadderFilter {
    /// Filter stages (4 poles)
    stages: [f64; 4],
    /// Feedback path
    feedback: f64,
    /// Sample rate
    sample_rate: f64,
    /// Port specification
    spec: PortSpec,
}

impl DiodeLadderFilter {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            stages: [0.0; 4],
            feedback: 0.0,
            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "cutoff", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "res", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(3, "fm", SignalKind::CvBipolar).with_attenuverter(),
                    PortDef::new(4, "keytrack", SignalKind::VoltPerOctave),
                    PortDef::new(5, "keytrack_amt", SignalKind::CvUnipolar).with_default(0.0),
                    PortDef::new(6, "drive", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "pole1", SignalKind::Audio), // 6dB/oct
                    PortDef::new(12, "pole2", SignalKind::Audio), // 12dB/oct
                    PortDef::new(13, "pole3", SignalKind::Audio), // 18dB/oct
                ],
            },
        }
    }

    /// Diode saturation curve - asymmetric soft clipping
    #[inline]
    fn diode_sat(x: f64) -> f64 {
        // Asymmetric tanh-like saturation mimicking diode behavior
        if x >= 0.0 {
            Libm::<f64>::tanh(x * 1.2)
        } else {
            Libm::<f64>::tanh(x * 0.8)
        }
    }

    /// Run the 4-stage saturated one-pole cascade once for input `u` (volts)
    /// against the current stage states, using the true TPT one-pole update.
    ///
    /// `big_g = g/(1+g)` is the ZDF integrator gain. Returns the four stage
    /// outputs `y` and the updated states `new_s` (`= 2·y - s_old`, giving the
    /// bilinear pole `(1-g)/(1+g)`), without mutating `self`. Keeping this pure
    /// lets the resonance feedback be resolved within the sample by evaluating
    /// the cascade a few times before committing the state.
    #[inline]
    fn run_cascade(u: f64, s: &[f64; 4], big_g: f64) -> ([f64; 4], [f64; 4]) {
        let mut y = [0.0f64; 4];
        let mut new_s = [0.0f64; 4];
        // Drive into the first stage through the diode nonlinearity (±5 V scale).
        let mut x = Self::diode_sat(u / 5.0) * 5.0;
        for i in 0..4 {
            let v = (x - s[i]) * big_g; // v = (x - s)·g/(1+g)
            let yi = v + s[i]; // TPT output
            y[i] = yi;
            new_s[i] = yi + v; // = 2·y - s_old  (bilinear pole)
                               // Inter-stage diode saturation feeds the next pole.
            x = Self::diode_sat(yi / 5.0) * 5.0;
        }
        (y, new_s)
    }
}

impl Default for DiodeLadderFilter {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for DiodeLadderFilter {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize so a non-finite input can never poison the ladder
        // feedback stages (which would otherwise latch NaN forever).
        let input = sanitize_audio(inputs.get_or(0, 0.0));
        let cutoff_cv = inputs.get_or(1, 0.5) + inputs.get_or(3, 0.0);
        let res = inputs.get_or(2, 0.0).clamp(0.0, 1.0);
        let keytrack_voct = inputs.get_or(4, 0.0);
        let keytrack_amt = inputs.get_or(5, 0.0).clamp(0.0, 1.0);
        let drive = inputs.get_or(6, 0.0).clamp(0.0, 1.0);

        // Calculate base cutoff frequency (20 Hz - 20 kHz)
        let base_cutoff_hz = 20.0 * Libm::<f64>::pow(1000.0, cutoff_cv.clamp(0.0, 1.0));

        // Apply keyboard tracking
        let keytrack_multiplier = Libm::<f64>::pow(2.0, keytrack_voct * keytrack_amt);
        let cutoff_hz = (base_cutoff_hz * keytrack_multiplier).clamp(20.0, 20000.0);

        // TPT prewarp: g = tan(π·fc/fs); big_g = g/(1+g) is the ZDF one-pole gain.
        // Clamp fc just below Nyquist so tan() stays well-conditioned.
        let max_fc = 0.49 * self.sample_rate;
        let fc = Libm::<f64>::fmin(cutoff_hz, max_fc);
        let wc = PI * fc / self.sample_rate;
        let g = Libm::<f64>::tan(wc);
        let big_g = g / (1.0 + g);

        // Resonance with self-oscillation capability
        // k = 4 for self-oscillation in 4-pole ladder
        let k = res * 4.0;

        // Drive amount for input saturation
        let drive_gain = 1.0 + drive * 3.0;

        // Apply input drive
        let input_driven = Self::diode_sat(input / 5.0 * drive_gain) * 5.0;

        // Resolve the resonance feedback *within* this sample (Q012). The global
        // k·output term makes the cascade an implicit system; rather than reading
        // the previous sample's output (a full unit delay that detunes resonance
        // and self-oscillation pitch), we approximate the zero-delay solution with
        // a short fixed-point iteration. Two passes over the (nonlinear) cascade
        // with the stage states held fixed get the feedback estimate close to the
        // converged value; the diode saturation on the feedback keeps it bounded,
        // so it is stable even at maximum resonance. Documented as a 2-iteration
        // fixed-point approximation of the true ZDF ladder solve.
        let mut fb_norm = self.feedback; // start from last sample's stage-4 output
        for _ in 0..2 {
            let fb = Self::diode_sat(fb_norm * k);
            let u = input_driven - fb * 5.0;
            let (y, _) = Self::run_cascade(u, &self.stages, big_g);
            fb_norm = y[3] / 5.0;
        }

        // Final pass with the converged feedback; this one commits the state.
        let fb = Self::diode_sat(fb_norm * k);
        let u = input_driven - fb * 5.0;
        let (y, new_s) = Self::run_cascade(u, &self.stages, big_g);

        // Commit state with denormal flushing (Q011/Q012 stability).
        self.stages[0] = flush_denorm(new_s[0]);
        self.stages[1] = flush_denorm(new_s[1]);
        self.stages[2] = flush_denorm(new_s[2]);
        self.stages[3] = flush_denorm(new_s[3]);
        self.feedback = flush_denorm(y[3] / 5.0);

        // Outputs (all normalized to ±5V range)
        outputs.set(10, y[3]); // 24dB/oct (main output)
        outputs.set(11, y[0]); // 6dB/oct
        outputs.set(12, y[1]); // 12dB/oct
        outputs.set(13, y[2]); // 18dB/oct
    }

    fn reset(&mut self) {
        self.stages = [0.0; 4];
        self.feedback = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "diode_ladder"
    }
}

/// 3-Band Parametric Equalizer
///
/// A flexible tone-shaping EQ with:
/// - Low shelf (50-500 Hz)
/// - Parametric mid with adjustable Q (200 Hz - 8 kHz)
/// - High shelf (2-12 kHz)
///
/// Each band has ±12dB gain range. Uses biquad filters in
/// Transposed Direct Form II for numerical stability.
pub struct ParametricEq {
    // Biquad state for each band (z1, z2)
    low_state: [f64; 2],
    mid_state: [f64; 2],
    high_state: [f64; 2],
    // Cached biquad coefficients [b0, b1, b2, a1, a2] per band (Q109). The three
    // coefficient sets are pow/cos/sin/sqrt-heavy; caching lets the tick path
    // reuse them and recompute only the band whose parameters actually changed.
    low_coefs: [f64; 5],
    mid_coefs: [f64; 5],
    high_coefs: [f64; 5],
    // Last-seen resolved parameters that determine each band's coefficients.
    // Seeded with NaN so the first tick always recomputes (NaN != anything).
    cached_low: [f64; 2],  // [low_freq, low_gain_db]
    cached_mid: [f64; 3],  // [mid_freq, mid_gain_db, mid_q]
    cached_high: [f64; 2], // [high_freq, high_gain_db]
    /// Number of per-band coefficient recomputes performed (diagnostics/tests).
    recompute_count: u64,
    sample_rate: f64,
    spec: PortSpec,
}

impl ParametricEq {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            low_state: [0.0; 2],
            mid_state: [0.0; 2],
            high_state: [0.0; 2],
            low_coefs: [0.0; 5],
            mid_coefs: [0.0; 5],
            high_coefs: [0.0; 5],
            cached_low: [f64::NAN; 2],
            cached_mid: [f64::NAN; 3],
            cached_high: [f64::NAN; 2],
            recompute_count: 0,
            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "low_gain", SignalKind::CvBipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(2, "low_freq", SignalKind::CvUnipolar)
                        .with_default(0.2)
                        .with_attenuverter(),
                    PortDef::new(3, "mid_gain", SignalKind::CvBipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(4, "mid_freq", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(5, "mid_q", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(6, "high_gain", SignalKind::CvBipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(7, "high_freq", SignalKind::CvUnipolar)
                        .with_default(0.7)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }

    /// Calculate low shelf biquad coefficients
    /// Returns [b0, b1, b2, a1, a2] normalized
    fn calc_low_shelf(freq: f64, gain_db: f64, sample_rate: f64) -> [f64; 5] {
        let a = Libm::<f64>::pow(10.0, gain_db / 40.0);
        let w0 = TAU * freq / sample_rate;
        let cos_w0 = Libm::<f64>::cos(w0);
        let sin_w0 = Libm::<f64>::sin(w0);
        let alpha = sin_w0 / 2.0 * Libm::<f64>::sqrt(2.0);
        let sqrt_a = Libm::<f64>::sqrt(a);

        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;

        [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
    }

    /// Calculate high shelf biquad coefficients
    fn calc_high_shelf(freq: f64, gain_db: f64, sample_rate: f64) -> [f64; 5] {
        let a = Libm::<f64>::pow(10.0, gain_db / 40.0);
        let w0 = TAU * freq / sample_rate;
        let cos_w0 = Libm::<f64>::cos(w0);
        let sin_w0 = Libm::<f64>::sin(w0);
        let alpha = sin_w0 / 2.0 * Libm::<f64>::sqrt(2.0);
        let sqrt_a = Libm::<f64>::sqrt(a);

        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;

        [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
    }

    /// Calculate peaking EQ biquad coefficients
    fn calc_peaking(freq: f64, gain_db: f64, q: f64, sample_rate: f64) -> [f64; 5] {
        let a = Libm::<f64>::pow(10.0, gain_db / 40.0);
        let w0 = TAU * freq / sample_rate;
        let cos_w0 = Libm::<f64>::cos(w0);
        let sin_w0 = Libm::<f64>::sin(w0);
        let alpha = sin_w0 / (2.0 * q);

        let a0 = 1.0 + alpha / a;
        let b0 = (1.0 + alpha * a) / a0;
        let b1 = (-2.0 * cos_w0) / a0;
        let b2 = (1.0 - alpha * a) / a0;
        let a1 = (-2.0 * cos_w0) / a0;
        let a2 = (1.0 - alpha / a) / a0;

        [b0, b1, b2, a1, a2]
    }

    /// Process a sample through a biquad filter (Transposed Direct Form II)
    #[inline]
    fn process_biquad(input: f64, coefs: &[f64; 5], state: &mut [f64; 2]) -> f64 {
        let output = coefs[0] * input + state[0];
        state[0] = coefs[1] * input - coefs[3] * output + state[1];
        state[1] = coefs[2] * input - coefs[4] * output;
        output
    }
}

impl Default for ParametricEq {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for ParametricEq {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize the audio input so a non-finite sample can never latch
        // the recursive biquad state to NaN/Inf permanently (matching Svf and
        // DiodeLadderFilter).
        let input = sanitize_audio(inputs.get_or(0, 0.0));

        // Map CV to parameters
        // Gain: bipolar CV ±5V maps to ±12dB
        let low_gain_db = (inputs.get_or(1, 0.0) / 5.0) * 12.0;
        let mid_gain_db = (inputs.get_or(3, 0.0) / 5.0) * 12.0;
        let high_gain_db = (inputs.get_or(6, 0.0) / 5.0) * 12.0;

        // Frequencies (exponential mapping)
        let low_freq_cv = inputs.get_or(2, 0.2).clamp(0.0, 1.0);
        let low_freq = 50.0 * Libm::<f64>::pow(10.0, low_freq_cv); // 50-500 Hz

        let mid_freq_cv = inputs.get_or(4, 0.5).clamp(0.0, 1.0);
        let mid_freq = 200.0 * Libm::<f64>::pow(40.0, mid_freq_cv); // 200 Hz - 8 kHz

        let high_freq_cv = inputs.get_or(7, 0.7).clamp(0.0, 1.0);
        let high_freq = 2000.0 + high_freq_cv * 10000.0; // 2-12 kHz

        // Mid Q: 0.5 to 10
        let mid_q_cv = inputs.get_or(5, 0.5).clamp(0.0, 1.0);
        let mid_q = 0.5 + mid_q_cv * 9.5;

        // Clamp frequencies to Nyquist
        let nyquist = self.sample_rate * 0.45;
        let low_freq = low_freq.clamp(20.0, nyquist);
        let mid_freq = mid_freq.clamp(20.0, nyquist);
        let high_freq = high_freq.clamp(20.0, nyquist);

        // Recompute biquad coefficients only when a band's parameters actually
        // change (Q109). Each calc_* is pow/cos/sin/sqrt-heavy; with static params
        // this skips all of it and just runs the three process_biquad calls. The
        // reused coefficients are bit-identical to recomputing them, so the output
        // is unchanged.
        let low_params = [low_freq, low_gain_db];
        if self.cached_low != low_params {
            self.low_coefs = Self::calc_low_shelf(low_freq, low_gain_db, self.sample_rate);
            self.cached_low = low_params;
            self.recompute_count += 1;
        }
        let mid_params = [mid_freq, mid_gain_db, mid_q];
        if self.cached_mid != mid_params {
            self.mid_coefs = Self::calc_peaking(mid_freq, mid_gain_db, mid_q, self.sample_rate);
            self.cached_mid = mid_params;
            self.recompute_count += 1;
        }
        let high_params = [high_freq, high_gain_db];
        if self.cached_high != high_params {
            self.high_coefs = Self::calc_high_shelf(high_freq, high_gain_db, self.sample_rate);
            self.cached_high = high_params;
            self.recompute_count += 1;
        }

        // Process through the cascade
        let mut signal = input;
        signal = Self::process_biquad(signal, &self.low_coefs, &mut self.low_state);
        signal = Self::process_biquad(signal, &self.mid_coefs, &mut self.mid_state);
        signal = Self::process_biquad(signal, &self.high_coefs, &mut self.high_state);

        outputs.set(10, signal);
    }

    fn reset(&mut self) {
        self.low_state = [0.0; 2];
        self.mid_state = [0.0; 2];
        self.high_state = [0.0; 2];
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        // Coefficients depend on sample_rate; invalidate the cache so the next
        // tick recomputes them for the new rate even if freq/gain/Q are unchanged.
        self.cached_low = [f64::NAN; 2];
        self.cached_mid = [f64::NAN; 3];
        self.cached_high = [f64::NAN; 2];
        self.reset();
    }

    fn type_id(&self) -> &'static str {
        "parametric_eq"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::common::{measure_max_output, SAFE_AUDIO_LIMIT};

    #[test]
    fn test_svf_filter() {
        let mut svf = Svf::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Low cutoff should attenuate high frequencies
        inputs.set(0, 5.0); // Input signal
        inputs.set(1, 0.1); // Low cutoff

        svf.tick(&inputs, &mut outputs);

        // LP output should exist
        assert!(outputs.get(10).is_some());
    }
    #[test]
    fn test_svf_default_reset_sample_rate() {
        let mut svf = Svf::default();
        assert!(svf.sample_rate == 44100.0);

        svf.set_sample_rate(48000.0);
        assert!(svf.sample_rate == 48000.0);

        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 1.0);
        for _ in 0..100 {
            svf.tick(&inputs, &mut outputs);
        }

        svf.reset();
        // Field renamed from `low` to the TPT integrator state `ic1eq` in the
        // Zavalishin SVF rewrite; reset must still clear it to zero.
        assert!(svf.ic1eq == 0.0);

        assert_eq!(svf.type_id(), "svf");
    }
    #[test]
    fn test_diode_ladder_filter_coverage() {
        use crate::{Crosstalk, DiodeLadderFilter, GroundLoop};

        // DiodeLadderFilter
        let mut dlf = DiodeLadderFilter::default();
        assert!(dlf.sample_rate == 44100.0);

        dlf.set_sample_rate(48000.0);
        assert!(dlf.sample_rate == 48000.0);

        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 1.0);
        for _ in 0..100 {
            dlf.tick(&inputs, &mut outputs);
        }

        dlf.reset();
        assert!(dlf.stages[0] == 0.0);

        assert_eq!(dlf.type_id(), "diode_ladder");

        // Crosstalk
        let mut crosstalk = Crosstalk::default();
        crosstalk.set_sample_rate(48000.0);
        inputs.set(0, 1.0);
        inputs.set(1, 2.0);
        crosstalk.tick(&inputs, &mut outputs);
        crosstalk.reset();
        assert_eq!(crosstalk.type_id(), "crosstalk");

        // GroundLoop
        let mut gl = GroundLoop::default();
        gl.set_sample_rate(48000.0);
        gl.tick(&inputs, &mut outputs);
        gl.reset();
        assert_eq!(gl.type_id(), "ground_loop");
    }
    #[test]
    fn test_parametric_eq_passthrough() {
        let mut eq = ParametricEq::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // With 0 gain on all bands, signal should pass through unchanged
        inputs.set(0, 1.0); // Input signal
        inputs.set(1, 0.0); // Low gain = 0dB
        inputs.set(3, 0.0); // Mid gain = 0dB
        inputs.set(6, 0.0); // High gain = 0dB

        // Process several samples to reach steady state
        for _ in 0..1000 {
            eq.tick(&inputs, &mut outputs);
        }

        let out = outputs.get(10).unwrap();
        // Should be approximately 1.0 (input) after settling
        assert!((out - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_parametric_eq_nan_recovery() {
        // Q160: a non-finite input must not permanently latch the recursive
        // biquad state to NaN. After poisoning, a clean signal must recover.
        let mut eq = ParametricEq::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 0.0);
        inputs.set(3, 0.0);
        inputs.set(6, 0.0);

        for &bad in &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            inputs.set(0, bad);
            eq.tick(&inputs, &mut outputs);
        }

        // Feed a clean signal; the cascade must return to finite output.
        inputs.set(0, 0.5);
        let mut last = 0.0;
        for _ in 0..2000 {
            eq.tick(&inputs, &mut outputs);
            last = outputs.get(10).unwrap();
        }
        assert!(
            last.is_finite(),
            "ParametricEq output stayed non-finite after a NaN input: {last}"
        );
    }

    #[test]
    fn test_parametric_eq_low_boost() {
        let mut eq = ParametricEq::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Boost low frequencies by 12dB (+5V)
        inputs.set(0, 1.0);
        inputs.set(1, 5.0); // +12dB low gain
        inputs.set(2, 0.0); // Low frequency at minimum (50 Hz)

        for _ in 0..1000 {
            eq.tick(&inputs, &mut outputs);
        }

        let out = outputs.get(10).unwrap();
        // With boosted lows, DC-like signal should be amplified
        assert!(out > 1.0);
        assert!(out.is_finite());
    }
    #[test]
    fn test_parametric_eq_mid_cut() {
        let mut eq = ParametricEq::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Cut mid frequencies
        inputs.set(0, 1.0);
        inputs.set(3, -5.0); // -12dB mid gain
        inputs.set(5, 1.0); // High Q for narrow cut

        for _ in 0..1000 {
            eq.tick(&inputs, &mut outputs);
        }

        let out = outputs.get(10).unwrap();
        assert!(out.is_finite());
    }
    #[test]
    fn test_parametric_eq_high_boost() {
        let mut eq = ParametricEq::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 1.0);
        inputs.set(6, 5.0); // +12dB high gain

        for _ in 0..1000 {
            eq.tick(&inputs, &mut outputs);
        }

        let out = outputs.get(10).unwrap();
        assert!(out.is_finite());
    }
    #[test]
    fn test_parametric_eq_default_reset_sample_rate() {
        let mut eq = ParametricEq::default();
        assert!(eq.sample_rate == 44100.0);

        // Process some samples with non-zero gain (0dB passthrough keeps state at zero)
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 1.0);
        inputs.set(1, 2.5); // +6dB low gain (bipolar CV)
        for _ in 0..100 {
            eq.tick(&inputs, &mut outputs);
        }

        // Verify state is non-zero (filter is active with non-zero gain)
        assert!(eq.low_state[0] != 0.0 || eq.low_state[1] != 0.0);

        // Reset should clear state
        eq.reset();
        assert_eq!(eq.low_state, [0.0; 2]);
        assert_eq!(eq.mid_state, [0.0; 2]);
        assert_eq!(eq.high_state, [0.0; 2]);

        // Set sample rate
        eq.set_sample_rate(48000.0);
        assert_eq!(eq.sample_rate, 48000.0);

        assert_eq!(eq.type_id(), "parametric_eq");
        assert_eq!(eq.port_spec().inputs.len(), 8);
        assert_eq!(eq.port_spec().outputs.len(), 1);
    }
    #[test]
    fn test_parametric_eq_frequency_ranges() {
        let mut eq = ParametricEq::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test with extreme frequency settings
        inputs.set(0, 1.0);
        inputs.set(2, 0.0); // Min low freq (50 Hz)
        inputs.set(4, 0.0); // Min mid freq (200 Hz)
        inputs.set(7, 0.0); // Min high freq (2 kHz)

        for _ in 0..100 {
            eq.tick(&inputs, &mut outputs);
        }
        assert!(outputs.get(10).unwrap().is_finite());

        eq.reset();
        inputs.set(2, 1.0); // Max low freq (500 Hz)
        inputs.set(4, 1.0); // Max mid freq (8 kHz)
        inputs.set(7, 1.0); // Max high freq (12 kHz)

        for _ in 0..100 {
            eq.tick(&inputs, &mut outputs);
        }
        assert!(outputs.get(10).unwrap().is_finite());
    }
    #[test]
    fn test_parametric_eq_stability() {
        let mut eq = ParametricEq::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test with impulse input
        inputs.set(0, 5.0); // Strong impulse
        inputs.set(1, 5.0); // Extreme gain settings
        inputs.set(3, 5.0);
        inputs.set(6, 5.0);
        inputs.set(5, 1.0); // High Q

        eq.tick(&inputs, &mut outputs);

        // Continue with zero input
        inputs.set(0, 0.0);
        for _ in 0..10000 {
            eq.tick(&inputs, &mut outputs);
        }

        // Should decay to near zero, not blow up
        let out = outputs.get(10).unwrap();
        assert!(out.is_finite());
        assert!(out.abs() < 0.01);
    }
    #[test]
    fn test_svf_high_resonance_bounded() {
        // Test that SVF outputs stay bounded at various high resonance values
        // This catches the gap between 0.8-0.95 where no clipping was applied
        let test_resonances = [0.8, 0.85, 0.9, 0.92, 0.94, 0.96, 0.98, 1.0];

        for &res in &test_resonances {
            let mut svf = Svf::new(44100.0);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();

            inputs.set(0, 5.0); // Full scale input
            inputs.set(1, 0.5); // Mid cutoff
            inputs.set(2, res); // Resonance

            let max = measure_max_output(10000, || {
                svf.tick(&inputs, &mut outputs);
                // Check all outputs: LP, BP, HP, Notch
                let lp = outputs.get(10).unwrap_or(0.0).abs();
                let bp = outputs.get(11).unwrap_or(0.0).abs();
                let hp = outputs.get(12).unwrap_or(0.0).abs();
                let notch = outputs.get(13).unwrap_or(0.0).abs();
                lp.max(bp).max(hp).max(notch)
            });

            assert!(
                max <= SAFE_AUDIO_LIMIT,
                "SVF output {} exceeds safe limit {} at resonance {}",
                max,
                SAFE_AUDIO_LIMIT,
                res
            );
        }
    }
    #[test]
    fn test_svf_low_cutoff_transient_bounded() {
        // Low cutoff + high resonance + step input = potential for ringing
        let mut svf = Svf::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Very low cutoff (20Hz range)
        inputs.set(1, 0.0); // Minimum cutoff CV
        inputs.set(2, 0.9); // High resonance

        // Step input from 0 to 5V
        inputs.set(0, 0.0);
        for _ in 0..100 {
            svf.tick(&inputs, &mut outputs);
        }

        inputs.set(0, 5.0); // Step!
        let max = measure_max_output(5000, || {
            svf.tick(&inputs, &mut outputs);
            outputs.get(10).unwrap_or(0.0).abs()
        });

        assert!(
            max <= SAFE_AUDIO_LIMIT,
            "SVF transient response {} exceeds safe limit {} at low cutoff",
            max,
            SAFE_AUDIO_LIMIT
        );
    }
    #[test]
    fn test_svf_self_oscillation_bounded() {
        // Self-oscillation mode should produce bounded output
        let mut svf = Svf::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0); // No input - pure self-oscillation
        inputs.set(1, 0.5); // Mid cutoff
        inputs.set(2, 1.0); // Maximum resonance

        // Kick-start oscillation with a brief impulse
        inputs.set(0, 1.0);
        svf.tick(&inputs, &mut outputs);
        inputs.set(0, 0.0);

        // Let it oscillate for a while
        let max = measure_max_output(20000, || {
            svf.tick(&inputs, &mut outputs);
            outputs.get(10).unwrap_or(0.0).abs()
        });

        assert!(
            max <= SAFE_AUDIO_LIMIT,
            "SVF self-oscillation {} exceeds safe limit {}",
            max,
            SAFE_AUDIO_LIMIT
        );
    }
    #[test]
    fn test_svf_extreme_input_bounded() {
        // Even with garbage input (20V), output should be bounded
        let mut svf = Svf::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 20.0); // Way over nominal!
        inputs.set(1, 0.5);
        inputs.set(2, 0.9);

        let max = measure_max_output(1000, || {
            svf.tick(&inputs, &mut outputs);
            outputs.get(10).unwrap_or(0.0).abs()
        });

        assert!(
            max <= SAFE_AUDIO_LIMIT * 2.0, // Allow 2x for extreme input
            "SVF with extreme input {} exceeds limit {}",
            max,
            SAFE_AUDIO_LIMIT * 2.0
        );
    }
    #[test]
    fn test_diode_ladder_high_resonance_bounded() {
        // Diode ladder filter should also be bounded
        let mut filter = DiodeLadderFilter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 5.0); // Input
        inputs.set(1, 0.5); // Cutoff
        inputs.set(2, 1.0); // Max resonance

        let max = measure_max_output(10000, || {
            filter.tick(&inputs, &mut outputs);
            outputs.get(10).unwrap_or(0.0).abs()
        });

        assert!(
            max <= SAFE_AUDIO_LIMIT,
            "Diode ladder output {} exceeds safe limit {}",
            max,
            SAFE_AUDIO_LIMIT
        );
    }

    // ----- Wave B remediation tests -----------------------------------------

    /// CV that maps to a target cutoff frequency through `20 * 1000^cv`.
    fn cutoff_cv_for(freq_hz: f64) -> f64 {
        (freq_hz / 20.0).ln() / 1000.0_f64.ln()
    }

    fn rms(samples: &[f64]) -> f64 {
        let sum_sq: f64 = samples.iter().map(|x| x * x).sum();
        (sum_sq / samples.len() as f64).sqrt()
    }

    /// Q009: at maximum resonance with the cutoff CV maxed and a continuous
    /// drive, the SVF must stay finite and bounded indefinitely. The old
    /// Chamberlin core drove `self.low`/`self.band` to inf→NaN under exactly
    /// these conditions (negative damping, clip only on the output copies), so
    /// this test would fail on the pre-remediation code.
    #[test]
    fn test_svf_max_resonance_finite_200k() {
        let mut svf = Svf::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 1.0); // continuous DC-plus-transient drive seeds the loop
        inputs.set(1, 1.0); // cutoff CV maxed (would freeze the old Chamberlin f)
        inputs.set(2, 1.0); // maximum resonance (old code: negative damping)

        let mut max_abs = 0.0f64;
        for n in 0..200_000 {
            svf.tick(&inputs, &mut outputs);
            for &id in &[10u32, 11, 12, 13] {
                let v = outputs.get(id).unwrap();
                assert!(
                    v.is_finite(),
                    "SVF output {id} became non-finite at sample {n}"
                );
                max_abs = max_abs.max(v.abs());
            }
        }
        assert!(
            max_abs < 50.0,
            "SVF max resonance output unbounded: {max_abs}"
        );
    }

    /// Q010: the LP -3 dB corner must land near the requested cutoff across the
    /// advertised range, including well above the old ~fs/6 (~7.3 kHz) freeze.
    /// At Butterworth damping (Q = 1/sqrt(2)) the LP magnitude at fc is exactly
    /// -3 dB (0.707), while the passband gain is unity, so the RMS ratio between
    /// a tone at fc and a tone deep in the passband should be ~0.707.
    #[test]
    fn test_svf_cutoff_accuracy() {
        let sample_rate = 44100.0;
        // res giving k = 2 - 2*res = sqrt(2)  => Butterworth Q = 1/sqrt(2).
        let res = (2.0 - core::f64::consts::SQRT_2) / 2.0;

        for &target_fc in &[1000.0_f64, 10_000.0_f64] {
            let cv = cutoff_cv_for(target_fc);

            let measure = |freq: f64| -> f64 {
                let mut svf = Svf::new(sample_rate);
                let mut inputs = PortValues::new();
                let mut outputs = PortValues::new();
                inputs.set(1, cv);
                inputs.set(2, res);
                let mut out = alloc::vec::Vec::new();
                let dt = freq / sample_rate;
                let mut phase = 0.0f64;
                for n in 0..40_000 {
                    let s = Libm::<f64>::sin(TAU * phase);
                    phase += dt;
                    if phase >= 1.0 {
                        phase -= 1.0;
                    }
                    inputs.set(0, s);
                    svf.tick(&inputs, &mut outputs);
                    if n >= 20_000 {
                        out.push(outputs.get(10).unwrap());
                    }
                }
                rms(&out)
            };

            let passband = measure(target_fc / 8.0);
            let at_fc = measure(target_fc);
            let ratio = at_fc / passband;
            assert!(
                (ratio - core::f64::consts::FRAC_1_SQRT_2).abs() < 0.10,
                "SVF -3dB point off at fc={target_fc}: ratio {ratio} (expected ~0.707)"
            );
        }
    }

    /// Q009/Q010: at res=1 the SVF must self-oscillate as a *sustained* bounded
    /// tone rather than diverging or dying out. Kick it with a single impulse
    /// and confirm the ring is still present (and bounded) more than one second
    /// later.
    #[test]
    fn test_svf_self_oscillation_sustained() {
        let sample_rate = 44100.0;
        let mut svf = Svf::new(sample_rate);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(1, cutoff_cv_for(2000.0)); // ~2 kHz
        inputs.set(2, 1.0); // maximum resonance

        // Single-sample impulse kick.
        inputs.set(0, 5.0);
        svf.tick(&inputs, &mut outputs);
        inputs.set(0, 0.0);

        let mut window = alloc::vec::Vec::new();
        for n in 0..66_150 {
            // 1.5 s
            svf.tick(&inputs, &mut outputs);
            let v = outputs.get(11).unwrap(); // bandpass shows the oscillation
            assert!(v.is_finite());
            if n >= 44_100 {
                window.push(v); // measure the 1.0 s .. 1.5 s window
            }
        }
        let r = rms(&window);
        assert!(
            (0.01..20.0).contains(&r),
            "SVF self-oscillation not sustained/bounded after 1s: rms {r}"
        );
    }

    /// Q011/Q012: the resonant peak of the ladder must sit at the set cutoff.
    /// The corrected TPT one-pole (Q011) tunes each stage's pole correctly, and
    /// resolving the resonance feedback within the sample (Q012) removes the
    /// unit-delay detuning, so a small-signal frequency sweep at high resonance
    /// peaks within ~5% of the requested 2 kHz.
    #[test]
    fn test_diode_ladder_resonance_peak_frequency() {
        let sample_rate = 44100.0;
        let target_fc = 2000.0;
        let cv = cutoff_cv_for(target_fc);

        // Steady-state RMS of the main output for a small sine at `freq`.
        let gain_at = |freq: f64| -> f64 {
            let mut filter = DiodeLadderFilter::new(sample_rate);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(1, cv);
            inputs.set(2, 1.0); // maximum resonance -> sharp peak at cutoff
            let dt = freq / sample_rate;
            let mut phase = 0.0f64;
            let mut buf = alloc::vec::Vec::new();
            for n in 0..40_000 {
                let s = 0.1 * Libm::<f64>::sin(TAU * phase);
                phase += dt;
                if phase >= 1.0 {
                    phase -= 1.0;
                }
                inputs.set(0, s);
                filter.tick(&inputs, &mut outputs);
                if n >= 20_000 {
                    buf.push(outputs.get(10).unwrap());
                }
            }
            rms(&buf)
        };

        // Uniform sweep around the target; locate the peak bin, then refine to a
        // sub-grid estimate with parabolic interpolation of the three points
        // around it (standard peak-picking; removes grid quantization bias).
        let spacing = 100.0;
        let sweep: alloc::vec::Vec<f64> = (0..13).map(|i| 1400.0 + spacing * i as f64).collect();
        let gains: alloc::vec::Vec<f64> = sweep.iter().map(|&f| gain_at(f)).collect();
        let mut peak = 1;
        for i in 1..gains.len() - 1 {
            if gains[i] > gains[peak] {
                peak = i;
            }
        }
        assert!(
            peak > 0 && peak < gains.len() - 1,
            "peak fell on sweep edge"
        );
        let (a, b, c) = (gains[peak - 1], gains[peak], gains[peak + 1]);
        let denom = a - 2.0 * b + c;
        let delta = if denom != 0.0 {
            0.5 * (a - c) / denom
        } else {
            0.0
        };
        let peak_f = sweep[peak] + delta * spacing;
        let err = (peak_f - target_fc).abs() / target_fc;
        assert!(
            err < 0.05,
            "Diode ladder resonant peak at {peak_f:.0} Hz, off from {target_fc} Hz by {:.1}%",
            err * 100.0
        );
    }

    /// Q011/Q012: maximum resonance must remain finite and bounded over a long
    /// run at several cutoffs (denormal-flushed TPT states + saturated feedback).
    #[test]
    fn test_diode_ladder_max_resonance_stable_100k() {
        for &cv in &[0.1_f64, 0.5, 0.9] {
            let mut filter = DiodeLadderFilter::new(44100.0);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(0, 5.0);
            inputs.set(1, cv);
            inputs.set(2, 1.0);

            let mut max_abs = 0.0f64;
            for n in 0..100_000 {
                filter.tick(&inputs, &mut outputs);
                for &id in &[10u32, 11, 12, 13] {
                    let v = outputs.get(id).unwrap();
                    assert!(v.is_finite(), "diode out {id} non-finite at {n} (cv={cv})");
                    max_abs = max_abs.max(v.abs());
                }
            }
            assert!(
                max_abs <= SAFE_AUDIO_LIMIT,
                "diode unbounded {max_abs} at cv={cv}"
            );
        }
    }

    /// Q109: caching the biquad coefficients must not change the output. Compare
    /// the module (cached) against a reference that recomputes the coefficients
    /// every sample; with static params the two must be bit-identical.
    #[test]
    fn test_parametric_eq_caching_bit_identical() {
        let sample_rate = 44100.0;
        let mut eq = ParametricEq::new(sample_rate);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Static (non-default) params for all three bands.
        inputs.set(1, 3.0); // low gain
        inputs.set(2, 0.4); // low freq
        inputs.set(3, -2.0); // mid gain
        inputs.set(4, 0.6); // mid freq
        inputs.set(5, 0.7); // mid q
        inputs.set(6, 4.0); // high gain
        inputs.set(7, 0.5); // high freq

        // Reference: recompute coefficients every sample (pre-Q109 behaviour).
        let low_gain_db = (3.0 / 5.0) * 12.0;
        let mid_gain_db = (-2.0 / 5.0) * 12.0;
        let high_gain_db = (4.0 / 5.0) * 12.0;
        let low_freq = (50.0 * Libm::<f64>::pow(10.0, 0.4)).clamp(20.0, sample_rate * 0.45);
        let mid_freq = (200.0 * Libm::<f64>::pow(40.0, 0.6)).clamp(20.0, sample_rate * 0.45);
        let high_freq: f64 = (2000.0 + 0.5 * 10000.0_f64).clamp(20.0, sample_rate * 0.45);
        let mid_q = 0.5 + 0.7 * 9.5;
        let low_c = ParametricEq::calc_low_shelf(low_freq, low_gain_db, sample_rate);
        let mid_c = ParametricEq::calc_peaking(mid_freq, mid_gain_db, mid_q, sample_rate);
        let high_c = ParametricEq::calc_high_shelf(high_freq, high_gain_db, sample_rate);
        let mut ref_low = [0.0; 2];
        let mut ref_mid = [0.0; 2];
        let mut ref_high = [0.0; 2];

        let mut phase = 0.0f64;
        for _ in 0..2000 {
            let s = Libm::<f64>::sin(TAU * phase);
            phase += 500.0 / sample_rate;
            if phase >= 1.0 {
                phase -= 1.0;
            }
            inputs.set(0, s);
            eq.tick(&inputs, &mut outputs);
            let got = outputs.get(10).unwrap();

            let mut r = ParametricEq::process_biquad(s, &low_c, &mut ref_low);
            r = ParametricEq::process_biquad(r, &mid_c, &mut ref_mid);
            r = ParametricEq::process_biquad(r, &high_c, &mut ref_high);

            assert_eq!(
                got.to_bits(),
                r.to_bits(),
                "cached EQ output differs from recompute"
            );
        }
    }

    /// Q109: with static params the coefficients are computed once per band and
    /// then reused; only a genuine parameter change triggers a recompute.
    #[test]
    fn test_parametric_eq_recompute_count() {
        let mut eq = ParametricEq::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 1.0);
        inputs.set(1, 2.0);
        inputs.set(3, 1.0);
        inputs.set(6, -1.0);

        for _ in 0..100 {
            eq.tick(&inputs, &mut outputs);
        }
        // Three bands, computed once on the first tick, reused thereafter.
        assert_eq!(
            eq.recompute_count, 3,
            "static params should not recompute per sample"
        );

        // Change only the mid band -> exactly one additional recompute.
        inputs.set(3, 2.0);
        eq.tick(&inputs, &mut outputs);
        assert_eq!(
            eq.recompute_count, 4,
            "changing one band should recompute only that band"
        );

        // Static again -> no further recomputes.
        for _ in 0..50 {
            eq.tick(&inputs, &mut outputs);
        }
        assert_eq!(
            eq.recompute_count, 4,
            "returning to static must not recompute"
        );
    }

    // ---- Q158: ParametricEq real frequency response (in-band vs out-of-band) ----

    #[test]
    fn test_parametric_eq_mid_band_response() {
        let sample_rate = 44100.0;
        // Mid band default CV 0.5 -> 200 * 40^0.5 Hz; drive a tone right at that
        // peaking-filter center and one far below it (out of band).
        let mid_freq = 200.0 * Libm::<f64>::pow(40.0, 0.5);
        let out_of_band = mid_freq / 8.0;

        // Steady-state RMS at `tone_hz` for a given mid-gain CV (input 3).
        let measure = |tone_hz: f64, mid_gain_cv: f64| -> f64 {
            let mut eq = ParametricEq::new(sample_rate);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(3, mid_gain_cv); // mid gain (bipolar CV, ±5V -> ±12dB)
            inputs.set(5, 1.0); // high mid-Q (narrow) so the band is well isolated
            let dt = tone_hz / sample_rate;
            let mut phase = 0.0f64;
            let mut out = alloc::vec::Vec::new();
            for n in 0..40_000 {
                let s = Libm::<f64>::sin(TAU * phase);
                phase += dt;
                if phase >= 1.0 {
                    phase -= 1.0;
                }
                inputs.set(0, s);
                eq.tick(&inputs, &mut outputs);
                if n >= 20_000 {
                    out.push(outputs.get(10).unwrap());
                }
            }
            rms(&out)
        };

        // +12 dB boost at the center: the in-band tone is amplified ~+12 dB
        // relative to the out-of-band tone (which sees the flat parts of the EQ).
        let boost_in = measure(mid_freq, 5.0);
        let boost_out = measure(out_of_band, 5.0);
        let boost_db = 20.0 * Libm::<f64>::log10(boost_in / boost_out);
        assert!(
            (9.0..=13.0).contains(&boost_db),
            "mid +12dB boost: expected ~12dB in-band, got {boost_db:.2}dB"
        );

        // -12 dB cut at the center: the in-band tone is attenuated well below
        // the out-of-band tone.
        let cut_in = measure(mid_freq, -5.0);
        let cut_out = measure(out_of_band, -5.0);
        let cut_db = 20.0 * Libm::<f64>::log10(cut_in / cut_out);
        assert!(
            (-13.0..=-9.0).contains(&cut_db),
            "mid -12dB cut: expected ~-12dB in-band, got {cut_db:.2}dB"
        );
    }
}
