//! Delay-based and time-domain effect modules.

use super::common::{env_coef, read_interpolated};
use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use alloc::vec;
use alloc::vec::Vec;
use core::f64::consts::TAU;
use libm::Libm;

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
/// Maximum delay time is 2 seconds at any sample rate.
pub struct DelayLine {
    buffer: Vec<f64>,
    write_pos: usize,
    sample_rate: f64,
    /// Slew-smoothed read distance, tracking the delay setpoint gradually to
    /// avoid zipper/pitch glitches when the `time` CV jumps.
    smoothed_delay: f64,
    /// One-pole retain coefficient for `smoothed_delay` (sample-rate aware).
    delay_smooth_coef: f64,
    /// Whether `smoothed_delay` has been snapped to its first setpoint yet.
    delay_primed: bool,
    spec: PortSpec,
}

impl DelayLine {
    /// Maximum delay time in seconds
    const MAX_DELAY_SECS: f64 = 2.0;

    /// Time constant for delay-time smoothing (a few ms de-zippers modulation
    /// without audibly lagging deliberate delay-time changes).
    const DELAY_SMOOTH_SECS: f64 = 0.005;

    pub fn new(sample_rate: f64) -> Self {
        let buffer_size = (sample_rate * Self::MAX_DELAY_SECS) as usize + 1;
        Self {
            buffer: vec![0.0; buffer_size],
            write_pos: 0,
            sample_rate,
            smoothed_delay: 0.0,
            delay_smooth_coef: env_coef(Self::DELAY_SMOOTH_SECS, sample_rate),
            delay_primed: false,
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
        let input = inputs.get_or(0, 0.0);
        let time_cv = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
        let feedback = inputs.get_or(2, 0.0).clamp(0.0, 0.99); // Prevent runaway
        let mix = inputs.get_or(3, 0.5).clamp(0.0, 1.0);

        // Map time CV (0-1) to delay time (1ms to max delay, exponential)
        let min_delay_ms = 1.0;
        let max_delay_ms = Self::MAX_DELAY_SECS * 1000.0;
        let delay_ms = min_delay_ms * Libm::<f64>::pow(max_delay_ms / min_delay_ms, time_cv);
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

        // Write input + feedback to buffer
        self.buffer[self.write_pos] = input + delayed * feedback;

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
        let buffer_size = (sample_rate * Self::MAX_DELAY_SECS) as usize + 1;
        self.buffer = vec![0.0; buffer_size];
        self.write_pos = 0;
        self.smoothed_delay = 0.0;
        self.delay_smooth_coef = env_coef(Self::DELAY_SMOOTH_SECS, sample_rate);
        self.delay_primed = false;
    }

    fn breaks_feedback_cycle(&self) -> bool {
        true
    }

    fn type_id(&self) -> &'static str {
        "delay_line"
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
        let input = inputs.get_or(0, 0.0);
        let rate_cv = inputs.get_or(1, 0.3).clamp(0.0, 1.0);
        let depth_cv = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let mix = inputs.get_or(3, 0.5).clamp(0.0, 1.0);

        // Map rate CV to LFO frequency (0.1 Hz to 5 Hz)
        let lfo_freq = 0.1 * Libm::<f64>::pow(50.0, rate_cv);

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
            *buffer = vec![0.0; buffer_size];
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
        let input = inputs.get_or(0, 0.0);
        let rate_cv = inputs.get_or(1, 0.3).clamp(0.0, 1.0);
        let depth_cv = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let feedback = inputs.get_or(3, 0.0).clamp(-0.95, 0.95);
        let mix = inputs.get_or(4, 0.5).clamp(0.0, 1.0);
        let spread = inputs.get_or(5, 0.5).clamp(0.0, 1.0);

        let lfo_freq = 0.05 * Libm::<f64>::pow(100.0, rate_cv);
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
            *buffer = vec![0.0; buffer_size];
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
    spec: PortSpec,
}

impl Phaser {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            allpass_x1: [[0.0; 6]; 2],
            allpass_y1: [[0.0; 6]; 2],
            lfo_phase: 0.0,
            sample_rate,
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
        let input = inputs.get_or(0, 0.0);
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

        let lfo_freq = 0.05 * Libm::<f64>::pow(100.0, rate_cv);

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

            let omega = TAU * freq / self.sample_rate;
            let tan_w = Libm::<f64>::tan(omega * 0.5);
            let coef = (1.0 - tan_w) / (1.0 + tan_w);

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
    spec: PortSpec,
}

impl Tremolo {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            lfo_phase: 0.0,
            sample_rate,
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

        // Rate: 0.1Hz to 20Hz (exponential)
        let lfo_freq = 0.1 * Libm::<f64>::pow(200.0, rate_cv);

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

        // Rate: 0.1Hz to 15Hz (exponential)
        let lfo_freq = 0.1 * Libm::<f64>::pow(150.0, rate_cv);

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
        self.buffer.resize(buffer_size, 0.0);
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
        let input = inputs.get_or(0, 0.0);
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
}
