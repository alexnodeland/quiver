//! Envelope, amplifier, and dynamics modules.

use super::common::{db_to_gain, env_coef, gain_to_db, GATE_HIGH_V, GATE_THRESHOLD_V};
use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use alloc::vec;
use libm::Libm;

/// ADSR stage enumeration
#[derive(Debug, Clone, Copy, PartialEq)]
enum AdsrStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// ADSR Envelope Generator
///
/// A classic Attack-Decay-Sustain-Release envelope with gate and retrigger inputs.
/// Outputs normal and inverted envelope signals, plus end-of-cycle trigger.
pub struct Adsr {
    stage: AdsrStage,
    level: f64,
    sample_rate: f64,
    last_gate: f64,
    last_retrig: f64,
    spec: PortSpec,
}

impl Adsr {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            stage: AdsrStage::Idle,
            level: 0.0,
            sample_rate,
            last_gate: 0.0,
            last_retrig: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "gate", SignalKind::Gate),
                    PortDef::new(1, "retrig", SignalKind::Trigger),
                    PortDef::new(2, "attack", SignalKind::CvUnipolar)
                        .with_default(0.1)
                        .with_attenuverter(),
                    PortDef::new(3, "decay", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(4, "sustain", SignalKind::CvUnipolar)
                        .with_default(0.7)
                        .with_attenuverter(),
                    PortDef::new(5, "release", SignalKind::CvUnipolar)
                        .with_default(0.4)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "env", SignalKind::CvUnipolar),
                    PortDef::new(11, "inv", SignalKind::CvUnipolar),
                    PortDef::new(12, "eoc", SignalKind::Trigger),
                ],
            },
        }
    }

    fn cv_to_time(&self, cv: f64) -> f64 {
        // Map 0-1 CV to 1ms - 10s (exponential)
        0.001 * Libm::<f64>::pow(10000.0, cv.clamp(0.0, 1.0))
    }
}

impl Default for Adsr {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Adsr {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let gate = inputs.get_or(0, 0.0);
        let retrig = inputs.get_or(1, 0.0);
        let attack_time = self.cv_to_time(inputs.get_or(2, 0.1));
        let decay_time = self.cv_to_time(inputs.get_or(3, 0.3));
        let sustain_level = inputs.get_or(4, 0.7).clamp(0.0, 1.0);
        let release_time = self.cv_to_time(inputs.get_or(5, 0.4));

        let gate_high = gate > GATE_THRESHOLD_V;
        let gate_rising = gate_high && self.last_gate <= GATE_THRESHOLD_V;
        let gate_falling = !gate_high && self.last_gate > GATE_THRESHOLD_V;
        let retrig_rising = retrig > GATE_THRESHOLD_V && self.last_retrig <= GATE_THRESHOLD_V;

        // State transitions
        if gate_rising || (retrig_rising && gate_high) {
            self.stage = AdsrStage::Attack;
        } else if gate_falling && self.stage != AdsrStage::Idle {
            self.stage = AdsrStage::Release;
        }

        // Calculate rates
        let attack_rate = 1.0 / (attack_time * self.sample_rate);
        let decay_rate = 1.0 / (decay_time * self.sample_rate);
        let release_rate = 1.0 / (release_time * self.sample_rate);

        // Process current stage
        let mut eoc = 0.0;
        match self.stage {
            AdsrStage::Idle => {
                self.level = 0.0;
            }
            AdsrStage::Attack => {
                self.level += attack_rate;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay => {
                self.level -= decay_rate;
                if self.level <= sustain_level {
                    self.level = sustain_level;
                    self.stage = AdsrStage::Sustain;
                }
            }
            AdsrStage::Sustain => {
                self.level = sustain_level;
            }
            AdsrStage::Release => {
                self.level -= release_rate;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = AdsrStage::Idle;
                    eoc = GATE_HIGH_V; // End-of-cycle trigger
                }
            }
        }

        self.last_gate = gate;
        self.last_retrig = retrig;

        // Output scaled to standard modular levels
        outputs.set(10, self.level * 10.0); // 0-10V unipolar
        outputs.set(11, (1.0 - self.level) * 10.0); // Inverted
        outputs.set(12, eoc);
    }

    fn reset(&mut self) {
        self.stage = AdsrStage::Idle;
        self.level = 0.0;
        self.last_gate = 0.0;
        self.last_retrig = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "adsr"
    }
}

/// Voltage-Controlled Amplifier (VCA)
///
/// A simple amplifier with CV control. Useful for amplitude modulation.
pub struct Vca {
    spec: PortSpec,
}

impl Vca {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "cv", SignalKind::CvUnipolar)
                        .with_default(10.0)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }
}

impl Default for Vca {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for Vca {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let cv = inputs.get_or(1, 10.0).clamp(0.0, 10.0) / 10.0;
        outputs.set(10, input * cv);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "vca"
    }
}

/// Limiter
///
/// A dynamics processor that prevents signals from exceeding a threshold.
/// Supports both hard and soft limiting modes.
pub struct Limiter {
    sample_rate: f64,
    envelope: f64,
    spec: PortSpec,
}

impl Limiter {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            envelope: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "threshold", SignalKind::CvUnipolar)
                        .with_default(0.8)
                        .with_attenuverter(),
                    PortDef::new(2, "release", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(3, "soft", SignalKind::Gate).with_default(5.0),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "gr", SignalKind::CvUnipolar),
                ],
            },
        }
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Limiter {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let threshold = inputs.get_or(1, 0.8).clamp(0.01, 1.0) * 5.0;
        let release_cv = inputs.get_or(2, 0.3).clamp(0.0, 1.0);
        let soft_mode = inputs.get_or(3, 5.0) > GATE_THRESHOLD_V;

        let release_ms = 10.0 + release_cv * 990.0;
        let release_coef = env_coef(release_ms / 1000.0, self.sample_rate);

        let abs_input = Libm::<f64>::fabs(input);

        if abs_input > self.envelope {
            self.envelope = abs_input;
        } else {
            self.envelope = release_coef * self.envelope + (1.0 - release_coef) * abs_input;
        }

        let gain = if self.envelope > threshold {
            if soft_mode {
                let over = self.envelope / threshold;
                threshold / self.envelope * Libm::<f64>::tanh(over - 1.0) + 1.0 / over
            } else {
                threshold / self.envelope
            }
        } else {
            1.0
        };

        outputs.set(10, input * gain);
        outputs.set(11, (1.0 - gain) * 10.0);
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "limiter"
    }
}

/// Noise Gate
///
/// A dynamics processor that attenuates signals below a threshold.
pub struct NoiseGate {
    sample_rate: f64,
    envelope: f64,
    gate_state: f64,
    spec: PortSpec,
}

impl NoiseGate {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            envelope: 0.0,
            gate_state: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "threshold", SignalKind::CvUnipolar)
                        .with_default(0.1)
                        .with_attenuverter(),
                    PortDef::new(2, "attack", SignalKind::CvUnipolar)
                        .with_default(0.1)
                        .with_attenuverter(),
                    PortDef::new(3, "release", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(4, "range", SignalKind::CvUnipolar)
                        .with_default(1.0)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "gate", SignalKind::Gate),
                ],
            },
        }
    }
}

impl Default for NoiseGate {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for NoiseGate {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let threshold = inputs.get_or(1, 0.1).clamp(0.0, 1.0) * 5.0;
        let attack_cv = inputs.get_or(2, 0.1).clamp(0.0, 1.0);
        let release_cv = inputs.get_or(3, 0.3).clamp(0.0, 1.0);
        let range = inputs.get_or(4, 1.0).clamp(0.0, 1.0);

        let attack_ms = 0.1 + attack_cv * 49.9;
        let release_ms = 10.0 + release_cv * 490.0;
        let attack_coef = env_coef(attack_ms / 1000.0, self.sample_rate);
        let release_coef = env_coef(release_ms / 1000.0, self.sample_rate);

        let abs_input = Libm::<f64>::fabs(input);
        if abs_input > self.envelope {
            self.envelope = attack_coef * self.envelope + (1.0 - attack_coef) * abs_input;
        } else {
            self.envelope = release_coef * self.envelope + (1.0 - release_coef) * abs_input;
        }

        let open_threshold = threshold;
        let close_threshold = threshold * 0.7;

        if self.envelope > open_threshold {
            self.gate_state = attack_coef * self.gate_state + (1.0 - attack_coef) * 1.0;
        } else if self.envelope < close_threshold {
            self.gate_state *= release_coef;
        }

        let gain = (1.0 - range) + range * self.gate_state;
        outputs.set(10, input * gain);
        outputs.set(
            11,
            if self.gate_state > 0.5 {
                GATE_HIGH_V
            } else {
                0.0
            },
        );
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
        self.gate_state = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "noise_gate"
    }
}

/// Compressor
///
/// A dynamics processor that reduces the dynamic range of audio signals.
pub struct Compressor {
    sample_rate: f64,
    envelope: f64,
    spec: PortSpec,
}

impl Compressor {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            envelope: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "threshold", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "ratio", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(3, "attack", SignalKind::CvUnipolar)
                        .with_default(0.2)
                        .with_attenuverter(),
                    PortDef::new(4, "release", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(5, "makeup", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(6, "sidechain", SignalKind::Audio),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "gr", SignalKind::CvUnipolar),
                ],
            },
        }
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Compressor {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let threshold_cv = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
        let ratio_cv = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let attack_cv = inputs.get_or(3, 0.2).clamp(0.0, 1.0);
        let release_cv = inputs.get_or(4, 0.3).clamp(0.0, 1.0);
        let makeup_cv = inputs.get_or(5, 0.0).clamp(0.0, 1.0);
        let sidechain = inputs.get_or(6, input);

        let threshold = threshold_cv * 5.0;
        let ratio = 1.0 + ratio_cv * 19.0;
        let attack_ms = 0.1 + attack_cv * 99.9;
        let release_ms = 10.0 + release_cv * 990.0;
        let makeup_gain = 1.0 + makeup_cv * 3.0;

        let attack_coef = env_coef(attack_ms / 1000.0, self.sample_rate);
        let release_coef = env_coef(release_ms / 1000.0, self.sample_rate);

        let abs_sidechain = Libm::<f64>::fabs(sidechain);
        if abs_sidechain > self.envelope {
            self.envelope = attack_coef * self.envelope + (1.0 - attack_coef) * abs_sidechain;
        } else {
            self.envelope = release_coef * self.envelope + (1.0 - release_coef) * abs_sidechain;
        }

        let gain = if self.envelope > threshold && threshold > 0.0 {
            let over_db = gain_to_db(self.envelope / threshold);
            let compressed_db = over_db / ratio;
            let gain_reduction_db = over_db - compressed_db;
            db_to_gain(-gain_reduction_db)
        } else {
            1.0
        };

        outputs.set(10, input * gain * makeup_gain);
        outputs.set(11, (1.0 - gain) * 10.0);
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "compressor"
    }
}

/// Envelope Follower
///
/// Extracts the amplitude envelope from an audio signal.
pub struct EnvelopeFollower {
    sample_rate: f64,
    envelope: f64,
    spec: PortSpec,
}

impl EnvelopeFollower {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            envelope: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "attack", SignalKind::CvUnipolar)
                        .with_default(0.2)
                        .with_attenuverter(),
                    PortDef::new(2, "release", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(3, "gain", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::CvUnipolar),
                    PortDef::new(11, "inv", SignalKind::CvUnipolar),
                ],
            },
        }
    }
}

impl Default for EnvelopeFollower {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for EnvelopeFollower {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let attack_cv = inputs.get_or(1, 0.2).clamp(0.0, 1.0);
        let release_cv = inputs.get_or(2, 0.3).clamp(0.0, 1.0);
        let gain = inputs.get_or(3, 0.5).clamp(0.0, 1.0) * 4.0;

        let attack_ms = 0.1 + attack_cv * 99.9;
        let release_ms = 1.0 + release_cv * 999.0;
        let attack_coef = env_coef(attack_ms / 1000.0, self.sample_rate);
        let release_coef = env_coef(release_ms / 1000.0, self.sample_rate);

        let abs_input = Libm::<f64>::fabs(input);
        if abs_input > self.envelope {
            self.envelope = attack_coef * self.envelope + (1.0 - attack_coef) * abs_input;
        } else {
            self.envelope = release_coef * self.envelope + (1.0 - release_coef) * abs_input;
        }

        let out = (self.envelope * gain).clamp(0.0, 10.0);
        outputs.set(10, out);
        outputs.set(11, 10.0 - out);
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "envelope_follower"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analog::Saturator;
    use crate::modules::common::{measure_max_output, SAFE_AUDIO_LIMIT};

    #[test]
    fn test_adsr_envelope() {
        let mut adsr = Adsr::new(1000.0); // 1kHz for easy math
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Fast attack
        inputs.set(2, 0.1);

        // Gate on
        inputs.set(0, 5.0);

        // Run attack phase
        for _ in 0..100 {
            adsr.tick(&inputs, &mut outputs);
        }

        // Should have risen from 0
        let level = outputs.get(10).unwrap();
        assert!(level > 0.0);
    }
    #[test]
    fn test_vca() {
        let mut vca = Vca::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 5.0); // Input
        inputs.set(1, 5.0); // Half CV

        vca.tick(&inputs, &mut outputs);

        let out = outputs.get(10).unwrap();
        assert!((out - 2.5).abs() < 0.01);
    }
    #[test]
    fn test_limiter() {
        let mut limiter = Limiter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test with signal above threshold
        inputs.set(0, 10.0); // Way above threshold
        inputs.set(1, 0.5); // Threshold
        for _ in 0..100 {
            limiter.tick(&inputs, &mut outputs);
        }

        // Output should be limited
        let out = outputs.get(10).unwrap();
        assert!(out.abs() < 10.0);
        assert!(out.is_finite());
    }
    #[test]
    fn test_limiter_default() {
        let limiter = Limiter::default();
        assert_eq!(limiter.type_id(), "limiter");
    }
    #[test]
    fn test_noise_gate() {
        let mut gate = NoiseGate::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test with signal below threshold
        inputs.set(0, 0.01); // Very quiet
        inputs.set(1, 0.5); // Threshold
        for _ in 0..1000 {
            gate.tick(&inputs, &mut outputs);
        }

        // Gate should be closed, output attenuated
        let out = outputs.get(10).unwrap();
        assert!(out.abs() < 0.1);

        // Gate output should be closed
        let gate_out = outputs.get(11).unwrap();
        assert!(gate_out < 2.5);
    }
    #[test]
    fn test_noise_gate_default() {
        let gate = NoiseGate::default();
        assert_eq!(gate.type_id(), "noise_gate");
    }
    #[test]
    fn test_compressor() {
        let mut comp = Compressor::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Signal above threshold
        inputs.set(0, 5.0);
        inputs.set(1, 0.2); // Low threshold
        inputs.set(2, 0.8); // High ratio
        for _ in 0..100 {
            comp.tick(&inputs, &mut outputs);
        }

        let out = outputs.get(10).unwrap();
        assert!(out.is_finite());

        // Should have some gain reduction
        let gr = outputs.get(11).unwrap();
        assert!(gr >= 0.0);
    }
    #[test]
    fn test_compressor_default() {
        let comp = Compressor::default();
        assert_eq!(comp.type_id(), "compressor");
    }
    #[test]
    fn test_envelope_follower() {
        let mut ef = EnvelopeFollower::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Feed signal
        inputs.set(0, 5.0);
        for _ in 0..1000 {
            ef.tick(&inputs, &mut outputs);
        }

        let out = outputs.get(10).unwrap();
        assert!(out > 0.0);
        assert!(out.is_finite());

        // Inverted output
        let inv = outputs.get(11).unwrap();
        assert!(inv.is_finite());
    }
    #[test]
    fn test_envelope_follower_default() {
        let ef = EnvelopeFollower::default();
        assert_eq!(ef.type_id(), "envelope_follower");
    }
    #[test]
    fn test_adsr_default_reset_sample_rate() {
        let mut adsr = Adsr::default();
        assert!(adsr.sample_rate == 44100.0);

        adsr.set_sample_rate(48000.0);
        assert!(adsr.sample_rate == 48000.0);

        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 5.0); // Gate high
        for _ in 0..100 {
            adsr.tick(&inputs, &mut outputs);
        }

        adsr.reset();
        assert!(adsr.level == 0.0);
        assert!(adsr.stage == AdsrStage::Idle);

        assert_eq!(adsr.type_id(), "adsr");
    }
    #[test]
    fn test_vca_default_reset_sample_rate() {
        let mut vca = Vca::default();
        vca.reset();
        vca.set_sample_rate(48000.0);
        assert_eq!(vca.type_id(), "vca");
    }
    #[test]
    fn test_adsr_full_cycle() {
        let mut adsr = Adsr::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set fast envelope
        inputs.set(1, 10.0); // Fast attack
        inputs.set(2, 10.0); // Fast decay
        inputs.set(3, 5.0); // 50% sustain
        inputs.set(4, 10.0); // Fast release

        // Gate on - attack
        inputs.set(0, 5.0);
        for _ in 0..1000 {
            adsr.tick(&inputs, &mut outputs);
        }

        // Should have output during attack
        let peak = outputs.get(10).unwrap();
        assert!(peak > 0.0);

        // Continue through decay to sustain
        for _ in 0..1000 {
            adsr.tick(&inputs, &mut outputs);
        }

        // Gate off - release
        inputs.set(0, 0.0);
        for _ in 0..1000 {
            adsr.tick(&inputs, &mut outputs);
        }

        // Should be near zero after release
        let after_release = outputs.get(10).unwrap();
        assert!(after_release < 0.1);
    }
    #[test]
    fn test_adsr_output_bounded() {
        let mut adsr = Adsr::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Fast attack, instant release
        inputs.set(2, 0.0); // Attack
        inputs.set(3, 0.0); // Decay
        inputs.set(4, 1.0); // Sustain
        inputs.set(5, 0.0); // Release

        // Gate on
        inputs.set(0, 5.0);

        let max = measure_max_output(10000, || {
            adsr.tick(&inputs, &mut outputs);
            outputs.get(10).unwrap_or(0.0).abs()
        });

        assert!(
            max <= 10.5, // ADSR outputs 0-10V
            "ADSR output {} exceeds expected 0-10V range",
            max
        );
    }
    #[test]
    fn test_limiter_prevents_spikes() {
        let mut limiter = Limiter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set threshold to 3V
        inputs.set(1, 0.3); // Threshold CV (0-1 maps to 0-5V)

        // Feed in a 10V spike
        inputs.set(0, 10.0);

        limiter.tick(&inputs, &mut outputs);
        let out = outputs.get(10).unwrap_or(0.0);

        assert!(
            out.abs() <= 5.0,
            "Limiter failed to limit 10V input, got {}",
            out
        );
    }
    #[test]
    fn test_saturator_prevents_spikes() {
        let mut sat = Saturator::new(0.8); // High drive
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Large input
        inputs.set(0, 20.0);

        sat.tick(&inputs, &mut outputs);
        let out = outputs.get(10).unwrap_or(0.0);

        assert!(
            out.abs() <= SAFE_AUDIO_LIMIT,
            "Saturator failed to limit input, got {}",
            out
        );
    }
}
