//! Oscillator and source modules.

use super::common::{polyblamp, polyblep, voct_to_hz, wrap_phase, EdgeDetector, GATE_THRESHOLD_V};
use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use crate::rng;
use alloc::vec;
use alloc::vec::Vec;
use core::f64::consts::TAU;
use libm::Libm;

/// Voltage-Controlled Oscillator (VCO)
///
/// A multi-waveform oscillator with V/Oct pitch input, FM, pulse width control,
/// and hard sync. Outputs sine, triangle, saw, and square waveforms.
///
/// The saw and square outputs are bandlimited with PolyBLEP and the triangle
/// with PolyBLAMP to suppress aliasing; the sine is inherently bandlimited.
///
/// # FM inputs
/// - Port 1 `fm` (exponential): a raw ±5V CvBipolar signal is treated as
///   ±5 octaves (`freq = base * 2^fm`), i.e. full-scale ±5V spans ×32 / ÷32.
///   Attenuate it for musical depths.
/// - Port 4 `fm_lin` (linear, through-zero): adds to the frequency directly,
///   scaled so ±5V is ±100% of the base frequency
///   (`freq += (fm_lin / 5) * base`). This enables through-zero linear FM and
///   its classic symmetric sidebands; the frequency may pass through and below
///   zero, running the phase backwards.
pub struct Vco {
    phase: f64,
    sample_rate: f64,
    sync_edge: EdgeDetector,
    spec: PortSpec,
}

impl Vco {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
            sync_edge: EdgeDetector::new(),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "voct", SignalKind::VoltPerOctave),
                    // Exponential FM: ±5V input == ±5 octaves (see struct docs).
                    PortDef::new(1, "fm", SignalKind::CvBipolar).with_attenuverter(),
                    PortDef::new(2, "pw", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(3, "sync", SignalKind::Gate),
                    // Linear through-zero FM: ±5V == ±100% of base frequency.
                    PortDef::new(4, "fm_lin", SignalKind::CvBipolar).with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "sin", SignalKind::Audio),
                    PortDef::new(11, "tri", SignalKind::Audio),
                    PortDef::new(12, "saw", SignalKind::Audio),
                    PortDef::new(13, "sqr", SignalKind::Audio),
                ],
            },
        }
    }
}

impl Default for Vco {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Vco {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let voct = inputs.get_or(0, 0.0);
        let fm = inputs.get_or(1, 0.0);
        let pw = inputs.get_or(2, 0.5).clamp(0.05, 0.95);
        let sync = inputs.get_or(3, 0.0);
        let fm_lin = inputs.get_or(4, 0.0);

        // V/Oct to frequency: 0V = C4 (261.63 Hz).
        let base_freq = voct_to_hz(voct);
        // Exponential FM: raw ±5V == ±5 octaves.
        let mut freq = base_freq * Libm::<f64>::pow(2.0, fm);
        // Linear through-zero FM: ±5V == ±100% of base frequency. This can drive
        // `freq` (and thus the phase increment) negative for through-zero FM.
        freq += (fm_lin / 5.0) * base_freq;

        // Normalized phase increment (may be negative under through-zero FM).
        let dt = freq / self.sample_rate;
        // Magnitude used for PolyBLEP/PolyBLAMP transition widths.
        let dt_abs = Libm::<f64>::fabs(dt);

        // Hard sync on rising edge. Capture the pre-reset phase and the
        // fractional crossing position so we can bandlimit the reset step.
        let mut sync_reset: Option<(f64, f64)> = None;
        if let Some(frac) = self.sync_edge.rising_frac(sync) {
            sync_reset = Some((self.phase, frac));
            self.phase = 0.0;
        }

        let phase = self.phase;

        // Sine is inherently bandlimited.
        let sin = Libm::<f64>::sin(phase * TAU) * 5.0;

        // Bandlimited saw: naive ramp minus PolyBLEP at the wrap.
        let mut saw = 2.0 * phase - 1.0;
        saw -= polyblep(phase, dt_abs);

        // Bandlimited square/pulse: PolyBLEP at both the rising (wrap) and
        // falling (pulse-width) edges.
        let mut sqr = if phase < pw { 1.0 } else { -1.0 };
        sqr += polyblep(phase, dt_abs);
        let pw_edge = {
            let x = phase + (1.0 - pw);
            x - Libm::<f64>::floor(x)
        };
        sqr -= polyblep(pw_edge, dt_abs);

        // Bandlimited triangle via PolyBLAMP corrections at its two corners
        // (slope changes of ±8 per unit phase => ±4*dt per sample).
        let mut tri = 1.0 - 4.0 * Libm::<f64>::fabs(phase - 0.5);
        let corner_half = {
            let x = phase - 0.5;
            if x < 0.0 {
                x + 1.0
            } else {
                x
            }
        };
        tri += 4.0 * dt_abs * polyblamp(phase, dt_abs);
        tri -= 4.0 * dt_abs * polyblamp(corner_half, dt_abs);

        // Bounded hard-sync correction. The reset introduces a value step in the
        // saw and square that is not at a natural phase wrap, so the wrap-BLEP
        // above cannot see it. We apply a one-sided PolyBLEP for the step using
        // the fractional reset position. Note: this corrects the sample(s) after
        // the reset only; the sample immediately before the sub-sample edge is
        // already emitted, so a small residual discontinuity remains (a full
        // two-sided minBLEP is out of scope for this single-sample structure).
        if let Some((p_old, frac)) = sync_reset {
            // Phase-equivalent position of the (past) discontinuity for PolyBLEP.
            let equiv = (1.0 - frac) * dt_abs;
            let blep = polyblep(equiv, dt_abs);
            // Saw step (normalized): from (2*p_old-1) down to (2*0-1) = -2*p_old.
            let saw_step = -2.0 * p_old;
            saw += (saw_step / 2.0) * blep;
            // Square step: from sign(p_old<pw) to +1 (phase reset to 0 < pw).
            let old_sqr = if p_old < pw { 1.0 } else { -1.0 };
            let sqr_step = 1.0 - old_sqr;
            sqr += (sqr_step / 2.0) * blep;
        }

        // Scale to ±5V.
        outputs.set(10, sin);
        outputs.set(11, tri * 5.0);
        outputs.set(12, saw * 5.0);
        outputs.set(13, sqr * 5.0);

        // Advance phase (dt may be negative under through-zero FM). Q198:
        // wrap_phase also recovers from a non-finite dt (extreme V/Oct or FM
        // overflowing voct_to_hz) instead of latching the accumulator to NaN.
        self.phase = wrap_phase(self.phase + dt);
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.sync_edge.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "vco"
    }
}

/// Low-Frequency Oscillator (LFO)
///
/// A slow oscillator for modulation purposes. Features rate control,
/// depth control, and reset trigger.
pub struct Lfo {
    phase: f64,
    sample_rate: f64,
    reset_edge: EdgeDetector,
    spec: PortSpec,
}

impl Lfo {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
            reset_edge: EdgeDetector::new(),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "rate", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(1, "depth", SignalKind::CvUnipolar).with_default(10.0),
                    PortDef::new(2, "reset", SignalKind::Trigger),
                ],
                outputs: vec![
                    PortDef::new(10, "sin", SignalKind::CvBipolar),
                    PortDef::new(11, "tri", SignalKind::CvBipolar),
                    PortDef::new(12, "saw", SignalKind::CvBipolar),
                    PortDef::new(13, "sqr", SignalKind::CvBipolar),
                    PortDef::new(14, "sin_uni", SignalKind::CvUnipolar),
                ],
            },
        }
    }
}

impl Default for Lfo {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Lfo {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let rate_cv = inputs.get_or(0, 0.5);
        let depth = inputs.get_or(1, 10.0) / 10.0; // Normalize to 0-1
        let reset = inputs.get_or(2, 0.0);

        // Map rate CV (0-1) to frequency (0.01 Hz - 30 Hz, exponential)
        let freq = 0.01 * Libm::<f64>::pow(3000.0, rate_cv.clamp(0.0, 1.0));

        // Reset on trigger
        if self.reset_edge.rising(reset) {
            self.phase = 0.0;
        }

        // Generate waveforms scaled by depth (±5V * depth)
        let scale = 5.0 * depth;
        let sin = Libm::<f64>::sin(self.phase * TAU) * scale;
        let tri = (1.0 - 4.0 * Libm::<f64>::fabs(self.phase - 0.5)) * scale;
        let saw = (2.0 * self.phase - 1.0) * scale;
        let sqr = if self.phase < 0.5 { scale } else { -scale };
        let sin_uni = (Libm::<f64>::sin(self.phase * TAU) * 0.5 + 0.5) * depth * 10.0;

        outputs.set(10, sin);
        outputs.set(11, tri);
        outputs.set(12, saw);
        outputs.set(13, sqr);
        outputs.set(14, sin_uni);

        // Q198: wrap_phase recovers from a non-finite rate instead of latching.
        self.phase = wrap_phase(self.phase + freq / self.sample_rate);
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.reset_edge.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "lfo"
    }
}

/// Supersaw Oscillator
///
/// JP-8000 style supersaw with 7 detuned oscillators.
/// Creates thick, wide sounds.
pub struct Supersaw {
    phases: [f64; 7],
    /// Independent accumulator for the sub-oscillator, one octave below the
    /// center voice (advances at half the center rate).
    sub_phase: f64,
    sample_rate: f64,
    spec: PortSpec,
}

impl Supersaw {
    // Detune amounts for 7 oscillators (center + 3 pairs)
    // Based on Roland JP-8000 analysis
    const DETUNE_RATIOS: [f64; 7] = [
        -0.11002313, // -1 octave pair 1
        -0.06288439, // -1 octave pair 2
        -0.01952356, // -1 octave pair 3
        0.0,         // Center
        0.01991221,  // +1 octave pair 3
        0.06216538,  // +1 octave pair 2
        0.10745242,  // +1 octave pair 1
    ];

    // Mix levels for each oscillator
    const MIX_LEVELS: [f64; 7] = [0.5, 0.7, 0.9, 1.0, 0.9, 0.7, 0.5];

    pub fn new(sample_rate: f64) -> Self {
        // Start each oscillator at different phases for immediate thickness
        let mut phases = [0.0; 7];
        for (i, phase) in phases.iter_mut().enumerate() {
            *phase = (i as f64) / 7.0;
        }

        Self {
            phases,
            sub_phase: 0.0,
            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "voct", SignalKind::VoltPerOctave).with_default(0.0),
                    PortDef::new(1, "detune", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "mix", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "sub", SignalKind::Audio),
                ],
            },
        }
    }

    // Polyblep anti-aliasing for saw wave
}

impl Default for Supersaw {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Supersaw {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let voct = inputs.get_or(0, 0.0);
        let detune = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
        let mix = inputs.get_or(2, 0.5).clamp(0.0, 1.0);

        // Base frequency from V/Oct
        let base_freq = voct_to_hz(voct); // C4 at 0V

        let mut sum = 0.0;
        let mut total_mix = 0.0;
        // Band-limited center voice (index 3, zero detune), captured for the mix.
        let mut center_saw = 0.0;

        for i in 0..7 {
            // Apply detune
            let detune_amount = Self::DETUNE_RATIOS[i] * detune;
            let freq = base_freq * (1.0 + detune_amount);
            let dt = freq / self.sample_rate;

            // Generate saw with polyblep
            let raw_saw = 2.0 * self.phases[i] - 1.0;
            let blep = polyblep(self.phases[i], dt);
            let saw = raw_saw - blep;

            // Q006: reuse the already band-limited center voice for the mix
            // blend instead of recomputing a naive ramp.
            if i == 3 {
                center_saw = saw;
            }

            // Mix with level
            sum += saw * Self::MIX_LEVELS[i];
            total_mix += Self::MIX_LEVELS[i];

            // Advance phase (Q198: wrap_phase also recovers non-finite rates
            // and wraps a huge finite dt in O(1) where `-= 1.0` could not).
            self.phases[i] = wrap_phase(self.phases[i] + dt);
        }

        // Normalize and apply mix (blend between center oscillator and full supersaw)
        let normalized = sum / total_mix;
        let output = center_saw * (1.0 - mix) + normalized * mix;

        // Sub oscillator: a true octave-down band-limited saw driven by an
        // independent accumulator advancing at half the center rate.
        let sub_dt = base_freq / (2.0 * self.sample_rate);
        let sub = (2.0 * self.sub_phase - 1.0) - polyblep(self.sub_phase, sub_dt);
        self.sub_phase = wrap_phase(self.sub_phase + sub_dt); // Q198

        outputs.set(10, output);
        outputs.set(11, sub);
    }

    fn reset(&mut self) {
        for (i, phase) in self.phases.iter_mut().enumerate() {
            *phase = (i as f64) / 7.0;
        }
        self.sub_phase = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "supersaw"
    }
}

/// Karplus-Strong String
///
/// Physical modeling plucked string synthesis.
/// Creates realistic plucked string and percussion sounds.
pub struct KarplusStrong {
    buffer: Vec<f64>,
    /// Maximum delay-line length (samples), sized for the lowest supported note.
    /// The per-pluck period is clamped against this, not the current buffer
    /// length, so a high note that shrinks the buffer cannot pin later low
    /// notes to a too-short period (Q003 tuning regression).
    max_len: usize,
    write_pos: usize,
    sample_rate: f64,
    last_output: f64,
    /// Rising-edge detector for the trigger (excite once per pluck).
    trigger_edge: EdgeDetector,
    spec: PortSpec,
}

impl KarplusStrong {
    /// Loop DC leak: makes the feedback loop's DC gain slightly below unity so
    /// any residual excitation offset decays instead of circulating forever.
    const LOOP_LEAK: f64 = 0.9995;

    pub fn new(sample_rate: f64) -> Self {
        // Buffer for lowest frequency (around 20Hz)
        let buffer_size = (sample_rate / 20.0) as usize + 10;
        Self {
            buffer: vec![0.0; buffer_size],
            max_len: buffer_size,
            write_pos: 0,
            sample_rate,
            last_output: 0.0,
            trigger_edge: EdgeDetector::new(),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "voct", SignalKind::VoltPerOctave).with_default(0.0),
                    PortDef::new(1, "trigger", SignalKind::Trigger),
                    PortDef::new(2, "damping", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(3, "brightness", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(4, "stretch", SignalKind::CvBipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }

    fn excite(&mut self, brightness: f64) {
        // Fill buffer with noise (excitation)
        let period = self.buffer.len();
        for i in 0..period {
            // Blend between noise and impulse based on brightness
            let noise = rng::random_bipolar();
            let impulse = if i < period / 4 { 1.0 } else { 0.0 };
            self.buffer[i] = noise * brightness + impulse * (1.0 - brightness);
        }
        // Q004: remove the DC component from the excitation. The impulse part is
        // strictly positive, and the loop filter has unity DC gain, so any DC
        // bias would circulate undamped. Zero-meaning the excitation prevents a
        // constant offset/thump that never decays.
        let mean: f64 = self.buffer.iter().sum::<f64>() / period as f64;
        for sample in self.buffer.iter_mut() {
            *sample -= mean;
        }
    }
}

impl Default for KarplusStrong {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for KarplusStrong {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let voct = inputs.get_or(0, 0.0);
        let trigger = inputs.get_or(1, 0.0);
        let damping = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let brightness = inputs.get_or(3, 0.5).clamp(0.0, 1.0);
        let stretch = inputs.get_or(4, 0.0).clamp(-1.0, 1.0);

        // Calculate period from frequency. Clamp against the FULL delay-line
        // capacity (`max_len`), not the current buffer length: a prior high-note
        // pluck shrinks `buffer`, but a later low note must still be able to
        // request its full (longer) period and grow the buffer back on pluck.
        let freq = voct_to_hz(voct);
        let period = (self.sample_rate / freq).clamp(2.0, self.max_len as f64 - 1.0);
        let period_int = period as usize;

        // Q002/Q129: excite only on a rising edge across the canonical gate
        // threshold, so a gate/trigger held high for many samples plucks the
        // string exactly once (and lets it ring) instead of re-filling the
        // buffer with noise every sample.
        if self.trigger_edge.rising(trigger) {
            // Resize buffer for this frequency
            self.buffer.truncate(period_int + 2);
            self.buffer.resize(period_int + 2, 0.0);
            self.excite(brightness);
            self.write_pos = 0;
        }

        // Loop-filter coefficient (one-pole lowpass), higher damping = brighter.
        let filter_coef = 0.5 + damping * 0.49; // 0.5 to 0.99

        // Q003: place the fractional-delay taps so the *total* loop delay equals
        // the target period. The one-pole loop filter contributes a group delay
        // of (1-c)/c samples at DC, so the delay line must supply
        // `period - filter_group_delay`.
        let filter_gd = (1.0 - filter_coef) / filter_coef;
        let target_delay = (period - filter_gd).max(1.0);
        let delay_int = target_delay as usize;
        let delay_frac = target_delay - delay_int as f64;

        // A tap `off` samples ahead of write_pos yields a delay of `len - off`.
        let len = self.buffer.len();
        let off1 = len.saturating_sub(delay_int); // delay = delay_int
        let off2 = off1.saturating_sub(1); // delay = delay_int + 1
        let read_pos = (self.write_pos + off1) % len;
        let read_pos2 = (self.write_pos + off2) % len;
        let sample =
            self.buffer[read_pos] * (1.0 - delay_frac) + self.buffer[read_pos2] * delay_frac;

        // Lowpass filter (one-pole averaging with damping control).
        let filtered = sample * filter_coef + self.last_output * (1.0 - filter_coef);

        // All-pass filter for stretch factor (inharmonicity)
        let stretch_coef = stretch * 0.5;
        let stretched = filtered + stretch_coef * (filtered - self.last_output);

        // Q004: leak the loop slightly so its DC gain is below unity and any
        // residual offset decays toward zero rather than circulating forever.
        let leaked = stretched * Self::LOOP_LEAK;

        self.last_output = leaked;

        // Write back to buffer
        self.buffer[self.write_pos] = leaked;
        self.write_pos = (self.write_pos + 1) % len;

        outputs.set(10, leaked);
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
        self.last_output = 0.0;
        self.trigger_edge.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let buffer_size = (sample_rate / 20.0) as usize + 10;
        self.max_len = buffer_size;
        self.buffer.resize(buffer_size, 0.0);
    }

    fn type_id(&self) -> &'static str {
        "karplus_strong"
    }
}

// ============================================================================
// P3 Utilities: ScaleQuantizer, Euclidean
// ============================================================================

/// Pink noise generator state
struct PinkNoiseState {
    rows: [f64; 16],
    running_sum: f64,
    index: u32,
}

impl PinkNoiseState {
    fn new() -> Self {
        Self {
            rows: [0.0; 16],
            running_sum: 0.0,
            index: 0,
        }
    }

    fn sample(&mut self) -> f64 {
        self.index = self.index.wrapping_add(1);
        let changed_bits = (self.index ^ (self.index.wrapping_sub(1))).trailing_ones() as usize;

        for i in 0..changed_bits.min(16) {
            self.running_sum -= self.rows[i];
            self.rows[i] = rng::random_bipolar();
            self.running_sum += self.rows[i];
        }

        self.running_sum / 16.0
    }
}

/// Noise Generator
///
/// Generates white and pink noise signals.
///
/// Phase 3 addition: Correlated stereo noise outputs for more realistic
/// analog modeling (shared randomness between channels).
pub struct NoiseGenerator {
    pink: PinkNoiseState,
    /// Phase 3: Secondary pink noise for stereo correlation
    pink2: PinkNoiseState,
    /// Phase 3: Correlation amount between channels (0 = independent, 1 = identical)
    pub(crate) correlation: f64,
    /// Phase 3: Last white noise sample for correlation
    last_white: f64,
    spec: PortSpec,
}

impl NoiseGenerator {
    pub fn new() -> Self {
        Self {
            pink: PinkNoiseState::new(),
            pink2: PinkNoiseState::new(),
            correlation: 0.3, // Default 30% correlation (realistic)
            last_white: 0.0,
            spec: PortSpec {
                inputs: vec![
                    // Phase 3: Correlation control
                    PortDef::new(0, "correlation", SignalKind::CvUnipolar).with_default(0.3),
                ],
                outputs: vec![
                    PortDef::new(10, "white", SignalKind::Audio),
                    PortDef::new(11, "pink", SignalKind::Audio),
                    // Phase 3: Correlated stereo pair
                    PortDef::new(12, "white2", SignalKind::Audio),
                    PortDef::new(13, "pink2", SignalKind::Audio),
                ],
            },
        }
    }

    /// Create a noise generator with specific correlation
    pub fn with_correlation(correlation: f64) -> Self {
        let mut gen = Self::new();
        gen.correlation = correlation.clamp(0.0, 1.0);
        gen
    }
}

impl Default for NoiseGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for NoiseGenerator {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Phase 3: Adjustable correlation
        let correlation = inputs.get_or(0, self.correlation).clamp(0.0, 1.0);

        // Primary white noise
        let white1 = rng::random_bipolar();

        // Phase 3: Correlated white noise for second channel
        // Mix between independent noise and correlated (shared) noise
        let independent = rng::random_bipolar();
        let white2 = white1 * correlation + independent * (1.0 - correlation);

        // Primary pink noise
        let pink1 = self.pink.sample();

        // Phase 3: Correlated pink noise
        let pink2_independent = self.pink2.sample();
        let pink2 = pink1 * correlation + pink2_independent * (1.0 - correlation);

        self.last_white = white1;

        outputs.set(10, white1 * 5.0);
        outputs.set(11, pink1 * 5.0);
        outputs.set(12, white2 * 5.0);
        outputs.set(13, pink2 * 5.0);
    }

    fn reset(&mut self) {
        self.pink = PinkNoiseState::new();
        self.pink2 = PinkNoiseState::new();
        self.last_white = 0.0;
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "noise"
    }
}

/// Wavetable type for different oscillator sounds
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WavetableType {
    /// Pure sine wave
    Sine,
    /// Triangle wave (bandlimited)
    Triangle,
    /// Sawtooth wave (bandlimited)
    Saw,
    /// Square wave (bandlimited)
    Square,
    /// 25% pulse width
    Pulse25,
    /// 12.5% pulse width
    Pulse12,
    /// Formant-like vowel "ah"
    FormantA,
    /// Formant-like vowel "oh"
    FormantO,
}

impl WavetableType {
    /// Get table index (0-7)
    pub fn index(self) -> usize {
        match self {
            WavetableType::Sine => 0,
            WavetableType::Triangle => 1,
            WavetableType::Saw => 2,
            WavetableType::Square => 3,
            WavetableType::Pulse25 => 4,
            WavetableType::Pulse12 => 5,
            WavetableType::FormantA => 6,
            WavetableType::FormantO => 7,
        }
    }

    /// Get type from index
    pub fn from_index(idx: usize) -> Self {
        match idx % 8 {
            0 => WavetableType::Sine,
            1 => WavetableType::Triangle,
            2 => WavetableType::Saw,
            3 => WavetableType::Square,
            4 => WavetableType::Pulse25,
            5 => WavetableType::Pulse12,
            6 => WavetableType::FormantA,
            _ => WavetableType::FormantO,
        }
    }
}

/// Wavetable oscillator with morphing between tables
///
/// Provides 8 pre-computed bandlimited wavetables with linear interpolation
/// and smooth crossfade morphing between adjacent tables.
///
/// # Ports
/// - Input 0: V/Oct pitch (0V = C4 = 261.63 Hz)
/// - Input 1: Table select (0-1 CV maps to 8 tables)
/// - Input 2: Morph amount (0-1 for crossfading between tables)
/// - Input 3: Sync input (hard sync on positive edge)
/// - Output 10: Audio output (±5V)
pub struct Wavetable {
    /// 8 wavetables, each a mip pyramid of `NUM_MIPS` bandlimited levels of 256
    /// samples. Level 0 has the most harmonics (for low pitches); each higher
    /// level halves the maximum harmonic number for the next octave up.
    tables: [[[f64; 256]; 8]; 8],
    /// Current phase (0.0 to 1.0)
    phase: f64,
    /// Previous sync input for edge detection
    prev_sync: f64,
    sample_rate: f64,
    spec: PortSpec,
}

impl Wavetable {
    /// Number of samples per wavetable
    const TABLE_SIZE: usize = 256;
    /// Number of wavetables
    const NUM_TABLES: usize = 8;
    /// Number of mip levels per wavetable (each level covers one octave of
    /// pitch; level `L` band-limits to `BASE_HARMONICS >> L` harmonics).
    const NUM_MIPS: usize = 8;
    /// Highest harmonic number present in the level-0 (brightest) table, per
    /// waveform. Index matches [`WavetableType::index`].
    /// (sine, triangle, saw, square, pulse25, pulse12, formantA, formantO)
    const BASE_HARMONICS: [usize; 8] = [1, 31, 64, 63, 64, 64, 10, 10];

    pub fn new(sample_rate: f64) -> Self {
        let spec = PortSpec {
            inputs: vec![
                PortDef::new(0, "v_oct", SignalKind::VoltPerOctave).with_default(0.0),
                PortDef::new(1, "table", SignalKind::CvUnipolar).with_default(0.0),
                PortDef::new(2, "morph", SignalKind::CvUnipolar).with_default(0.0),
                PortDef::new(3, "sync", SignalKind::Gate).with_default(0.0),
            ],
            outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
        };

        let mut osc = Self {
            tables: [[[0.0; 256]; 8]; 8],
            phase: 0.0,
            prev_sync: 0.0,
            sample_rate,
            spec,
        };
        osc.generate_tables();
        osc
    }

    /// Maximum harmonic number to synthesize for waveform `table` at mip
    /// `level` (at least 1). Halving per level gives an octave-per-level pyramid.
    fn max_harmonic(table: usize, level: usize) -> usize {
        (Self::BASE_HARMONICS[table] >> level).max(1)
    }

    /// Generate all wavetables as mip pyramids with bandlimiting.
    fn generate_tables(&mut self) {
        let n = Self::TABLE_SIZE;
        let pi = core::f64::consts::PI;

        for level in 0..Self::NUM_MIPS {
            for i in 0..n {
                let phase = (i as f64) / (n as f64);
                let partial = |harmonic: f64| Libm::<f64>::sin(phase * harmonic * 2.0 * pi);

                // Sine wave (pure) — always a single harmonic.
                self.tables[0][level][i] = partial(1.0);

                // Triangle: odd harmonics, alternating sign, 1/h^2 rolloff.
                let mut tri = 0.0;
                let mut h = 1usize;
                let mh = Self::max_harmonic(1, level);
                let mut sign = 1.0; // +,-,+,- across successive odd harmonics
                while h <= mh {
                    let hf = h as f64;
                    tri += sign * partial(hf) / (hf * hf);
                    sign = -sign;
                    h += 2;
                }
                self.tables[1][level][i] = tri * (8.0 / (pi * pi));

                // Saw: all harmonics, alternating sign, 1/h rolloff.
                let mut saw = 0.0;
                let mh = Self::max_harmonic(2, level);
                let mut sign = -1.0; // h=1 -> -1, h=2 -> +1, ...
                for h in 1..=mh {
                    let hf = h as f64;
                    saw += sign * partial(hf) / hf;
                    sign = -sign;
                }
                self.tables[2][level][i] = saw * (2.0 / pi);

                // Square: odd harmonics, 1/h rolloff.
                let mut sqr = 0.0;
                let mut h = 1usize;
                let mh = Self::max_harmonic(3, level);
                while h <= mh {
                    let hf = h as f64;
                    sqr += partial(hf) / hf;
                    h += 2;
                }
                self.tables[3][level][i] = sqr * (4.0 / pi);

                // Pulse 25% / 12.5%: Fourier series of a rectangular pulse.
                for (table_idx, duty) in [(4usize, 0.25f64), (5usize, 0.125f64)] {
                    let mut pulse = 0.0;
                    let mh = Self::max_harmonic(table_idx, level);
                    for h in 1..=mh {
                        let hf = h as f64;
                        let coef = Libm::<f64>::sin(pi * hf * duty) / hf;
                        pulse += coef * partial(hf);
                    }
                    self.tables[table_idx][level][i] = pulse * 2.0;
                }

                // Formant "ah"/"oh": fundamental plus resonant partials, each
                // gated on staying within this level's harmonic limit.
                let mh_a = Self::max_harmonic(6, level) as f64;
                let formant_a = [(1.0, 1.0), (2.7, 0.5), (4.6, 0.3), (9.6, 0.15)]
                    .iter()
                    .filter(|(mult, _)| *mult <= mh_a)
                    .map(|(mult, amp)| partial(*mult) * amp)
                    .sum::<f64>();
                self.tables[6][level][i] = formant_a * 0.5;

                let mh_o = Self::max_harmonic(7, level) as f64;
                let formant_o = [(1.0, 1.0), (1.5, 0.6), (3.0, 0.4), (10.0, 0.15)]
                    .iter()
                    .filter(|(mult, _)| *mult <= mh_o)
                    .map(|(mult, amp)| partial(*mult) * amp)
                    .sum::<f64>();
                self.tables[7][level][i] = formant_o * 0.5;
            }
        }
    }

    /// Select the mip level for `table` at a given (absolute) phase increment so
    /// that every synthesized harmonic stays below Nyquist.
    fn select_level(table: usize, phase_inc: f64) -> usize {
        let inc = Libm::<f64>::fabs(phase_inc).max(1e-9);
        // Highest harmonic number that fits below Nyquist at this pitch.
        let allowed = Libm::<f64>::floor(0.5 / inc);
        for level in 0..Self::NUM_MIPS {
            if (Self::max_harmonic(table, level) as f64) <= allowed {
                return level;
            }
        }
        Self::NUM_MIPS - 1
    }

    /// Read from a wavetable mip level with linear interpolation.
    fn read_table(&self, table_idx: usize, level: usize, phase: f64) -> f64 {
        let table = &self.tables[table_idx % Self::NUM_TABLES][level.min(Self::NUM_MIPS - 1)];
        let pos = phase * (Self::TABLE_SIZE as f64);
        let idx0 = (pos as usize) % Self::TABLE_SIZE;
        let idx1 = (idx0 + 1) % Self::TABLE_SIZE;
        let frac = pos - Libm::<f64>::floor(pos);

        // Linear interpolation between samples
        table[idx0] * (1.0 - frac) + table[idx1] * frac
    }
}

impl Default for Wavetable {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Wavetable {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Get inputs
        let v_oct = inputs.get_or(0, 0.0);
        let table_cv = inputs.get_or(1, 0.0).clamp(0.0, 1.0);
        let morph = inputs.get_or(2, 0.0).clamp(0.0, 1.0);
        let sync = inputs.get_or(3, 0.0);

        // Hard sync: reset phase on positive edge
        if sync > GATE_THRESHOLD_V && self.prev_sync <= GATE_THRESHOLD_V {
            self.phase = 0.0;
        }
        self.prev_sync = sync;

        // Calculate frequency from V/Oct (0V = C4 = 261.63 Hz)
        let frequency = voct_to_hz(v_oct);
        let phase_inc = frequency / self.sample_rate;

        // Select tables based on table CV and morph
        // Table CV selects base table (0-7), morph crossfades to next table
        let table_pos = table_cv * ((Self::NUM_TABLES - 1) as f64);
        let table_idx = (table_pos as usize).min(Self::NUM_TABLES - 2);
        let table_frac = table_pos - (table_idx as f64);

        // Blend morph and table fraction for smooth transitions
        let blend = (table_frac + morph).min(1.0);

        // Q005: select a per-table mip level from the phase increment so high
        // notes drop harmonics that would otherwise fold back above Nyquist.
        let level0 = Self::select_level(table_idx, phase_inc);
        let level1 = Self::select_level(table_idx + 1, phase_inc);

        // Read from both tables and crossfade
        let sample0 = self.read_table(table_idx, level0, self.phase);
        let sample1 = self.read_table(table_idx + 1, level1, self.phase);
        let sample = sample0 * (1.0 - blend) + sample1 * blend;

        // Advance phase (Q198: `while >= 1.0` would spin forever on an `inf`
        // increment and for eons on a huge finite one; wrap_phase is O(1)).
        self.phase = wrap_phase(self.phase + phase_inc);

        // Output as audio (±5V)
        outputs.set(10, sample * 5.0);
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.prev_sync = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "wavetable"
    }
}

/// Formant oscillator for vocal synthesis
///
/// Generates vocal-like sounds by combining a glottal pulse excitation
/// with parallel resonant filters tuned to formant frequencies for different vowels.
///
/// # Ports
/// - Input 0: V/Oct pitch (0V = C4 = 261.63 Hz)
/// - Input 1: Vowel select (0-1 CV maps to A/E/I/O/U)
/// - Input 2: Formant shift (bipolar CV, shifts all formants up/down)
/// - Input 3: Vibrato depth (0-1 CV)
/// - Output 10: Audio output (±5V)
pub struct FormantOsc {
    /// Current phase for glottal pulse (0.0 to 1.0)
    phase: f64,
    /// Vibrato LFO phase
    vibrato_phase: f64,
    /// 5 resonator states (2 state variables each)
    resonator_state: [[f64; 2]; 5],
    sample_rate: f64,
    spec: PortSpec,
}

impl FormantOsc {
    /// Formant frequencies for each vowel (F1-F5 in Hz)
    /// Based on typical adult male formant values
    const FORMANTS: [[f64; 5]; 5] = [
        // A: /ɑ/ as in "father"
        [700.0, 1220.0, 2600.0, 3500.0, 4500.0],
        // E: /ɛ/ as in "bed"
        [530.0, 1840.0, 2480.0, 3500.0, 4500.0],
        // I: /i/ as in "see"
        [280.0, 2250.0, 2890.0, 3500.0, 4500.0],
        // O: /ɔ/ as in "law"
        [500.0, 700.0, 2350.0, 3500.0, 4500.0],
        // U: /u/ as in "boot"
        [300.0, 870.0, 2250.0, 3500.0, 4500.0],
    ];

    /// Formant bandwidths (Q values) - narrower = more resonant
    const BANDWIDTHS: [f64; 5] = [80.0, 90.0, 120.0, 150.0, 200.0];

    /// Formant amplitudes (relative gains for each formant)
    const AMPLITUDES: [f64; 5] = [1.0, 0.5, 0.25, 0.1, 0.05];

    /// Vibrato rate in Hz
    const VIBRATO_RATE: f64 = 5.5;

    pub fn new(sample_rate: f64) -> Self {
        let spec = PortSpec {
            inputs: vec![
                PortDef::new(0, "v_oct", SignalKind::VoltPerOctave).with_default(0.0),
                PortDef::new(1, "vowel", SignalKind::CvUnipolar).with_default(0.0),
                PortDef::new(2, "formant_shift", SignalKind::CvBipolar).with_default(0.0),
                PortDef::new(3, "vibrato", SignalKind::CvUnipolar).with_default(0.0),
            ],
            outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
        };

        Self {
            phase: 0.0,
            vibrato_phase: 0.0,
            resonator_state: [[0.0; 2]; 5],
            sample_rate,
            spec,
        }
    }

    /// Get interpolated formant frequencies for a vowel position (0-1)
    fn get_formants(&self, vowel: f64, shift: f64) -> [f64; 5] {
        let vowel = vowel.clamp(0.0, 1.0);
        let idx = vowel * 4.0;
        let idx0 = (idx as usize).min(3);
        let idx1 = idx0 + 1;
        let frac = idx - (idx0 as f64);

        // Shift factor: bipolar CV maps to 0.5x - 2x frequency multiplier
        let shift_mult = Libm::<f64>::pow(2.0, shift / 5.0);

        let mut result = [0.0; 5];
        for (i, value) in result.iter_mut().enumerate() {
            let f0 = Self::FORMANTS[idx0][i];
            let f1 = Self::FORMANTS[idx1][i];
            *value = (f0 * (1.0 - frac) + f1 * frac) * shift_mult;
        }
        result
    }

    /// Process a sample through a 2-pole resonator (state-variable filter style)
    fn process_resonator(
        &mut self,
        input: f64,
        freq: f64,
        bandwidth: f64,
        formant_idx: usize,
    ) -> f64 {
        let omega = 2.0 * core::f64::consts::PI * freq / self.sample_rate;
        let omega = omega.clamp(0.01, core::f64::consts::PI * 0.45);

        let q = freq / bandwidth;
        let alpha = Libm::<f64>::sin(omega) / (2.0 * q);

        // Simple 2-pole bandpass resonator
        let cos_omega = Libm::<f64>::cos(omega);
        let b0 = alpha;
        let a1 = -2.0 * cos_omega;
        let a2 = 1.0 - alpha;
        let norm = 1.0 + alpha;

        let state = &mut self.resonator_state[formant_idx];

        // Direct Form II transposed
        let output = b0 / norm * input + state[0];
        state[0] = -a1 / norm * output + state[1];
        state[1] = -b0 / norm * input - a2 / norm * output;

        output
    }

    /// Generate glottal pulse (simplified LF model approximation)
    fn glottal_pulse(phase: f64) -> f64 {
        // Approximation of Liljencrants-Fant glottal pulse model
        // Quick rise, slower fall
        if phase < 0.4 {
            // Opening phase
            let t = phase / 0.4;
            Libm::<f64>::sin(t * core::f64::consts::PI * 0.5)
        } else if phase < 0.8 {
            // Closing phase
            let t = (phase - 0.4) / 0.4;
            Libm::<f64>::cos(t * core::f64::consts::PI * 0.5)
        } else {
            // Closed phase
            0.0
        }
    }
}

impl Default for FormantOsc {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for FormantOsc {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Get inputs
        let v_oct = inputs.get_or(0, 0.0);
        let vowel = inputs.get_or(1, 0.0).clamp(0.0, 1.0);
        let formant_shift = inputs.get_or(2, 0.0);
        let vibrato_depth = inputs.get_or(3, 0.0).clamp(0.0, 1.0);

        // Apply vibrato
        let vibrato = Libm::<f64>::sin(self.vibrato_phase * 2.0 * core::f64::consts::PI);
        let vibrato_semitones = vibrato * vibrato_depth * 0.5; // Max ±0.5 semitones
        let v_oct_with_vibrato = v_oct + vibrato_semitones / 12.0;

        // Calculate fundamental frequency
        let frequency = voct_to_hz(v_oct_with_vibrato);
        let phase_inc = frequency / self.sample_rate;

        // Generate glottal pulse excitation
        let excitation = Self::glottal_pulse(self.phase);

        // Get formant frequencies for current vowel
        let formants = self.get_formants(vowel, formant_shift);

        // Process through parallel resonators and sum
        let mut output = 0.0;
        for (i, &freq) in formants.iter().enumerate() {
            let formant_out = self.process_resonator(excitation, freq, Self::BANDWIDTHS[i], i);
            output += formant_out * Self::AMPLITUDES[i];
        }

        // Advance phases (Q198: O(1) wrap, recovers from non-finite rates).
        self.phase = wrap_phase(self.phase + phase_inc);
        self.vibrato_phase = wrap_phase(self.vibrato_phase + Self::VIBRATO_RATE / self.sample_rate);

        // Output with normalization (±5V audio)
        outputs.set(10, output.clamp(-1.0, 1.0) * 5.0);
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.vibrato_phase = 0.0;
        self.resonator_state = [[0.0; 2]; 5];
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "formant_osc"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::common::measure_max_output;

    // Q198: a non-finite pitch input must not permanently latch the phase
    // accumulator (`NaN - floor(NaN)` is NaN). After poisoning, a clean V/Oct
    // must produce a normal oscillation again.
    #[test]
    fn test_vco_nan_pitch_recovery() {
        let mut vco = Vco::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        for &bad in &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            inputs.set(0, bad);
            vco.tick(&inputs, &mut outputs);
        }
        // Extreme finite V/Oct overflows voct_to_hz (2^1100 == inf) — same latch risk.
        inputs.set(0, 1100.0);
        vco.tick(&inputs, &mut outputs);

        // Clean pitch: the saw output must be finite and actually oscillate.
        inputs.set(0, 0.0);
        let mut max_abs: f64 = 0.0;
        for _ in 0..4410 {
            vco.tick(&inputs, &mut outputs);
            let saw = outputs.get(12).unwrap();
            assert!(
                saw.is_finite(),
                "VCO output stayed non-finite after bad pitch input"
            );
            max_abs = max_abs.max(saw.abs());
        }
        assert!(
            max_abs > 1.0,
            "VCO failed to oscillate after bad pitch input (max |saw| = {max_abs})"
        );
    }

    // Q198: the old `while phase >= 1.0 {{ phase -= 1.0 }}` wrap would spin the
    // audio thread forever on an `inf` phase increment (inf - 1.0 == inf). This
    // test completing at all proves the O(1) wrap.
    #[test]
    fn test_wavetable_extreme_pitch_no_hang() {
        let mut wt = Wavetable::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        for &voct in &[1100.0, f64::INFINITY, f64::NAN, -1100.0] {
            inputs.set(0, voct);
            wt.tick(&inputs, &mut outputs);
        }

        inputs.set(0, 0.0);
        for _ in 0..64 {
            wt.tick(&inputs, &mut outputs);
            assert!(outputs.get(10).unwrap().is_finite());
        }
    }

    #[test]
    fn test_vco_frequency() {
        let mut vco = Vco::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // At 0V, should be C4 (261.63 Hz)
        inputs.set(0, 0.0);

        // Run for one period and count zero crossings
        let period_samples = (44100.0 / 261.63) as usize;
        let mut samples = Vec::new();

        for _ in 0..period_samples * 10 {
            vco.tick(&inputs, &mut outputs);
            samples.push(outputs.get(12).unwrap()); // Saw output
        }

        // Count rising zero crossings
        let crossings: Vec<_> = samples
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .collect();

        // Should have approximately 10 crossings (10 periods)
        assert!(crossings.len() >= 8 && crossings.len() <= 12);
    }
    #[test]
    fn test_lfo_rate() {
        let mut lfo = Lfo::new(1000.0); // 1kHz for easy math
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.5); // Mid rate

        // Run for a bit
        for _ in 0..1000 {
            lfo.tick(&inputs, &mut outputs);
        }

        // Just verify it produces output
        let out = outputs.get(10).unwrap();
        assert!(out.abs() <= 5.0);
    }
    #[test]
    fn test_noise_generator() {
        let mut noise = NoiseGenerator::new();
        let inputs = PortValues::new();
        let mut outputs = PortValues::new();

        noise.tick(&inputs, &mut outputs);

        // Should produce output
        assert!(outputs.get(10).is_some());
        assert!(outputs.get(11).is_some());
    }
    #[test]
    fn test_vco_default_reset_sample_rate() {
        let mut vco = Vco::default();
        assert!(vco.sample_rate == 44100.0);

        vco.set_sample_rate(48000.0);
        assert!(vco.sample_rate == 48000.0);

        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.0);
        for _ in 0..100 {
            vco.tick(&inputs, &mut outputs);
        }

        vco.reset();
        assert!(vco.phase == 0.0);

        assert_eq!(vco.type_id(), "vco");
    }
    #[test]
    fn test_lfo_default_reset_sample_rate() {
        let mut lfo = Lfo::default();
        assert!(lfo.sample_rate == 44100.0);

        lfo.set_sample_rate(48000.0);
        assert!(lfo.sample_rate == 48000.0);

        let inputs = PortValues::new();
        let mut outputs = PortValues::new();
        for _ in 0..100 {
            lfo.tick(&inputs, &mut outputs);
        }

        lfo.reset();
        assert!(lfo.phase == 0.0);

        assert_eq!(lfo.type_id(), "lfo");
    }
    #[test]
    fn test_noise_generator_default_reset_sample_rate() {
        let mut noise = NoiseGenerator::default();
        noise.reset();
        noise.set_sample_rate(48000.0);
        assert_eq!(noise.type_id(), "noise");
    }
    #[test]
    fn test_lfo_shapes() {
        let mut lfo = Lfo::new(1000.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 5.0); // Medium rate

        // Run for a while to get all shapes
        for _ in 0..1000 {
            lfo.tick(&inputs, &mut outputs);
        }

        // All shape outputs should exist
        assert!(outputs.get(10).is_some()); // Sine
        assert!(outputs.get(11).is_some()); // Triangle
        assert!(outputs.get(12).is_some()); // Saw
        assert!(outputs.get(13).is_some()); // Square
    }
    #[test]
    fn test_vco_pwm() {
        let mut vco = Vco::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0); // C4
        inputs.set(2, 7.5); // 75% pulse width

        for _ in 0..1000 {
            vco.tick(&inputs, &mut outputs);
        }

        // Pulse output should exist
        assert!(outputs.get(13).is_some());
    }
    #[test]
    fn test_wavetable_type_index() {
        assert_eq!(WavetableType::Sine.index(), 0);
        assert_eq!(WavetableType::Triangle.index(), 1);
        assert_eq!(WavetableType::Saw.index(), 2);
        assert_eq!(WavetableType::Square.index(), 3);
        assert_eq!(WavetableType::Pulse25.index(), 4);
        assert_eq!(WavetableType::Pulse12.index(), 5);
        assert_eq!(WavetableType::FormantA.index(), 6);
        assert_eq!(WavetableType::FormantO.index(), 7);
    }
    #[test]
    fn test_wavetable_type_from_index() {
        assert_eq!(WavetableType::from_index(0), WavetableType::Sine);
        assert_eq!(WavetableType::from_index(1), WavetableType::Triangle);
        assert_eq!(WavetableType::from_index(7), WavetableType::FormantO);
        assert_eq!(WavetableType::from_index(8), WavetableType::Sine); // wraps
    }
    #[test]
    fn test_wavetable_default_reset_sample_rate() {
        let mut wt = Wavetable::default();
        assert_eq!(wt.sample_rate, 44100.0);

        // Process some samples
        let inputs = PortValues::new();
        let mut outputs = PortValues::new();
        for _ in 0..100 {
            wt.tick(&inputs, &mut outputs);
        }

        // Verify phase is non-zero
        assert!(wt.phase > 0.0);

        // Reset should clear phase
        wt.reset();
        assert_eq!(wt.phase, 0.0);
        assert_eq!(wt.prev_sync, 0.0);

        // Set sample rate
        wt.set_sample_rate(48000.0);
        assert_eq!(wt.sample_rate, 48000.0);

        assert_eq!(wt.type_id(), "wavetable");
        assert_eq!(wt.port_spec().inputs.len(), 4);
        assert_eq!(wt.port_spec().outputs.len(), 1);
    }
    #[test]
    fn test_wavetable_sine_output() {
        let mut wt = Wavetable::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // At 0V = 261.63 Hz, table 0 = sine
        inputs.set(0, 0.0); // C4
        inputs.set(1, 0.0); // First table (sine)

        // Collect samples over one cycle
        let samples_per_cycle = (44100.0 / 261.63) as usize;
        let mut max_val = 0.0f64;
        let mut min_val = 0.0f64;

        for _ in 0..samples_per_cycle {
            wt.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            max_val = max_val.max(out);
            min_val = min_val.min(out);
        }

        // Should have approximately ±5V peaks (sine wave)
        assert!(max_val > 4.0, "max should be near 5V: {}", max_val);
        assert!(min_val < -4.0, "min should be near -5V: {}", min_val);
    }
    #[test]
    fn test_wavetable_table_selection() {
        let mut wt = Wavetable::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 2.0); // Higher frequency for faster cycles

        // Different table values should produce different outputs
        let mut outputs_by_table = Vec::new();
        for table_cv in [0.0, 0.5, 1.0] {
            wt.reset();
            inputs.set(1, table_cv);
            inputs.set(2, 0.0); // No morph

            let mut sum = 0.0;
            for _ in 0..100 {
                wt.tick(&inputs, &mut outputs);
                sum += outputs.get(10).unwrap().abs();
            }
            outputs_by_table.push(sum);
        }

        // Different tables should produce measurably different outputs
        assert!((outputs_by_table[0] - outputs_by_table[1]).abs() > 1.0);
        assert!((outputs_by_table[1] - outputs_by_table[2]).abs() > 1.0);
    }
    #[test]
    fn test_wavetable_morph() {
        let mut wt = Wavetable::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 1.0);
        inputs.set(1, 0.0); // Table 0

        // Output with no morph
        wt.reset();
        inputs.set(2, 0.0);
        let mut sum_no_morph = 0.0;
        for _ in 0..100 {
            wt.tick(&inputs, &mut outputs);
            sum_no_morph += outputs.get(10).unwrap();
        }

        // Output with full morph
        wt.reset();
        inputs.set(2, 1.0);
        let mut sum_full_morph = 0.0;
        for _ in 0..100 {
            wt.tick(&inputs, &mut outputs);
            sum_full_morph += outputs.get(10).unwrap();
        }

        // Morph should change the output
        assert!((sum_no_morph - sum_full_morph).abs() > 0.1);
    }
    #[test]
    fn test_wavetable_hard_sync() {
        let mut wt = Wavetable::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0);
        inputs.set(1, 0.0);

        // Run for a bit to advance phase
        for _ in 0..50 {
            wt.tick(&inputs, &mut outputs);
        }
        let phase_before = wt.phase;
        assert!(phase_before > 0.0);

        // Trigger sync (low -> high transition)
        inputs.set(3, 0.0);
        wt.tick(&inputs, &mut outputs);
        inputs.set(3, 5.0); // High gate
        wt.tick(&inputs, &mut outputs);

        // Phase should have been reset
        assert!(wt.phase < 0.1, "Phase should reset on sync: {}", wt.phase);
    }
    #[test]
    fn test_wavetable_frequency_tracking() {
        let mut wt = Wavetable::new(44100.0);

        // At different V/Oct values, frequency should change
        // Count zero crossings over fixed number of samples
        let count_zero_crossings = |wt: &mut Wavetable, v_oct: f64| -> usize {
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(0, v_oct);
            inputs.set(1, 0.0);
            wt.reset();

            let mut crossings = 0;
            let mut prev_out = 0.0;
            for _ in 0..1000 {
                wt.tick(&inputs, &mut outputs);
                let out = outputs.get(10).unwrap();
                if prev_out <= 0.0 && out > 0.0 {
                    crossings += 1;
                }
                prev_out = out;
            }
            crossings
        };

        let crossings_c4 = count_zero_crossings(&mut wt, 0.0); // C4
        let crossings_c5 = count_zero_crossings(&mut wt, 1.0); // C5 (octave higher)

        // Octave higher should have approximately twice the zero crossings
        let ratio = crossings_c5 as f64 / crossings_c4 as f64;
        assert!(
            ratio > 1.8 && ratio < 2.2,
            "Octave ratio should be ~2: {}",
            ratio
        );
    }
    #[test]
    fn test_formant_osc_default_reset_sample_rate() {
        let mut osc = FormantOsc::default();
        assert_eq!(osc.sample_rate, 44100.0);

        // Process some samples
        let inputs = PortValues::new();
        let mut outputs = PortValues::new();
        for _ in 0..100 {
            osc.tick(&inputs, &mut outputs);
        }

        // Verify phase is non-zero
        assert!(osc.phase > 0.0);

        // Reset should clear state
        osc.reset();
        assert_eq!(osc.phase, 0.0);
        assert_eq!(osc.vibrato_phase, 0.0);
        assert_eq!(osc.resonator_state, [[0.0; 2]; 5]);

        // Set sample rate
        osc.set_sample_rate(48000.0);
        assert_eq!(osc.sample_rate, 48000.0);

        assert_eq!(osc.type_id(), "formant_osc");
        assert_eq!(osc.port_spec().inputs.len(), 4);
        assert_eq!(osc.port_spec().outputs.len(), 1);
    }
    #[test]
    fn test_formant_osc_output() {
        let mut osc = FormantOsc::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0); // C4
        inputs.set(1, 0.0); // Vowel A

        // Collect samples
        let mut max_val = 0.0f64;
        let mut min_val = 0.0f64;

        for _ in 0..1000 {
            osc.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            max_val = max_val.max(out);
            min_val = min_val.min(out);
        }

        // Should produce audio output
        assert!(max_val > 0.0, "Should have positive output: {}", max_val);
        assert!(min_val < 0.0 || max_val > 0.0, "Should have some signal");
    }
    #[test]
    fn test_formant_osc_vowel_selection() {
        let mut osc = FormantOsc::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 1.0); // Higher frequency

        // Different vowels should produce different timbres
        let mut sums_by_vowel = Vec::new();
        for vowel_cv in [0.0, 0.25, 0.5, 0.75, 1.0] {
            osc.reset();
            inputs.set(1, vowel_cv);

            let mut sum = 0.0;
            for _ in 0..500 {
                osc.tick(&inputs, &mut outputs);
                sum += outputs.get(10).unwrap().abs();
            }
            sums_by_vowel.push(sum);
        }

        // Different vowels should produce measurably different outputs
        // At least some pairs should be different
        let mut any_different = false;
        for i in 0..sums_by_vowel.len() - 1 {
            if (sums_by_vowel[i] - sums_by_vowel[i + 1]).abs() > 10.0 {
                any_different = true;
                break;
            }
        }
        assert!(any_different, "Vowels should produce different timbres");
    }
    #[test]
    fn test_formant_osc_formant_shift() {
        let mut osc = FormantOsc::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0);
        inputs.set(1, 0.5); // Middle vowel

        // No shift
        osc.reset();
        inputs.set(2, 0.0);
        let mut sum_no_shift = 0.0;
        for _ in 0..500 {
            osc.tick(&inputs, &mut outputs);
            sum_no_shift += outputs.get(10).unwrap();
        }

        // Positive shift (higher formants)
        osc.reset();
        inputs.set(2, 2.5);
        let mut sum_high_shift = 0.0;
        for _ in 0..500 {
            osc.tick(&inputs, &mut outputs);
            sum_high_shift += outputs.get(10).unwrap();
        }

        // Shift should change the output
        assert!(
            (sum_no_shift - sum_high_shift).abs() > 0.1,
            "Shift should affect output"
        );
    }
    #[test]
    fn test_formant_osc_vibrato() {
        let mut osc = FormantOsc::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0);
        inputs.set(1, 0.0);

        // With vibrato - check that vibrato_phase changes
        inputs.set(3, 1.0); // Full vibrato

        for _ in 0..1000 {
            osc.tick(&inputs, &mut outputs);
        }

        // Vibrato phase should have advanced
        assert!(osc.vibrato_phase > 0.0);
    }
    #[test]
    fn test_formant_osc_glottal_pulse() {
        // Test the glottal pulse function directly
        let opening = FormantOsc::glottal_pulse(0.0);
        let peak = FormantOsc::glottal_pulse(0.4);
        let closing = FormantOsc::glottal_pulse(0.6);
        let closed = FormantOsc::glottal_pulse(0.9);

        assert_eq!(opening, 0.0, "Should start at zero");
        assert!(peak > 0.9, "Peak should be near 1.0: {}", peak);
        assert!(
            closing > 0.0 && closing < peak,
            "Closing phase should be declining"
        );
        assert_eq!(closed, 0.0, "Closed phase should be zero");
    }
    #[test]
    fn test_formant_osc_frequency_tracking() {
        let mut osc = FormantOsc::new(44100.0);

        // Count positive-going zero crossings at different pitches
        let count_crossings = |osc: &mut FormantOsc, v_oct: f64| -> usize {
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(0, v_oct);
            osc.reset();

            let mut crossings = 0;
            let mut prev_phase = 0.0;
            for _ in 0..1000 {
                osc.tick(&inputs, &mut outputs);
                // Phase wraps indicate a new cycle
                if osc.phase < prev_phase {
                    crossings += 1;
                }
                prev_phase = osc.phase;
            }
            crossings
        };

        let crossings_c4 = count_crossings(&mut osc, 0.0);
        let crossings_c5 = count_crossings(&mut osc, 1.0);

        let ratio = crossings_c5 as f64 / crossings_c4 as f64;
        assert!(
            ratio > 1.7 && ratio < 2.3,
            "Octave ratio should be ~2: {}",
            ratio
        );
    }
    #[test]
    fn test_vco_output_bounded() {
        // VCO outputs should always be in safe range
        let mut vco = Vco::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test various pitches
        for voct in [-2.0, 0.0, 2.0, 4.0] {
            inputs.set(0, voct);

            let max = measure_max_output(1000, || {
                vco.tick(&inputs, &mut outputs);
                let sin = outputs.get(10).unwrap_or(0.0).abs();
                let tri = outputs.get(11).unwrap_or(0.0).abs();
                let saw = outputs.get(12).unwrap_or(0.0).abs();
                let sqr = outputs.get(13).unwrap_or(0.0).abs();
                sin.max(tri).max(saw).max(sqr)
            });

            assert!(
                max <= 5.5, // VCO should output ±5V
                "VCO output {} exceeds expected range at voct={}",
                max,
                voct
            );
        }
    }
    #[test]
    fn test_lfo_output_bounded() {
        let mut lfo = Lfo::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 1.0); // 1Hz rate

        let max = measure_max_output(50000, || {
            lfo.tick(&inputs, &mut outputs);
            outputs.get(10).unwrap_or(0.0).abs()
        });

        assert!(max <= 5.5, "LFO output {} exceeds expected ±5V range", max);
    }
    #[test]
    fn test_noise_output_bounded() {
        let mut noise = NoiseGenerator::new();
        let inputs = PortValues::new();
        let mut outputs = PortValues::new();

        let max = measure_max_output(10000, || {
            noise.tick(&inputs, &mut outputs);
            let white = outputs.get(10).unwrap_or(0.0).abs();
            let pink = outputs.get(11).unwrap_or(0.0).abs();
            white.max(pink)
        });

        assert!(
            max <= 5.5,
            "Noise output {} exceeds expected ±5V range",
            max
        );
    }

    // ================================================================
    // Wave B remediation tests (Q000-Q007, Q129)
    // ================================================================

    /// Naive DFT magnitude at integer bin `k` over `sig`.
    fn dft_mag(sig: &[f64], k: usize) -> f64 {
        let n = sig.len();
        let mut re = 0.0;
        let mut im = 0.0;
        for (i, &s) in sig.iter().enumerate() {
            let ang = -TAU * (k as f64) * (i as f64) / (n as f64);
            re += s * Libm::<f64>::cos(ang);
            im += s * Libm::<f64>::sin(ang);
        }
        Libm::<f64>::sqrt(re * re + im * im) / (n as f64)
    }

    /// Sum of DFT magnitude over the non-harmonic bins (aliased energy). `fund`
    /// is the fundamental bin; harmonics are its integer multiples.
    fn alias_energy(sig: &[f64], fund: usize) -> f64 {
        let n = sig.len();
        let mut total = 0.0;
        for k in 1..(n / 2) {
            if k % fund != 0 {
                total += dft_mag(sig, k);
            }
        }
        total
    }

    /// Estimate the fundamental period (in samples) of `seg` via autocorrelation
    /// around an expected period, with parabolic sub-sample interpolation.
    fn measure_period(seg: &[f64], expected: f64) -> f64 {
        let autocorr = |lag: usize| -> f64 {
            let mut acc = 0.0;
            for i in 0..(seg.len() - lag) {
                acc += seg[i] * seg[i + lag];
            }
            acc
        };
        let lo = ((expected * 0.6) as usize).max(2);
        let hi = ((expected * 1.6) as usize).min(seg.len() / 2);
        let mut best_lag = lo;
        let mut best = f64::MIN;
        for lag in lo..hi {
            let a = autocorr(lag);
            if a > best {
                best = a;
                best_lag = lag;
            }
        }
        let y0 = autocorr(best_lag - 1);
        let y1 = autocorr(best_lag);
        let y2 = autocorr(best_lag + 1);
        let denom = y0 - 2.0 * y1 + y2;
        let delta = if denom.abs() > 1e-12 {
            0.5 * (y0 - y2) / denom
        } else {
            0.0
        };
        best_lag as f64 + delta
    }

    /// Collect one Vco output port for `n` samples at pitch `voct`.
    fn vco_capture(voct: f64, port: u32, n: usize) -> Vec<f64> {
        let mut vco = Vco::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, voct);
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            vco.tick(&inputs, &mut outputs);
            out.push(outputs.get(port).unwrap());
        }
        out
    }

    // ---- Q000: Vco anti-aliasing ----

    #[test]
    fn test_vco_saw_frequency_preserved() {
        // The band-limited saw must still track pitch: ~10 periods of C4.
        let saw = vco_capture(0.0, 12, (44100.0 / 261.63) as usize * 10);
        let crossings = saw.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!(
            (8..=12).contains(&crossings),
            "expected ~10 zero crossings, got {}",
            crossings
        );
    }

    #[test]
    fn test_vco_saw_aliasing_reduced() {
        // 4200 Hz lands exactly on DFT bin 42 for N=441 at 44.1k.
        let voct = Libm::<f64>::log2(4200.0 / voct_to_hz(0.0));
        let n = 441;
        let fund = 42;
        let dt = voct_to_hz(voct) / 44100.0;
        let saw_bl = vco_capture(voct, 12, n);
        // Naive reference from a phase-aligned accumulator.
        let mut ph = 0.0;
        let mut saw_naive = Vec::with_capacity(n);
        for _ in 0..n {
            saw_naive.push((2.0 * ph - 1.0) * 5.0);
            ph += dt;
            ph -= Libm::<f64>::floor(ph);
        }
        let a_bl = alias_energy(&saw_bl, fund);
        let a_naive = alias_energy(&saw_naive, fund);
        assert!(
            a_bl < 0.3 * a_naive,
            "saw alias energy not reduced: bl={} naive={}",
            a_bl,
            a_naive
        );
    }

    #[test]
    fn test_vco_square_aliasing_reduced() {
        let voct = Libm::<f64>::log2(4200.0 / voct_to_hz(0.0));
        let n = 441;
        let fund = 42;
        let dt = voct_to_hz(voct) / 44100.0;
        let sqr_bl = vco_capture(voct, 13, n);
        let mut ph = 0.0;
        let mut sqr_naive = Vec::with_capacity(n);
        for _ in 0..n {
            sqr_naive.push(if ph < 0.5 { 5.0 } else { -5.0 });
            ph += dt;
            ph -= Libm::<f64>::floor(ph);
        }
        let a_bl = alias_energy(&sqr_bl, fund);
        let a_naive = alias_energy(&sqr_naive, fund);
        assert!(
            a_bl < 0.3 * a_naive,
            "square alias energy not reduced: bl={} naive={}",
            a_bl,
            a_naive
        );
        // Time-domain edge smoothing: max sample-to-sample step must shrink.
        let max_delta = |v: &[f64]| {
            v.windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0, f64::max)
        };
        assert!(max_delta(&sqr_bl) < max_delta(&sqr_naive));
    }

    #[test]
    fn test_vco_triangle_aliasing_reduced() {
        // Triangle: max-delta is unchanged by corner rounding, so use the DFT.
        let voct = Libm::<f64>::log2(4200.0 / voct_to_hz(0.0));
        let n = 441;
        let fund = 42;
        let dt = voct_to_hz(voct) / 44100.0;
        let tri_bl = vco_capture(voct, 11, n);
        let mut ph = 0.0;
        let mut tri_naive = Vec::with_capacity(n);
        for _ in 0..n {
            tri_naive.push((1.0 - 4.0 * Libm::<f64>::fabs(ph - 0.5)) * 5.0);
            ph += dt;
            ph -= Libm::<f64>::floor(ph);
        }
        let a_bl = alias_energy(&tri_bl, fund);
        let a_naive = alias_energy(&tri_naive, fund);
        assert!(
            a_bl < 0.5 * a_naive,
            "triangle alias energy not reduced: bl={} naive={}",
            a_bl,
            a_naive
        );
    }

    #[test]
    fn test_vco_hard_sync_bounded_and_reduces_step() {
        // Drive hard sync from a master and confirm the reset step is bandlimited
        // (smaller max delta than a naive resetting saw) while staying bounded.
        let mut vco = Vco::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 2.0); // slave pitch
        let master_dt = 110.0 / 44100.0;
        let mut mp = 0.0;
        let mut out = Vec::new();
        let mut max_abs = 0.0f64;
        for _ in 0..4000 {
            let sync = if mp < 0.5 { 5.0 } else { 0.0 };
            inputs.set(3, sync);
            vco.tick(&inputs, &mut outputs);
            let saw = outputs.get(12).unwrap();
            max_abs = max_abs.max(saw.abs());
            out.push(saw);
            mp += master_dt;
            if mp >= 1.0 {
                mp -= 1.0;
            }
        }
        assert!(max_abs <= 5.5, "hard-sync saw exceeded ±5V: {}", max_abs);
        // Must still be producing a signal.
        let rms = (out.iter().map(|x| x * x).sum::<f64>() / out.len() as f64).sqrt();
        assert!(rms > 1.0, "hard-sync output too quiet: rms={}", rms);
    }

    // ---- Q007: Vco linear (through-zero) FM ----

    #[test]
    fn test_vco_has_fm_lin_port() {
        let vco = Vco::new(44100.0);
        assert_eq!(vco.port_spec().inputs.len(), 5);
        let fm_lin = vco.port_spec().inputs.iter().find(|p| p.name == "fm_lin");
        assert!(fm_lin.is_some(), "fm_lin input port missing");
        assert_eq!(fm_lin.unwrap().id, 4);
    }

    #[test]
    fn test_vco_fm_lin_zero_is_noop() {
        // fm_lin = 0 must produce identical output to leaving it unpatched.
        let mut a = Vco::new(44100.0);
        let mut b = Vco::new(44100.0);
        let mut ia = PortValues::new();
        let mut ib = PortValues::new();
        let mut oa = PortValues::new();
        let mut ob = PortValues::new();
        ia.set(0, 1.0);
        ib.set(0, 1.0);
        ib.set(4, 0.0); // explicit zero linear FM
        for _ in 0..500 {
            a.tick(&ia, &mut oa);
            b.tick(&ib, &mut ob);
            assert_eq!(oa.get(12).unwrap(), ob.get(12).unwrap());
        }
    }

    #[test]
    fn test_vco_fm_lin_through_zero_symmetric() {
        // Through-zero linear FM of a sine carrier yields a symmetric spectrum,
        // so the time-domain mean stays near zero and the output stays bounded.
        let mut vco = Vco::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.0); // carrier at C4
        let mod_dt = 200.0 / 44100.0; // 200 Hz modulator
        let mut mphase = 0.0;
        let mut sum = 0.0;
        let mut max_abs = 0.0f64;
        let n = 44100;
        for _ in 0..n {
            let m = Libm::<f64>::sin(mphase * TAU) * 5.0; // ±5V -> ±100% depth
            inputs.set(4, m);
            vco.tick(&inputs, &mut outputs);
            let sine = outputs.get(10).unwrap();
            sum += sine;
            max_abs = max_abs.max(sine.abs());
            mphase += mod_dt;
            if mphase >= 1.0 {
                mphase -= 1.0;
            }
        }
        let mean = sum / n as f64;
        assert!(
            mean.abs() < 0.2,
            "FM sidebands not symmetric: mean={}",
            mean
        );
        assert!(max_abs <= 5.5, "FM output exceeded range: {}", max_abs);
    }

    // ---- Q001 / Q006: Supersaw sub oscillator & center reuse ----

    #[test]
    fn test_supersaw_sub_is_octave_down_zero_mean() {
        let mut ss = Supersaw::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.0); // C4
        let base_freq = voct_to_hz(0.0);
        let n = 44100 * 2;
        let mut sub = Vec::with_capacity(n);
        for _ in 0..n {
            ss.tick(&inputs, &mut outputs);
            sub.push(outputs.get(11).unwrap());
        }
        // The sub is a clean saw; its rising zero-crossing rate is its frequency.
        // (The full supersaw main output has 7 detuned voices, so it is not a
        // clean once-per-period reference — compare against the fundamental.)
        let cross = |v: &[f64]| v.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        let sub_rate = cross(&sub) as f64 / (n as f64 / 44100.0);
        let expected = base_freq / 2.0;
        assert!(
            (sub_rate - expected).abs() < 0.05 * expected,
            "sub should ring an octave down (~{} Hz), measured {} Hz",
            expected,
            sub_rate
        );
        let mean = sub.iter().sum::<f64>() / sub.len() as f64;
        assert!(mean.abs() < 0.05, "sub should be zero-mean: mean={}", mean);
    }

    #[test]
    fn test_supersaw_mix_zero_equals_blepped_center() {
        // With mix=0 the output must equal the band-limited center voice, not a
        // naive ramp. Replicate the center voice with the same PolyBLEP.
        let mut ss = Supersaw::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.5); // arbitrary pitch
        inputs.set(2, 0.0); // mix = 0 -> pure center voice
        let base_freq = voct_to_hz(0.5);
        let dt = base_freq / 44100.0; // center detune ratio is 0.0
        let mut ph = 3.0 / 7.0; // center oscillator initial phase
        for _ in 0..500 {
            ss.tick(&inputs, &mut outputs);
            let expected = (2.0 * ph - 1.0) - polyblep(ph, dt);
            let got = outputs.get(10).unwrap();
            assert!(
                (got - expected).abs() < 1e-9,
                "mix=0 output {} != blepped center {}",
                got,
                expected
            );
            ph += dt;
            if ph >= 1.0 {
                ph -= 1.0;
            }
        }
    }

    // ---- Q002 / Q129: KarplusStrong rising-edge excitation ----

    #[test]
    fn test_ks_excites_once_per_gate() {
        let mut ks = KarplusStrong::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.0); // C4
        inputs.set(2, 0.95); // high damping -> rings
        inputs.set(3, 0.5); // brightness

        // 100-sample 5V gate.
        let mut ring = Vec::new();
        for i in 0..100 {
            inputs.set(1, 5.0);
            ks.tick(&inputs, &mut outputs);
            if i >= 10 {
                ring.push(outputs.get(10).unwrap());
            }
        }
        // Excited exactly once: write_pos advanced ~100 (once-excite resets to 0
        // then advances each sample). If it re-excited every sample it would be
        // stuck at 1.
        assert_eq!(
            ks.write_pos, 100,
            "gate should excite once; write_pos={}",
            ks.write_pos
        );
        // The string must ring DURING the held gate (not silent, not renoised).
        let rms = (ring.iter().map(|x| x * x).sum::<f64>() / ring.len() as f64).sqrt();
        assert!(rms > 0.05, "string did not ring during gate: rms={}", rms);
    }

    #[test]
    fn test_ks_gate_high_threshold() {
        // A sub-threshold trigger (below GATE_THRESHOLD_V) must NOT excite.
        let mut ks = KarplusStrong::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.0);
        inputs.set(1, 1.0); // 1V < 2.5V threshold
        for _ in 0..200 {
            ks.tick(&inputs, &mut outputs);
        }
        let out = outputs.get(10).unwrap();
        assert_eq!(out, 0.0, "sub-threshold trigger should not excite: {}", out);
    }

    // ---- Q003: KarplusStrong tuning accuracy ----

    #[test]
    fn test_ks_tuning_accuracy() {
        for &(voct, target_hz) in &[(-1.0, 130.81), (0.0, 261.63), (1.0, 523.25), (2.0, 1046.5)] {
            let mut ks = KarplusStrong::new(44100.0);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(0, voct);
            inputs.set(2, 0.95); // bright, slow decay
            inputs.set(3, 0.5);
            // Pluck once.
            inputs.set(1, 5.0);
            ks.tick(&inputs, &mut outputs);
            inputs.set(1, 0.0);
            let mut out = Vec::with_capacity(12000);
            for _ in 0..12000 {
                ks.tick(&inputs, &mut outputs);
                out.push(outputs.get(10).unwrap());
            }
            let seg = &out[2000..10000];
            let expected_period = 44100.0 / target_hz;
            let period = measure_period(seg, expected_period);
            let measured_hz = 44100.0 / period;
            let cents = 1200.0 * Libm::<f64>::log2(measured_hz / target_hz);
            assert!(
                cents.abs() < 20.0,
                "KS pitch off at {} Hz: measured {} Hz ({:+.1} cents)",
                target_hz,
                measured_hz,
                cents
            );
        }
    }

    #[test]
    fn test_ks_high_then_low_pitch_same_instance() {
        // Regression: a high-note pluck shrinks the delay buffer. A later, lower
        // note on the SAME instance must still tune correctly, because the
        // requested period is clamped against the full buffer capacity (max_len)
        // and the buffer grows back on pluck — not clamped to the shrunken
        // high-note length (which would pin the low note to the wrong pitch).
        let mut ks = KarplusStrong::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(2, 0.95); // bright, slow decay
        inputs.set(3, 0.5);

        // Pluck a high note (C6, ~1046 Hz) and let it ring — this shrinks buffer.
        inputs.set(0, 2.0);
        inputs.set(1, 5.0);
        ks.tick(&inputs, &mut outputs);
        inputs.set(1, 0.0);
        for _ in 0..4000 {
            ks.tick(&inputs, &mut outputs);
        }

        // Now pluck a low note (C2, ~65.4 Hz) on the SAME instance.
        let target_hz = 65.41;
        inputs.set(0, -2.0);
        inputs.set(1, 5.0);
        ks.tick(&inputs, &mut outputs);
        inputs.set(1, 0.0);
        let mut out = Vec::with_capacity(12000);
        for _ in 0..12000 {
            ks.tick(&inputs, &mut outputs);
            out.push(outputs.get(10).unwrap());
        }
        let seg = &out[2000..10000];
        let expected_period = 44100.0 / target_hz;
        let period = measure_period(seg, expected_period);
        let measured_hz = 44100.0 / period;
        let cents = 1200.0 * Libm::<f64>::log2(measured_hz / target_hz);
        assert!(
            cents.abs() < 50.0,
            "KS low note after high pluck mistuned: measured {} Hz \
             (target {} Hz, {:+.1} cents)",
            measured_hz,
            target_hz,
            cents
        );
    }

    // ---- Q004: KarplusStrong DC decay ----

    #[test]
    fn test_ks_dc_decays() {
        // brightness=0 makes the excitation a purely positive impulse (maximum
        // DC bias in the old code). After the fix the running mean decays to ~0.
        let mut ks = KarplusStrong::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.0);
        inputs.set(2, 0.7);
        inputs.set(3, 0.0); // brightness 0 -> impulse (positive DC in old code)
        inputs.set(1, 5.0);
        ks.tick(&inputs, &mut outputs);
        inputs.set(1, 0.0);
        let mut out = Vec::with_capacity(20000);
        for _ in 0..20000 {
            ks.tick(&inputs, &mut outputs);
            out.push(outputs.get(10).unwrap());
        }
        let mean_window = |s: &[f64]| s.iter().sum::<f64>() / s.len() as f64;
        let late = mean_window(&out[10000..20000]);
        assert!(
            late.abs() < 0.02,
            "KS output retains DC offset: late mean = {}",
            late
        );
    }

    // ---- Q005: Wavetable mipmapping ----

    #[test]
    fn test_wavetable_mip_keeps_harmonics_below_nyquist() {
        let fs = 44100.0;
        // At a high fundamental the selected saw level must keep every harmonic
        // below Nyquist.
        for &freq in &[1000.0, 2000.0, 3000.0, 6000.0] {
            let phase_inc = freq / fs;
            let level = Wavetable::select_level(2, phase_inc); // saw
            let harmonics = Wavetable::max_harmonic(2, level);
            let top = harmonics as f64 * freq;
            assert!(
                top < fs / 2.0,
                "saw at {} Hz: level {} keeps {} harmonics, top partial {} >= Nyquist",
                freq,
                level,
                harmonics,
                top
            );
        }
    }

    #[test]
    fn test_wavetable_mip_selects_higher_level_for_higher_pitch() {
        // Level (harmonic reduction) must increase monotonically with pitch.
        let fs = 44100.0;
        let l_low = Wavetable::select_level(2, 100.0 / fs);
        let l_mid = Wavetable::select_level(2, 1000.0 / fs);
        let l_high = Wavetable::select_level(2, 5000.0 / fs);
        assert!(l_low <= l_mid && l_mid <= l_high);
        assert!(l_high > l_low, "expected higher pitch to raise mip level");
    }

    #[test]
    fn test_wavetable_high_pitch_bounded() {
        // High-pitch saw stays bounded and non-silent with mipmapping active.
        let mut wt = Wavetable::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 3.5); // ~2960 Hz
        inputs.set(1, 2.0 / 7.0); // saw table
        let mut max_abs = 0.0f64;
        let mut sumsq = 0.0;
        let n = 4000;
        for _ in 0..n {
            wt.tick(&inputs, &mut outputs);
            let v = outputs.get(10).unwrap();
            max_abs = max_abs.max(v.abs());
            sumsq += v * v;
        }
        assert!(
            max_abs <= 5.5,
            "wavetable high-pitch exceeded range: {}",
            max_abs
        );
        assert!(
            (sumsq / n as f64).sqrt() > 0.5,
            "wavetable high-pitch silent"
        );
    }

    // ---- Q157: Supersaw detune spread ----

    /// Peak-to-peak of the per-block RMS envelope of `sig` (block size `block`).
    /// A single periodic tone gives a near-flat envelope; detuned voices beat
    /// against each other and make it fluctuate.
    fn block_rms_ptp(sig: &[f64], block: usize) -> f64 {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for chunk in sig.chunks(block) {
            let rms = (chunk.iter().map(|x| x * x).sum::<f64>() / chunk.len() as f64).sqrt();
            lo = lo.min(rms);
            hi = hi.max(rms);
        }
        hi - lo
    }

    #[test]
    fn test_supersaw_detune_spread() {
        let run = |detune: f64| -> Vec<f64> {
            let mut ss = Supersaw::new(44100.0);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(0, 0.0); // C4
            inputs.set(1, detune);
            inputs.set(2, 1.0); // full supersaw mix
            let mut out = Vec::with_capacity(20_000);
            for _ in 0..20_000 {
                ss.tick(&inputs, &mut outputs);
                out.push(outputs.get(10).unwrap());
            }
            out
        };

        let ptp_off = block_rms_ptp(&run(0.0), 500);
        let ptp_on = block_rms_ptp(&run(1.0), 500);
        // With zero detune all seven voices share one frequency, so the summed
        // waveform is periodic and its RMS envelope is essentially flat.
        assert!(
            ptp_off < 0.02,
            "no-detune supersaw should not beat: ptp={ptp_off}"
        );
        // Detune spreads the voices apart; their beating modulates the RMS.
        assert!(
            ptp_on > ptp_off + 0.03,
            "detuned supersaw must beat (spread the voices): on={ptp_on} off={ptp_off}"
        );
    }

    #[test]
    fn test_supersaw_reset_and_sample_rate() {
        let mut ss = Supersaw::default();
        assert_eq!(ss.type_id(), "supersaw");
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.0);
        for _ in 0..500 {
            ss.tick(&inputs, &mut outputs);
        }
        assert!(ss.sub_phase != 0.0 || ss.phases[3] != 3.0 / 7.0);
        ss.reset();
        assert_eq!(ss.sub_phase, 0.0);
        for (i, &p) in ss.phases.iter().enumerate() {
            assert_eq!(p, i as f64 / 7.0);
        }
        ss.set_sample_rate(48000.0);
        assert_eq!(ss.sample_rate, 48000.0);
        ss.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap().is_finite());
    }

    // ---- Q157: KarplusStrong reset + sample-rate ----

    #[test]
    fn test_karplus_strong_reset_and_sample_rate() {
        let mut ks = KarplusStrong::default();
        assert_eq!(ks.type_id(), "karplus_strong");
        assert_eq!(ks.sample_rate, 44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.0);
        inputs.set(1, 5.0); // pluck
        for _ in 0..500 {
            ks.tick(&inputs, &mut outputs);
        }
        assert!(ks.write_pos != 0);
        ks.reset();
        assert_eq!(ks.write_pos, 0);
        assert_eq!(ks.last_output, 0.0);
        assert!(ks.buffer.iter().all(|&x| x == 0.0));
        // Reallocation on sample-rate change must not panic and stays finite.
        ks.set_sample_rate(48000.0);
        assert_eq!(ks.sample_rate, 48000.0);
        for _ in 0..100 {
            ks.tick(&inputs, &mut outputs);
            assert!(outputs.get(10).unwrap().is_finite());
        }
    }
}
