//! Filter modules.

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
/// Phase 3 additions:
/// - Self-oscillation at high resonance values
/// - Keyboard tracking for filter-follows-pitch
pub struct Svf {
    low: f64,
    band: f64,
    sample_rate: f64,
    spec: PortSpec,
}

impl Svf {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            low: 0.0,
            band: 0.0,
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
        let input = inputs.get_or(0, 0.0);
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

        let f = 2.0 * Libm::<f64>::sin(PI * cutoff_hz / self.sample_rate);
        let f = Libm::<f64>::fmin(f, 0.99); // Prevent instability

        // Phase 3: Self-oscillation at high resonance
        // When res > 0.95, allow Q to go below zero for self-oscillation
        let q = if res > 0.95 {
            // Self-oscillation zone: Q becomes negative, causing oscillation
            let osc_amount = (res - 0.95) / 0.05; // 0 to 1 in the 0.95-1.0 range
            0.1 - osc_amount * 0.15 // Goes from 0.1 to -0.05
        } else {
            1.0 - res * 0.9 // Normal resonance: higher res = lower damping
        };

        // SVF topology with self-oscillation support
        let high = input - self.low - q * self.band;
        self.band += f * high;
        self.low += f * self.band;
        let notch = high + self.low;

        // Safety soft-clipping function: smooth limiting at ±limit volts
        // Uses tanh for gradual saturation, preserving sound quality
        #[inline]
        fn safe_clip(x: f64, limit: f64) -> f64 {
            if x.abs() <= limit {
                x
            } else {
                // Soft clip: asymptotic approach to limit
                limit * Libm::<f64>::tanh(x / limit)
            }
        }

        // Apply different clipping thresholds based on resonance
        // High resonance (self-oscillation): clip at ±5V to prevent runaway
        // Normal operation: clip at ±10V as safety net
        let clip_limit = if res > 0.95 { 5.0 } else { 10.0 };

        outputs.set(10, safe_clip(self.low, clip_limit)); // LP
        outputs.set(11, safe_clip(self.band, clip_limit)); // BP
        outputs.set(12, safe_clip(high, clip_limit)); // HP
        outputs.set(13, safe_clip(notch, clip_limit)); // Notch
    }

    fn reset(&mut self) {
        self.low = 0.0;
        self.band = 0.0;
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
        let input = inputs.get_or(0, 0.0);
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

        // Calculate filter coefficient (using bilinear transform approximation)
        let wc = PI * cutoff_hz / self.sample_rate;
        let g = Libm::<f64>::tan(wc);
        let g1 = g / (1.0 + g);

        // Resonance with self-oscillation capability
        // k = 4 for self-oscillation in 4-pole ladder
        let k = res * 4.0;

        // Drive amount for input saturation
        let drive_gain = 1.0 + drive * 3.0;

        // Apply input drive
        let input_driven = Self::diode_sat(input / 5.0 * drive_gain) * 5.0;

        // Feedback with saturation
        let fb = Self::diode_sat(self.feedback * k);

        // Input with resonance feedback subtracted
        let u = input_driven - fb * 5.0;

        // 4-pole ladder with diode saturation at each stage
        let s1 = self.stages[0] + g1 * (Self::diode_sat(u / 5.0) * 5.0 - self.stages[0]);
        let s2 = self.stages[1] + g1 * (Self::diode_sat(s1 / 5.0) * 5.0 - self.stages[1]);
        let s3 = self.stages[2] + g1 * (Self::diode_sat(s2 / 5.0) * 5.0 - self.stages[2]);
        let s4 = self.stages[3] + g1 * (Self::diode_sat(s3 / 5.0) * 5.0 - self.stages[3]);

        // Update state
        self.stages[0] = s1;
        self.stages[1] = s2;
        self.stages[2] = s3;
        self.stages[3] = s4;
        self.feedback = s4 / 5.0;

        // Outputs (all normalized to ±5V range)
        outputs.set(10, s4); // 24dB/oct (main output)
        outputs.set(11, s1); // 6dB/oct
        outputs.set(12, s2); // 12dB/oct
        outputs.set(13, s3); // 18dB/oct
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
    sample_rate: f64,
    spec: PortSpec,
}

impl ParametricEq {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            low_state: [0.0; 2],
            mid_state: [0.0; 2],
            high_state: [0.0; 2],
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
        let input = inputs.get_or(0, 0.0);

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

        // Calculate biquad coefficients
        let low_coefs = Self::calc_low_shelf(low_freq, low_gain_db, self.sample_rate);
        let mid_coefs = Self::calc_peaking(mid_freq, mid_gain_db, mid_q, self.sample_rate);
        let high_coefs = Self::calc_high_shelf(high_freq, high_gain_db, self.sample_rate);

        // Process through the cascade
        let mut signal = input;
        signal = Self::process_biquad(signal, &low_coefs, &mut self.low_state);
        signal = Self::process_biquad(signal, &mid_coefs, &mut self.mid_state);
        signal = Self::process_biquad(signal, &high_coefs, &mut self.high_state);

        outputs.set(10, signal);
    }

    fn reset(&mut self) {
        self.low_state = [0.0; 2];
        self.mid_state = [0.0; 2];
        self.high_state = [0.0; 2];
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
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
        assert!(svf.low == 0.0);

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
}
