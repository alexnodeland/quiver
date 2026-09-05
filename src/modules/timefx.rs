//! Delay-based and time-domain effect modules.

use super::common::{env_coef, read_interpolated, sanitize_audio, Memo};
use crate::analog::saturation;
use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use alloc::vec;
use alloc::vec::Vec;
use core::f64::consts::TAU;
use libm::Libm;

/// Give `buffer` exactly `len` zeroed samples, reusing its allocation when the
/// length is already right (the common `Patch::add` → `set_sample_rate` at an
/// unchanged rate) and reallocating only when it is not.
fn resize_cleared(buffer: &mut Vec<f64>, len: usize) {
    if buffer.len() == len {
        buffer.fill(0.0);
    } else {
        *buffer = vec![0.0; len];
    }
}

/// Unit Delay (single sample delay)
///
/// Delays a signal by one sample. Essential for feedback loops.
pub struct UnitDelay {
    buffer: f64,
    spec: PortSpec,
}

impl UnitDelay {
    pub fn new() -> Self {
        Self {
            buffer: 0.0,
            spec: PortSpec {
                inputs: vec![PortDef::new(0, "in", SignalKind::Audio)],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }
}

impl Default for UnitDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for UnitDelay {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        outputs.set(10, self.buffer);
        self.buffer = input;
    }

    fn reset(&mut self) {
        self.buffer = 0.0;
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn breaks_feedback_cycle(&self) -> bool {
        true
    }

    fn type_id(&self) -> &'static str {
        "unit_delay"
    }
}

/// Delay Line
///
/// A multi-sample delay line with feedback and wet/dry mix.
/// Supports CV-controlled delay time for effects like chorus and flanging.
///
/// The default maximum delay time is 2 seconds at any sample rate; a longer
/// buffer can be requested with [`DelayLine::with_max_delay`]. Two opt-in
/// modes cover tape-echo territory: [`DelayLine::with_unclamped_feedback`]
/// permits feedback at and past unity (self-oscillation) with tape-style
/// saturation in the recirculation path, and [`DelayLine::with_linear_time`]
/// reinterprets the `time` input as seconds directly instead of the
/// exponential CV map. [`DelayLine::tape`] combines all three.
pub struct DelayLine {
    buffer: Vec<f64>,
    write_pos: usize,
    sample_rate: f64,
    /// Maximum delay time in seconds for this instance (buffer is sized from it).
    max_delay_secs: f64,
    /// Opt-in: allow feedback past unity, with saturation in the loop.
    unclamped_feedback: bool,
    /// Opt-in: `time` input is seconds directly rather than exponential CV.
    linear_time: bool,
    /// Registry identity — `"delay_line"`, or `"tape_delay"` for [`DelayLine::tape`].
    type_id_str: &'static str,
    /// Slew-smoothed read distance, tracking the delay setpoint gradually to
    /// avoid zipper/pitch glitches when the `time` CV jumps.
    smoothed_delay: f64,
    /// One-pole retain coefficient for `smoothed_delay` (sample-rate aware).
    delay_smooth_coef: f64,
    /// Whether `smoothed_delay` has been snapped to its first setpoint yet.
    delay_primed: bool,
    /// Memoized time map `1ms · (max_ms)^cv` (one `pow` per sample while static).
    delay_ms_memo: Memo<1, f64>,
    spec: PortSpec,
}

impl DelayLine {
    /// Default maximum delay time in seconds
    const MAX_DELAY_SECS: f64 = 2.0;

    /// Feedback ceiling in unclamped mode. Past-unity growth is bounded by the
    /// in-loop saturation, so this is a sanity rail, not the safety mechanism.
    const UNCLAMPED_FEEDBACK_MAX: f64 = 1.5;

    /// Maximum delay for the [`DelayLine::tape`] preset: comfortable headroom
    /// above the 1.5–8 s tape-echo range (~4.6 MB of f64 buffer at 48 kHz).
    const TAPE_MAX_DELAY_SECS: f64 = 12.0;

    /// Time constant for delay-time smoothing (a few ms de-zippers modulation
    /// without audibly lagging deliberate delay-time changes).
    const DELAY_SMOOTH_SECS: f64 = 0.005;

    /// Longest buffer [`with_max_delay`](Self::with_max_delay) will allocate, in
    /// seconds (~46 MB of `f64` at 96 kHz). A caller-supplied maximum is clamped
    /// to `0.001..=MAX_DELAY_CAP_SECS`, and a non-finite one falls back to the
    /// 2 s default, so a bad value cannot request a multi-gigabyte buffer.
    pub const MAX_DELAY_CAP_SECS: f64 = 60.0;

    pub fn new(sample_rate: f64) -> Self {
        Self::with_max_delay(sample_rate, Self::MAX_DELAY_SECS)
    }

    /// A delay line whose buffer holds up to `max_delay_secs` of signal.
    ///
    /// `DelayLine::new` delegates here with the 2 s default, so existing
    /// patches are unaffected. The exponential time map spans
    /// `1 ms..max_delay_secs` for whatever maximum is chosen. `max_delay_secs`
    /// is bounded to `0.001..=`[`MAX_DELAY_CAP_SECS`](Self::MAX_DELAY_CAP_SECS)
    /// (non-finite → the 2 s default).
    pub fn with_max_delay(sample_rate: f64, max_delay_secs: f64) -> Self {
        let max_delay_secs = if max_delay_secs.is_finite() {
            max_delay_secs.clamp(0.001, Self::MAX_DELAY_CAP_SECS)
        } else {
            Self::MAX_DELAY_SECS
        };
        let buffer_size = (sample_rate * max_delay_secs) as usize + 1;
        Self {
            buffer: vec![0.0; buffer_size],
            write_pos: 0,
            sample_rate,
            max_delay_secs,
            unclamped_feedback: false,
            linear_time: false,
            type_id_str: "delay_line",
            smoothed_delay: 0.0,
            delay_smooth_coef: env_coef(Self::DELAY_SMOOTH_SECS, sample_rate),
            delay_primed: false,
            delay_ms_memo: Memo::new(0.0),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "time", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "feedback", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(3, "mix", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }

    /// Opt in to feedback at and past unity (up to 1.5), with tape-style
    /// saturation applied to the recirculated sample so past-unity feedback
    /// compresses into mud instead of growing without bound. Hardware delays
    /// self-oscillate; with this flag, so does this one. Non-finite input is
    /// already sanitised before it can enter the buffer, so a NaN cannot latch.
    pub fn with_unclamped_feedback(mut self) -> Self {
        self.unclamped_feedback = true;
        self
    }

    /// Opt in to a linear time input: the `time` port takes **seconds**
    /// directly (clamped to `0..max_delay_secs`) instead of the exponential
    /// `1 ms · (max_ms)^cv` map. The 5 ms read-distance slew still applies, so
    /// delay-time changes glide in pitch exactly as in the default mode.
    pub fn with_linear_time(mut self) -> Self {
        self.linear_time = true;
        self
    }

    /// Tape-echo preset: 12 s maximum delay, linear-seconds time input, and
    /// unclamped feedback with saturation in the loop. Registered in the
    /// module registry as `"tape_delay"`.
    pub fn tape(sample_rate: f64) -> Self {
        let mut tape = Self::with_max_delay(sample_rate, Self::TAPE_MAX_DELAY_SECS)
            .with_unclamped_feedback()
            .with_linear_time();
        tape.type_id_str = "tape_delay";
        tape
    }
}

impl Default for DelayLine {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for DelayLine {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize so a non-finite input can never enter the feedback
        // buffer (where it would recirculate forever, latching NaN).
        let input = sanitize_audio(inputs.get_or(0, 0.0));
        let feedback_ceiling = if self.unclamped_feedback {
            Self::UNCLAMPED_FEEDBACK_MAX // Runaway is bounded by in-loop saturation
        } else {
            0.99 // Prevent runaway
        };
        let feedback = inputs.get_or(2, 0.0).clamp(0.0, feedback_ceiling);
        let mix = inputs.get_or(3, 0.5).clamp(0.0, 1.0);

        let delay_ms = if self.linear_time {
            // Linear mode: the `time` input is seconds, straight through.
            inputs.get_or(1, 0.5).clamp(0.0, self.max_delay_secs) * 1000.0
        } else {
            // Map time CV (0-1) to delay time (1ms to max delay, exponential),
            // memoized on the time CV (bit-exact miss path).
            let time_cv = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
            let max_delay_ms = self.max_delay_secs * 1000.0;
            self.delay_ms_memo.get_or_compute([time_cv], || {
                let min_delay_ms = 1.0;
                min_delay_ms * Libm::<f64>::pow(max_delay_ms / min_delay_ms, time_cv)
            })
        };
        let target_delay =
            (delay_ms * self.sample_rate / 1000.0).clamp(1.0, (self.buffer.len() - 1) as f64);

        // Slew-limit the read distance toward its setpoint with a one-pole
        // smoother so a step in `time` glides instead of jumping (no clicks).
        // Snap on the first tick so startup does not sweep up from zero.
        if self.delay_primed {
            self.smoothed_delay =
                target_delay + (self.smoothed_delay - target_delay) * self.delay_smooth_coef;
        } else {
            self.smoothed_delay = target_delay;
            self.delay_primed = true;
        }
        let delay_samples = self.smoothed_delay;

        // Read from delay line
        let delayed = read_interpolated(&self.buffer, self.write_pos, delay_samples);

        // Write input + feedback to buffer. In unclamped mode the recirculated
        // sample passes through tape-style saturation (unity gain at the
        // origin) so past-unity feedback compresses instead of detonating.
        let recirculated = input + delayed * feedback;
        self.buffer[self.write_pos] = if self.unclamped_feedback {
            saturation::tanh_sat(recirculated / 5.0, 1.0) * 5.0
        } else {
            recirculated
        };

        // Advance write position
        self.write_pos = (self.write_pos + 1) % self.buffer.len();

        // Mix dry and wet signals
        let output = input * (1.0 - mix) + delayed * mix;
        outputs.set(10, output);
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
        self.smoothed_delay = 0.0;
        self.delay_primed = false;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let buffer_size = (sample_rate * self.max_delay_secs) as usize + 1;
        // `Patch::add` calls this right after construction, usually at the same
        // rate: keep the (possibly multi-megabyte tape) buffer instead of
        // allocating it twice; the state is cleared either way.
        resize_cleared(&mut self.buffer, buffer_size);
        self.write_pos = 0;
        self.smoothed_delay = 0.0;
        self.delay_smooth_coef = env_coef(Self::DELAY_SMOOTH_SECS, sample_rate);
        self.delay_primed = false;
    }

    fn breaks_feedback_cycle(&self) -> bool {
        true
    }

    fn type_id(&self) -> &'static str {
        self.type_id_str
    }
}

/// Chorus Effect
///
/// Classic chorus effect using multiple modulated delay lines.
/// Creates a rich, shimmering sound by mixing slightly detuned copies
/// of the input signal.
pub struct Chorus {
    /// Three delay lines for rich chorus
    delay_buffers: [Vec<f64>; 3],
    write_pos: usize,
    /// LFO phases for each voice
    lfo_phases: [f64; 3],
    sample_rate: f64,
    /// Memoized rate map `0.1 · 50^cv` (one `pow` per sample while static).
    rate_memo: Memo<1, f64>,
    spec: PortSpec,
}

impl Chorus {
    /// Maximum modulation delay in milliseconds
    const MAX_MOD_DELAY_MS: f64 = 25.0;
    /// Base delay in milliseconds
    const BASE_DELAY_MS: f64 = 7.0;

    /// Modulated delay (in samples) for one chorus voice.
    ///
    /// The LFO term is made **unipolar** (`sin*0.5 + 0.5`, range `0..=1`) so the
    /// delay stays within `[base, base + mod_depth]` and is always positive.
    /// A bipolar sweep (`base + sin*mod_depth`) goes negative whenever
    /// `mod_depth > base` — which is true at the stock `depth_cv = 0.5`
    /// (`base = 7 ms`, `mod_depth = 12.5 ms`), where it clamps the trough of the
    /// sweep to 1 sample and one-sidedly distorts the chorus.
    #[inline]
    fn voice_delay_samples(base_delay_samples: f64, mod_depth_samples: f64, lfo_val: f64) -> f64 {
        base_delay_samples + (lfo_val * 0.5 + 0.5) * mod_depth_samples
    }

    pub fn new(sample_rate: f64) -> Self {
        let buffer_size =
            ((Self::MAX_MOD_DELAY_MS + Self::BASE_DELAY_MS) * sample_rate / 1000.0) as usize + 10;
        Self {
            delay_buffers: [
                vec![0.0; buffer_size],
                vec![0.0; buffer_size],
                vec![0.0; buffer_size],
            ],
            write_pos: 0,
            // Offset phases for each voice to create movement
            lfo_phases: [0.0, 0.33, 0.67],
            sample_rate,
            rate_memo: Memo::new(0.0),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "rate", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(2, "depth", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(3, "mix", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "left", SignalKind::Audio),
                    PortDef::new(12, "right", SignalKind::Audio),
                ],
            },
        }
    }
}

impl Default for Chorus {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Chorus {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize so a non-finite input can never enter the modulated
        // delay buffer.
        let input = sanitize_audio(inputs.get_or(0, 0.0));
        let rate_cv = inputs.get_or(1, 0.3).clamp(0.0, 1.0);
        let depth_cv = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let mix = inputs.get_or(3, 0.5).clamp(0.0, 1.0);

        // Map rate CV to LFO frequency (0.1 Hz to 5 Hz), memoized on the rate
        // CV (bit-exact miss path).
        let lfo_freq = self
            .rate_memo
            .get_or_compute([rate_cv], || 0.1 * Libm::<f64>::pow(50.0, rate_cv));

        // Map depth CV to modulation depth in ms
        let mod_depth_ms = depth_cv * Self::MAX_MOD_DELAY_MS;

        let base_delay_samples = Self::BASE_DELAY_MS * self.sample_rate / 1000.0;
        let mod_depth_samples = mod_depth_ms * self.sample_rate / 1000.0;

        let mut wet_sum = 0.0;
        let mut left_sum = 0.0;
        let mut right_sum = 0.0;

        for i in 0..3 {
            // Calculate modulated delay for this voice
            let lfo_val = Libm::<f64>::sin(self.lfo_phases[i] * core::f64::consts::TAU);
            let delay_samples =
                Self::voice_delay_samples(base_delay_samples, mod_depth_samples, lfo_val)
                    .clamp(1.0, (self.delay_buffers[i].len() - 1) as f64);

            // Read from this voice's delay line
            let delayed = read_interpolated(&self.delay_buffers[i], self.write_pos, delay_samples);

            wet_sum += delayed;

            // Stereo spread: voice 0 center, voice 1 left, voice 2 right
            match i {
                0 => {
                    left_sum += delayed * 0.5;
                    right_sum += delayed * 0.5;
                }
                1 => left_sum += delayed,
                2 => right_sum += delayed,
                _ => {}
            }

            // Write input to this voice's delay buffer
            self.delay_buffers[i][self.write_pos] = input;

            // Advance LFO phase with slight detuning between voices
            let freq_mult = 1.0 + (i as f64 - 1.0) * 0.1; // Slight frequency offset
            let phase_inc = lfo_freq * freq_mult / self.sample_rate;
            self.lfo_phases[i] += phase_inc;
            if self.lfo_phases[i] >= 1.0 {
                self.lfo_phases[i] -= 1.0;
            }
        }

        // Normalize wet signal (3 voices)
        wet_sum /= 3.0;
        left_sum /= 2.0;
        right_sum /= 2.0;

        // Advance write position
        self.write_pos = (self.write_pos + 1) % self.delay_buffers[0].len();

        // Mix dry and wet
        let mono_out = input * (1.0 - mix) + wet_sum * mix;
        let left_out = input * (1.0 - mix) + left_sum * mix;
        let right_out = input * (1.0 - mix) + right_sum * mix;

        outputs.set(10, mono_out);
        outputs.set(11, left_out);
        outputs.set(12, right_out);
    }

    fn reset(&mut self) {
        for buffer in &mut self.delay_buffers {
            buffer.fill(0.0);
        }
        self.write_pos = 0;
        self.lfo_phases = [0.0, 0.33, 0.67];
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let buffer_size =
            ((Self::MAX_MOD_DELAY_MS + Self::BASE_DELAY_MS) * sample_rate / 1000.0) as usize + 10;
        for buffer in &mut self.delay_buffers {
            resize_cleared(buffer, buffer_size);
        }
        self.write_pos = 0;
    }

    fn type_id(&self) -> &'static str {
        "chorus"
    }
}

/// Flanger
///
/// Classic flanging effect using a short modulated delay with feedback.
///
/// Mono-in, stereo-out: the two delay lines share one LFO but read it at a
/// per-channel phase offset controlled by the `spread` input, decorrelating the
/// left and right sweeps. The legacy `out` port reproduces the historical mono
/// channel exactly and is bit-identical to `left`, so existing patches keep
/// working; connect `left`/`right` for the stereo image.
pub struct Flanger {
    /// Dual delay lines, indexed `[left, right]`.
    buffers: [Vec<f64>; 2],
    write_pos: usize,
    lfo_phase: f64,
    sample_rate: f64,
    /// Memoized rate map `0.05 · 100^cv` (one `pow` per sample while static).
    rate_memo: Memo<1, f64>,
    spec: PortSpec,
}

impl Flanger {
    const MAX_DELAY_MS: f64 = 10.0;

    pub fn new(sample_rate: f64) -> Self {
        let buffer_size = (sample_rate * Self::MAX_DELAY_MS / 1000.0) as usize + 10;
        Self {
            buffers: [vec![0.0; buffer_size], vec![0.0; buffer_size]],
            write_pos: 0,
            lfo_phase: 0.0,
            sample_rate,
            rate_memo: Memo::new(0.0),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "rate", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(2, "depth", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(3, "feedback", SignalKind::CvBipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(4, "mix", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    // Stereo spread: 0 collapses to mono (L==R==out), 1 offsets
                    // the right sweep by 180 degrees for maximum decorrelation.
                    PortDef::new(5, "spread", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "left", SignalKind::Audio),
                    PortDef::new(12, "right", SignalKind::Audio),
                ],
            },
        }
    }
}

impl Default for Flanger {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Flanger {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize so a non-finite input can never enter the feedback
        // delay buffer.
        let input = sanitize_audio(inputs.get_or(0, 0.0));
        let rate_cv = inputs.get_or(1, 0.3).clamp(0.0, 1.0);
        let depth_cv = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let feedback = inputs.get_or(3, 0.0).clamp(-0.95, 0.95);
        let mix = inputs.get_or(4, 0.5).clamp(0.0, 1.0);
        let spread = inputs.get_or(5, 0.5).clamp(0.0, 1.0);

        // Rate map memoized on the rate CV (bit-exact miss path).
        let lfo_freq = self
            .rate_memo
            .get_or_compute([rate_cv], || 0.05 * Libm::<f64>::pow(100.0, rate_cv));
        let base_delay_ms = 1.0;
        let mod_depth_ms = depth_cv * (Self::MAX_DELAY_MS - base_delay_ms);

        // Per-channel LFO phase offset: spread 0..1 maps to 0..0.5 cycles
        // (0..180 degrees). The left channel tracks the base phase (so `out`
        // stays bit-identical to the historical mono behavior); the right
        // channel leads by the offset to decorrelate the two sweeps.
        let phase_offset = spread * 0.5;
        let max_read = (self.buffers[0].len() - 1) as f64;

        let mut wet = [0.0; 2];
        for (ch, w) in wet.iter_mut().enumerate() {
            let phase = self.lfo_phase + if ch == 0 { 0.0 } else { phase_offset };
            let lfo = (Libm::<f64>::sin(phase * TAU) + 1.0) * 0.5;
            let delay_ms = base_delay_ms + lfo * mod_depth_ms;
            let delay_samples = (delay_ms * self.sample_rate / 1000.0).clamp(1.0, max_read);
            let delayed = read_interpolated(&self.buffers[ch], self.write_pos, delay_samples);
            // Per-channel feedback tap keeps the two lines independent.
            self.buffers[ch][self.write_pos] = input + delayed * feedback;
            *w = delayed;
        }

        self.lfo_phase += lfo_freq / self.sample_rate;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        self.write_pos = (self.write_pos + 1) % self.buffers[0].len();

        let left = input * (1.0 - mix) + wet[0] * mix;
        let right = input * (1.0 - mix) + wet[1] * mix;
        // `out` mirrors `left` for backward compatibility with mono patches.
        outputs.set(10, left);
        outputs.set(11, left);
        outputs.set(12, right);
    }

    fn reset(&mut self) {
        for buffer in &mut self.buffers {
            buffer.fill(0.0);
        }
        self.write_pos = 0;
        self.lfo_phase = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let buffer_size = (sample_rate * Self::MAX_DELAY_MS / 1000.0) as usize + 10;
        for buffer in &mut self.buffers {
            resize_cleared(buffer, buffer_size);
        }
        self.write_pos = 0;
    }

    fn type_id(&self) -> &'static str {
        "flanger"
    }
}

/// Phaser
///
/// Classic phaser effect using cascaded all-pass filters.
///
/// Mono-in, stereo-out: two independent allpass chains share one LFO but read
/// it at a per-channel phase offset controlled by the `spread` input, giving
/// decorrelated left/right notch sweeps and per-channel feedback taps. The
/// legacy `out` port reproduces the historical mono channel exactly and is
/// bit-identical to `left`, so existing patches keep working.
pub struct Phaser {
    /// Previous input per allpass stage (`x[n-1]`), indexed `[channel][stage]`.
    allpass_x1: [[f64; 6]; 2],
    /// Previous output per allpass stage (`y[n-1]`), indexed `[channel][stage]`.
    allpass_y1: [[f64; 6]; 2],
    lfo_phase: f64,
    sample_rate: f64,
    /// Memoized rate map `0.05 · 100^cv` (one `pow` per sample while static).
    rate_memo: Memo<1, f64>,
    /// Memoized allpass coefficient `(1-tan(ω/2))/(1+tan(ω/2))`, keyed on the
    /// swept center frequency. It hits whenever `depth` is zero (the sweep
    /// freezes); with an active sweep it misses per sample, costing only the
    /// key compare on top of the original math.
    coef_memo: Memo<2, f64>,
    spec: PortSpec,
}

impl Phaser {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            allpass_x1: [[0.0; 6]; 2],
            allpass_y1: [[0.0; 6]; 2],
            lfo_phase: 0.0,
            sample_rate,
            rate_memo: Memo::new(0.0),
            coef_memo: Memo::new(0.0),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "rate", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(2, "depth", SignalKind::CvUnipolar)
                        .with_default(0.7)
                        .with_attenuverter(),
                    PortDef::new(3, "feedback", SignalKind::CvBipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(4, "mix", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(5, "stages", SignalKind::CvUnipolar).with_default(1.0),
                    // Stereo spread: 0 collapses to mono (L==R==out), 1 offsets
                    // the right sweep by 180 degrees for maximum decorrelation.
                    PortDef::new(6, "spread", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "left", SignalKind::Audio),
                    PortDef::new(12, "right", SignalKind::Audio),
                ],
            },
        }
    }

    /// First-order allpass section with a truly flat magnitude response.
    ///
    /// Implements `H(z) = (coef + z^-1) / (1 + coef z^-1)`, i.e.
    /// `y[n] = coef*x[n] + x[n-1] - coef*y[n-1]`. Unit magnitude holds at every
    /// frequency (DC gain `+1`, Nyquist gain `-1`) for all `|coef| < 1`, so
    /// cascading these produces the phase-only notches a phaser needs rather
    /// than the moving-lowpass coloration of the previous (non-allpass)
    /// topology.
    fn allpass(input: f64, x1: &mut f64, y1: &mut f64, coef: f64) -> f64 {
        let output = coef * input + *x1 - coef * *y1;
        *x1 = input;
        *y1 = output;
        output
    }
}

impl Default for Phaser {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Phaser {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize so a non-finite input can never enter the all-pass
        // feedback chain.
        let input = sanitize_audio(inputs.get_or(0, 0.0));
        let rate_cv = inputs.get_or(1, 0.3).clamp(0.0, 1.0);
        let depth = inputs.get_or(2, 0.7).clamp(0.0, 1.0);
        let feedback = inputs.get_or(3, 0.0).clamp(-0.95, 0.95);
        let mix = inputs.get_or(4, 0.5).clamp(0.0, 1.0);
        let stages_cv = inputs.get_or(5, 1.0).clamp(0.0, 1.0);

        let num_stages = if stages_cv < 0.33 {
            2
        } else if stages_cv < 0.66 {
            4
        } else {
            6
        };

        let spread = inputs.get_or(6, 0.5).clamp(0.0, 1.0);

        // Rate map memoized on the rate CV (bit-exact miss path).
        let lfo_freq = self
            .rate_memo
            .get_or_compute([rate_cv], || 0.05 * Libm::<f64>::pow(100.0, rate_cv));

        let min_freq = 200.0;
        let max_freq = 4000.0;

        // Per-channel LFO phase offset: spread 0..1 maps to 0..0.5 cycles
        // (0..180 degrees). The left channel tracks the base phase (so `out`
        // stays bit-identical to the historical mono behavior); the right
        // channel leads by the offset to decorrelate the notch sweeps.
        let phase_offset = spread * 0.5;

        let mut wet = [0.0; 2];
        for (ch, w) in wet.iter_mut().enumerate() {
            let phase = self.lfo_phase + if ch == 0 { 0.0 } else { phase_offset };
            let lfo = Libm::<f64>::sin(phase * TAU);
            let freq = min_freq + (lfo * 0.5 + 0.5) * depth * (max_freq - min_freq);

            // Allpass coefficient memoized on the swept frequency (bit-exact
            // miss path). Shared across channels: at zero depth both sweeps
            // freeze on the same frequency and the second channel hits.
            let sample_rate = self.sample_rate;
            let coef = self.coef_memo.get_or_compute([freq, sample_rate], || {
                let omega = TAU * freq / sample_rate;
                let tan_w = Libm::<f64>::tan(omega * 0.5);
                (1.0 - tan_w) / (1.0 + tan_w)
            });

            // Per-channel feedback tap from this chain's last stage.
            let mut signal = input + self.allpass_y1[ch][num_stages - 1] * feedback;
            for i in 0..num_stages {
                signal = Self::allpass(
                    signal,
                    &mut self.allpass_x1[ch][i],
                    &mut self.allpass_y1[ch][i],
                    coef,
                );
            }
            *w = signal;
        }

        self.lfo_phase += lfo_freq / self.sample_rate;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }

        let left = input * (1.0 - mix) + wet[0] * mix;
        let right = input * (1.0 - mix) + wet[1] * mix;
        // `out` mirrors `left` for backward compatibility with mono patches.
        outputs.set(10, left);
        outputs.set(11, left);
        outputs.set(12, right);
    }

    fn reset(&mut self) {
        self.allpass_x1 = [[0.0; 6]; 2];
        self.allpass_y1 = [[0.0; 6]; 2];
        self.lfo_phase = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "phaser"
    }
}

// ============================================================================
// P3 Effects: Tremolo, Vibrato, Distortion
// ============================================================================

/// Tremolo
///
/// Amplitude modulation effect with adjustable rate, depth, and waveform.
/// Creates classic "wobbly" volume effect.
pub struct Tremolo {
    lfo_phase: f64,
    sample_rate: f64,
    /// Memoized rate map `0.1 · 200^cv` (one `pow` per sample while static).
    rate_memo: Memo<1, f64>,
    spec: PortSpec,
}

impl Tremolo {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            lfo_phase: 0.0,
            sample_rate,
            rate_memo: Memo::new(0.0),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "rate", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(2, "depth", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(3, "shape", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }
}

impl Default for Tremolo {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Tremolo {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let rate_cv = inputs.get_or(1, 0.3).clamp(0.0, 1.0);
        let depth = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let shape = inputs.get_or(3, 0.0).clamp(0.0, 1.0);

        // Rate: 0.1Hz to 20Hz (exponential), memoized on the rate CV
        // (bit-exact miss path).
        let lfo_freq = self
            .rate_memo
            .get_or_compute([rate_cv], || 0.1 * Libm::<f64>::pow(200.0, rate_cv));

        // Generate LFO: blend between sine and triangle based on shape
        let phase_rad = self.lfo_phase * TAU;
        let sine = Libm::<f64>::sin(phase_rad);
        let triangle = 1.0 - 4.0 * Libm::<f64>::fabs(self.lfo_phase - 0.5);
        let lfo = sine * (1.0 - shape) + triangle * shape;

        // Advance phase
        self.lfo_phase += lfo_freq / self.sample_rate;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }

        // Apply amplitude modulation
        // LFO ranges -1 to 1, convert to modulation amount
        let modulation = 1.0 - depth * 0.5 * (1.0 - lfo);
        outputs.set(10, input * modulation);
    }

    fn reset(&mut self) {
        self.lfo_phase = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "tremolo"
    }
}

/// Vibrato
///
/// Pitch modulation effect using a modulated delay line.
/// Creates classic pitch wobble effect.
pub struct Vibrato {
    buffer: Vec<f64>,
    write_pos: usize,
    lfo_phase: f64,
    sample_rate: f64,
    /// Memoized rate map `0.1 · 150^cv` (one `pow` per sample while static).
    rate_memo: Memo<1, f64>,
    spec: PortSpec,
}

impl Vibrato {
    const MAX_DELAY_MS: f64 = 20.0;

    pub fn new(sample_rate: f64) -> Self {
        let buffer_size = (sample_rate * Self::MAX_DELAY_MS / 1000.0) as usize + 10;
        Self {
            buffer: vec![0.0; buffer_size],
            write_pos: 0,
            lfo_phase: 0.0,
            sample_rate,
            rate_memo: Memo::new(0.0),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "rate", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(2, "depth", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(3, "mix", SignalKind::CvUnipolar)
                        .with_default(1.0)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }
}

impl Default for Vibrato {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Vibrato {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let rate_cv = inputs.get_or(1, 0.3).clamp(0.0, 1.0);
        let depth = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let mix = inputs.get_or(3, 1.0).clamp(0.0, 1.0);

        // Rate: 0.1Hz to 15Hz (exponential), memoized on the rate CV
        // (bit-exact miss path).
        let lfo_freq = self
            .rate_memo
            .get_or_compute([rate_cv], || 0.1 * Libm::<f64>::pow(150.0, rate_cv));

        // Base delay at center of modulation range
        let base_delay_ms = Self::MAX_DELAY_MS * 0.5;
        let mod_depth_ms = depth * base_delay_ms * 0.9;

        // Sinusoidal LFO
        let lfo = Libm::<f64>::sin(self.lfo_phase * TAU);
        self.lfo_phase += lfo_freq / self.sample_rate;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }

        // Calculate modulated delay
        let delay_ms = base_delay_ms + lfo * mod_depth_ms;
        let delay_samples =
            (delay_ms * self.sample_rate / 1000.0).clamp(1.0, (self.buffer.len() - 1) as f64);

        // Read before writing (matching DelayLine/Flanger/Chorus) so the
        // minimum effective delay is `delay_samples`, not one sample shorter.
        let delayed = read_interpolated(&self.buffer, self.write_pos, delay_samples);

        // Write to buffer and advance
        self.buffer[self.write_pos] = input;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();

        outputs.set(10, input * (1.0 - mix) + delayed * mix);
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
        self.lfo_phase = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let buffer_size = (sample_rate * Self::MAX_DELAY_MS / 1000.0) as usize + 10;
        // Reset the buffer and write cursor: `write_pos` is a direct (non-modulo)
        // index at the write site, so a stale value left over from a larger
        // buffer would index out of bounds after lowering the sample rate
        // shrinks the buffer. Matches Chorus/Flanger/DelayLine.
        self.buffer = vec![0.0; buffer_size];
        self.write_pos = 0;
        self.lfo_phase = 0.0;
    }

    fn type_id(&self) -> &'static str {
        "vibrato"
    }
}

/// Freeverb-style comb filter tunings at 44.1kHz
const COMB_TUNINGS_44100: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];

/// Freeverb-style all-pass filter tunings at 44.1kHz
const ALLPASS_TUNINGS_44100: [usize; 4] = [556, 441, 341, 225];

/// Stereo spread (samples offset for right channel)
const STEREO_SPREAD: usize = 23;

/// Maximum buffer size for comb filters (accommodates up to 96kHz)
const MAX_COMB_SIZE: usize = 4096;

/// Maximum buffer size for all-pass filters
const MAX_ALLPASS_SIZE: usize = 1500;

/// Maximum pre-delay buffer (100ms at 96kHz)
const MAX_PREDELAY_SIZE: usize = 9600;

/// Algorithmic reverb using Freeverb architecture
///
/// Features 8 parallel comb filters with damping, followed by
/// 4 series all-pass filters for diffusion. Produces stereo output.
///
/// # Ports
/// - Input 0: Audio input
/// - Input 1: Room size (0-1, default 0.5)
/// - Input 2: Damping (0-1, default 0.5)
/// - Input 3: Wet/dry mix (0-1, default 0.5)
/// - Input 4: Pre-delay time (0-1, maps to 0-100ms)
/// - Output 10: Left channel
/// - Output 11: Right channel
pub struct Reverb {
    // Comb filters (8 left, 8 right) - heap allocated due to size
    comb_buffers_l: Vec<Vec<f64>>,
    comb_buffers_r: Vec<Vec<f64>>,
    comb_pos_l: [usize; 8],
    comb_pos_r: [usize; 8],
    comb_filter_state_l: [f64; 8], // Lowpass state for damping
    comb_filter_state_r: [f64; 8],

    // All-pass filters (4 left, 4 right)
    allpass_buffers_l: Vec<Vec<f64>>,
    allpass_buffers_r: Vec<Vec<f64>>,
    allpass_pos_l: [usize; 4],
    allpass_pos_r: [usize; 4],

    // Pre-delay
    predelay_buffer: Vec<f64>,
    predelay_pos: usize,

    // Current tunings (scaled for sample rate)
    comb_lengths: [usize; 8],
    allpass_lengths: [usize; 4],
    /// Right-channel decorrelation offset, scaled with sample rate.
    stereo_spread: usize,

    sample_rate: f64,
    spec: PortSpec,
}

impl Reverb {
    /// Create a new reverb with the given sample rate
    pub fn new(sample_rate: f64) -> Self {
        let mut reverb = Self {
            comb_buffers_l: (0..8).map(|_| vec![0.0; MAX_COMB_SIZE]).collect(),
            comb_buffers_r: (0..8).map(|_| vec![0.0; MAX_COMB_SIZE]).collect(),
            comb_pos_l: [0; 8],
            comb_pos_r: [0; 8],
            comb_filter_state_l: [0.0; 8],
            comb_filter_state_r: [0.0; 8],

            allpass_buffers_l: (0..4).map(|_| vec![0.0; MAX_ALLPASS_SIZE]).collect(),
            allpass_buffers_r: (0..4).map(|_| vec![0.0; MAX_ALLPASS_SIZE]).collect(),
            allpass_pos_l: [0; 4],
            allpass_pos_r: [0; 4],

            predelay_buffer: vec![0.0; MAX_PREDELAY_SIZE],
            predelay_pos: 0,

            comb_lengths: [0; 8],
            allpass_lengths: [0; 4],
            stereo_spread: STEREO_SPREAD,

            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "size", SignalKind::CvUnipolar).with_default(0.5),
                    PortDef::new(2, "damping", SignalKind::CvUnipolar).with_default(0.5),
                    PortDef::new(3, "mix", SignalKind::CvUnipolar).with_default(0.5),
                    PortDef::new(4, "predelay", SignalKind::CvUnipolar).with_default(0.0),
                ],
                outputs: vec![
                    PortDef::new(10, "left", SignalKind::Audio),
                    PortDef::new(11, "right", SignalKind::Audio),
                ],
            },
        };
        reverb.update_tunings();
        reverb
    }

    /// Update filter tunings based on sample rate
    fn update_tunings(&mut self) {
        let ratio = self.sample_rate / 44100.0;

        for (i, &base) in COMB_TUNINGS_44100.iter().enumerate() {
            self.comb_lengths[i] = ((base as f64 * ratio) as usize).min(MAX_COMB_SIZE - 1);
        }

        for (i, &base) in ALLPASS_TUNINGS_44100.iter().enumerate() {
            self.allpass_lengths[i] = ((base as f64 * ratio) as usize).min(MAX_ALLPASS_SIZE - 1);
        }

        // Scale the stereo decorrelation offset with sample rate too, so the
        // right channel stays as decorrelated at 96 kHz as it is at 44.1 kHz
        // (a raw 23-sample offset would shrink relative to the tunings).
        self.stereo_spread = (Libm::<f64>::round(STEREO_SPREAD as f64 * ratio) as usize).max(1);
    }

    /// Process a single comb filter with damping
    #[inline]
    fn process_comb(
        buffer: &mut [f64],
        pos: &mut usize,
        filter_state: &mut f64,
        input: f64,
        length: usize,
        feedback: f64,
        damping: f64,
    ) -> f64 {
        let output = buffer[*pos];

        // Damping lowpass filter
        *filter_state = output * (1.0 - damping) + *filter_state * damping;

        // Write input + filtered feedback
        buffer[*pos] = input + *filter_state * feedback;

        *pos += 1;
        if *pos >= length {
            *pos = 0;
        }

        output
    }

    /// Process a single all-pass filter
    #[inline]
    fn process_allpass(buffer: &mut [f64], pos: &mut usize, input: f64, length: usize) -> f64 {
        const ALLPASS_FEEDBACK: f64 = 0.5;

        let buffered = buffer[*pos];
        let output = -input + buffered;

        buffer[*pos] = input + buffered * ALLPASS_FEEDBACK;

        *pos += 1;
        if *pos >= length {
            *pos = 0;
        }

        output
    }
}

impl Default for Reverb {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Reverb {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize so a non-finite input can never enter the comb/allpass
        // feedback network (where it would latch NaN across the whole tail).
        let input = sanitize_audio(inputs.get_or(0, 0.0));
        let size = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
        let damping = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let mix = inputs.get_or(3, 0.5).clamp(0.0, 1.0);
        let predelay_cv = inputs.get_or(4, 0.0).clamp(0.0, 1.0);

        // Freeverb scaling
        let room_scale = 0.28 + size * 0.7;
        let damp = damping * 0.4;

        // Pre-delay (0-100ms)
        let predelay_samples =
            (predelay_cv * 0.1 * self.sample_rate).min(MAX_PREDELAY_SIZE as f64 - 1.0) as usize;

        // Write to pre-delay buffer
        self.predelay_buffer[self.predelay_pos] = input;
        let predelay_read_pos = if self.predelay_pos >= predelay_samples {
            self.predelay_pos - predelay_samples
        } else {
            MAX_PREDELAY_SIZE - (predelay_samples - self.predelay_pos)
        };
        let predelayed = if predelay_samples > 0 {
            self.predelay_buffer[predelay_read_pos]
        } else {
            input
        };
        self.predelay_pos = (self.predelay_pos + 1) % MAX_PREDELAY_SIZE;

        // Process 8 parallel comb filters (accumulate for left and right)
        let mut comb_out_l = 0.0;
        let mut comb_out_r = 0.0;

        for i in 0..8 {
            // Left channel
            let length_l = self.comb_lengths[i];
            comb_out_l += Self::process_comb(
                &mut self.comb_buffers_l[i],
                &mut self.comb_pos_l[i],
                &mut self.comb_filter_state_l[i],
                predelayed,
                length_l,
                room_scale,
                damp,
            );

            // Right channel (with stereo spread offset for decorrelation)
            let length_r = (self.comb_lengths[i] + self.stereo_spread).min(MAX_COMB_SIZE - 1);
            comb_out_r += Self::process_comb(
                &mut self.comb_buffers_r[i],
                &mut self.comb_pos_r[i],
                &mut self.comb_filter_state_r[i],
                predelayed,
                length_r,
                room_scale,
                damp,
            );
        }

        // Scale comb output
        comb_out_l *= 0.125;
        comb_out_r *= 0.125;

        // Process 4 series all-pass filters
        let mut allpass_out_l = comb_out_l;
        let mut allpass_out_r = comb_out_r;

        for i in 0..4 {
            let length_l = self.allpass_lengths[i];
            allpass_out_l = Self::process_allpass(
                &mut self.allpass_buffers_l[i],
                &mut self.allpass_pos_l[i],
                allpass_out_l,
                length_l,
            );

            let length_r = (self.allpass_lengths[i] + self.stereo_spread).min(MAX_ALLPASS_SIZE - 1);
            allpass_out_r = Self::process_allpass(
                &mut self.allpass_buffers_r[i],
                &mut self.allpass_pos_r[i],
                allpass_out_r,
                length_r,
            );
        }

        // Wet/dry mix
        let left = input * (1.0 - mix) + allpass_out_l * mix;
        let right = input * (1.0 - mix) + allpass_out_r * mix;

        outputs.set(10, left);
        outputs.set(11, right);
    }

    fn reset(&mut self) {
        for buf in &mut self.comb_buffers_l {
            buf.iter_mut().for_each(|x| *x = 0.0);
        }
        for buf in &mut self.comb_buffers_r {
            buf.iter_mut().for_each(|x| *x = 0.0);
        }
        self.comb_pos_l = [0; 8];
        self.comb_pos_r = [0; 8];
        self.comb_filter_state_l = [0.0; 8];
        self.comb_filter_state_r = [0.0; 8];

        for buf in &mut self.allpass_buffers_l {
            buf.iter_mut().for_each(|x| *x = 0.0);
        }
        for buf in &mut self.allpass_buffers_r {
            buf.iter_mut().for_each(|x| *x = 0.0);
        }
        self.allpass_pos_l = [0; 4];
        self.allpass_pos_r = [0; 4];

        self.predelay_buffer.iter_mut().for_each(|x| *x = 0.0);
        self.predelay_pos = 0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.update_tunings();
        self.reset();
    }

    fn type_id(&self) -> &'static str {
        "reverb"
    }
}

// =============================================================================
// Vocoder - Spectral Vocoding Effect
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_delay() {
        let mut delay = UnitDelay::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // First sample
        inputs.set(0, 1.0);
        delay.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 0.0).abs() < 0.01); // Should be initial value

        // Second sample
        inputs.set(0, 2.0);
        delay.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 1.0).abs() < 0.01); // Should be previous input
    }
    #[test]
    fn test_delay_line() {
        let mut delay = DelayLine::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set delay time to minimum and mix to wet only
        inputs.set(1, 0.0); // Minimum time
        inputs.set(2, 0.0); // No feedback
        inputs.set(3, 1.0); // 100% wet

        // Feed an impulse
        inputs.set(0, 1.0);
        delay.tick(&inputs, &mut outputs);

        // First output should be from empty buffer (near zero)
        let first_out = outputs.get(10).unwrap();
        assert!(first_out.abs() < 0.1);

        // Continue processing
        inputs.set(0, 0.0);
        for _ in 0..100 {
            delay.tick(&inputs, &mut outputs);
        }

        // Eventually should output our impulse
        let out = outputs.get(10).unwrap();
        assert!(out.is_finite());
    }
    #[test]
    fn test_delay_line_feedback() {
        let mut delay = DelayLine::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set high feedback
        inputs.set(1, 0.0); // Minimum time
        inputs.set(2, 0.5); // 50% feedback
        inputs.set(3, 0.5); // 50% wet

        // Feed an impulse
        inputs.set(0, 1.0);
        delay.tick(&inputs, &mut outputs);

        // Process more samples with no input
        inputs.set(0, 0.0);
        for _ in 0..1000 {
            delay.tick(&inputs, &mut outputs);
        }

        // Output should still be finite (feedback doesn't blow up)
        let out = outputs.get(10).unwrap();
        assert!(out.is_finite());
    }
    #[test]
    fn test_delay_line_reset() {
        let mut delay = DelayLine::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Feed some signal
        inputs.set(0, 1.0);
        for _ in 0..100 {
            delay.tick(&inputs, &mut outputs);
        }

        // Reset
        delay.reset();

        // Buffer should be cleared
        inputs.set(0, 0.0);
        inputs.set(3, 1.0); // 100% wet
        delay.tick(&inputs, &mut outputs);
        let out = outputs.get(10).unwrap();
        assert!(out.abs() < 0.01);
    }
    #[test]
    fn test_delay_line_with_max_delay_sizes_buffer() {
        // Default is unchanged: 2 s at the given sample rate.
        assert_eq!(DelayLine::new(1000.0).buffer.len(), 2001);
        // A requested maximum sizes the buffer accordingly.
        assert_eq!(DelayLine::with_max_delay(1000.0, 8.0).buffer.len(), 8001);
        // set_sample_rate resizes from the per-instance maximum, not the default.
        let mut long = DelayLine::with_max_delay(1000.0, 8.0);
        long.set_sample_rate(2000.0);
        assert_eq!(long.buffer.len(), 16001);
    }

    #[test]
    fn test_delay_line_long_linear_delay_echoes() {
        // 3 s echo through an 8 s buffer, with the time input in seconds.
        let sr = 1000.0;
        let mut delay = DelayLine::with_max_delay(sr, 8.0).with_linear_time();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(1, 3.0); // 3 seconds, linear
        inputs.set(2, 0.0); // no feedback
        inputs.set(3, 1.0); // fully wet

        inputs.set(0, 1.0);
        delay.tick(&inputs, &mut outputs);
        inputs.set(0, 0.0);

        let mut peak_at = 0;
        let mut peak = 0.0_f64;
        for n in 1..3100 {
            delay.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap().abs();
            if out > peak {
                peak = out;
                peak_at = n;
            }
        }
        assert!(peak > 0.5, "echo should emerge, peak={peak}");
        assert!(
            (2990..=3010).contains(&peak_at),
            "echo should land ~3000 samples later, landed at {peak_at}"
        );
    }

    #[test]
    fn test_delay_line_feedback_clamped_by_default() {
        // Without the opt-in, a feedback input past unity clamps to 0.99 and decays.
        let mut delay = DelayLine::new(1000.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(1, 0.0); // minimum time
        inputs.set(2, 1.2); // past unity, will be clamped
        inputs.set(3, 1.0); // fully wet

        inputs.set(0, 1.0);
        delay.tick(&inputs, &mut outputs);
        inputs.set(0, 0.0);

        let mut late_peak = 0.0_f64;
        for n in 0..4000 {
            delay.tick(&inputs, &mut outputs);
            if n >= 3900 {
                late_peak = late_peak.max(outputs.get(10).unwrap().abs());
            }
        }
        assert!(
            late_peak < 1e-3,
            "clamped feedback must decay, got {late_peak}"
        );
    }

    #[test]
    fn test_delay_line_unclamped_feedback_self_oscillates_bounded() {
        // With the opt-in, feedback past unity sustains — and the in-loop
        // saturation keeps it bounded rather than letting it detonate.
        let mut delay = DelayLine::new(1000.0).with_unclamped_feedback();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(1, 0.0); // minimum time
        inputs.set(2, 1.2); // past unity, honoured in this mode
        inputs.set(3, 1.0); // fully wet

        inputs.set(0, 1.0);
        delay.tick(&inputs, &mut outputs);
        inputs.set(0, 0.0);

        let mut late_peak = 0.0_f64;
        let mut overall_peak = 0.0_f64;
        for n in 0..4000 {
            delay.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap().abs();
            overall_peak = overall_peak.max(out);
            if n >= 3900 {
                late_peak = late_peak.max(out);
            }
        }
        assert!(
            late_peak > 0.5,
            "unclamped feedback must sustain, got {late_peak}"
        );
        assert!(
            overall_peak <= 5.0 + 1e-9,
            "saturation must bound the loop at the ±5V rail, got {overall_peak}"
        );
    }

    #[test]
    fn test_delay_line_linear_time_preserves_slew() {
        // The 5 ms read-distance slew must keep applying in linear mode: a step
        // in the time input glides rather than jumping.
        let sr = 1000.0;
        let mut delay = DelayLine::with_max_delay(sr, 8.0).with_linear_time();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(1, 1.0); // 1 second
        inputs.set(0, 0.0);
        delay.tick(&inputs, &mut outputs);
        // First tick snaps to the setpoint rather than sweeping up from zero.
        assert!((delay.smoothed_delay - 1000.0).abs() < 1e-6);

        inputs.set(1, 2.0); // step to 2 seconds
        delay.tick(&inputs, &mut outputs);
        // One-pole smoothing with a 5 ms time constant at 1 kHz retains
        // exp(-0.2) ≈ 0.819 of the gap per tick: the read distance must have
        // moved, but only a fraction of the way.
        assert!(
            delay.smoothed_delay > 1000.0 && delay.smoothed_delay < 1500.0,
            "time step must glide, smoothed_delay={}",
            delay.smoothed_delay
        );
    }

    #[test]
    fn test_tape_delay_preset() {
        let tape = DelayLine::tape(1000.0);
        assert_eq!(tape.type_id(), "tape_delay");
        assert_eq!(tape.buffer.len(), 12001);
        assert!(tape.unclamped_feedback);
        assert!(tape.linear_time);
        // The plain constructor keeps its identity.
        assert_eq!(DelayLine::new(1000.0).type_id(), "delay_line");
    }

    #[test]
    fn test_chorus() {
        let mut chorus = Chorus::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Default settings
        inputs.set(0, 0.5); // Input signal

        // Process several samples to let LFOs move
        for _ in 0..1000 {
            chorus.tick(&inputs, &mut outputs);
        }

        // Should produce output on all three ports
        let mono = outputs.get(10).unwrap();
        let left = outputs.get(11).unwrap();
        let right = outputs.get(12).unwrap();

        assert!(mono.is_finite());
        assert!(left.is_finite());
        assert!(right.is_finite());
    }
    #[test]
    fn test_chorus_stereo_spread() {
        let mut chorus = Chorus::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set mix to 100% wet
        inputs.set(0, 1.0); // Input signal
        inputs.set(1, 0.5); // Rate
        inputs.set(2, 0.5); // Depth
        inputs.set(3, 1.0); // 100% wet

        // Process many samples
        let mut left_sum = 0.0;
        let mut right_sum = 0.0;
        for _ in 0..10000 {
            chorus.tick(&inputs, &mut outputs);
            left_sum += outputs.get(11).unwrap().abs();
            right_sum += outputs.get(12).unwrap().abs();
        }

        // Both channels should have significant output
        assert!(left_sum > 1.0);
        assert!(right_sum > 1.0);
    }
    #[test]
    fn test_chorus_reset() {
        let mut chorus = Chorus::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Feed signal
        inputs.set(0, 1.0);
        for _ in 0..1000 {
            chorus.tick(&inputs, &mut outputs);
        }

        // Reset
        chorus.reset();

        // Check LFO phases are reset
        inputs.set(0, 0.0);
        inputs.set(3, 1.0); // 100% wet
        chorus.tick(&inputs, &mut outputs);

        // Output should be near zero after reset with zero input
        let out = outputs.get(10).unwrap();
        assert!(out.abs() < 0.1);
    }
    #[test]
    fn test_delay_line_type_id() {
        let delay = DelayLine::new(44100.0);
        assert_eq!(delay.type_id(), "delay_line");
    }
    #[test]
    fn test_chorus_type_id() {
        let chorus = Chorus::new(44100.0);
        assert_eq!(chorus.type_id(), "chorus");
    }
    #[test]
    fn test_delay_line_default() {
        let delay = DelayLine::default();
        assert_eq!(delay.type_id(), "delay_line");
    }
    #[test]
    fn test_chorus_default() {
        let chorus = Chorus::default();
        assert_eq!(chorus.type_id(), "chorus");
    }
    #[test]
    fn test_flanger() {
        let mut flanger = Flanger::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 1.0);
        for _ in 0..1000 {
            flanger.tick(&inputs, &mut outputs);
        }

        let out = outputs.get(10).unwrap();
        assert!(out.is_finite());
    }
    #[test]
    fn test_flanger_default() {
        let flanger = Flanger::default();
        assert_eq!(flanger.type_id(), "flanger");
    }
    #[test]
    fn test_phaser() {
        let mut phaser = Phaser::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 1.0);
        for _ in 0..1000 {
            phaser.tick(&inputs, &mut outputs);
        }

        let out = outputs.get(10).unwrap();
        assert!(out.is_finite());
    }
    #[test]
    fn test_phaser_default() {
        let phaser = Phaser::default();
        assert_eq!(phaser.type_id(), "phaser");
    }
    #[test]
    fn test_phaser_stages() {
        let mut phaser = Phaser::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 1.0);
        inputs.set(5, 0.0); // 2 stages

        for _ in 0..100 {
            phaser.tick(&inputs, &mut outputs);
        }
        let out_2 = outputs.get(10).unwrap();

        phaser.reset();
        inputs.set(5, 1.0); // 6 stages

        for _ in 0..100 {
            phaser.tick(&inputs, &mut outputs);
        }
        let out_6 = outputs.get(10).unwrap();

        // Both should produce valid output
        assert!(out_2.is_finite());
        assert!(out_6.is_finite());
    }

    // Q144: Flanger and Phaser are now mono-in / stereo-out. The `out` port
    // (id 10) must stay bit-identical to `left` (id 11) for backward compat,
    // spread=0 must collapse to a mono image (L==R==out), and spread>0 must
    // decorrelate the left and right channels.

    #[test]
    fn test_flanger_out_mirrors_left_and_mono_at_zero_spread() {
        let mut flanger = Flanger::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(2, 0.8); // depth
        inputs.set(4, 1.0); // full wet exposes the wet paths
        inputs.set(5, 0.0); // spread = 0 -> mono

        for k in 0..5000 {
            inputs.set(0, Libm::<f64>::sin(k as f64 * 0.03));
            flanger.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            let left = outputs.get(11).unwrap();
            let right = outputs.get(12).unwrap();
            assert_eq!(out, left, "out must equal left");
            assert_eq!(left, right, "spread=0 must give bit-identical L/R");
        }
    }

    #[test]
    fn test_flanger_stereo_decorrelation() {
        let mut flanger = Flanger::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(1, 0.5); // rate
        inputs.set(2, 0.9); // depth
        inputs.set(4, 1.0); // full wet
        inputs.set(5, 1.0); // spread = 180 degrees

        let mut diff = 0.0;
        for k in 0..20000 {
            inputs.set(0, Libm::<f64>::sin(k as f64 * 0.05));
            flanger.tick(&inputs, &mut outputs);
            // `out` still tracks the left channel with spread engaged.
            assert_eq!(outputs.get(10).unwrap(), outputs.get(11).unwrap());
            let left = outputs.get(11).unwrap();
            let right = outputs.get(12).unwrap();
            diff += (left - right).abs();
        }
        assert!(
            diff > 1.0,
            "left/right should decorrelate with spread; diff = {diff}"
        );
    }

    #[test]
    fn test_phaser_out_mirrors_left_and_mono_at_zero_spread() {
        let mut phaser = Phaser::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(2, 0.8); // depth
        inputs.set(4, 1.0); // full wet
        inputs.set(6, 0.0); // spread = 0 -> mono

        for k in 0..5000 {
            inputs.set(0, Libm::<f64>::sin(k as f64 * 0.03));
            phaser.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            let left = outputs.get(11).unwrap();
            let right = outputs.get(12).unwrap();
            assert_eq!(out, left, "out must equal left");
            assert_eq!(left, right, "spread=0 must give bit-identical L/R");
        }
    }

    #[test]
    fn test_phaser_stereo_decorrelation() {
        let mut phaser = Phaser::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(2, 0.9); // depth
        inputs.set(4, 1.0); // full wet
        inputs.set(6, 1.0); // spread = 180 degrees

        let mut diff = 0.0;
        for k in 0..20000 {
            inputs.set(0, Libm::<f64>::sin(k as f64 * 0.07));
            phaser.tick(&inputs, &mut outputs);
            assert_eq!(outputs.get(10).unwrap(), outputs.get(11).unwrap());
            let left = outputs.get(11).unwrap();
            let right = outputs.get(12).unwrap();
            diff += (left - right).abs();
        }
        assert!(
            diff > 1.0,
            "phaser left/right should decorrelate with spread; diff = {diff}"
        );
    }

    #[test]
    fn test_unit_delay_default_reset_sample_rate() {
        let mut delay = UnitDelay::default();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 5.0);
        delay.tick(&inputs, &mut outputs);

        delay.reset();
        assert!(delay.buffer == 0.0);

        delay.set_sample_rate(48000.0);
        assert_eq!(delay.type_id(), "unit_delay");
    }
    #[test]
    fn test_reverb_default_reset_sample_rate() {
        let mut reverb = Reverb::default();
        assert_eq!(reverb.sample_rate, 44100.0);

        // Feed some signal
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.5);
        reverb.tick(&inputs, &mut outputs);

        // Reset should clear buffers
        reverb.reset();
        assert_eq!(reverb.predelay_pos, 0);
        assert_eq!(reverb.comb_pos_l, [0; 8]);
        assert_eq!(reverb.comb_pos_r, [0; 8]);
        assert_eq!(reverb.allpass_pos_l, [0; 4]);
        assert_eq!(reverb.allpass_pos_r, [0; 4]);

        // Sample rate change
        reverb.set_sample_rate(48000.0);
        assert_eq!(reverb.sample_rate, 48000.0);

        assert_eq!(reverb.type_id(), "reverb");
        assert_eq!(reverb.port_spec().inputs.len(), 5);
        assert_eq!(reverb.port_spec().outputs.len(), 2);
    }
    #[test]
    fn test_reverb_stereo_output() {
        let mut reverb = Reverb::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Send an impulse
        inputs.set(0, 1.0);
        inputs.set(3, 1.0); // Full wet
        reverb.tick(&inputs, &mut outputs);

        // Feed silence and track total energy
        inputs.set(0, 0.0);
        let mut total_energy = 0.0;
        for _ in 0..3000 {
            reverb.tick(&inputs, &mut outputs);
            total_energy += outputs.get(10).unwrap().abs();
            total_energy += outputs.get(11).unwrap().abs();
        }

        // We should have accumulated some reverb energy
        assert!(
            total_energy > 0.01,
            "Reverb should produce output after impulse, got total_energy={}",
            total_energy
        );
    }
    #[test]
    fn test_reverb_dry_signal() {
        let mut reverb = Reverb::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Full dry
        inputs.set(0, 0.75);
        inputs.set(3, 0.0); // Mix = 0 (full dry)
        reverb.tick(&inputs, &mut outputs);

        let left = outputs.get(10).unwrap();
        let right = outputs.get(11).unwrap();

        // With 0% wet, output should equal input
        assert!(
            (left - 0.75).abs() < 0.001,
            "Full dry should pass through: got {}",
            left
        );
        assert!(
            (right - 0.75).abs() < 0.001,
            "Full dry should pass through: got {}",
            right
        );
    }
    #[test]
    fn test_reverb_room_size() {
        let mut reverb1 = Reverb::new(44100.0);
        let mut reverb2 = Reverb::new(44100.0);
        let mut inputs1 = PortValues::new();
        let mut inputs2 = PortValues::new();
        let mut outputs1 = PortValues::new();
        let mut outputs2 = PortValues::new();

        // Impulse response with different room sizes
        inputs1.set(0, 1.0);
        inputs1.set(1, 0.1); // Small room
        inputs1.set(3, 1.0); // Full wet
        reverb1.tick(&inputs1, &mut outputs1);

        inputs2.set(0, 1.0);
        inputs2.set(1, 0.9); // Large room
        inputs2.set(3, 1.0); // Full wet
        reverb2.tick(&inputs2, &mut outputs2);

        // Process more samples with silence
        inputs1.set(0, 0.0);
        inputs2.set(0, 0.0);
        let mut energy1 = 0.0;
        let mut energy2 = 0.0;

        for _ in 0..5000 {
            reverb1.tick(&inputs1, &mut outputs1);
            reverb2.tick(&inputs2, &mut outputs2);
            energy1 += outputs1.get(10).unwrap().abs();
            energy2 += outputs2.get(10).unwrap().abs();
        }

        // Larger room should have longer decay (more energy over time)
        assert!(
            energy2 > energy1,
            "Larger room should have longer decay: small={}, large={}",
            energy1,
            energy2
        );
    }
    #[test]
    fn test_reverb_predelay() {
        let mut reverb = Reverb::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // With predelay, the wet signal should be delayed
        inputs.set(0, 1.0); // Impulse
        inputs.set(3, 1.0); // Full wet
        inputs.set(4, 1.0); // Max predelay (100ms = 4410 samples at 44.1kHz)

        // First tick
        reverb.tick(&inputs, &mut outputs);

        // At sample 0, with 100ms predelay, wet signal should still be 0
        let first_output = outputs.get(10).unwrap();

        // Feed silence and track energy
        inputs.set(0, 0.0);
        let mut total_energy = 0.0;

        // Run enough samples to pass the predelay plus comb filter delay
        for _ in 0..6000 {
            reverb.tick(&inputs, &mut outputs);
            total_energy += outputs.get(10).unwrap().abs();
        }

        assert!(
            total_energy > 0.01,
            "Reverb should appear after predelay period, got energy={}",
            total_energy
        );
        assert!(
            first_output.abs() < 0.001,
            "First sample should be near zero due to predelay, got {}",
            first_output
        );
    }
    #[test]
    fn test_reverb_damping() {
        let mut reverb_low = Reverb::new(44100.0);
        let mut reverb_high = Reverb::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs_low = PortValues::new();
        let mut outputs_high = PortValues::new();

        // Impulse
        inputs.set(0, 1.0);
        inputs.set(2, 0.1); // Low damping
        inputs.set(3, 1.0);
        reverb_low.tick(&inputs, &mut outputs_low);

        inputs.set(2, 0.9); // High damping
        reverb_high.tick(&inputs, &mut outputs_high);

        // Process more
        inputs.set(0, 0.0);
        for _ in 0..3000 {
            reverb_low.tick(&inputs, &mut outputs_low);
            reverb_high.tick(&inputs, &mut outputs_high);
        }

        // Both should produce some output (the damping affects character, not overall level dramatically)
        // This test verifies both modes work without errors
        let out_low = outputs_low.get(10).unwrap();
        let out_high = outputs_high.get(10).unwrap();

        // Just verify they produce valid output
        assert!(out_low.is_finite());
        assert!(out_high.is_finite());
    }
    #[test]
    fn test_reverb_tunings_scale_with_sample_rate() {
        let reverb_44 = Reverb::new(44100.0);
        let reverb_48 = Reverb::new(48000.0);

        // Higher sample rate should have proportionally longer comb lengths
        let ratio = 48000.0 / 44100.0;

        for i in 0..8 {
            let expected = (reverb_44.comb_lengths[i] as f64 * ratio) as usize;
            assert!(
                (reverb_48.comb_lengths[i] as i64 - expected as i64).abs() < 2,
                "Comb filter {} should scale with sample rate",
                i
            );
        }
    }

    /// Drive a sinusoid through a single allpass stage and return the
    /// steady-state RMS gain (input RMS / output RMS).
    #[cfg(test)]
    fn allpass_rms_gain(coef: f64, freq_norm: f64) -> f64 {
        let mut x1 = 0.0;
        let mut y1 = 0.0;
        let n = 40_000;
        let warmup = 8_000;
        let mut sum_in = 0.0;
        let mut sum_out = 0.0;
        for i in 0..n {
            let x = Libm::<f64>::sin(TAU * freq_norm * i as f64);
            let y = Phaser::allpass(x, &mut x1, &mut y1, coef);
            if i >= warmup {
                sum_in += x * x;
                sum_out += y * y;
            }
        }
        Libm::<f64>::sqrt(sum_out / sum_in)
    }

    #[test]
    fn test_phaser_allpass_unit_magnitude() {
        // Q020: a genuine first-order allpass must have unit magnitude at every
        // frequency. Check near DC and near Nyquist for several coefficients.
        // The previous (non-allpass) topology gave ~0.2 gain at Nyquist for
        // coef = 0.5 — this test would have failed there.
        for &coef in &[-0.6, -0.2, 0.2, 0.5, 0.8] {
            let dc_gain = allpass_rms_gain(coef, 0.001);
            let nyq_gain = allpass_rms_gain(coef, 0.499);
            assert!(
                (dc_gain - 1.0).abs() < 0.01,
                "DC gain {} not ~1.0 for coef {}",
                dc_gain,
                coef
            );
            assert!(
                (nyq_gain - 1.0).abs() < 0.01,
                "Nyquist gain {} not ~1.0 for coef {}",
                nyq_gain,
                coef
            );
        }
    }

    #[test]
    fn test_chorus_delay_stays_positive() {
        // Q021: across the full LFO cycle at maximum depth the per-voice delay
        // must stay strictly above the 1-sample clamp floor, so the sweep is
        // never one-sidedly flattened.
        let sample_rate = 44100.0;
        let base = Chorus::BASE_DELAY_MS * sample_rate / 1000.0;
        let mod_depth = Chorus::MAX_MOD_DELAY_MS * sample_rate / 1000.0;

        let steps = 2000;
        let mut min_delay = f64::INFINITY;
        for k in 0..steps {
            let lfo = Libm::<f64>::sin((k as f64 / steps as f64) * TAU);
            let delay = Chorus::voice_delay_samples(base, mod_depth, lfo);
            min_delay = min_delay.min(delay);
        }
        assert!(
            min_delay > 1.0,
            "minimum chorus delay {} hit the clamp floor",
            min_delay
        );
        // The trough of a unipolar sweep sits exactly at the base delay.
        let trough = Chorus::voice_delay_samples(base, mod_depth, -1.0);
        assert!(
            (trough - base).abs() < 1e-9,
            "trough {} != base {}",
            trough,
            base
        );
    }

    #[test]
    fn test_delay_line_time_smoothing() {
        // Q022: a step in the time CV must not move the read distance
        // discontinuously; the one-pole smoother eases it over many samples.
        let mut delay = DelayLine::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0);
        inputs.set(1, 0.0); // minimum delay time
        inputs.set(2, 0.0);
        inputs.set(3, 0.5);
        delay.tick(&inputs, &mut outputs); // prime (snaps to min)
        let start = delay.smoothed_delay;

        // Step the time CV to maximum.
        inputs.set(1, 1.0);
        delay.tick(&inputs, &mut outputs);
        let after_one = delay.smoothed_delay;

        let full_jump = (delay.buffer.len() - 1) as f64 - start;
        let moved = after_one - start;
        assert!(moved > 0.0, "smoother did not move toward setpoint");
        assert!(
            moved < full_jump * 0.05,
            "smoother jumped {} of a {}-sample step in one tick",
            moved,
            full_jump
        );

        // It should take many samples to traverse most of the step.
        let mut ticks = 1;
        while delay.smoothed_delay < start + full_jump * 0.9 && ticks < 100_000 {
            delay.tick(&inputs, &mut outputs);
            ticks += 1;
        }
        assert!(
            ticks > 100,
            "smoother converged too fast in {} ticks",
            ticks
        );
    }

    #[test]
    fn test_vibrato_exact_delay() {
        // Q023: with modulation depth 0 the delay is constant, so an impulse
        // must emerge after exactly round(delay_ms * fs / 1000) samples. The
        // pre-fix write-before-read order produced this one sample early.
        let sample_rate = 44100.0;
        let mut vib = Vibrato::new(sample_rate);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(1, 0.3); // rate (irrelevant at zero depth)
        inputs.set(2, 0.0); // depth = 0 -> constant delay
        inputs.set(3, 1.0); // 100% wet

        let expected = (Vibrato::MAX_DELAY_MS * 0.5 * sample_rate / 1000.0).round() as usize;

        inputs.set(0, 1.0); // impulse at tick 0
        vib.tick(&inputs, &mut outputs);
        let mut peak_idx = if outputs.get(10).unwrap().abs() > 0.5 {
            Some(0usize)
        } else {
            None
        };

        inputs.set(0, 0.0);
        for i in 1..(expected + 50) {
            vib.tick(&inputs, &mut outputs);
            if peak_idx.is_none() && outputs.get(10).unwrap().abs() > 0.5 {
                peak_idx = Some(i);
            }
        }
        assert_eq!(
            peak_idx,
            Some(expected),
            "impulse emerged at {:?}, expected {}",
            peak_idx,
            expected
        );
    }

    #[test]
    fn test_reverb_stereo_spread_scales_with_sample_rate() {
        // Q024: the right-channel decorrelation offset must scale with sample
        // rate, not stay a fixed 23 samples.
        let reverb_44 = Reverb::new(44100.0);
        assert_eq!(reverb_44.stereo_spread, STEREO_SPREAD);

        let reverb_88 = Reverb::new(88200.0);
        // 88.2 kHz is exactly 2x -> spread ~46, not 23.
        assert_eq!(reverb_88.stereo_spread, 46);

        for i in 0..8 {
            let length_l = reverb_88.comb_lengths[i];
            let length_r = (length_l + reverb_88.stereo_spread).min(MAX_COMB_SIZE - 1);
            assert_eq!(
                length_r - length_l,
                46,
                "right comb {} should lead left by the scaled spread",
                i
            );
        }
    }

    // ---- Q157: Tremolo unit tests ----

    #[test]
    fn test_tremolo_am_depth() {
        // A DC carrier isolates the amplitude modulation. Full depth must swing
        // the output across (nearly) the whole [0, carrier] range; zero depth
        // must leave the carrier untouched.
        let mut trem = Tremolo::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 1.0); // DC carrier
        inputs.set(1, 1.0); // fast rate (~20 Hz) to sweep the LFO quickly
        inputs.set(3, 0.0); // sine shape

        inputs.set(2, 1.0); // full depth
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for _ in 0..44_100 {
            trem.tick(&inputs, &mut outputs);
            let o = outputs.get(10).unwrap();
            assert!(o.is_finite());
            lo = lo.min(o);
            hi = hi.max(o);
        }
        assert!(
            lo < 0.1 && hi > 0.9,
            "full-depth AM must reach near 0 and near the carrier: lo={lo} hi={hi}"
        );

        trem.reset();
        inputs.set(2, 0.0); // zero depth
        let (mut lo0, mut hi0) = (f64::INFINITY, f64::NEG_INFINITY);
        for _ in 0..4410 {
            trem.tick(&inputs, &mut outputs);
            let o = outputs.get(10).unwrap();
            lo0 = lo0.min(o);
            hi0 = hi0.max(o);
        }
        assert!(
            (hi0 - lo0) < 1e-9 && (hi0 - 1.0).abs() < 1e-9,
            "zero-depth tremolo must pass the carrier unchanged: span={}",
            hi0 - lo0
        );
    }

    #[test]
    fn test_tremolo_reset_and_sample_rate() {
        let mut trem = Tremolo::default();
        assert_eq!(trem.type_id(), "tremolo");
        assert_eq!(trem.sample_rate, 44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 1.0);
        inputs.set(1, 0.5);
        for _ in 0..500 {
            trem.tick(&inputs, &mut outputs);
        }
        assert!(trem.lfo_phase != 0.0);
        trem.reset();
        assert_eq!(trem.lfo_phase, 0.0);
        trem.set_sample_rate(48000.0);
        assert_eq!(trem.sample_rate, 48000.0);
        trem.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap().is_finite());
    }

    // ---- Q157: Vibrato pitch-modulation depth ----

    /// Collect the spacing (in samples) between successive upward zero crossings
    /// of `sig`, then return `max_interval - min_interval`.
    fn zero_crossing_interval_spread(sig: &[f64]) -> f64 {
        let mut crossings = Vec::new();
        for i in 1..sig.len() {
            if sig[i - 1] <= 0.0 && sig[i] > 0.0 {
                crossings.push(i);
            }
        }
        if crossings.len() < 3 {
            return 0.0;
        }
        let mut min_iv = f64::INFINITY;
        let mut max_iv = f64::NEG_INFINITY;
        for w in crossings.windows(2) {
            let iv = (w[1] - w[0]) as f64;
            min_iv = min_iv.min(iv);
            max_iv = max_iv.max(iv);
        }
        max_iv - min_iv
    }

    #[test]
    fn test_vibrato_pitch_modulation_depth() {
        // Vibrato modulates a delay line, so the output pitch wobbles: the
        // spacing between the output's zero crossings must vary far more with a
        // large modulation depth than with zero depth (constant delay).
        let run = |depth: f64| -> f64 {
            let mut vib = Vibrato::new(44100.0);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(1, 0.78); // ~5 Hz LFO
            inputs.set(2, depth);
            inputs.set(3, 1.0); // 100% wet
            let mut out = Vec::with_capacity(20_000);
            let dt = 500.0 / 44100.0;
            let mut phase = 0.0f64;
            for _ in 0..20_000 {
                let s = Libm::<f64>::sin(TAU * phase);
                phase += dt;
                if phase >= 1.0 {
                    phase -= 1.0;
                }
                inputs.set(0, s);
                vib.tick(&inputs, &mut outputs);
                out.push(outputs.get(10).unwrap());
            }
            zero_crossing_interval_spread(&out)
        };

        let spread_off = run(0.0);
        let spread_on = run(0.8);
        assert!(
            spread_off < 3.0,
            "zero-depth vibrato should have near-constant pitch: spread={spread_off}"
        );
        assert!(
            spread_on > spread_off + 10.0,
            "depth-0.8 vibrato must wobble the pitch: on={spread_on} off={spread_off}"
        );
    }

    #[test]
    fn test_vibrato_reset_and_sample_rate() {
        let mut vib = Vibrato::default();
        assert_eq!(vib.type_id(), "vibrato");
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 1.0);
        inputs.set(2, 0.5);
        for _ in 0..500 {
            vib.tick(&inputs, &mut outputs);
        }
        assert!(vib.lfo_phase != 0.0);
        vib.reset();
        assert_eq!(vib.lfo_phase, 0.0);
        assert_eq!(vib.write_pos, 0);
        assert!(vib.buffer.iter().all(|&x| x == 0.0));
        vib.set_sample_rate(48000.0);
        assert_eq!(vib.sample_rate, 48000.0);
        vib.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap().is_finite());
    }

    #[test]
    fn test_vibrato_lowering_sample_rate_does_not_panic() {
        // Regression: at 96kHz the delay buffer is large; ticking advances
        // write_pos to a large value. Lowering the sample rate shrinks the
        // buffer, and the write site indexes it directly (non-modulo). If
        // set_sample_rate leaves write_pos stale, the next tick panics with
        // index-out-of-bounds. It must reset write_pos.
        let mut vib = Vibrato::new(96000.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.5);
        inputs.set(2, 0.5);
        // Advance write_pos well past a shrunken buffer's length.
        for _ in 0..1000 {
            vib.tick(&inputs, &mut outputs);
        }
        // Lower the sample rate: buffer shrinks from ~1930 to ~451 samples.
        vib.set_sample_rate(22050.0);
        assert_eq!(vib.write_pos, 0, "write_pos must be reset after resize");
        // Must not panic on the next tick.
        vib.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap().is_finite());
    }

    // ---- Coefficient memoization (perf) ------------------------------------

    /// Memoization must be observationally invisible: a phaser whose memos
    /// (rate map and allpass coefficient) are invalidated before every tick
    /// executes the pre-memoization computation every sample and must agree
    /// bit-for-bit with the memoized phaser. Covers a frozen sweep (depth 0,
    /// coefficient memo hits) and an active sweep with per-sample-modulated
    /// rate (both memos miss).
    #[test]
    fn test_phaser_memo_bit_identical() {
        let mut memoized = Phaser::new(44100.0);
        let mut forced = Phaser::new(44100.0);
        let mut inputs = PortValues::new();
        let mut out_m = PortValues::new();
        let mut out_f = PortValues::new();

        for n in 0..20_000u32 {
            let t = n as f64;
            inputs.set(0, Libm::<f64>::sin(t * 0.043) * 3.0);
            inputs.set(3, 0.4); // feedback
            inputs.set(6, 0.5); // spread
            if n < 10_000 {
                // Frozen sweep: rate/coef memos hit after the first sample.
                inputs.set(1, 0.3);
                inputs.set(2, 0.0);
            } else {
                // Active sweep + modulated rate: memos miss every sample.
                inputs.set(1, 0.3 + 0.2 * Libm::<f64>::sin(t * 0.001));
                inputs.set(2, 0.7);
            }

            memoized.tick(&inputs, &mut out_m);
            forced.rate_memo.invalidate();
            forced.coef_memo.invalidate();
            forced.tick(&inputs, &mut out_f);

            for &id in &[10u32, 11, 12] {
                assert_eq!(
                    out_m.get(id).unwrap().to_bits(),
                    out_f.get(id).unwrap().to_bits(),
                    "Phaser output {id} diverged at sample {n}"
                );
            }
        }
        // In the frozen half both channels freeze on the same frequency, so
        // the coefficient is computed once, not once per channel per sample.
        assert!(memoized.rate_memo.recompute_count() <= 10_001);
    }

    /// Same equivalence for the delay line's memoized exponential time map,
    /// exercising the feedback path (memoized values feed recirculating state).
    #[test]
    fn test_delay_line_memo_bit_identical() {
        let mut memoized = DelayLine::new(44100.0);
        let mut forced = DelayLine::new(44100.0);
        let mut inputs = PortValues::new();
        let mut out_m = PortValues::new();
        let mut out_f = PortValues::new();

        for n in 0..20_000u32 {
            let t = n as f64;
            inputs.set(0, Libm::<f64>::sin(t * 0.029) * 4.0);
            inputs.set(2, 0.6); // feedback
            inputs.set(3, 0.5); // mix
            if n < 10_000 {
                inputs.set(1, 0.4);
            } else {
                // Per-sample-modulated delay time (memo misses every sample).
                inputs.set(1, 0.4 + 0.1 * Libm::<f64>::sin(t * 0.0007));
            }

            memoized.tick(&inputs, &mut out_m);
            forced.delay_ms_memo.invalidate();
            forced.tick(&inputs, &mut out_f);

            assert_eq!(
                out_m.get(10).unwrap().to_bits(),
                out_f.get(10).unwrap().to_bits(),
                "DelayLine output diverged at sample {n}"
            );
        }
        assert!(memoized.delay_ms_memo.recompute_count() <= 10_001);
    }
}
