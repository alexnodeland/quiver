//! Core DSP Modules
//!
//! This module provides the essential building blocks for synthesis:
//! oscillators, filters, envelopes, amplifiers, and utilities.

use crate::port::{GraphModule, ParamDef, ParamId, PortDef, PortSpec, PortValues, SignalKind};
use crate::rng;
use alloc::format;
use alloc::vec;
use core::f64::consts::{PI, TAU};
use libm::Libm;

/// Voltage-Controlled Oscillator (VCO)
///
/// A multi-waveform oscillator with V/Oct pitch input, FM, pulse width control,
/// and hard sync. Outputs sine, triangle, saw, and square waveforms.
pub struct Vco {
    phase: f64,
    sample_rate: f64,
    last_sync: f64,
    spec: PortSpec,
}

impl Vco {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
            last_sync: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "voct", SignalKind::VoltPerOctave),
                    PortDef::new(1, "fm", SignalKind::CvBipolar).with_attenuverter(),
                    PortDef::new(2, "pw", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(3, "sync", SignalKind::Gate),
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

        // V/Oct to frequency: 0V = C4 (261.63 Hz)
        let base_freq = 261.63 * Libm::<f64>::pow(2.0, voct);
        let freq = base_freq * Libm::<f64>::pow(2.0, fm);

        // Hard sync on rising edge
        if sync > 2.5 && self.last_sync <= 2.5 {
            self.phase = 0.0;
        }
        self.last_sync = sync;

        // Generate waveforms (±5V range)
        let sin = Libm::<f64>::sin(self.phase * TAU) * 5.0;
        let tri = (1.0 - 4.0 * Libm::<f64>::fabs(self.phase - 0.5)) * 5.0;
        let saw = (2.0 * self.phase - 1.0) * 5.0;
        let sqr = if self.phase < pw { 5.0 } else { -5.0 };

        outputs.set(10, sin);
        outputs.set(11, tri);
        outputs.set(12, saw);
        outputs.set(13, sqr);

        // Advance phase
        let new_phase = self.phase + freq / self.sample_rate;
        self.phase = new_phase - Libm::<f64>::floor(new_phase);
        if self.phase < 0.0 {
            self.phase += 1.0;
        }
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.last_sync = 0.0;
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
    last_reset: f64,
    spec: PortSpec,
}

impl Lfo {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
            last_reset: 0.0,
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
        if reset > 2.5 && self.last_reset <= 2.5 {
            self.phase = 0.0;
        }
        self.last_reset = reset;

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

        let new_phase = self.phase + freq / self.sample_rate;
        self.phase = new_phase - Libm::<f64>::floor(new_phase);
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.last_reset = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "lfo"
    }
}

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

        // Soft clip to prevent runaway in self-oscillation mode
        let band_out = if res > 0.95 {
            Libm::<f64>::tanh(self.band * 0.5) * 2.0
        } else {
            self.band
        };

        outputs.set(10, self.low);
        outputs.set(11, band_out);
        outputs.set(12, high);
        outputs.set(13, notch);
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

        let gate_high = gate > 2.5;
        let gate_rising = gate_high && self.last_gate <= 2.5;
        let gate_falling = !gate_high && self.last_gate > 2.5;
        let retrig_rising = retrig > 2.5 && self.last_retrig <= 2.5;

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
                    eoc = 5.0; // End-of-cycle trigger
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

/// Multi-channel Mixer
///
/// Sums multiple audio inputs into a single output.
pub struct Mixer {
    num_channels: usize,
    spec: PortSpec,
}

impl Mixer {
    pub fn new(num_channels: usize) -> Self {
        let inputs = (0..num_channels)
            .map(|i| {
                PortDef::new(i as u32, format!("ch{}", i), SignalKind::Audio).with_attenuverter()
            })
            .collect();

        Self {
            num_channels,
            spec: PortSpec {
                inputs,
                outputs: vec![PortDef::new(100, "out", SignalKind::Audio)],
            },
        }
    }
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new(4)
    }
}

impl GraphModule for Mixer {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let sum: f64 = (0..self.num_channels)
            .map(|i| inputs.get_or(i as u32, 0.0))
            .sum();
        outputs.set(100, sum);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "mixer"
    }
}

/// DC Offset module
///
/// Adds a constant offset to a signal.
pub struct Offset {
    pub(crate) offset: f64,
    spec: PortSpec,
}

impl Offset {
    pub fn new(offset: f64) -> Self {
        Self {
            offset,
            spec: PortSpec {
                inputs: vec![PortDef::new(0, "in", SignalKind::CvBipolar)],
                outputs: vec![PortDef::new(10, "out", SignalKind::CvBipolar)],
            },
        }
    }

    pub fn set_offset(&mut self, offset: f64) {
        self.offset = offset;
    }
}

impl Default for Offset {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl GraphModule for Offset {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        outputs.set(10, input + self.offset);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "offset"
    }

    fn params(&self) -> &[ParamDef] {
        static PARAMS: &[ParamDef] = &[];
        PARAMS
    }

    fn get_param(&self, id: ParamId) -> Option<f64> {
        if id == 0 {
            Some(self.offset)
        } else {
            None
        }
    }

    fn set_param(&mut self, id: ParamId, value: f64) {
        if id == 0 {
            self.offset = value;
        }
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
    spec: PortSpec,
}

impl DelayLine {
    /// Maximum delay time in seconds
    const MAX_DELAY_SECS: f64 = 2.0;

    pub fn new(sample_rate: f64) -> Self {
        let buffer_size = (sample_rate * Self::MAX_DELAY_SECS) as usize + 1;
        Self {
            buffer: vec![0.0; buffer_size],
            write_pos: 0,
            sample_rate,
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

    /// Read from the delay line with linear interpolation
    fn read_interpolated(&self, delay_samples: f64) -> f64 {
        let buffer_len = self.buffer.len();
        let delay_int = delay_samples as usize;
        let frac = delay_samples - delay_int as f64;

        // Calculate read positions (wrapping)
        let read_pos1 = (self.write_pos + buffer_len - delay_int) % buffer_len;
        let read_pos2 = (self.write_pos + buffer_len - delay_int - 1) % buffer_len;

        // Linear interpolation
        let sample1 = self.buffer[read_pos1];
        let sample2 = self.buffer[read_pos2];
        sample1 * (1.0 - frac) + sample2 * frac
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
        let delay_samples =
            (delay_ms * self.sample_rate / 1000.0).clamp(1.0, (self.buffer.len() - 1) as f64);

        // Read from delay line
        let delayed = self.read_interpolated(delay_samples);

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
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let buffer_size = (sample_rate * Self::MAX_DELAY_SECS) as usize + 1;
        self.buffer = vec![0.0; buffer_size];
        self.write_pos = 0;
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

    /// Read from a delay buffer with linear interpolation
    fn read_interpolated(buffer: &[f64], write_pos: usize, delay_samples: f64) -> f64 {
        let buffer_len = buffer.len();
        let delay_int = delay_samples as usize;
        let frac = delay_samples - delay_int as f64;

        let read_pos1 = (write_pos + buffer_len - delay_int) % buffer_len;
        let read_pos2 = (write_pos + buffer_len - delay_int - 1) % buffer_len;

        let sample1 = buffer[read_pos1];
        let sample2 = buffer[read_pos2];
        sample1 * (1.0 - frac) + sample2 * frac
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
            let delay_samples = base_delay_samples + lfo_val * mod_depth_samples;
            let delay_samples = delay_samples.clamp(1.0, (self.delay_buffers[i].len() - 1) as f64);

            // Read from this voice's delay line
            let delayed =
                Self::read_interpolated(&self.delay_buffers[i], self.write_pos, delay_samples);

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
        let soft_mode = inputs.get_or(3, 5.0) > 2.5;

        let release_ms = 10.0 + release_cv * 990.0;
        let release_coef = Libm::<f64>::exp(-1.0 / (release_ms * self.sample_rate / 1000.0));

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
        let attack_coef = Libm::<f64>::exp(-1.0 / (attack_ms * self.sample_rate / 1000.0));
        let release_coef = Libm::<f64>::exp(-1.0 / (release_ms * self.sample_rate / 1000.0));

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
        outputs.set(11, if self.gate_state > 0.5 { 5.0 } else { 0.0 });
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

        let attack_coef = Libm::<f64>::exp(-1.0 / (attack_ms * self.sample_rate / 1000.0));
        let release_coef = Libm::<f64>::exp(-1.0 / (release_ms * self.sample_rate / 1000.0));

        let abs_sidechain = Libm::<f64>::fabs(sidechain);
        if abs_sidechain > self.envelope {
            self.envelope = attack_coef * self.envelope + (1.0 - attack_coef) * abs_sidechain;
        } else {
            self.envelope = release_coef * self.envelope + (1.0 - release_coef) * abs_sidechain;
        }

        let gain = if self.envelope > threshold && threshold > 0.0 {
            let over_db = 20.0 * Libm::<f64>::log10(self.envelope / threshold);
            let compressed_db = over_db / ratio;
            let gain_reduction_db = over_db - compressed_db;
            Libm::<f64>::pow(10.0, -gain_reduction_db / 20.0)
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
        let attack_coef = Libm::<f64>::exp(-1.0 / (attack_ms * self.sample_rate / 1000.0));
        let release_coef = Libm::<f64>::exp(-1.0 / (release_ms * self.sample_rate / 1000.0));

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

/// Bitcrusher
///
/// Lo-fi effect that reduces bit depth and sample rate.
pub struct Bitcrusher {
    hold_sample: f64,
    hold_counter: f64,
    spec: PortSpec,
}

impl Bitcrusher {
    pub fn new() -> Self {
        Self {
            hold_sample: 0.0,
            hold_counter: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "bits", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "downsample", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }
}

impl Default for Bitcrusher {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for Bitcrusher {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let bits_cv = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
        let downsample_cv = inputs.get_or(2, 0.0).clamp(0.0, 1.0);

        let bits = 1.0 + bits_cv * 15.0;
        let downsample_factor = 1.0 + downsample_cv * 63.0;

        self.hold_counter += 1.0;
        if self.hold_counter >= downsample_factor {
            self.hold_counter = 0.0;
            self.hold_sample = input;
        }

        let levels = Libm::<f64>::pow(2.0, bits);
        let normalized = (self.hold_sample / 5.0 + 1.0) * 0.5;
        let quantized = Libm::<f64>::floor(normalized * levels) / levels;
        outputs.set(10, (quantized * 2.0 - 1.0) * 5.0);
    }

    fn reset(&mut self) {
        self.hold_sample = 0.0;
        self.hold_counter = 0.0;
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "bitcrusher"
    }
}

/// Flanger
///
/// Classic flanging effect using a short modulated delay with feedback.
pub struct Flanger {
    buffer: Vec<f64>,
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
                    PortDef::new(3, "feedback", SignalKind::CvBipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(4, "mix", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }

    fn read_interpolated(&self, delay_samples: f64) -> f64 {
        let buffer_len = self.buffer.len();
        let delay_int = delay_samples as usize;
        let frac = delay_samples - delay_int as f64;
        let read_pos1 = (self.write_pos + buffer_len - delay_int) % buffer_len;
        let read_pos2 = (self.write_pos + buffer_len - delay_int - 1) % buffer_len;
        self.buffer[read_pos1] * (1.0 - frac) + self.buffer[read_pos2] * frac
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

        let lfo_freq = 0.05 * Libm::<f64>::pow(100.0, rate_cv);
        let base_delay_ms = 1.0;
        let mod_depth_ms = depth_cv * (Self::MAX_DELAY_MS - base_delay_ms);

        let lfo = (Libm::<f64>::sin(self.lfo_phase * TAU) + 1.0) * 0.5;
        self.lfo_phase += lfo_freq / self.sample_rate;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }

        let delay_ms = base_delay_ms + lfo * mod_depth_ms;
        let delay_samples =
            (delay_ms * self.sample_rate / 1000.0).clamp(1.0, (self.buffer.len() - 1) as f64);

        let delayed = self.read_interpolated(delay_samples);
        self.buffer[self.write_pos] = input + delayed * feedback;
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
        self.buffer = vec![0.0; buffer_size];
        self.write_pos = 0;
    }

    fn type_id(&self) -> &'static str {
        "flanger"
    }
}

/// Phaser
///
/// Classic phaser effect using cascaded all-pass filters.
pub struct Phaser {
    allpass_states: [f64; 6],
    lfo_phase: f64,
    sample_rate: f64,
    spec: PortSpec,
}

impl Phaser {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            allpass_states: [0.0; 6],
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
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }

    fn allpass(input: f64, state: &mut f64, coef: f64) -> f64 {
        let output = *state + coef * (input - *state);
        *state = input + coef * (output - input);
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

        let lfo_freq = 0.05 * Libm::<f64>::pow(100.0, rate_cv);
        let lfo = Libm::<f64>::sin(self.lfo_phase * TAU);
        self.lfo_phase += lfo_freq / self.sample_rate;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }

        let min_freq = 200.0;
        let max_freq = 4000.0;
        let freq = min_freq + (lfo * 0.5 + 0.5) * depth * (max_freq - min_freq);

        let w = TAU * freq / self.sample_rate;
        let tan_w = Libm::<f64>::tan(w * 0.5);
        let coef = (1.0 - tan_w) / (1.0 + tan_w);

        let mut signal = input + self.allpass_states[num_stages - 1] * feedback;

        for i in 0..num_stages {
            signal = Self::allpass(signal, &mut self.allpass_states[i], coef);
        }

        outputs.set(10, input * (1.0 - mix) + signal * mix);
    }

    fn reset(&mut self) {
        self.allpass_states = [0.0; 6];
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

    fn read_interpolated(&self, delay_samples: f64) -> f64 {
        let buffer_len = self.buffer.len();
        let delay_int = delay_samples as usize;
        let frac = delay_samples - delay_int as f64;
        let read_pos1 = (self.write_pos + buffer_len - delay_int) % buffer_len;
        let read_pos2 = (self.write_pos + buffer_len - delay_int - 1) % buffer_len;
        self.buffer[read_pos1] * (1.0 - frac) + self.buffer[read_pos2] * frac
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

        // Write to buffer
        self.buffer[self.write_pos] = input;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();

        // Calculate modulated delay
        let delay_ms = base_delay_ms + lfo * mod_depth_ms;
        let delay_samples =
            (delay_ms * self.sample_rate / 1000.0).clamp(1.0, (self.buffer.len() - 1) as f64);

        let delayed = self.read_interpolated(delay_samples);
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

/// Distortion
///
/// Waveshaping distortion with multiple algorithms:
/// - Soft clip (tanh-style)
/// - Hard clip
/// - Foldback
/// - Asymmetric (tube-style)
pub struct Distortion {
    spec: PortSpec,
}

impl Distortion {
    pub fn new(_sample_rate: f64) -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "drive", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "tone", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(3, "mode", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(4, "mix", SignalKind::CvUnipolar)
                        .with_default(1.0)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }

    // Soft clip using tanh-style curve
    fn soft_clip(x: f64, drive: f64) -> f64 {
        let gained = x * (1.0 + drive * 10.0);
        // Fast tanh approximation
        let x2 = gained * gained;
        gained * (27.0 + x2) / (27.0 + 9.0 * x2)
    }

    // Hard clip
    fn hard_clip(x: f64, drive: f64) -> f64 {
        let gained = x * (1.0 + drive * 10.0);
        gained.clamp(-1.0, 1.0)
    }

    // Foldback distortion
    fn foldback(x: f64, drive: f64) -> f64 {
        let gained = x * (1.0 + drive * 5.0);
        let threshold = 1.0;
        let mut folded = gained;
        while folded > threshold || folded < -threshold {
            if folded > threshold {
                folded = 2.0 * threshold - folded;
            } else if folded < -threshold {
                folded = -2.0 * threshold - folded;
            }
        }
        folded
    }

    // Asymmetric tube-style distortion
    fn asymmetric(x: f64, drive: f64) -> f64 {
        let gained = x * (1.0 + drive * 8.0);
        if gained >= 0.0 {
            // Softer positive clipping
            1.0 - Libm::<f64>::exp(-gained)
        } else {
            // Harder negative clipping
            -Self::soft_clip(-gained, drive * 0.5)
        }
    }
}

impl Default for Distortion {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Distortion {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let drive = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
        let tone = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let mode = inputs.get_or(3, 0.0).clamp(0.0, 1.0);
        let mix = inputs.get_or(4, 1.0).clamp(0.0, 1.0);

        // Select distortion mode (quantized to 4 modes)
        let mode_idx = (mode * 3.99) as u8;
        let distorted = match mode_idx {
            0 => Self::soft_clip(input, drive),
            1 => Self::hard_clip(input, drive),
            2 => Self::foldback(input, drive),
            _ => Self::asymmetric(input, drive),
        };

        // Simple tone control: blend between original and low-passed
        // Higher tone = more highs preserved
        let filtered = distorted * tone + distorted * (1.0 - tone) * 0.7;

        outputs.set(10, input * (1.0 - mix) + filtered * mix);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "distortion"
    }
}

// ============================================================================
// P3 Oscillators: Supersaw, Karplus-Strong
// ============================================================================

/// Supersaw Oscillator
///
/// JP-8000 style supersaw with 7 detuned oscillators.
/// Creates thick, wide sounds.
pub struct Supersaw {
    phases: [f64; 7],
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
    fn polyblep(t: f64, dt: f64) -> f64 {
        if t < dt {
            let t = t / dt;
            2.0 * t - t * t - 1.0
        } else if t > 1.0 - dt {
            let t = (t - 1.0) / dt;
            t * t + 2.0 * t + 1.0
        } else {
            0.0
        }
    }
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
        let base_freq = 261.63 * Libm::<f64>::pow(2.0, voct); // C4 at 0V

        let mut sum = 0.0;
        let mut total_mix = 0.0;

        for i in 0..7 {
            // Apply detune
            let detune_amount = Self::DETUNE_RATIOS[i] * detune;
            let freq = base_freq * (1.0 + detune_amount);
            let dt = freq / self.sample_rate;

            // Generate saw with polyblep
            let raw_saw = 2.0 * self.phases[i] - 1.0;
            let blep = Self::polyblep(self.phases[i], dt);
            let saw = raw_saw - blep;

            // Mix with level
            sum += saw * Self::MIX_LEVELS[i];
            total_mix += Self::MIX_LEVELS[i];

            // Advance phase
            self.phases[i] += dt;
            if self.phases[i] >= 1.0 {
                self.phases[i] -= 1.0;
            }
        }

        // Normalize and apply mix (blend between center oscillator and full supersaw)
        let normalized = sum / total_mix;
        let center_saw = 2.0 * self.phases[3] - 1.0;
        let output = center_saw * (1.0 - mix) + normalized * mix;

        // Sub oscillator (octave down from center)
        let sub_phase = (self.phases[3] * 0.5) % 1.0;
        let sub = 2.0 * sub_phase - 1.0;

        outputs.set(10, output);
        outputs.set(11, sub);
    }

    fn reset(&mut self) {
        for (i, phase) in self.phases.iter_mut().enumerate() {
            *phase = (i as f64) / 7.0;
        }
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
    write_pos: usize,
    sample_rate: f64,
    last_output: f64,
    spec: PortSpec,
}

impl KarplusStrong {
    pub fn new(sample_rate: f64) -> Self {
        // Buffer for lowest frequency (around 20Hz)
        let buffer_size = (sample_rate / 20.0) as usize + 10;
        Self {
            buffer: vec![0.0; buffer_size],
            write_pos: 0,
            sample_rate,
            last_output: 0.0,
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

        // Calculate period from frequency
        let freq = 261.63 * Libm::<f64>::pow(2.0, voct);
        let period = (self.sample_rate / freq).clamp(2.0, self.buffer.len() as f64 - 1.0);
        let period_int = period as usize;

        // Trigger excitation
        if trigger > 0.5 {
            // Resize buffer for this frequency
            self.buffer.truncate(period_int + 2);
            self.buffer.resize(period_int + 2, 0.0);
            self.excite(brightness);
            self.write_pos = 0;
        }

        // Read from buffer with interpolation
        let read_pos = (self.write_pos + 1) % self.buffer.len();
        let read_pos2 = (self.write_pos + 2) % self.buffer.len();
        let frac = period.fract();
        let sample = self.buffer[read_pos] * (1.0 - frac) + self.buffer[read_pos2] * frac;

        // Lowpass filter (simple averaging with damping control)
        // Higher damping = more filtering = faster decay
        let filter_coef = 0.5 + damping * 0.49; // 0.5 to 0.99
        let filtered = sample * filter_coef + self.last_output * (1.0 - filter_coef);

        // All-pass filter for stretch factor (inharmonicity)
        let stretch_coef = stretch * 0.5;
        let stretched = filtered + stretch_coef * (filtered - self.last_output);

        self.last_output = stretched;

        // Write back to buffer
        self.buffer[self.write_pos] = stretched;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();

        outputs.set(10, stretched);
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
        self.last_output = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let buffer_size = (sample_rate / 20.0) as usize + 10;
        self.buffer.resize(buffer_size, 0.0);
    }

    fn type_id(&self) -> &'static str {
        "karplus_strong"
    }
}

// ============================================================================
// P3 Utilities: ScaleQuantizer, Euclidean
// ============================================================================

/// Scale Quantizer
///
/// Quantizes CV input to musical scale notes.
/// Supports major, minor, pentatonic, and chromatic scales.
pub struct ScaleQuantizer {
    spec: PortSpec,
}

impl ScaleQuantizer {
    // Scale intervals (semitones from root)
    const CHROMATIC: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    const MAJOR: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
    const MINOR: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];
    const PENT_MAJOR: [u8; 5] = [0, 2, 4, 7, 9];
    const PENT_MINOR: [u8; 5] = [0, 3, 5, 7, 10];
    const DORIAN: [u8; 7] = [0, 2, 3, 5, 7, 9, 10];
    const BLUES: [u8; 6] = [0, 3, 5, 6, 7, 10];

    pub fn new(_sample_rate: f64) -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::VoltPerOctave),
                    PortDef::new(1, "root", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(2, "scale", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::VoltPerOctave),
                    PortDef::new(11, "trigger", SignalKind::Trigger),
                ],
            },
        }
    }

    fn quantize_to_scale(note: i32, scale: &[u8]) -> i32 {
        let octave = if note >= 0 {
            note / 12
        } else {
            (note - 11) / 12
        };
        let semitone = note.rem_euclid(12);

        // Find closest note in scale
        let mut closest = scale[0] as i32;
        let mut min_dist = i32::MAX;

        for &s in scale {
            let dist = (semitone - s as i32).abs();
            let wrap_dist = (12 - dist).min(dist);
            if wrap_dist < min_dist {
                min_dist = wrap_dist;
                closest = s as i32;
            }
        }

        octave * 12 + closest
    }
}

impl Default for ScaleQuantizer {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for ScaleQuantizer {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let root_cv = inputs.get_or(1, 0.0).clamp(0.0, 1.0);
        let scale_cv = inputs.get_or(2, 0.0).clamp(0.0, 1.0);

        // Root note (0-11 semitones)
        let root = (root_cv * 11.99) as i32;

        // Convert V/Oct to semitones from C4
        let semitones_from_c4 = (input * 12.0).round() as i32;

        // Adjust for root
        let relative_note = semitones_from_c4 - root;

        // Select scale
        let scale_idx = (scale_cv * 6.99) as u8;
        let quantized = match scale_idx {
            0 => Self::quantize_to_scale(relative_note, &Self::CHROMATIC),
            1 => Self::quantize_to_scale(relative_note, &Self::MAJOR),
            2 => Self::quantize_to_scale(relative_note, &Self::MINOR),
            3 => Self::quantize_to_scale(relative_note, &Self::PENT_MAJOR),
            4 => Self::quantize_to_scale(relative_note, &Self::PENT_MINOR),
            5 => Self::quantize_to_scale(relative_note, &Self::DORIAN),
            _ => Self::quantize_to_scale(relative_note, &Self::BLUES),
        };

        // Convert back to V/Oct with root offset
        let output_voct = (quantized + root) as f64 / 12.0;

        // Generate trigger on note change (simple comparison)
        let trigger = if (output_voct - input).abs() > 0.001 {
            5.0
        } else {
            0.0
        };

        outputs.set(10, output_voct);
        outputs.set(11, trigger);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "scale_quantizer"
    }
}

/// Euclidean Rhythm Generator
///
/// Generates euclidean rhythms - evenly distributed pulses.
/// Classic algorithm used in many world music traditions.
pub struct Euclidean {
    step: usize,
    pattern: Vec<bool>,
    last_clock: f64,
    spec: PortSpec,
}

impl Euclidean {
    pub fn new(_sample_rate: f64) -> Self {
        Self {
            step: 0,
            pattern: vec![true; 16],
            last_clock: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "clock", SignalKind::Trigger),
                    PortDef::new(1, "steps", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "pulses", SignalKind::CvUnipolar)
                        .with_default(0.25)
                        .with_attenuverter(),
                    PortDef::new(3, "rotation", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(4, "reset", SignalKind::Trigger),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Trigger),
                    PortDef::new(11, "accent", SignalKind::Trigger),
                ],
            },
        }
    }

    fn generate_pattern(steps: usize, pulses: usize) -> Vec<bool> {
        if steps == 0 || pulses == 0 {
            return vec![false; steps.max(1)];
        }

        let pulses = pulses.min(steps);
        let mut pattern = vec![false; steps];

        // Bresenham-style euclidean distribution
        let mut bucket = 0;
        for slot in pattern.iter_mut().take(steps) {
            bucket += pulses;
            if bucket >= steps {
                bucket -= steps;
                *slot = true;
            }
        }

        pattern
    }
}

impl Default for Euclidean {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Euclidean {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let clock = inputs.get_or(0, 0.0);
        let steps_cv = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
        let pulses_cv = inputs.get_or(2, 0.25).clamp(0.0, 1.0);
        let rotation_cv = inputs.get_or(3, 0.0).clamp(0.0, 1.0);
        let reset = inputs.get_or(4, 0.0);

        // Calculate steps (2-16) and pulses
        let steps = 2 + (steps_cv * 14.99) as usize;
        let pulses = (pulses_cv * steps as f64) as usize;

        // Regenerate pattern if parameters changed
        if self.pattern.len() != steps {
            self.pattern = Self::generate_pattern(steps, pulses);
        }

        // Handle reset
        if reset > 0.5 {
            self.step = 0;
        }

        // Detect clock rising edge
        let trigger = clock > 0.5 && self.last_clock <= 0.5;
        self.last_clock = clock;

        let mut out = 0.0;
        let mut accent = 0.0;

        if trigger {
            // Apply rotation
            let rotation = (rotation_cv * (steps - 1) as f64) as usize;
            let rotated_step = (self.step + rotation) % steps;

            if self.pattern[rotated_step] {
                out = 5.0;
                // Accent on downbeat (step 0)
                if self.step == 0 {
                    accent = 5.0;
                }
            }

            self.step = (self.step + 1) % steps;
        }

        outputs.set(10, out);
        outputs.set(11, accent);
    }

    fn reset(&mut self) {
        self.step = 0;
        self.last_clock = 0.0;
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "euclidean"
    }
}

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

/// Crosstalk Simulator
///
/// Simulates signal crosstalk between adjacent channels, a common
/// phenomenon in analog audio equipment where signals "leak" between
/// channels due to capacitive coupling or poor isolation.
///
/// This is a Phase 3 addition.
pub struct Crosstalk {
    sample_rate: f64,
    /// High-frequency emphasis filter states
    hf_state: [f64; 2],
    spec: PortSpec,
}

impl Crosstalk {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            hf_state: [0.0; 2],
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in_a", SignalKind::Audio),
                    PortDef::new(1, "in_b", SignalKind::Audio),
                    // Crosstalk amount (0-1, typically very low in real gear)
                    PortDef::new(2, "amount", SignalKind::CvUnipolar).with_default(0.01),
                    // Frequency-dependent crosstalk (higher = more HF crosstalk)
                    PortDef::new(3, "hf_emphasis", SignalKind::CvUnipolar).with_default(0.5),
                ],
                outputs: vec![
                    PortDef::new(10, "out_a", SignalKind::Audio),
                    PortDef::new(11, "out_b", SignalKind::Audio),
                ],
            },
        }
    }
}

impl Default for Crosstalk {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Crosstalk {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let in_a = inputs.get_or(0, 0.0);
        let in_b = inputs.get_or(1, 0.0);
        let amount = inputs.get_or(2, 0.01).clamp(0.0, 0.5);
        let hf_emphasis = inputs.get_or(3, 0.5).clamp(0.0, 1.0);

        // High-pass filter coefficient for HF emphasis (crosstalk is typically worse at HF)
        let hf_coef = 0.1 + hf_emphasis * 0.4;

        // Extract high-frequency component for emphasized crosstalk
        let hf_a = in_a - self.hf_state[0];
        let hf_b = in_b - self.hf_state[1];
        self.hf_state[0] += hf_coef * (in_a - self.hf_state[0]);
        self.hf_state[1] += hf_coef * (in_b - self.hf_state[1]);

        // Mix original signal with emphasized HF crosstalk from other channel
        let crosstalk_to_a = (in_b * (1.0 - hf_emphasis) + hf_b * hf_emphasis) * amount;
        let crosstalk_to_b = (in_a * (1.0 - hf_emphasis) + hf_a * hf_emphasis) * amount;

        outputs.set(10, in_a + crosstalk_to_a);
        outputs.set(11, in_b + crosstalk_to_b);
    }

    fn reset(&mut self) {
        self.hf_state = [0.0; 2];
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "crosstalk"
    }
}

/// Ground Loop Simulator
///
/// Simulates ground loop hum and related power supply interference,
/// common in analog audio equipment. Adds realistic 50/60 Hz hum
/// with harmonics and modulation from signal activity.
///
/// This is a Phase 3 addition.
pub struct GroundLoop {
    sample_rate: f64,
    /// Hum oscillator phase
    phase: f64,
    /// Hum frequency (50 or 60 Hz)
    pub(crate) frequency: f64,
    /// Thermal modulation state
    thermal_state: f64,
    spec: PortSpec,
}

impl GroundLoop {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            phase: 0.0,
            frequency: 60.0, // Default to 60 Hz (North America)
            thermal_state: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    // Hum level (typically very low)
                    PortDef::new(1, "level", SignalKind::CvUnipolar).with_default(0.005),
                    // Signal-dependent modulation (thermal effects)
                    PortDef::new(2, "modulation", SignalKind::CvUnipolar).with_default(0.1),
                    // Frequency select (0 = 50 Hz, 1 = 60 Hz)
                    PortDef::new(3, "freq_select", SignalKind::CvUnipolar).with_default(1.0),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }

    /// Create a 50 Hz ground loop (Europe, etc.)
    pub fn hz_50(sample_rate: f64) -> Self {
        let mut gl = Self::new(sample_rate);
        gl.frequency = 50.0;
        gl
    }

    /// Create a 60 Hz ground loop (North America)
    pub fn hz_60(sample_rate: f64) -> Self {
        let mut gl = Self::new(sample_rate);
        gl.frequency = 60.0;
        gl
    }
}

impl Default for GroundLoop {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for GroundLoop {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let level = inputs.get_or(1, 0.005).clamp(0.0, 0.1);
        let modulation = inputs.get_or(2, 0.1).clamp(0.0, 1.0);
        let freq_select = inputs.get_or(3, 1.0);

        // Select frequency based on input
        let freq = if freq_select > 0.5 { 60.0 } else { 50.0 };

        // Update thermal state based on signal energy (slow integration)
        let signal_energy = Libm::<f64>::pow(input / 5.0, 2.0);
        self.thermal_state += (signal_energy - self.thermal_state) * 0.0001;

        // Modulated hum level based on signal activity
        let modulated_level = level * (1.0 + self.thermal_state * modulation * 10.0);

        // Generate hum with harmonics (fundamental + 2nd + 3rd harmonic)
        let fundamental = Libm::<f64>::sin(self.phase * TAU);
        let second_harmonic = Libm::<f64>::sin(self.phase * 2.0 * TAU) * 0.5;
        let third_harmonic = Libm::<f64>::sin(self.phase * 3.0 * TAU) * 0.25;
        let hum = (fundamental + second_harmonic + third_harmonic) * modulated_level * 5.0;

        // Advance phase
        let new_phase = self.phase + freq / self.sample_rate;
        self.phase = new_phase - Libm::<f64>::floor(new_phase);

        outputs.set(10, input + hum);
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.thermal_state = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "ground_loop"
    }
}

/// Step Sequencer
///
/// An 8-step sequencer with clock and reset inputs.
pub struct StepSequencer {
    steps: [f64; 8],
    gates: [bool; 8],
    current: usize,
    last_clock: f64,
    last_reset: f64,
    spec: PortSpec,
}

impl StepSequencer {
    pub fn new() -> Self {
        Self {
            steps: [0.0; 8],
            gates: [true; 8],
            current: 0,
            last_clock: 0.0,
            last_reset: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "clock", SignalKind::Clock),
                    PortDef::new(1, "reset", SignalKind::Trigger),
                ],
                outputs: vec![
                    PortDef::new(10, "cv", SignalKind::VoltPerOctave),
                    PortDef::new(11, "gate", SignalKind::Gate),
                    PortDef::new(12, "trig", SignalKind::Trigger),
                ],
            },
        }
    }

    pub fn set_step(&mut self, index: usize, voltage: f64, gate: bool) {
        if index < 8 {
            self.steps[index] = voltage;
            self.gates[index] = gate;
        }
    }

    pub fn get_step(&self, index: usize) -> Option<(f64, bool)> {
        if index < 8 {
            Some((self.steps[index], self.gates[index]))
        } else {
            None
        }
    }
}

impl Default for StepSequencer {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for StepSequencer {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let clock = inputs.get_or(0, 0.0);
        let reset = inputs.get_or(1, 0.0);

        let clock_rising = clock > 2.5 && self.last_clock <= 2.5;
        let reset_rising = reset > 2.5 && self.last_reset <= 2.5;

        let mut trigger = 0.0;

        if reset_rising {
            self.current = 0;
            trigger = 5.0;
        } else if clock_rising {
            self.current = (self.current + 1) % 8;
            trigger = 5.0;
        }

        self.last_clock = clock;
        self.last_reset = reset;

        let cv = self.steps[self.current];
        let gate = if self.gates[self.current] && clock > 2.5 {
            5.0
        } else {
            0.0
        };

        outputs.set(10, cv);
        outputs.set(11, gate);
        outputs.set(12, trigger);
    }

    fn reset(&mut self) {
        self.current = 0;
        self.last_clock = 0.0;
        self.last_reset = 0.0;
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "step_sequencer"
    }
}

/// Stereo Output
///
/// The final output module that provides left and right audio outputs.
/// Right input is normalled to left for mono compatibility.
pub struct StereoOutput {
    spec: PortSpec,
}

impl StereoOutput {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "left", SignalKind::Audio),
                    PortDef::new(1, "right", SignalKind::Audio).normalled_to(0),
                ],
                outputs: vec![
                    PortDef::new(0, "left", SignalKind::Audio),
                    PortDef::new(1, "right", SignalKind::Audio),
                ],
            },
        }
    }
}

impl Default for StereoOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for StereoOutput {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let left = inputs.get_or(0, 0.0);
        let right = inputs.get_or(1, left); // Mono fallback

        outputs.set(0, left);
        outputs.set(1, right);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "stereo_output"
    }
}

/// Sample and Hold
///
/// Samples the input signal when triggered and holds the value until the next trigger.
pub struct SampleAndHold {
    held_value: f64,
    last_trigger: f64,
    spec: PortSpec,
}

impl SampleAndHold {
    pub fn new() -> Self {
        Self {
            held_value: 0.0,
            last_trigger: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::CvBipolar),
                    PortDef::new(1, "trig", SignalKind::Trigger),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::CvBipolar)],
            },
        }
    }
}

impl Default for SampleAndHold {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for SampleAndHold {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let trigger = inputs.get_or(1, 0.0);

        // Sample on rising edge
        if trigger > 2.5 && self.last_trigger <= 2.5 {
            self.held_value = input;
        }
        self.last_trigger = trigger;

        outputs.set(10, self.held_value);
    }

    fn reset(&mut self) {
        self.held_value = 0.0;
        self.last_trigger = 0.0;
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "sample_hold"
    }
}

/// Slew Limiter
///
/// Limits the rate of change of a signal, creating portamento/glide effects.
/// Separate rise and fall times allow asymmetric behavior.
pub struct SlewLimiter {
    current: f64,
    sample_rate: f64,
    spec: PortSpec,
}

impl SlewLimiter {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            current: 0.0,
            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::CvBipolar),
                    PortDef::new(1, "rise", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "fall", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::CvBipolar)],
            },
        }
    }

    fn cv_to_rate(&self, cv: f64) -> f64 {
        // Map 0-1 CV to rate: 0 = instant, 1 = very slow (~10 seconds)
        // Rate is in units per sample
        let time = 0.001 + Libm::<f64>::pow(cv.clamp(0.0, 1.0), 2.0) * 10.0; // 1ms to 10s
        1.0 / (time * self.sample_rate)
    }
}

impl Default for SlewLimiter {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for SlewLimiter {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let target = inputs.get_or(0, 0.0);
        let rise_cv = inputs.get_or(1, 0.5);
        let fall_cv = inputs.get_or(2, 0.5);

        let diff = target - self.current;

        if diff > 0.0 {
            // Rising
            let rate = self.cv_to_rate(rise_cv);
            self.current += Libm::<f64>::fmin(diff, rate * 10.0); // Scale for voltage range
        } else if diff < 0.0 {
            // Falling
            let rate = self.cv_to_rate(fall_cv);
            self.current += Libm::<f64>::fmax(diff, -rate * 10.0);
        }

        outputs.set(10, self.current);
    }

    fn reset(&mut self) {
        self.current = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "slew_limiter"
    }
}

/// Quantizer
///
/// Quantizes input CV to musical scale degrees.
/// Supports chromatic, major, minor, and pentatonic scales.
pub struct Quantizer {
    pub(crate) scale: Scale,
    spec: PortSpec,
}

/// Musical scales for quantization
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scale {
    Chromatic,
    Major,
    Minor,
    PentatonicMajor,
    PentatonicMinor,
    Dorian,
    Mixolydian,
    Blues,
}

impl Scale {
    /// Returns the semitone offsets for this scale (relative to root)
    fn semitones(&self) -> &'static [i32] {
        match self {
            Scale::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::PentatonicMajor => &[0, 2, 4, 7, 9],
            Scale::PentatonicMinor => &[0, 3, 5, 7, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::Blues => &[0, 3, 5, 6, 7, 10],
        }
    }
}

impl Quantizer {
    pub fn new(scale: Scale) -> Self {
        Self {
            scale,
            spec: PortSpec {
                inputs: vec![PortDef::new(0, "in", SignalKind::VoltPerOctave)],
                outputs: vec![PortDef::new(10, "out", SignalKind::VoltPerOctave)],
            },
        }
    }

    pub fn chromatic() -> Self {
        Self::new(Scale::Chromatic)
    }

    pub fn major() -> Self {
        Self::new(Scale::Major)
    }

    pub fn minor() -> Self {
        Self::new(Scale::Minor)
    }

    pub fn set_scale(&mut self, scale: Scale) {
        self.scale = scale;
    }

    fn quantize(&self, voltage: f64) -> f64 {
        let semitones = self.scale.semitones();

        // Convert voltage to semitones (1V = 12 semitones)
        let total_semitones = voltage * 12.0;

        // Find octave and position within octave
        let octave = Libm::<f64>::floor(total_semitones / 12.0);
        let within_octave = total_semitones - octave * 12.0;

        // Find nearest scale degree
        let mut nearest = semitones[0];
        let mut min_dist = f64::MAX;

        for &semi in semitones {
            let dist = (within_octave - semi as f64).abs();
            if dist < min_dist {
                min_dist = dist;
                nearest = semi;
            }
            // Also check wrapping to next octave
            let dist_wrap = (within_octave - (semi + 12) as f64).abs();
            if dist_wrap < min_dist {
                min_dist = dist_wrap;
                nearest = semi + 12;
            }
        }

        // Convert back to voltage
        (octave * 12.0 + nearest as f64) / 12.0
    }
}

impl Default for Quantizer {
    fn default() -> Self {
        Self::chromatic()
    }
}

impl GraphModule for Quantizer {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let quantized = self.quantize(input);
        outputs.set(10, quantized);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "quantizer"
    }
}

/// Clock Generator
///
/// Generates clock pulses at a specified tempo (BPM).
pub struct Clock {
    phase: f64,
    sample_rate: f64,
    spec: PortSpec,
}

impl Clock {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "bpm", SignalKind::CvUnipolar)
                        .with_default(1.2) // 120 BPM when scaled
                        .with_attenuverter(),
                    PortDef::new(1, "reset", SignalKind::Trigger),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Clock),
                    PortDef::new(11, "div2", SignalKind::Clock),
                    PortDef::new(12, "div4", SignalKind::Clock),
                ],
            },
        }
    }

    fn cv_to_bpm(cv: f64) -> f64 {
        // Map 0-10V to 20-300 BPM (exponential)
        20.0 * Libm::<f64>::pow(15.0, cv / 10.0)
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Clock {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let bpm_cv = inputs.get_or(0, 1.2); // Default ~120 BPM
        let reset = inputs.get_or(1, 0.0);

        let bpm = Self::cv_to_bpm(bpm_cv);
        let freq = bpm / 60.0; // Hz

        // Reset on trigger
        if reset > 2.5 {
            self.phase = 0.0;
        }

        // Main clock output (short pulse at start of each cycle)
        let pulse_width = 0.1; // 10% duty cycle
        let main_out = if self.phase < pulse_width { 5.0 } else { 0.0 };

        // Divided outputs (using integer phase counting would be cleaner,
        // but this works for demonstration)
        let div2_raw = self.phase * 0.5;
        let div4_raw = self.phase * 0.25;
        let div2_phase = div2_raw - Libm::<f64>::floor(div2_raw);
        let div4_phase = div4_raw - Libm::<f64>::floor(div4_raw);
        let div2_out = if div2_phase < pulse_width { 5.0 } else { 0.0 };
        let div4_out = if div4_phase < pulse_width { 5.0 } else { 0.0 };

        outputs.set(10, main_out);
        outputs.set(11, div2_out);
        outputs.set(12, div4_out);

        // Advance phase
        let new_phase = self.phase + freq / self.sample_rate;
        self.phase = new_phase - Libm::<f64>::floor(new_phase);
    }

    fn reset(&mut self) {
        self.phase = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "clock"
    }
}

/// Attenuverter
///
/// Attenuates and/or inverts a signal. The level control goes from
/// -1 (inverted full scale) through 0 (silence) to +1 (full scale).
pub struct Attenuverter {
    spec: PortSpec,
}

impl Attenuverter {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::CvBipolar),
                    PortDef::new(1, "level", SignalKind::CvBipolar).with_default(5.0), // Default to unity gain
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::CvBipolar)],
            },
        }
    }
}

impl Default for Attenuverter {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for Attenuverter {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let level = inputs.get_or(1, 5.0) / 5.0; // Normalize to -1..+1

        outputs.set(10, input * level);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "attenuverter"
    }
}

/// Multiple (Signal Splitter)
///
/// Takes one input and copies it to multiple outputs.
/// Useful for sending one signal to multiple destinations.
pub struct Multiple {
    spec: PortSpec,
}

impl Multiple {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![PortDef::new(0, "in", SignalKind::CvBipolar)],
                outputs: vec![
                    PortDef::new(10, "out1", SignalKind::CvBipolar),
                    PortDef::new(11, "out2", SignalKind::CvBipolar),
                    PortDef::new(12, "out3", SignalKind::CvBipolar),
                    PortDef::new(13, "out4", SignalKind::CvBipolar),
                ],
            },
        }
    }
}

impl Default for Multiple {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for Multiple {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);

        outputs.set(10, input);
        outputs.set(11, input);
        outputs.set(12, input);
        outputs.set(13, input);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "multiple"
    }
}

// ============================================================================
// Phase 2 Modules: Hardware Fidelity
// ============================================================================

/// Ring Modulator
///
/// Multiplies two audio signals together, producing sum and difference frequencies.
/// Classic technique for metallic, bell-like, and atonal sounds.
pub struct RingModulator {
    spec: PortSpec,
}

impl RingModulator {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "carrier", SignalKind::Audio),
                    PortDef::new(1, "modulator", SignalKind::Audio),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }
}

impl Default for RingModulator {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for RingModulator {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let carrier = inputs.get_or(0, 0.0);
        let modulator = inputs.get_or(1, 0.0);

        // Ring modulation is simple multiplication
        // Normalize by 5.0 to keep output in ±5V range (both inputs are ±5V)
        let out = (carrier * modulator) / 5.0;
        outputs.set(10, out);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "ring_mod"
    }
}

/// Crossfader / Panner
///
/// Crossfades between two audio inputs or pans a mono input across stereo outputs.
/// The position control goes from -5V (full A/left) to +5V (full B/right).
pub struct Crossfader {
    spec: PortSpec,
}

impl Crossfader {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "a", SignalKind::Audio),
                    PortDef::new(1, "b", SignalKind::Audio),
                    PortDef::new(2, "pos", SignalKind::CvBipolar).with_default(0.0),
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

impl Default for Crossfader {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for Crossfader {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let a = inputs.get_or(0, 0.0);
        let b = inputs.get_or(1, 0.0);
        let pos = inputs.get_or(2, 0.0);

        // Map position from -5V to +5V to 0.0 to 1.0
        let mix = ((pos / 5.0) + 1.0) / 2.0;
        let mix = mix.clamp(0.0, 1.0);

        // Equal-power crossfade for smoother transitions
        let a_gain = Libm::<f64>::sqrt(1.0 - mix);
        let b_gain = Libm::<f64>::sqrt(mix);

        // Main output: crossfade between A and B
        let out = a * a_gain + b * b_gain;
        outputs.set(10, out);

        // Stereo outputs: pan the main output
        // At pos=-5V: full left, at pos=+5V: full right
        outputs.set(11, out * a_gain); // Left
        outputs.set(12, out * b_gain); // Right
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "crossfader"
    }
}

/// Logic AND Gate
///
/// Outputs high (+5V) only when both inputs are high (>2.5V).
pub struct LogicAnd {
    spec: PortSpec,
}

impl LogicAnd {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "a", SignalKind::Gate),
                    PortDef::new(1, "b", SignalKind::Gate),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Gate)],
            },
        }
    }
}

impl Default for LogicAnd {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for LogicAnd {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let a = inputs.get_or(0, 0.0) > 2.5;
        let b = inputs.get_or(1, 0.0) > 2.5;

        outputs.set(10, if a && b { 5.0 } else { 0.0 });
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "logic_and"
    }
}

/// Logic OR Gate
///
/// Outputs high (+5V) when either or both inputs are high (>2.5V).
pub struct LogicOr {
    spec: PortSpec,
}

impl LogicOr {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "a", SignalKind::Gate),
                    PortDef::new(1, "b", SignalKind::Gate),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Gate)],
            },
        }
    }
}

impl Default for LogicOr {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for LogicOr {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let a = inputs.get_or(0, 0.0) > 2.5;
        let b = inputs.get_or(1, 0.0) > 2.5;

        outputs.set(10, if a || b { 5.0 } else { 0.0 });
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "logic_or"
    }
}

/// Logic XOR Gate
///
/// Outputs high (+5V) when exactly one input is high (>2.5V).
pub struct LogicXor {
    spec: PortSpec,
}

impl LogicXor {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "a", SignalKind::Gate),
                    PortDef::new(1, "b", SignalKind::Gate),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Gate)],
            },
        }
    }
}

impl Default for LogicXor {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for LogicXor {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let a = inputs.get_or(0, 0.0) > 2.5;
        let b = inputs.get_or(1, 0.0) > 2.5;

        outputs.set(10, if a ^ b { 5.0 } else { 0.0 });
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "logic_xor"
    }
}

/// Logic NOT Gate (Inverter)
///
/// Inverts the input: outputs high (+5V) when input is low, and vice versa.
pub struct LogicNot {
    spec: PortSpec,
}

impl LogicNot {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![PortDef::new(0, "in", SignalKind::Gate)],
                outputs: vec![PortDef::new(10, "out", SignalKind::Gate)],
            },
        }
    }
}

impl Default for LogicNot {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for LogicNot {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0) > 2.5;
        outputs.set(10, if input { 0.0 } else { 5.0 });
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "logic_not"
    }
}

/// Comparator
///
/// Compares two CV inputs and outputs a gate based on the comparison.
/// Outputs high (+5V) when A > B, otherwise low (0V).
/// Also provides inverted output (A <= B).
pub struct Comparator {
    spec: PortSpec,
}

impl Comparator {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "a", SignalKind::CvBipolar),
                    PortDef::new(1, "b", SignalKind::CvBipolar),
                ],
                outputs: vec![
                    PortDef::new(10, "gt", SignalKind::Gate), // A > B
                    PortDef::new(11, "lt", SignalKind::Gate), // A < B
                    PortDef::new(12, "eq", SignalKind::Gate), // A ≈ B (within threshold)
                ],
            },
        }
    }
}

impl Default for Comparator {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for Comparator {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let a = inputs.get_or(0, 0.0);
        let b = inputs.get_or(1, 0.0);

        // Use a small threshold for equality comparison (hysteresis)
        let threshold = 0.01;

        let gt = a > b + threshold;
        let lt = a < b - threshold;
        let eq = !gt && !lt;

        outputs.set(10, if gt { 5.0 } else { 0.0 });
        outputs.set(11, if lt { 5.0 } else { 0.0 });
        outputs.set(12, if eq { 5.0 } else { 0.0 });
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "comparator"
    }
}

/// Rectifier
///
/// Performs full-wave and half-wave rectification of audio/CV signals.
/// Also provides absolute value output.
pub struct Rectifier {
    spec: PortSpec,
}

impl Rectifier {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![PortDef::new(0, "in", SignalKind::Audio)],
                outputs: vec![
                    PortDef::new(10, "full", SignalKind::Audio), // Full-wave rectified
                    PortDef::new(11, "half_pos", SignalKind::Audio), // Half-wave (positive)
                    PortDef::new(12, "half_neg", SignalKind::Audio), // Half-wave (negative, inverted)
                    PortDef::new(13, "abs", SignalKind::CvUnipolar), // Absolute value (0-10V)
                ],
            },
        }
    }
}

impl Default for Rectifier {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for Rectifier {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);

        // Full-wave rectification: absolute value, keeps ±5V range as 0-5V
        outputs.set(10, Libm::<f64>::fabs(input));

        // Half-wave positive: pass positive, block negative
        outputs.set(11, Libm::<f64>::fmax(input, 0.0));

        // Half-wave negative: pass negative inverted, block positive
        outputs.set(12, Libm::<f64>::fmax(-input, 0.0));

        // Absolute value scaled to 0-10V unipolar (input ±5V -> output 0-10V)
        outputs.set(13, Libm::<f64>::fabs(input) * 2.0);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "rectifier"
    }
}

/// Precision Adder
///
/// A high-precision CV adder/mixer with multiple inputs.
/// Useful for combining V/Oct signals for transposition.
/// Includes a precision 1V/octave offset output for tuning.
pub struct PrecisionAdder {
    spec: PortSpec,
}

impl PrecisionAdder {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in1", SignalKind::VoltPerOctave),
                    PortDef::new(1, "in2", SignalKind::VoltPerOctave),
                    PortDef::new(2, "in3", SignalKind::CvBipolar),
                    PortDef::new(3, "in4", SignalKind::CvBipolar),
                ],
                outputs: vec![
                    PortDef::new(10, "sum", SignalKind::VoltPerOctave),
                    PortDef::new(11, "inv", SignalKind::VoltPerOctave), // Inverted sum
                ],
            },
        }
    }
}

impl Default for PrecisionAdder {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for PrecisionAdder {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let sum = inputs.get_or(0, 0.0)
            + inputs.get_or(1, 0.0)
            + inputs.get_or(2, 0.0)
            + inputs.get_or(3, 0.0);

        outputs.set(10, sum);
        outputs.set(11, -sum);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "precision_adder"
    }
}

/// Voltage-Controlled Switch
///
/// Routes one of two inputs to the output based on a control signal.
/// When CV > 2.5V, output = B; otherwise output = A.
/// Also provides complementary outputs.
pub struct VcSwitch {
    spec: PortSpec,
}

impl VcSwitch {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "a", SignalKind::Audio),
                    PortDef::new(1, "b", SignalKind::Audio),
                    PortDef::new(2, "cv", SignalKind::Gate).with_default(0.0),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio), // Selected input
                    PortDef::new(11, "a_out", SignalKind::Audio), // A when selected, else 0
                    PortDef::new(12, "b_out", SignalKind::Audio), // B when selected, else 0
                ],
            },
        }
    }
}

impl Default for VcSwitch {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for VcSwitch {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let a = inputs.get_or(0, 0.0);
        let b = inputs.get_or(1, 0.0);
        let cv = inputs.get_or(2, 0.0);

        let select_b = cv > 2.5;

        if select_b {
            outputs.set(10, b);
            outputs.set(11, 0.0);
            outputs.set(12, b);
        } else {
            outputs.set(10, a);
            outputs.set(11, a);
            outputs.set(12, 0.0);
        }
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "vc_switch"
    }
}

/// Bernoulli Gate
///
/// A probabilistic gate router. On each trigger, randomly routes the signal
/// to one of two outputs based on a probability parameter.
/// Inspired by Mutable Instruments Branches.
pub struct BernoulliGate {
    last_trigger: f64,
    spec: PortSpec,
}

impl BernoulliGate {
    pub fn new() -> Self {
        Self {
            last_trigger: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "trig", SignalKind::Trigger),
                    PortDef::new(1, "prob", SignalKind::CvUnipolar).with_default(5.0), // 50% default
                ],
                outputs: vec![
                    PortDef::new(10, "a", SignalKind::Trigger),   // Output A
                    PortDef::new(11, "b", SignalKind::Trigger),   // Output B
                    PortDef::new(12, "gate_a", SignalKind::Gate), // Latched gate A
                    PortDef::new(13, "gate_b", SignalKind::Gate), // Latched gate B
                ],
            },
        }
    }
}

impl Default for BernoulliGate {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for BernoulliGate {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let trigger = inputs.get_or(0, 0.0);
        let prob = (inputs.get_or(1, 5.0) / 10.0).clamp(0.0, 1.0); // Normalize to 0-1

        let rising_edge = trigger > 2.5 && self.last_trigger <= 2.5;
        self.last_trigger = trigger;

        // Default: no trigger output
        let mut trig_a = 0.0;
        let mut trig_b = 0.0;

        if rising_edge {
            // Random decision based on probability
            let rand_val: f64 = rng::random();
            if rand_val < prob {
                trig_a = 5.0;
            } else {
                trig_b = 5.0;
            }
        }

        // Trigger outputs (momentary)
        outputs.set(10, trig_a);
        outputs.set(11, trig_b);

        // Gate outputs track which side was last triggered
        // These latch until the other side is triggered
        let gate_a = if trig_a > 0.0 {
            5.0
        } else if trig_b > 0.0 {
            0.0
        } else {
            outputs.get_or(12, 0.0) // Keep previous state
        };
        let gate_b = if trig_b > 0.0 {
            5.0
        } else if trig_a > 0.0 {
            0.0
        } else {
            outputs.get_or(13, 0.0) // Keep previous state
        };

        outputs.set(12, gate_a);
        outputs.set(13, gate_b);
    }

    fn reset(&mut self) {
        self.last_trigger = 0.0;
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "bernoulli_gate"
    }
}

/// Min module
///
/// Outputs the minimum of two input signals.
pub struct Min {
    spec: PortSpec,
}

impl Min {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "a", SignalKind::CvBipolar),
                    PortDef::new(1, "b", SignalKind::CvBipolar),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::CvBipolar)],
            },
        }
    }
}

impl Default for Min {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for Min {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let a = inputs.get_or(0, 0.0);
        let b = inputs.get_or(1, 0.0);
        outputs.set(10, Libm::<f64>::fmin(a, b));
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "min"
    }
}

/// Max module
///
/// Outputs the maximum of two input signals.
pub struct Max {
    spec: PortSpec,
}

impl Max {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "a", SignalKind::CvBipolar),
                    PortDef::new(1, "b", SignalKind::CvBipolar),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::CvBipolar)],
            },
        }
    }
}

impl Default for Max {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for Max {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let a = inputs.get_or(0, 0.0);
        let b = inputs.get_or(1, 0.0);
        outputs.set(10, Libm::<f64>::fmax(a, b));
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "max"
    }
}

// ============================================================================
// Planned Modules: ChordMemory
// ============================================================================

/// Chord type for the ChordMemory module
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChordType {
    Major,
    Minor,
    Seventh,
    MajorSeventh,
    MinorSeventh,
    Diminished,
    Augmented,
    Sus2,
    Sus4,
}

impl ChordType {
    /// Returns the semitone intervals for this chord type (relative to root)
    fn intervals(&self) -> &'static [i32] {
        match self {
            ChordType::Major => &[0, 4, 7],
            ChordType::Minor => &[0, 3, 7],
            ChordType::Seventh => &[0, 4, 7, 10],
            ChordType::MajorSeventh => &[0, 4, 7, 11],
            ChordType::MinorSeventh => &[0, 3, 7, 10],
            ChordType::Diminished => &[0, 3, 6],
            ChordType::Augmented => &[0, 4, 8],
            ChordType::Sus2 => &[0, 2, 7],
            ChordType::Sus4 => &[0, 5, 7],
        }
    }

    /// Select chord type from CV value (0.0-1.0)
    fn from_cv(cv: f64) -> Self {
        match (cv * 8.99) as u8 {
            0 => ChordType::Major,
            1 => ChordType::Minor,
            2 => ChordType::Seventh,
            3 => ChordType::MajorSeventh,
            4 => ChordType::MinorSeventh,
            5 => ChordType::Diminished,
            6 => ChordType::Augmented,
            7 => ChordType::Sus2,
            _ => ChordType::Sus4,
        }
    }
}

/// Chord Memory
///
/// Generates chord voicings from a root note. Outputs 4 V/Oct signals
/// representing chord voices. Supports 9 chord types with inversions
/// and voice spreading.
///
/// **Chord types** (selected via CV 0-1):
/// - Major, Minor, 7th, Maj7, Min7, Dim, Aug, Sus2, Sus4
///
/// **Inversion**: Rotates which note is the bass
/// **Spread**: Distributes voices across octaves
pub struct ChordMemory {
    spec: PortSpec,
}

impl ChordMemory {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "root", SignalKind::VoltPerOctave),
                    PortDef::new(1, "chord", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(2, "inversion", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(3, "spread", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "voice1", SignalKind::VoltPerOctave),
                    PortDef::new(11, "voice2", SignalKind::VoltPerOctave),
                    PortDef::new(12, "voice3", SignalKind::VoltPerOctave),
                    PortDef::new(13, "voice4", SignalKind::VoltPerOctave),
                ],
            },
        }
    }
}

impl Default for ChordMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for ChordMemory {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let root = inputs.get_or(0, 0.0);
        let chord_cv = inputs.get_or(1, 0.0).clamp(0.0, 1.0);
        let inversion_cv = inputs.get_or(2, 0.0).clamp(0.0, 1.0);
        let spread = inputs.get_or(3, 0.0).clamp(0.0, 1.0);

        let chord_type = ChordType::from_cv(chord_cv);
        let intervals = chord_type.intervals();
        let num_notes = intervals.len();

        // Calculate inversion (0, 1, 2, or 3)
        let inversion = ((inversion_cv * num_notes as f64) as usize) % num_notes;

        // Build chord voices
        let mut voices = [0.0f64; 4];
        for (i, voice) in voices.iter_mut().enumerate() {
            if i < num_notes {
                let interval_idx = (i + inversion) % num_notes;
                let semitones = intervals[interval_idx];

                // Add octave if the interval wrapped around due to inversion
                let octave_offset = if i + inversion >= num_notes { 1.0 } else { 0.0 };

                // Apply spread (voices spread across octaves)
                let spread_offset = spread * (i as f64 / 3.0);

                // Convert semitones to V/Oct (1V = 1 octave, so 1 semitone = 1/12 V)
                *voice = root + semitones as f64 / 12.0 + octave_offset + spread_offset;
            } else {
                // For 3-note chords, duplicate the root an octave up for voice 4
                // Apply spread to the duplicated voice as well
                let spread_offset = spread * (i as f64 / 3.0);
                *voice = root + 1.0 + spread_offset;
            }
        }

        outputs.set(10, voices[0]);
        outputs.set(11, voices[1]);
        outputs.set(12, voices[2]);
        outputs.set(13, voices[3]);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "chord_memory"
    }
}

// ============================================================================
// Planned Modules: ParametricEq
// ============================================================================

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
    /// 8 wavetables, each with 256 samples
    tables: [[f64; 256]; 8],
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
            tables: [[0.0; 256]; 8],
            phase: 0.0,
            prev_sync: 0.0,
            sample_rate,
            spec,
        };
        osc.generate_tables();
        osc
    }

    /// Generate all 8 wavetables with bandlimiting
    fn generate_tables(&mut self) {
        let n = Self::TABLE_SIZE;

        // Sine wave (pure)
        for i in 0..n {
            let phase = (i as f64) / (n as f64);
            self.tables[0][i] = Libm::<f64>::sin(phase * 2.0 * core::f64::consts::PI);
        }

        // Triangle wave (bandlimited with first 8 harmonics)
        for i in 0..n {
            let phase = (i as f64) / (n as f64);
            let mut sample = 0.0;
            for h in 0..8 {
                let harmonic = (2 * h + 1) as f64;
                let sign = if h % 2 == 0 { 1.0 } else { -1.0 };
                sample += sign * Libm::<f64>::sin(phase * harmonic * 2.0 * core::f64::consts::PI)
                    / (harmonic * harmonic);
            }
            self.tables[1][i] = sample * (8.0 / (core::f64::consts::PI * core::f64::consts::PI));
        }

        // Saw wave (bandlimited with first 16 harmonics)
        for i in 0..n {
            let phase = (i as f64) / (n as f64);
            let mut sample = 0.0;
            for h in 1..=16 {
                let harmonic = h as f64;
                let sign = if h % 2 == 0 { 1.0 } else { -1.0 };
                sample += sign * Libm::<f64>::sin(phase * harmonic * 2.0 * core::f64::consts::PI)
                    / harmonic;
            }
            self.tables[2][i] = sample * (2.0 / core::f64::consts::PI);
        }

        // Square wave (bandlimited with first 8 odd harmonics)
        for i in 0..n {
            let phase = (i as f64) / (n as f64);
            let mut sample = 0.0;
            for h in 0..8 {
                let harmonic = (2 * h + 1) as f64;
                sample +=
                    Libm::<f64>::sin(phase * harmonic * 2.0 * core::f64::consts::PI) / harmonic;
            }
            self.tables[3][i] = sample * (4.0 / core::f64::consts::PI);
        }

        // Pulse 25% (bandlimited)
        for i in 0..n {
            let phase = (i as f64) / (n as f64);
            let mut sample = 0.0;
            for h in 1..=16 {
                let harmonic = h as f64;
                let duty = 0.25;
                let coef = Libm::<f64>::sin(core::f64::consts::PI * harmonic * duty) / harmonic;
                sample += coef * Libm::<f64>::sin(phase * harmonic * 2.0 * core::f64::consts::PI);
            }
            self.tables[4][i] = sample * 2.0;
        }

        // Pulse 12.5% (bandlimited)
        for i in 0..n {
            let phase = (i as f64) / (n as f64);
            let mut sample = 0.0;
            for h in 1..=16 {
                let harmonic = h as f64;
                let duty = 0.125;
                let coef = Libm::<f64>::sin(core::f64::consts::PI * harmonic * duty) / harmonic;
                sample += coef * Libm::<f64>::sin(phase * harmonic * 2.0 * core::f64::consts::PI);
            }
            self.tables[5][i] = sample * 2.0;
        }

        // Formant "ah" (resonant peaks at F1=700Hz, F2=1200Hz, F3=2500Hz relative to fundamental)
        for i in 0..n {
            let phase = (i as f64) / (n as f64);
            let fundamental = Libm::<f64>::sin(phase * 2.0 * core::f64::consts::PI);
            let f1 = Libm::<f64>::sin(phase * 2.7 * 2.0 * core::f64::consts::PI) * 0.5; // ~F1
            let f2 = Libm::<f64>::sin(phase * 4.6 * 2.0 * core::f64::consts::PI) * 0.3; // ~F2
            let f3 = Libm::<f64>::sin(phase * 9.6 * 2.0 * core::f64::consts::PI) * 0.15; // ~F3
            self.tables[6][i] = (fundamental + f1 + f2 + f3) * 0.5;
        }

        // Formant "oh" (resonant peaks at F1=400Hz, F2=800Hz, F3=2600Hz relative to fundamental)
        for i in 0..n {
            let phase = (i as f64) / (n as f64);
            let fundamental = Libm::<f64>::sin(phase * 2.0 * core::f64::consts::PI);
            let f1 = Libm::<f64>::sin(phase * 1.5 * 2.0 * core::f64::consts::PI) * 0.6; // ~F1
            let f2 = Libm::<f64>::sin(phase * 3.0 * 2.0 * core::f64::consts::PI) * 0.4; // ~F2
            let f3 = Libm::<f64>::sin(phase * 10.0 * 2.0 * core::f64::consts::PI) * 0.15; // ~F3
            self.tables[7][i] = (fundamental + f1 + f2 + f3) * 0.5;
        }
    }

    /// Read from a wavetable with linear interpolation
    fn read_table(&self, table_idx: usize, phase: f64) -> f64 {
        let table = &self.tables[table_idx % Self::NUM_TABLES];
        let pos = phase * (Self::TABLE_SIZE as f64);
        let idx0 = (pos as usize) % Self::TABLE_SIZE;
        let idx1 = (idx0 + 1) % Self::TABLE_SIZE;
        let frac = pos - pos.floor();

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
        if sync > 2.5 && self.prev_sync <= 2.5 {
            self.phase = 0.0;
        }
        self.prev_sync = sync;

        // Calculate frequency from V/Oct (0V = C4 = 261.63 Hz)
        let frequency = 261.63 * Libm::<f64>::pow(2.0, v_oct);
        let phase_inc = frequency / self.sample_rate;

        // Select tables based on table CV and morph
        // Table CV selects base table (0-7), morph crossfades to next table
        let table_pos = table_cv * ((Self::NUM_TABLES - 1) as f64);
        let table_idx = (table_pos as usize).min(Self::NUM_TABLES - 2);
        let table_frac = table_pos - (table_idx as f64);

        // Blend morph and table fraction for smooth transitions
        let blend = (table_frac + morph).min(1.0);

        // Read from both tables and crossfade
        let sample0 = self.read_table(table_idx, self.phase);
        let sample1 = self.read_table(table_idx + 1, self.phase);
        let sample = sample0 * (1.0 - blend) + sample1 * blend;

        // Advance phase
        self.phase += phase_inc;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
        }

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
        let frequency = 261.63 * Libm::<f64>::pow(2.0, v_oct_with_vibrato);
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

        // Advance phases
        self.phase += phase_inc;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        self.vibrato_phase += Self::VIBRATO_RATE / self.sample_rate;
        while self.vibrato_phase >= 1.0 {
            self.vibrato_phase -= 1.0;
        }

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

/// Granular pitch shifter
///
/// Real-time pitch shifting using two overlapping grains with crossfade.
/// Uses a circular delay buffer with variable playback rate.
///
/// # Ports
/// - Input 0: Audio input
/// - Input 1: Pitch shift in semitones (-24 to +24, bipolar CV maps to range)
/// - Input 2: Window size (0-1 CV maps to 10-100ms)
/// - Input 3: Wet/dry mix (0-1)
/// - Output 10: Audio output
pub struct PitchShifter {
    /// Circular delay buffer (100ms at 48kHz max)
    buffer: [f64; 4800],
    /// Write position in buffer
    write_pos: usize,
    /// Two grain positions (fractional)
    grain_pos: [f64; 2],
    /// Two grain phases (0-1 for window position)
    grain_phase: [f64; 2],
    sample_rate: f64,
    spec: PortSpec,
}

impl PitchShifter {
    /// Maximum buffer size in samples (100ms at 48kHz)
    const BUFFER_SIZE: usize = 4800;

    pub fn new(sample_rate: f64) -> Self {
        let spec = PortSpec {
            inputs: vec![
                PortDef::new(0, "in", SignalKind::Audio),
                PortDef::new(1, "shift", SignalKind::CvBipolar).with_default(0.0),
                PortDef::new(2, "window", SignalKind::CvUnipolar).with_default(0.5),
                PortDef::new(3, "mix", SignalKind::CvUnipolar).with_default(1.0),
            ],
            outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
        };

        Self {
            buffer: [0.0; Self::BUFFER_SIZE],
            write_pos: 0,
            grain_pos: [0.0, 0.5 * Self::BUFFER_SIZE as f64], // Start 180° out of phase
            grain_phase: [0.0, 0.5],                          // 50% phase offset
            sample_rate,
            spec,
        }
    }

    /// Hann window function (0-1 maps to 0-1-0)
    fn hann_window(phase: f64) -> f64 {
        0.5 * (1.0 - Libm::<f64>::cos(phase * 2.0 * core::f64::consts::PI))
    }

    /// Read from circular buffer with linear interpolation
    fn read_buffer(&self, pos: f64) -> f64 {
        let pos = pos.rem_euclid(Self::BUFFER_SIZE as f64);
        let idx0 = pos as usize;
        let idx1 = (idx0 + 1) % Self::BUFFER_SIZE;
        let frac = pos - pos.floor();

        self.buffer[idx0] * (1.0 - frac) + self.buffer[idx1] * frac
    }
}

impl Default for PitchShifter {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for PitchShifter {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);

        // Map inputs
        // Shift: bipolar CV ±5V maps to ±24 semitones
        let shift_semitones = (inputs.get_or(1, 0.0) / 5.0) * 24.0;
        let shift_semitones = shift_semitones.clamp(-24.0, 24.0);

        // Window size: 10-100ms
        let window_cv = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let window_ms = 10.0 + window_cv * 90.0;
        let window_samples = (window_ms * self.sample_rate / 1000.0) as usize;
        let window_samples = window_samples.min(Self::BUFFER_SIZE / 2);

        // Mix
        let mix = inputs.get_or(3, 1.0).clamp(0.0, 1.0);

        // Write input to circular buffer
        self.buffer[self.write_pos] = input / 5.0; // Normalize from audio
        self.write_pos = (self.write_pos + 1) % Self::BUFFER_SIZE;

        // Calculate playback rate
        let rate = Libm::<f64>::pow(2.0, shift_semitones / 12.0);
        let phase_inc = 1.0 / window_samples as f64;

        // Process both grains
        let mut wet_output = 0.0;

        for i in 0..2 {
            // Read from buffer at grain position
            let sample = self.read_buffer(self.grain_pos[i]);

            // Apply Hann window
            let window = Self::hann_window(self.grain_phase[i]);
            wet_output += sample * window;

            // Advance grain position (write_pos - offset, at playback rate)
            // When rate > 1 (pitch up), read faster than write
            // When rate < 1 (pitch down), read slower than write
            self.grain_pos[i] += rate;

            // Wrap grain position
            if self.grain_pos[i] >= Self::BUFFER_SIZE as f64 {
                self.grain_pos[i] -= Self::BUFFER_SIZE as f64;
            } else if self.grain_pos[i] < 0.0 {
                self.grain_pos[i] += Self::BUFFER_SIZE as f64;
            }

            // Advance phase
            self.grain_phase[i] += phase_inc;

            // Reset grain when phase completes
            if self.grain_phase[i] >= 1.0 {
                self.grain_phase[i] -= 1.0;
                // Reset position to current write position minus half window
                self.grain_pos[i] = (self.write_pos as f64 - window_samples as f64 * 0.5)
                    .rem_euclid(Self::BUFFER_SIZE as f64);
            }
        }

        // Mix wet and dry
        let dry = input / 5.0;
        let output = dry * (1.0 - mix) + wet_output * mix;

        outputs.set(10, output * 5.0); // Scale back to audio
    }

    fn reset(&mut self) {
        self.buffer = [0.0; Self::BUFFER_SIZE];
        self.write_pos = 0;
        self.grain_pos = [0.0, Self::BUFFER_SIZE as f64 * 0.5];
        self.grain_phase = [0.0, 0.5];
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.reset();
    }

    fn type_id(&self) -> &'static str {
        "pitch_shifter"
    }
}

/// Arpeggiator pattern types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArpPattern {
    /// Play notes ascending
    Up,
    /// Play notes descending
    Down,
    /// Play notes up then down
    UpDown,
    /// Play notes in random order
    Random,
}

impl ArpPattern {
    /// Get pattern from CV (0-1 maps to 4 patterns)
    fn from_cv(cv: f64) -> Self {
        let cv = cv.clamp(0.0, 1.0);
        if cv < 0.25 {
            ArpPattern::Up
        } else if cv < 0.5 {
            ArpPattern::Down
        } else if cv < 0.75 {
            ArpPattern::UpDown
        } else {
            ArpPattern::Random
        }
    }
}

/// Pattern-based arpeggiator
///
/// Captures held notes and plays them back in sequence on each clock pulse.
/// Supports multiple octave ranges and different playback patterns.
///
/// # Ports
/// - Input 0: V/Oct input note
/// - Input 1: Gate input (captures notes on rising edge)
/// - Input 2: Clock input (advances sequence)
/// - Input 3: Pattern select (0-1 CV maps to Up/Down/UpDown/Random)
/// - Input 4: Octave range (0-1 CV maps to 1-4 octaves)
/// - Input 5: Reset input (gate)
/// - Output 10: V/Oct output
/// - Output 11: Gate output
/// - Output 12: Trigger output (pulse on each step)
pub struct Arpeggiator {
    /// Held notes buffer (V/Oct values)
    held_notes: [f64; 8],
    /// Number of held notes
    num_notes: usize,
    /// Current step in sequence
    current_step: usize,
    /// Direction for up-down pattern (true = up)
    direction_up: bool,
    /// Previous gate state for edge detection
    prev_gate: f64,
    /// Previous clock state for edge detection
    prev_clock: f64,
    /// Previous reset state for edge detection
    prev_reset: f64,
    /// Random number generator
    rng: crate::rng::Rng,
    /// Output gate state
    gate_out: f64,
    /// Trigger countdown (samples remaining)
    trigger_countdown: usize,
    sample_rate: f64,
    spec: PortSpec,
}

impl Arpeggiator {
    /// Trigger pulse length in ms
    const TRIGGER_MS: f64 = 1.0;

    pub fn new(sample_rate: f64) -> Self {
        let spec = PortSpec {
            inputs: vec![
                PortDef::new(0, "v_oct", SignalKind::VoltPerOctave).with_default(0.0),
                PortDef::new(1, "gate", SignalKind::Gate).with_default(0.0),
                PortDef::new(2, "clock", SignalKind::Clock).with_default(0.0),
                PortDef::new(3, "pattern", SignalKind::CvUnipolar).with_default(0.0),
                PortDef::new(4, "octaves", SignalKind::CvUnipolar).with_default(0.0),
                PortDef::new(5, "reset", SignalKind::Gate).with_default(0.0),
            ],
            outputs: vec![
                PortDef::new(10, "v_oct_out", SignalKind::VoltPerOctave),
                PortDef::new(11, "gate_out", SignalKind::Gate),
                PortDef::new(12, "trigger", SignalKind::Trigger),
            ],
        };

        Self {
            held_notes: [0.0; 8],
            num_notes: 0,
            current_step: 0,
            direction_up: true,
            prev_gate: 0.0,
            prev_clock: 0.0,
            prev_reset: 0.0,
            rng: crate::rng::Rng::from_seed(42),
            gate_out: 0.0,
            trigger_countdown: 0,
            sample_rate,
            spec,
        }
    }

    /// Add a note to the held notes buffer (keeps sorted)
    fn add_note(&mut self, note: f64) {
        if self.num_notes >= 8 {
            return;
        }

        // Insert in sorted order
        let mut insert_pos = self.num_notes;
        for i in 0..self.num_notes {
            if note < self.held_notes[i] {
                insert_pos = i;
                break;
            }
        }

        // Shift notes up
        for i in (insert_pos..self.num_notes).rev() {
            self.held_notes[i + 1] = self.held_notes[i];
        }

        self.held_notes[insert_pos] = note;
        self.num_notes += 1;
    }

    /// Remove a note from the held notes buffer
    pub fn remove_note(&mut self, note: f64) {
        // Find the note (with small tolerance for floating point)
        let mut found_idx = None;
        for i in 0..self.num_notes {
            if (self.held_notes[i] - note).abs() < 0.001 {
                found_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = found_idx {
            // Shift notes down
            for i in idx..self.num_notes - 1 {
                self.held_notes[i] = self.held_notes[i + 1];
            }
            self.num_notes -= 1;
        }
    }

    /// Get the current note based on step and pattern
    fn get_current_note(&mut self, pattern: ArpPattern, octaves: usize) -> f64 {
        if self.num_notes == 0 {
            return 0.0;
        }

        let total_steps = self.num_notes * octaves;
        let step = self.current_step % total_steps;

        let note_idx = match pattern {
            ArpPattern::Up => step % self.num_notes,
            ArpPattern::Down => (self.num_notes - 1) - (step % self.num_notes),
            ArpPattern::UpDown => {
                // Calculate position in up-down cycle
                let cycle_len = if self.num_notes > 1 {
                    (self.num_notes - 1) * 2
                } else {
                    1
                };
                let pos = step % cycle_len;
                if pos < self.num_notes {
                    pos
                } else {
                    (self.num_notes - 1) * 2 - pos
                }
            }
            ArpPattern::Random => (self.rng.next_u64() as usize) % self.num_notes,
        };

        let octave = step / self.num_notes;
        let base_note = self.held_notes[note_idx % self.num_notes];

        base_note + octave as f64 // Add octave offset (1V per octave)
    }
}

impl Default for Arpeggiator {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Arpeggiator {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let v_oct = inputs.get_or(0, 0.0);
        let gate = inputs.get_or(1, 0.0);
        let clock = inputs.get_or(2, 0.0);
        let pattern_cv = inputs.get_or(3, 0.0);
        let octaves_cv = inputs.get_or(4, 0.0);
        let reset = inputs.get_or(5, 0.0);

        let pattern = ArpPattern::from_cv(pattern_cv);
        let octaves = (1.0 + octaves_cv.clamp(0.0, 1.0) * 3.0) as usize; // 1-4 octaves

        // Handle gate input (note capture)
        // Notes are captured on gate rising edge and persist until reset
        if gate > 2.5 && self.prev_gate <= 2.5 {
            // Rising edge - add note
            self.add_note(v_oct);
        }
        self.prev_gate = gate;

        // Handle reset
        if reset > 2.5 && self.prev_reset <= 2.5 {
            self.current_step = 0;
            self.direction_up = true;
        }
        self.prev_reset = reset;

        // Handle clock (advance sequence)
        let mut trigger_out = 0.0;
        let clock_rising = clock > 2.5 && self.prev_clock <= 2.5 && self.num_notes > 0;

        if clock_rising {
            self.gate_out = 5.0;
            // Start trigger pulse
            self.trigger_countdown = (Self::TRIGGER_MS * self.sample_rate / 1000.0) as usize;
            trigger_out = 5.0;
        }
        self.prev_clock = clock;

        // Update trigger
        if self.trigger_countdown > 0 {
            self.trigger_countdown -= 1;
            trigger_out = 5.0;
        }

        // Gate follows clock (simplified - stays high while clock is high)
        if clock <= 2.5 {
            self.gate_out = 0.0;
        }

        // Get current note
        let v_oct_out = if self.num_notes > 0 {
            self.get_current_note(pattern, octaves)
        } else {
            0.0
        };

        // Advance step AFTER outputting current note
        if clock_rising {
            self.current_step += 1;
        }

        outputs.set(10, v_oct_out);
        outputs.set(
            11,
            if self.num_notes > 0 {
                self.gate_out
            } else {
                0.0
            },
        );
        outputs.set(12, trigger_out);
    }

    fn reset(&mut self) {
        self.held_notes = [0.0; 8];
        self.num_notes = 0;
        self.current_step = 0;
        self.direction_up = true;
        self.prev_gate = 0.0;
        self.prev_clock = 0.0;
        self.prev_reset = 0.0;
        self.gate_out = 0.0;
        self.trigger_countdown = 0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "arpeggiator"
    }
}

// =============================================================================
// Reverb - Algorithmic Reverb (Freeverb Style)
// =============================================================================

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
            let length_r = (self.comb_lengths[i] + STEREO_SPREAD).min(MAX_COMB_SIZE - 1);
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

            let length_r = (self.allpass_lengths[i] + STEREO_SPREAD).min(MAX_ALLPASS_SIZE - 1);
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

/// Maximum number of vocoder bands
const MAX_VOCODER_BANDS: usize = 16;

/// Minimum frequency for vocoder bands (Hz)
const VOCODER_FREQ_MIN: f64 = 100.0;

/// Maximum frequency for vocoder bands (Hz)
const VOCODER_FREQ_MAX: f64 = 8000.0;

/// Spectral vocoder with configurable band count
///
/// Uses bandpass filter banks for both analysis (modulator) and synthesis
/// (carrier), with envelope followers to extract amplitude from the modulator
/// and apply it to the carrier.
///
/// # Ports
/// - Input 0: Carrier input (typically oscillator)
/// - Input 1: Modulator input (typically voice)
/// - Input 2: Number of bands (CV 0-1 maps to 4-16 bands)
/// - Input 3: Envelope attack (0-1)
/// - Input 4: Envelope release (0-1)
/// - Output 10: Vocoded output
pub struct Vocoder {
    // Analysis (modulator) filters - state variable filter state [LP, HP] per band
    analysis_state: [[f64; 2]; MAX_VOCODER_BANDS],
    // Synthesis (carrier) filters
    synthesis_state: [[f64; 2]; MAX_VOCODER_BANDS],
    // Envelope followers for each band
    envelopes: [f64; MAX_VOCODER_BANDS],

    // Pre-computed band frequencies
    band_freqs: [f64; MAX_VOCODER_BANDS],

    sample_rate: f64,
    spec: PortSpec,
}

impl Vocoder {
    /// Create a new vocoder with the given sample rate
    pub fn new(sample_rate: f64) -> Self {
        let mut vocoder = Self {
            analysis_state: [[0.0; 2]; MAX_VOCODER_BANDS],
            synthesis_state: [[0.0; 2]; MAX_VOCODER_BANDS],
            envelopes: [0.0; MAX_VOCODER_BANDS],
            band_freqs: [0.0; MAX_VOCODER_BANDS],
            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "carrier", SignalKind::Audio),
                    PortDef::new(1, "modulator", SignalKind::Audio),
                    PortDef::new(2, "bands", SignalKind::CvUnipolar).with_default(1.0),
                    PortDef::new(3, "attack", SignalKind::CvUnipolar).with_default(0.3),
                    PortDef::new(4, "release", SignalKind::CvUnipolar).with_default(0.3),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        };
        vocoder.compute_band_freqs();
        vocoder
    }

    /// Compute logarithmically spaced band frequencies
    fn compute_band_freqs(&mut self) {
        let log_min = Libm::<f64>::log2(VOCODER_FREQ_MIN);
        let log_max = Libm::<f64>::log2(VOCODER_FREQ_MAX);

        for i in 0..MAX_VOCODER_BANDS {
            let t = i as f64 / (MAX_VOCODER_BANDS - 1) as f64;
            let log_freq = log_min + t * (log_max - log_min);
            self.band_freqs[i] = Libm::<f64>::exp2(log_freq);
        }
    }

    /// Process a single band using a state variable filter (bandpass)
    /// Returns the bandpass output
    #[inline]
    fn process_svf_bandpass(
        state: &mut [f64; 2],
        input: f64,
        freq: f64,
        q: f64,
        sample_rate: f64,
    ) -> f64 {
        // Frequency coefficient
        let f = 2.0 * Libm::<f64>::sin(core::f64::consts::PI * freq / sample_rate);
        let f = f.min(0.99); // Stability limit

        // Q factor (resonance)
        let q_inv = 1.0 / q;

        // State variable filter
        let low = state[0];
        let high = input - low - q_inv * state[1];
        let band = f * high + state[1];
        let new_low = f * band + low;

        state[0] = new_low;
        state[1] = band;

        band
    }
}

impl Default for Vocoder {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Vocoder {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let carrier = inputs.get_or(0, 0.0);
        let modulator = inputs.get_or(1, 0.0);
        let bands_cv = inputs.get_or(2, 1.0).clamp(0.0, 1.0);
        let attack_cv = inputs.get_or(3, 0.3).clamp(0.0, 1.0);
        let release_cv = inputs.get_or(4, 0.3).clamp(0.0, 1.0);

        // Map CV to band count (4-16)
        let num_bands = (4.0 + bands_cv * 12.0).round() as usize;
        let num_bands = num_bands.min(MAX_VOCODER_BANDS);

        // Compute envelope coefficients (10ms to 200ms range)
        let attack_time = 0.01 + attack_cv * 0.19;
        let release_time = 0.01 + release_cv * 0.19;
        let attack_coef = Libm::<f64>::exp(-1.0 / (attack_time * self.sample_rate));
        let release_coef = Libm::<f64>::exp(-1.0 / (release_time * self.sample_rate));

        // Q factor for bandpass filters
        let q = 2.0;

        let mut output = 0.0;

        for i in 0..num_bands {
            let freq = self.band_freqs[i * MAX_VOCODER_BANDS / num_bands];

            // Analysis path: filter modulator and extract envelope
            let analysis_band = Self::process_svf_bandpass(
                &mut self.analysis_state[i],
                modulator,
                freq,
                q,
                self.sample_rate,
            );

            // Envelope follower
            let rectified = analysis_band.abs();
            if rectified > self.envelopes[i] {
                self.envelopes[i] =
                    attack_coef * self.envelopes[i] + (1.0 - attack_coef) * rectified;
            } else {
                self.envelopes[i] =
                    release_coef * self.envelopes[i] + (1.0 - release_coef) * rectified;
            }

            // Synthesis path: filter carrier and apply envelope
            let synthesis_band = Self::process_svf_bandpass(
                &mut self.synthesis_state[i],
                carrier,
                freq,
                q,
                self.sample_rate,
            );

            // Apply envelope to carrier band
            output += synthesis_band * self.envelopes[i];
        }

        // Normalize by number of bands to prevent clipping
        output /= num_bands as f64;

        // Scale output
        outputs.set(10, output * 4.0);
    }

    fn reset(&mut self) {
        self.analysis_state = [[0.0; 2]; MAX_VOCODER_BANDS];
        self.synthesis_state = [[0.0; 2]; MAX_VOCODER_BANDS];
        self.envelopes = [0.0; MAX_VOCODER_BANDS];
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.compute_band_freqs();
        self.reset();
    }

    fn type_id(&self) -> &'static str {
        "vocoder"
    }
}

// =============================================================================
// Granular - Granular Synthesis/Processing Engine
// =============================================================================

/// Maximum number of concurrent grains
const MAX_GRAINS: usize = 16;

/// Granular buffer size (2 seconds at 48kHz)
const GRANULAR_BUFFER_SIZE: usize = 96000;

/// Represents a single active grain
#[derive(Clone, Copy)]
struct Grain {
    /// Whether this grain is active
    active: bool,
    /// Start position in the buffer (samples)
    start_pos: usize,
    /// Current phase within the grain (0.0 to 1.0)
    phase: f64,
    /// Grain size in samples
    size: usize,
    /// Playback speed (1.0 = normal, 2.0 = octave up)
    speed: f64,
}

impl Default for Grain {
    fn default() -> Self {
        Self {
            active: false,
            start_pos: 0,
            phase: 0.0,
            size: 4410, // 100ms default
            speed: 1.0,
        }
    }
}

/// Granular synthesis/processing engine
///
/// Records input audio into a circular buffer and plays back overlapping
/// grains with individual pitch shifting and envelope shaping.
///
/// # Ports
/// - Input 0: Audio input
/// - Input 1: Playback position (0-1 maps to buffer position)
/// - Input 2: Grain size (0-1 maps to 10ms-500ms)
/// - Input 3: Density (0-1 maps to 1-20 grains per second)
/// - Input 4: Pitch shift in semitones (-24 to +24)
/// - Input 5: Spray (position randomization, 0-1)
/// - Input 6: Freeze (gate > 2.5V stops recording)
/// - Output 10: Processed output
pub struct Granular {
    /// Circular input buffer
    buffer: Vec<f64>,
    /// Write position in buffer
    write_pos: usize,

    /// Pool of grains
    grains: [Grain; MAX_GRAINS],

    /// Timer for spawning new grains (counts down)
    spawn_timer: usize,

    /// Random number generator for spray and density jitter
    rng: crate::rng::Rng,

    sample_rate: f64,
    spec: PortSpec,
}

impl Granular {
    /// Create a new granular processor
    pub fn new(sample_rate: f64) -> Self {
        Self {
            buffer: vec![0.0; GRANULAR_BUFFER_SIZE],
            write_pos: 0,
            grains: [Grain::default(); MAX_GRAINS],
            spawn_timer: 0,
            rng: crate::rng::Rng::from_seed(42),
            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "position", SignalKind::CvUnipolar).with_default(0.5),
                    PortDef::new(2, "size", SignalKind::CvUnipolar).with_default(0.3),
                    PortDef::new(3, "density", SignalKind::CvUnipolar).with_default(0.5),
                    PortDef::new(4, "pitch", SignalKind::CvBipolar).with_default(0.0),
                    PortDef::new(5, "spray", SignalKind::CvUnipolar).with_default(0.1),
                    PortDef::new(6, "freeze", SignalKind::Gate).with_default(0.0),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }

    /// Compute Hann window value for grain envelope
    #[inline]
    fn hann_window(phase: f64) -> f64 {
        0.5 * (1.0 - Libm::<f64>::cos(2.0 * core::f64::consts::PI * phase))
    }

    /// Read from buffer with linear interpolation
    #[inline]
    pub fn read_buffer(&self, pos: f64) -> f64 {
        let pos = pos % GRANULAR_BUFFER_SIZE as f64;
        let index = pos as usize;
        let frac = pos - index as f64;

        let s0 = self.buffer[index % GRANULAR_BUFFER_SIZE];
        let s1 = self.buffer[(index + 1) % GRANULAR_BUFFER_SIZE];

        s0 + frac * (s1 - s0)
    }

    /// Spawn a new grain
    fn spawn_grain(&mut self, position: f64, size: usize, speed: f64, spray: f64) {
        // Find an inactive grain slot
        for grain in &mut self.grains {
            if !grain.active {
                // Calculate position with spray randomization
                let spray_offset = if spray > 0.0 {
                    (self.rng.next_f64() - 0.5) * spray * GRANULAR_BUFFER_SIZE as f64 * 0.5
                } else {
                    0.0
                };

                let base_pos = position * GRANULAR_BUFFER_SIZE as f64;
                let pos = (base_pos + spray_offset) as usize % GRANULAR_BUFFER_SIZE;

                grain.active = true;
                grain.start_pos = pos;
                grain.phase = 0.0;
                grain.size = size.max(100); // Minimum 100 samples
                grain.speed = speed;
                break;
            }
        }
    }
}

impl Default for Granular {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Granular {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let position = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
        let size_cv = inputs.get_or(2, 0.3).clamp(0.0, 1.0);
        let density_cv = inputs.get_or(3, 0.5).clamp(0.0, 1.0);
        let pitch_cv = inputs.get_or(4, 0.0).clamp(-5.0, 5.0);
        let spray = inputs.get_or(5, 0.1).clamp(0.0, 1.0);
        let freeze = inputs.get_or(6, 0.0);

        // Grain size: 10ms to 500ms
        let size_samples = ((0.01 + size_cv * 0.49) * self.sample_rate) as usize;

        // Density: 1-20 grains per second
        let grains_per_sec = 1.0 + density_cv * 19.0;
        let spawn_interval = (self.sample_rate / grains_per_sec) as usize;

        // Pitch shift: -5V to +5V maps to -60 to +60 semitones
        let semitones = pitch_cv * 12.0;
        let speed = Libm::<f64>::exp2(semitones / 12.0);

        // Record to buffer (unless frozen)
        if freeze <= 2.5 {
            self.buffer[self.write_pos] = input;
            self.write_pos = (self.write_pos + 1) % GRANULAR_BUFFER_SIZE;
        }

        // Spawn new grains based on density
        if self.spawn_timer == 0 {
            self.spawn_grain(position, size_samples, speed, spray);

            // Add jitter to spawn interval (±20%)
            let jitter = 1.0 + (self.rng.next_f64() - 0.5) * 0.4;
            self.spawn_timer = ((spawn_interval as f64) * jitter) as usize;
        } else {
            self.spawn_timer -= 1;
        }

        // Process all active grains
        let mut output = 0.0;
        let mut active_count = 0;

        for i in 0..MAX_GRAINS {
            if self.grains[i].active {
                let grain = &self.grains[i];

                // Calculate read position
                let read_offset = grain.phase * grain.size as f64 * grain.speed;
                let read_pos = grain.start_pos as f64 + read_offset;

                // Apply Hann window envelope
                let envelope = Self::hann_window(grain.phase);

                // Read from buffer (inline to avoid borrow issues)
                let pos = read_pos % GRANULAR_BUFFER_SIZE as f64;
                let index = pos as usize;
                let frac = pos - index as f64;
                let s0 = self.buffer[index % GRANULAR_BUFFER_SIZE];
                let s1 = self.buffer[(index + 1) % GRANULAR_BUFFER_SIZE];
                let sample = s0 + frac * (s1 - s0);

                output += sample * envelope;
                active_count += 1;

                // Advance phase and check completion
                let new_phase = self.grains[i].phase + 1.0 / self.grains[i].size as f64;
                self.grains[i].phase = new_phase;

                if new_phase >= 1.0 {
                    self.grains[i].active = false;
                }
            }
        }

        // Normalize output
        if active_count > 0 {
            output /= (active_count as f64).sqrt();
        }

        outputs.set(10, output);
    }

    fn reset(&mut self) {
        self.buffer.iter_mut().for_each(|x| *x = 0.0);
        self.write_pos = 0;
        self.grains = [Grain::default(); MAX_GRAINS];
        self.spawn_timer = 0;
        self.rng = crate::rng::Rng::from_seed(42);
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.reset();
    }

    fn type_id(&self) -> &'static str {
        "granular"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_mixer() {
        let mut mixer = Mixer::new(4);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 1.0);
        inputs.set(1, 2.0);
        inputs.set(2, 3.0);
        inputs.set(3, 4.0);

        mixer.tick(&inputs, &mut outputs);

        let out = outputs.get(100).unwrap();
        assert!((out - 10.0).abs() < 0.01);
    }

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
    fn test_bitcrusher() {
        let mut bc = Bitcrusher::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 2.5);
        inputs.set(1, 0.3); // Low bit depth
        inputs.set(2, 0.5); // Some downsampling
        bc.tick(&inputs, &mut outputs);

        let out = outputs.get(10).unwrap();
        assert!(out.is_finite());
    }

    #[test]
    fn test_bitcrusher_default() {
        let bc = Bitcrusher::default();
        assert_eq!(bc.type_id(), "bitcrusher");
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
    fn test_step_sequencer() {
        let mut seq = StepSequencer::new();
        seq.set_step(0, 0.0, true);
        seq.set_step(1, 0.5, true);
        seq.set_step(2, 1.0, true);

        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Initial state
        seq.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 0.0).abs() < 0.01);

        // Clock rising edge
        inputs.set(0, 5.0);
        seq.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 0.5).abs() < 0.01);

        // Clock falling edge, then rising again
        inputs.set(0, 0.0);
        seq.tick(&inputs, &mut outputs);
        inputs.set(0, 5.0);
        seq.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_sample_and_hold() {
        let mut sh = SampleAndHold::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set input value, no trigger
        inputs.set(0, 3.0);
        inputs.set(1, 0.0);
        sh.tick(&inputs, &mut outputs);
        // Initial held value should be 0
        assert!((outputs.get(10).unwrap() - 0.0).abs() < 0.01);

        // Trigger rising edge - should sample input
        inputs.set(1, 5.0);
        sh.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 3.0).abs() < 0.01);

        // Change input, but no new trigger - should hold previous value
        inputs.set(0, 7.0);
        sh.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 3.0).abs() < 0.01);

        // New trigger - should sample new value
        inputs.set(1, 0.0);
        sh.tick(&inputs, &mut outputs);
        inputs.set(1, 5.0);
        sh.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 7.0).abs() < 0.01);
    }

    #[test]
    fn test_slew_limiter() {
        let mut slew = SlewLimiter::new(1000.0); // 1kHz sample rate
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set rise/fall rates (normalized 0-1)
        inputs.set(1, 0.5); // Rise rate
        inputs.set(2, 0.5); // Fall rate

        // Step input from 0 to 5V
        inputs.set(0, 5.0);
        slew.tick(&inputs, &mut outputs);
        let first = outputs.get(10).unwrap();

        // Should start rising but not instantly reach target
        assert!(first > 0.0);
        assert!(first < 5.0);

        // Continue rising
        for _ in 0..100 {
            slew.tick(&inputs, &mut outputs);
        }
        // Should be close to target now
        let after_100 = outputs.get(10).unwrap();
        assert!(after_100 > first);
    }

    #[test]
    fn test_quantizer_chromatic() {
        let mut quant = Quantizer::new(Scale::Chromatic);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Exactly on a note
        inputs.set(0, 0.0); // C
        quant.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 0.0).abs() < 0.01);

        // Between C and C#
        inputs.set(0, 0.04); // 1/25 of a semitone above C
        quant.tick(&inputs, &mut outputs);
        // Should quantize to C (0.0)
        assert!((outputs.get(10).unwrap() - 0.0).abs() < 0.01);

        // Closer to C#
        inputs.set(0, 0.07);
        quant.tick(&inputs, &mut outputs);
        // Should quantize to C# (1/12 = 0.0833...)
        let expected_csharp = 1.0 / 12.0;
        assert!((outputs.get(10).unwrap() - expected_csharp).abs() < 0.01);
    }

    #[test]
    fn test_quantizer_major_scale() {
        let mut quant = Quantizer::new(Scale::Major);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // C# (1 semitone) should snap to C or D
        inputs.set(0, 1.0 / 12.0); // C#
        quant.tick(&inputs, &mut outputs);
        let out = outputs.get(10).unwrap();
        // Should be C (0) or D (2/12)
        assert!(out.abs() < 0.01 || (out - 2.0 / 12.0).abs() < 0.01);
    }

    #[test]
    fn test_clock() {
        let mut clock = Clock::new(1000.0); // 1kHz sample rate
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set tempo CV: 10V maps to 300 BPM (5 Hz), so 200 samples per beat
        inputs.set(0, 10.0); // Maximum tempo

        let mut trigger_count = 0;
        let mut last_trigger = 0.0;

        for _ in 0..1000 {
            clock.tick(&inputs, &mut outputs);
            let trigger = outputs.get(10).unwrap(); // Main clock output
            if trigger > 2.5 && last_trigger <= 2.5 {
                trigger_count += 1;
            }
            last_trigger = trigger;
        }

        // At 300 BPM (5 Hz), should get ~5 triggers per second
        // In 1000 samples at 1kHz, that's 5 triggers
        assert!(trigger_count >= 3);
    }

    #[test]
    fn test_attenuverter() {
        let mut att = Attenuverter::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test unity gain (5V = unity in 0-10V range)
        inputs.set(0, 5.0); // Input
        inputs.set(1, 5.0); // 5V = unity (1.0 multiplier)
        att.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 5.0).abs() < 0.1);

        // Test half attenuation (2.5V = 0.5 multiplier)
        inputs.set(1, 2.5);
        att.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 2.5).abs() < 0.1);

        // Test zero (0V = 0 multiplier)
        inputs.set(1, 0.0);
        att.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 0.0).abs() < 0.1);
    }

    #[test]
    fn test_multiple() {
        let mut mult = Multiple::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 3.5);
        mult.tick(&inputs, &mut outputs);

        // All 4 outputs should have the same value
        assert!((outputs.get(10).unwrap() - 3.5).abs() < 0.0001);
        assert!((outputs.get(11).unwrap() - 3.5).abs() < 0.0001);
        assert!((outputs.get(12).unwrap() - 3.5).abs() < 0.0001);
        assert!((outputs.get(13).unwrap() - 3.5).abs() < 0.0001);
    }

    // ========================================================================
    // Phase 2 Module Tests
    // ========================================================================

    #[test]
    fn test_ring_modulator() {
        let mut rm = RingModulator::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Both at +5V: should produce positive output
        inputs.set(0, 5.0); // Carrier
        inputs.set(1, 5.0); // Modulator
        rm.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 5.0).abs() < 0.1);

        // Opposite polarity: should produce negative output
        inputs.set(0, 5.0);
        inputs.set(1, -5.0);
        rm.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - (-5.0)).abs() < 0.1);

        // Zero modulator: should produce zero
        inputs.set(0, 5.0);
        inputs.set(1, 0.0);
        rm.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap()).abs() < 0.01);
    }

    #[test]
    fn test_crossfader() {
        let mut xf = Crossfader::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 5.0); // A
        inputs.set(1, -5.0); // B

        // Full A (pos = -5V)
        inputs.set(2, -5.0);
        xf.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 5.0).abs() < 0.1);

        // Full B (pos = +5V)
        inputs.set(2, 5.0);
        xf.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - (-5.0)).abs() < 0.1);

        // Center (pos = 0V): equal mix
        inputs.set(2, 0.0);
        xf.tick(&inputs, &mut outputs);
        // Equal power mix at center
        let out = outputs.get(10).unwrap();
        assert!(out.abs() < 1.0); // Should be near zero (equal mix of +5 and -5)
    }

    #[test]
    fn test_logic_and() {
        let mut gate = LogicAnd::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Both low
        inputs.set(0, 0.0);
        inputs.set(1, 0.0);
        gate.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() < 2.5);

        // One high
        inputs.set(0, 5.0);
        inputs.set(1, 0.0);
        gate.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() < 2.5);

        // Both high
        inputs.set(0, 5.0);
        inputs.set(1, 5.0);
        gate.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() > 2.5);
    }

    #[test]
    fn test_logic_or() {
        let mut gate = LogicOr::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Both low
        inputs.set(0, 0.0);
        inputs.set(1, 0.0);
        gate.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() < 2.5);

        // One high
        inputs.set(0, 5.0);
        inputs.set(1, 0.0);
        gate.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() > 2.5);

        // Both high
        inputs.set(0, 5.0);
        inputs.set(1, 5.0);
        gate.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() > 2.5);
    }

    #[test]
    fn test_logic_xor() {
        let mut gate = LogicXor::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Both low
        inputs.set(0, 0.0);
        inputs.set(1, 0.0);
        gate.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() < 2.5);

        // One high
        inputs.set(0, 5.0);
        inputs.set(1, 0.0);
        gate.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() > 2.5);

        // Both high
        inputs.set(0, 5.0);
        inputs.set(1, 5.0);
        gate.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() < 2.5);
    }

    #[test]
    fn test_logic_not() {
        let mut gate = LogicNot::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Low input
        inputs.set(0, 0.0);
        gate.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() > 2.5);

        // High input
        inputs.set(0, 5.0);
        gate.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() < 2.5);
    }

    #[test]
    fn test_comparator() {
        let mut cmp = Comparator::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // A > B
        inputs.set(0, 3.0);
        inputs.set(1, 1.0);
        cmp.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() > 2.5); // gt
        assert!(outputs.get(11).unwrap() < 2.5); // lt
        assert!(outputs.get(12).unwrap() < 2.5); // eq

        // A < B
        inputs.set(0, 1.0);
        inputs.set(1, 3.0);
        cmp.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() < 2.5); // gt
        assert!(outputs.get(11).unwrap() > 2.5); // lt
        assert!(outputs.get(12).unwrap() < 2.5); // eq

        // A ≈ B
        inputs.set(0, 2.0);
        inputs.set(1, 2.0);
        cmp.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() < 2.5); // gt
        assert!(outputs.get(11).unwrap() < 2.5); // lt
        assert!(outputs.get(12).unwrap() > 2.5); // eq
    }

    #[test]
    fn test_rectifier() {
        let mut rect = Rectifier::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Positive input
        inputs.set(0, 3.0);
        rect.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 3.0).abs() < 0.01); // full
        assert!((outputs.get(11).unwrap() - 3.0).abs() < 0.01); // half_pos
        assert!((outputs.get(12).unwrap()).abs() < 0.01); // half_neg

        // Negative input
        inputs.set(0, -3.0);
        rect.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 3.0).abs() < 0.01); // full (abs)
        assert!((outputs.get(11).unwrap()).abs() < 0.01); // half_pos
        assert!((outputs.get(12).unwrap() - 3.0).abs() < 0.01); // half_neg (inverted)
    }

    #[test]
    fn test_precision_adder() {
        let mut adder = PrecisionAdder::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 1.0);
        inputs.set(1, 2.0);
        inputs.set(2, 0.5);
        inputs.set(3, -0.5);
        adder.tick(&inputs, &mut outputs);

        assert!((outputs.get(10).unwrap() - 3.0).abs() < 0.01); // sum
        assert!((outputs.get(11).unwrap() - (-3.0)).abs() < 0.01); // inverted
    }

    #[test]
    fn test_vc_switch() {
        let mut sw = VcSwitch::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 3.0); // A
        inputs.set(1, 7.0); // B

        // CV low: select A
        inputs.set(2, 0.0);
        sw.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 3.0).abs() < 0.01);
        assert!((outputs.get(11).unwrap() - 3.0).abs() < 0.01);
        assert!((outputs.get(12).unwrap()).abs() < 0.01);

        // CV high: select B
        inputs.set(2, 5.0);
        sw.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 7.0).abs() < 0.01);
        assert!((outputs.get(11).unwrap()).abs() < 0.01);
        assert!((outputs.get(12).unwrap() - 7.0).abs() < 0.01);
    }

    #[test]
    fn test_bernoulli_gate() {
        let mut bg = BernoulliGate::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set probability to 100%
        inputs.set(1, 10.0);

        // Trigger rising edge
        inputs.set(0, 0.0);
        bg.tick(&inputs, &mut outputs);
        inputs.set(0, 5.0);
        bg.tick(&inputs, &mut outputs);

        // At 100% prob, should always go to A
        assert!(outputs.get(10).unwrap() > 2.5); // trig_a
        assert!(outputs.get(11).unwrap() < 2.5); // trig_b

        // Reset and test 0% probability
        bg.reset();
        inputs.set(1, 0.0);
        inputs.set(0, 0.0);
        bg.tick(&inputs, &mut outputs);
        inputs.set(0, 5.0);
        bg.tick(&inputs, &mut outputs);

        // At 0% prob, should always go to B
        assert!(outputs.get(10).unwrap() < 2.5); // trig_a
        assert!(outputs.get(11).unwrap() > 2.5); // trig_b
    }

    #[test]
    fn test_min() {
        let mut m = Min::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 3.0);
        inputs.set(1, 5.0);
        m.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 3.0).abs() < 0.01);

        inputs.set(0, 7.0);
        inputs.set(1, 2.0);
        m.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_max() {
        let mut m = Max::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 3.0);
        inputs.set(1, 5.0);
        m.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 5.0).abs() < 0.01);

        inputs.set(0, 7.0);
        inputs.set(1, 2.0);
        m.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 7.0).abs() < 0.01);
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
        assert!(adsr.stage == crate::modules::AdsrStage::Idle);

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
    fn test_mixer_default_reset_sample_rate() {
        let mut mixer = Mixer::default();
        mixer.reset();
        mixer.set_sample_rate(48000.0);
        assert_eq!(mixer.type_id(), "mixer");
    }

    #[test]
    fn test_stereo_output_default_reset_sample_rate() {
        let mut stereo = StereoOutput::default();
        stereo.reset();
        stereo.set_sample_rate(48000.0);
        assert_eq!(stereo.type_id(), "stereo_output");
    }

    #[test]
    fn test_offset_default_reset_sample_rate() {
        let mut offset = Offset::default();
        offset.reset();
        offset.set_sample_rate(48000.0);
        assert_eq!(offset.type_id(), "offset");
    }

    #[test]
    fn test_scale_enum_semitones() {
        let scale = Scale::Chromatic;
        assert!(scale.semitones().len() == 12);

        let scale = Scale::Major;
        assert!(scale.semitones().len() == 7);

        let scale = Scale::PentatonicMajor;
        assert!(scale.semitones().len() == 5);
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
    fn test_noise_generator_default_reset_sample_rate() {
        let mut noise = NoiseGenerator::default();
        noise.reset();
        noise.set_sample_rate(48000.0);
        assert_eq!(noise.type_id(), "noise");
    }

    #[test]
    fn test_step_sequencer_default_reset_sample_rate() {
        let mut seq = StepSequencer::default();
        seq.set_step(0, 1.0, true);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 5.0);
        seq.tick(&inputs, &mut outputs);

        seq.reset();
        assert!(seq.current == 0);
        assert!(seq.last_clock == 0.0);

        seq.set_sample_rate(48000.0);
        assert_eq!(seq.type_id(), "step_sequencer");
    }

    #[test]
    fn test_sample_and_hold_default_reset_sample_rate() {
        let mut sh = SampleAndHold::default();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 5.0);
        inputs.set(1, 5.0);
        sh.tick(&inputs, &mut outputs);

        sh.reset();
        assert!(sh.held_value == 0.0);

        sh.set_sample_rate(48000.0);
        assert_eq!(sh.type_id(), "sample_hold");
    }

    #[test]
    fn test_slew_limiter_default_reset_sample_rate() {
        let mut slew = SlewLimiter::default();
        assert!(slew.sample_rate == 44100.0);

        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 5.0);
        slew.tick(&inputs, &mut outputs);

        slew.reset();
        assert!(slew.current == 0.0);

        slew.set_sample_rate(48000.0);
        assert!(slew.sample_rate == 48000.0);

        assert_eq!(slew.type_id(), "slew_limiter");
    }

    #[test]
    fn test_quantizer_default_reset_sample_rate() {
        let mut quant = Quantizer::default();
        quant.reset();
        quant.set_sample_rate(48000.0);
        assert_eq!(quant.type_id(), "quantizer");
    }

    #[test]
    fn test_clock_default_reset_sample_rate() {
        let mut clock = Clock::default();
        assert!(clock.sample_rate == 44100.0);

        let inputs = PortValues::new();
        let mut outputs = PortValues::new();
        for _ in 0..100 {
            clock.tick(&inputs, &mut outputs);
        }

        clock.reset();
        assert!(clock.phase == 0.0);

        clock.set_sample_rate(48000.0);
        assert!(clock.sample_rate == 48000.0);

        assert_eq!(clock.type_id(), "clock");
    }

    #[test]
    fn test_attenuverter_default_reset_sample_rate() {
        let mut att = Attenuverter::default();
        att.reset();
        att.set_sample_rate(48000.0);
        assert_eq!(att.type_id(), "attenuverter");
    }

    #[test]
    fn test_multiple_default_reset_sample_rate() {
        let mut mult = Multiple::default();
        mult.reset();
        mult.set_sample_rate(48000.0);
        assert_eq!(mult.type_id(), "multiple");
    }

    #[test]
    fn test_ring_modulator_default_reset_sample_rate() {
        let mut rm = RingModulator::default();
        rm.reset();
        rm.set_sample_rate(48000.0);
        assert_eq!(rm.type_id(), "ring_mod");
    }

    #[test]
    fn test_crossfader_default_reset_sample_rate() {
        let mut xf = Crossfader::default();
        xf.reset();
        xf.set_sample_rate(48000.0);
        assert_eq!(xf.type_id(), "crossfader");
    }

    #[test]
    fn test_logic_and_default_reset_sample_rate() {
        let mut gate = LogicAnd::default();
        gate.reset();
        gate.set_sample_rate(48000.0);
        assert_eq!(gate.type_id(), "logic_and");
    }

    #[test]
    fn test_logic_or_default_reset_sample_rate() {
        let mut gate = LogicOr::default();
        gate.reset();
        gate.set_sample_rate(48000.0);
        assert_eq!(gate.type_id(), "logic_or");
    }

    #[test]
    fn test_logic_xor_default_reset_sample_rate() {
        let mut gate = LogicXor::default();
        gate.reset();
        gate.set_sample_rate(48000.0);
        assert_eq!(gate.type_id(), "logic_xor");
    }

    #[test]
    fn test_logic_not_default_reset_sample_rate() {
        let mut gate = LogicNot::default();
        gate.reset();
        gate.set_sample_rate(48000.0);
        assert_eq!(gate.type_id(), "logic_not");
    }

    #[test]
    fn test_comparator_default_reset_sample_rate() {
        let mut cmp = Comparator::default();
        cmp.reset();
        cmp.set_sample_rate(48000.0);
        assert_eq!(cmp.type_id(), "comparator");
    }

    #[test]
    fn test_rectifier_default_reset_sample_rate() {
        let mut rect = Rectifier::default();
        rect.reset();
        rect.set_sample_rate(48000.0);
        assert_eq!(rect.type_id(), "rectifier");
    }

    #[test]
    fn test_precision_adder_default_reset_sample_rate() {
        let mut adder = PrecisionAdder::default();
        adder.reset();
        adder.set_sample_rate(48000.0);
        assert_eq!(adder.type_id(), "precision_adder");
    }

    #[test]
    fn test_vc_switch_default_reset_sample_rate() {
        let mut sw = VcSwitch::default();
        sw.reset();
        sw.set_sample_rate(48000.0);
        assert_eq!(sw.type_id(), "vc_switch");
    }

    #[test]
    fn test_bernoulli_gate_default_reset_sample_rate() {
        let mut bg = BernoulliGate::default();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 5.0);
        bg.tick(&inputs, &mut outputs);

        bg.reset();
        assert!(bg.last_trigger == 0.0);

        bg.set_sample_rate(48000.0);
        assert_eq!(bg.type_id(), "bernoulli_gate");
    }

    #[test]
    fn test_min_default_reset_sample_rate() {
        let mut m = Min::default();
        m.reset();
        m.set_sample_rate(48000.0);
        assert_eq!(m.type_id(), "min");
    }

    #[test]
    fn test_max_default_reset_sample_rate() {
        let mut m = Max::default();
        m.reset();
        m.set_sample_rate(48000.0);
        assert_eq!(m.type_id(), "max");
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
    fn test_step_sequencer_skip_disabled() {
        let mut seq = StepSequencer::new();
        seq.set_step(0, 1.0, true);
        seq.set_step(1, 2.0, false); // Disabled step
        seq.set_step(2, 3.0, true);

        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Initial step
        seq.tick(&inputs, &mut outputs);
        let _out = outputs.get(10).unwrap_or(0.0);

        // Clock to next step
        inputs.set(0, 5.0);
        seq.tick(&inputs, &mut outputs);
    }

    #[test]
    fn test_quantizer_pentatonic_scale() {
        let mut quant = Quantizer::new(Scale::PentatonicMajor);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Pentatonic scale has notes: 0, 2, 4, 7, 9 semitones
        inputs.set(0, 0.0);
        quant.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap().abs() < 0.01);
    }

    #[test]
    fn test_quantizer_blues_scale() {
        let mut quant = Quantizer::new(Scale::Blues);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0);
        quant.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).is_some());
    }

    #[test]
    fn test_slew_limiter_falling() {
        let mut slew = SlewLimiter::new(1000.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // First, set to high value
        inputs.set(0, 5.0);
        inputs.set(1, 10.0); // Fast rise
        inputs.set(2, 0.5); // Slower fall
        for _ in 0..1000 {
            slew.tick(&inputs, &mut outputs);
        }

        // Now set to low value and observe falling behavior
        inputs.set(0, 0.0);
        slew.tick(&inputs, &mut outputs);
        let falling = outputs.get(10).unwrap();
        assert!(falling < 5.0);
        assert!(falling > 0.0);
    }

    #[test]
    fn test_scale_dorian_and_mixolydian() {
        let scale = Scale::Dorian;
        assert!(scale.semitones().len() == 7);

        let scale = Scale::Mixolydian;
        assert!(scale.semitones().len() == 7);
    }

    #[test]
    fn test_clock_subdivisions() {
        let mut clock = Clock::new(1000.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 5.0); // Medium tempo

        // Run and check all outputs exist
        for _ in 0..1000 {
            clock.tick(&inputs, &mut outputs);
        }

        // Should have all clock subdivision outputs
        assert!(outputs.get(10).is_some()); // Main
        assert!(outputs.get(11).is_some()); // /2
        assert!(outputs.get(12).is_some()); // /4
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

    // ========================================================================
    // ChordMemory Tests
    // ========================================================================

    #[test]
    fn test_chord_memory_major() {
        let mut cm = ChordMemory::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Root at C4 (0V), major chord (cv=0)
        inputs.set(0, 0.0);
        inputs.set(1, 0.0); // Major
        inputs.set(2, 0.0); // No inversion
        inputs.set(3, 0.0); // No spread

        cm.tick(&inputs, &mut outputs);

        // Major chord: root, major 3rd (+4 semitones), perfect 5th (+7 semitones)
        let voice1 = outputs.get(10).unwrap();
        let voice2 = outputs.get(11).unwrap();
        let voice3 = outputs.get(12).unwrap();
        let voice4 = outputs.get(13).unwrap();

        assert!((voice1 - 0.0).abs() < 0.01); // Root (C)
        assert!((voice2 - 4.0 / 12.0).abs() < 0.01); // Major 3rd (E)
        assert!((voice3 - 7.0 / 12.0).abs() < 0.01); // Perfect 5th (G)
        assert!((voice4 - 1.0).abs() < 0.01); // Octave (for 3-note chord, voice4 = root+1)
    }

    #[test]
    fn test_chord_memory_minor() {
        let mut cm = ChordMemory::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0);
        inputs.set(1, 0.15); // Minor (second chord type, cv ~0.111-0.222)

        cm.tick(&inputs, &mut outputs);

        // Minor chord: root, minor 3rd (+3 semitones), perfect 5th (+7 semitones)
        let voice2 = outputs.get(11).unwrap();
        assert!((voice2 - 3.0 / 12.0).abs() < 0.01); // Minor 3rd (Eb)
    }

    #[test]
    fn test_chord_memory_seventh() {
        let mut cm = ChordMemory::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0);
        inputs.set(1, 0.26); // Dominant 7th (cv ~0.222-0.333)

        cm.tick(&inputs, &mut outputs);

        // Dom7 chord: root, major 3rd, perfect 5th, minor 7th (+10 semitones)
        let voice4 = outputs.get(13).unwrap();
        assert!((voice4 - 10.0 / 12.0).abs() < 0.01); // Minor 7th (Bb)
    }

    #[test]
    fn test_chord_memory_inversion() {
        let mut cm = ChordMemory::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0);
        inputs.set(1, 0.0); // Major
        inputs.set(2, 0.4); // First inversion (for 3-note chord: ~1/3)

        cm.tick(&inputs, &mut outputs);

        // First inversion: E in bass, G, C (octave up)
        let voice1 = outputs.get(10).unwrap();
        let voice2 = outputs.get(11).unwrap();
        let voice3 = outputs.get(12).unwrap();

        // Voice 1 should be the 3rd (4 semitones = major 3rd)
        assert!((voice1 - 4.0 / 12.0).abs() < 0.01);
        // Voice 2 should be the 5th (7 semitones)
        assert!((voice2 - 7.0 / 12.0).abs() < 0.01);
        // Voice 3 should be root + octave (wrapped)
        assert!((voice3 - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_chord_memory_spread() {
        let mut cm = ChordMemory::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0);
        inputs.set(1, 0.0); // Major
        inputs.set(2, 0.0); // No inversion
        inputs.set(3, 1.0); // Full spread

        cm.tick(&inputs, &mut outputs);

        let voice1 = outputs.get(10).unwrap();
        let voice2 = outputs.get(11).unwrap();
        let voice3 = outputs.get(12).unwrap();
        let voice4 = outputs.get(13).unwrap();

        // With spread=1.0, voice4 should be ~1 octave higher than without spread
        // voice1: 0 + 0/3 = 0
        // voice2: 4/12 + 1/3 ≈ 0.666
        // voice3: 7/12 + 2/3 ≈ 1.25
        // voice4: 1.0 + 1.0 = 2.0 (for 3-note chord)
        assert!(voice1 < voice2);
        assert!(voice2 < voice3);
        assert!(voice3 < voice4);
    }

    #[test]
    fn test_chord_memory_all_chord_types() {
        let mut cm = ChordMemory::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test all 9 chord types produce valid output
        for i in 0..9 {
            let chord_cv = i as f64 / 9.0;
            inputs.set(0, 0.0);
            inputs.set(1, chord_cv);

            cm.tick(&inputs, &mut outputs);

            // All voices should have valid output
            assert!(outputs.get(10).is_some());
            assert!(outputs.get(11).is_some());
            assert!(outputs.get(12).is_some());
            assert!(outputs.get(13).is_some());
        }
    }

    #[test]
    fn test_chord_memory_default_reset_sample_rate() {
        let mut cm = ChordMemory::default();
        cm.reset();
        cm.set_sample_rate(48000.0);
        assert_eq!(cm.type_id(), "chord_memory");

        // Verify port spec
        assert_eq!(cm.port_spec().inputs.len(), 4);
        assert_eq!(cm.port_spec().outputs.len(), 4);
    }

    #[test]
    fn test_chord_type_intervals() {
        // Test that all chord types return valid intervals
        assert_eq!(ChordType::Major.intervals(), &[0, 4, 7]);
        assert_eq!(ChordType::Minor.intervals(), &[0, 3, 7]);
        assert_eq!(ChordType::Seventh.intervals(), &[0, 4, 7, 10]);
        assert_eq!(ChordType::MajorSeventh.intervals(), &[0, 4, 7, 11]);
        assert_eq!(ChordType::MinorSeventh.intervals(), &[0, 3, 7, 10]);
        assert_eq!(ChordType::Diminished.intervals(), &[0, 3, 6]);
        assert_eq!(ChordType::Augmented.intervals(), &[0, 4, 8]);
        assert_eq!(ChordType::Sus2.intervals(), &[0, 2, 7]);
        assert_eq!(ChordType::Sus4.intervals(), &[0, 5, 7]);
    }

    #[test]
    fn test_chord_type_from_cv() {
        assert_eq!(ChordType::from_cv(0.0), ChordType::Major);
        assert_eq!(ChordType::from_cv(0.12), ChordType::Minor);
        assert_eq!(ChordType::from_cv(0.23), ChordType::Seventh);
        assert_eq!(ChordType::from_cv(1.0), ChordType::Sus4);
    }

    // ========================================================================
    // ParametricEq Tests
    // ========================================================================

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
    fn test_pitch_shifter_default_reset_sample_rate() {
        let mut ps = PitchShifter::default();
        assert_eq!(ps.sample_rate, 44100.0);

        // Process some samples
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 2.5); // Audio input
        for _ in 0..100 {
            ps.tick(&inputs, &mut outputs);
        }

        // Verify buffer was written to
        assert!(ps.write_pos > 0);

        // Reset
        ps.reset();
        assert_eq!(ps.write_pos, 0);
        assert_eq!(ps.grain_phase, [0.0, 0.5]);

        // Set sample rate
        ps.set_sample_rate(48000.0);
        assert_eq!(ps.sample_rate, 48000.0);

        assert_eq!(ps.type_id(), "pitch_shifter");
        assert_eq!(ps.port_spec().inputs.len(), 4);
        assert_eq!(ps.port_spec().outputs.len(), 1);
    }

    #[test]
    fn test_pitch_shifter_hann_window() {
        // Test window function
        let start = PitchShifter::hann_window(0.0);
        let peak = PitchShifter::hann_window(0.5);
        let end = PitchShifter::hann_window(1.0);

        assert!(start.abs() < 0.01, "Window should start at 0: {}", start);
        assert!(
            (peak - 1.0).abs() < 0.01,
            "Window should peak at 1: {}",
            peak
        );
        assert!(end.abs() < 0.01, "Window should end at 0: {}", end);
    }

    #[test]
    fn test_pitch_shifter_passthrough() {
        let mut ps = PitchShifter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // No shift, full mix - should output delayed version of input
        inputs.set(1, 0.0); // No shift
        inputs.set(3, 1.0); // Full wet

        // Feed a sine wave
        let mut sum_out = 0.0;
        for i in 0..1000 {
            let input = Libm::<f64>::sin(i as f64 * 0.1) * 5.0;
            inputs.set(0, input);
            ps.tick(&inputs, &mut outputs);
            sum_out += outputs.get(10).unwrap().abs();
        }

        // Should have significant output
        assert!(sum_out > 100.0, "Should have output signal: {}", sum_out);
    }

    #[test]
    fn test_pitch_shifter_dry_wet_mix() {
        let mut ps = PitchShifter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Full dry - output should be close to input (after normalization)
        inputs.set(1, 0.0);
        inputs.set(3, 0.0); // Full dry

        let input_val = 2.5; // Some audio signal
        inputs.set(0, input_val);

        ps.tick(&inputs, &mut outputs);
        let dry_out = outputs.get(10).unwrap();

        // Dry output should be the input
        assert!(
            (dry_out - input_val).abs() < 0.1,
            "Dry output should match input: {} vs {}",
            dry_out,
            input_val
        );
    }

    #[test]
    fn test_pitch_shifter_shift_changes_output() {
        let mut ps = PitchShifter::new(44100.0);

        // Feed a signal and collect output with different shift values
        let collect_output = |ps: &mut PitchShifter, shift_cv: f64| -> f64 {
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(1, shift_cv);
            inputs.set(3, 1.0);
            ps.reset();

            let mut sum = 0.0;
            for i in 0..2000 {
                let input = Libm::<f64>::sin(i as f64 * 0.05) * 5.0;
                inputs.set(0, input);
                ps.tick(&inputs, &mut outputs);
                sum += outputs.get(10).unwrap();
            }
            sum
        };

        let sum_no_shift = collect_output(&mut ps, 0.0);
        let sum_up_octave = collect_output(&mut ps, 2.5); // +12 semitones
        let sum_down_octave = collect_output(&mut ps, -2.5); // -12 semitones

        // Different shifts should produce different outputs
        assert!(
            (sum_no_shift - sum_up_octave).abs() > 1.0,
            "Up shift should differ"
        );
        assert!(
            (sum_no_shift - sum_down_octave).abs() > 1.0,
            "Down shift should differ"
        );
    }

    #[test]
    fn test_pitch_shifter_buffer_wraparound() {
        let mut ps = PitchShifter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 2.5);
        inputs.set(1, 0.0);
        inputs.set(3, 1.0);

        // Process more samples than buffer size to test wraparound
        for _ in 0..10000 {
            ps.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            assert!(out.is_finite(), "Output should be finite");
        }

        // Write position should have wrapped
        assert!(ps.write_pos < PitchShifter::BUFFER_SIZE);
    }

    #[test]
    fn test_arp_pattern_from_cv() {
        assert_eq!(ArpPattern::from_cv(0.0), ArpPattern::Up);
        assert_eq!(ArpPattern::from_cv(0.1), ArpPattern::Up);
        assert_eq!(ArpPattern::from_cv(0.3), ArpPattern::Down);
        assert_eq!(ArpPattern::from_cv(0.6), ArpPattern::UpDown);
        assert_eq!(ArpPattern::from_cv(0.9), ArpPattern::Random);
        assert_eq!(ArpPattern::from_cv(1.0), ArpPattern::Random);
    }

    #[test]
    fn test_arpeggiator_default_reset_sample_rate() {
        let mut arp = Arpeggiator::default();
        assert_eq!(arp.sample_rate, 44100.0);

        // Add a note
        arp.add_note(0.0);
        assert_eq!(arp.num_notes, 1);

        // Reset should clear notes
        arp.reset();
        assert_eq!(arp.num_notes, 0);
        assert_eq!(arp.current_step, 0);

        // Set sample rate
        arp.set_sample_rate(48000.0);
        assert_eq!(arp.sample_rate, 48000.0);

        assert_eq!(arp.type_id(), "arpeggiator");
        assert_eq!(arp.port_spec().inputs.len(), 6);
        assert_eq!(arp.port_spec().outputs.len(), 3);
    }

    #[test]
    fn test_arpeggiator_add_remove_notes() {
        let mut arp = Arpeggiator::new(44100.0);

        // Add notes
        arp.add_note(0.0); // C4
        arp.add_note(0.5); // F#4
        arp.add_note(0.25); // D#4

        assert_eq!(arp.num_notes, 3);
        // Notes should be sorted
        assert_eq!(arp.held_notes[0], 0.0);
        assert_eq!(arp.held_notes[1], 0.25);
        assert_eq!(arp.held_notes[2], 0.5);

        // Remove middle note
        arp.remove_note(0.25);
        assert_eq!(arp.num_notes, 2);
        assert_eq!(arp.held_notes[0], 0.0);
        assert_eq!(arp.held_notes[1], 0.5);
    }

    #[test]
    fn test_arpeggiator_up_pattern() {
        let mut arp = Arpeggiator::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Add three notes (need to cycle gate for each)
        inputs.set(0, 0.0); // C4
        inputs.set(1, 5.0); // Gate high
        arp.tick(&inputs, &mut outputs);
        inputs.set(1, 0.0); // Gate low
        arp.tick(&inputs, &mut outputs);

        inputs.set(0, 0.333); // E4
        inputs.set(1, 5.0); // Gate high
        arp.tick(&inputs, &mut outputs);
        inputs.set(1, 0.0); // Gate low
        arp.tick(&inputs, &mut outputs);

        inputs.set(0, 0.583); // G4
        inputs.set(1, 5.0); // Gate high
        arp.tick(&inputs, &mut outputs);

        assert_eq!(arp.num_notes, 3);

        // Send clock pulses and check output
        inputs.set(3, 0.0); // Up pattern
        let mut notes_out = Vec::new();

        for _ in 0..6 {
            inputs.set(2, 5.0); // Clock high
            arp.tick(&inputs, &mut outputs);
            notes_out.push(outputs.get(10).unwrap());

            inputs.set(2, 0.0); // Clock low
            arp.tick(&inputs, &mut outputs);
        }

        // Should cycle through notes in ascending order
        assert!(notes_out[0] < notes_out[1]);
        assert!(notes_out[1] < notes_out[2]);
        // Then repeat
        assert!((notes_out[3] - notes_out[0]).abs() < 0.01);
    }

    #[test]
    fn test_arpeggiator_trigger_output() {
        let mut arp = Arpeggiator::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Add a note
        inputs.set(0, 0.0);
        inputs.set(1, 5.0);
        arp.tick(&inputs, &mut outputs);

        // Clock pulse should produce trigger
        inputs.set(2, 5.0);
        arp.tick(&inputs, &mut outputs);
        let trigger = outputs.get(12).unwrap();
        assert!(trigger > 0.0, "Should output trigger on clock");

        // Trigger should continue for a short time
        inputs.set(2, 0.0);
        arp.tick(&inputs, &mut outputs);
        let trigger2 = outputs.get(12).unwrap();
        assert!(trigger2 > 0.0, "Trigger should persist briefly");
    }

    #[test]
    fn test_arpeggiator_reset_input() {
        let mut arp = Arpeggiator::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Add notes and advance steps
        inputs.set(0, 0.0);
        inputs.set(1, 5.0);
        arp.tick(&inputs, &mut outputs);

        for _ in 0..5 {
            inputs.set(2, 5.0);
            arp.tick(&inputs, &mut outputs);
            inputs.set(2, 0.0);
            arp.tick(&inputs, &mut outputs);
        }

        let step_before = arp.current_step;
        assert!(step_before > 0);

        // Send reset
        inputs.set(5, 5.0);
        arp.tick(&inputs, &mut outputs);

        assert_eq!(arp.current_step, 0, "Reset should clear step");
    }

    #[test]
    fn test_arpeggiator_octaves() {
        let mut arp = Arpeggiator::new(44100.0);

        // Add one note
        arp.add_note(0.0); // C4

        // With 2 octaves, step 0 should give 0.0, step 1 should give 1.0 (octave higher)
        let note1 = arp.get_current_note(ArpPattern::Up, 2);
        arp.current_step = 1;
        let note2 = arp.get_current_note(ArpPattern::Up, 2);

        assert!(
            (note2 - note1 - 1.0).abs() < 0.01,
            "Second note should be 1 octave higher"
        );
    }

    // =========================================================================
    // Reverb Tests
    // =========================================================================

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

    // =========================================================================
    // Vocoder Tests
    // =========================================================================

    #[test]
    fn test_vocoder_default_reset_sample_rate() {
        let mut vocoder = Vocoder::default();
        assert_eq!(vocoder.sample_rate, 44100.0);

        // Feed some signal
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.5); // carrier
        inputs.set(1, 0.5); // modulator
        vocoder.tick(&inputs, &mut outputs);

        // Reset should clear state
        vocoder.reset();
        assert_eq!(vocoder.envelopes, [0.0; MAX_VOCODER_BANDS]);

        // Sample rate change
        vocoder.set_sample_rate(48000.0);
        assert_eq!(vocoder.sample_rate, 48000.0);

        assert_eq!(vocoder.type_id(), "vocoder");
        assert_eq!(vocoder.port_spec().inputs.len(), 5);
        assert_eq!(vocoder.port_spec().outputs.len(), 1);
    }

    #[test]
    fn test_vocoder_band_frequencies() {
        let vocoder = Vocoder::new(44100.0);

        // Check logarithmic spacing
        assert!(vocoder.band_freqs[0] >= VOCODER_FREQ_MIN - 1.0);
        assert!(vocoder.band_freqs[MAX_VOCODER_BANDS - 1] <= VOCODER_FREQ_MAX + 1.0);

        // Frequencies should be ascending
        for i in 1..MAX_VOCODER_BANDS {
            assert!(
                vocoder.band_freqs[i] > vocoder.band_freqs[i - 1],
                "Band frequencies should be ascending"
            );
        }
    }

    #[test]
    fn test_vocoder_silent_when_no_modulator() {
        let mut vocoder = Vocoder::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Carrier only, no modulator
        inputs.set(0, 0.8);
        inputs.set(1, 0.0);

        // Run for a while
        for _ in 0..1000 {
            vocoder.tick(&inputs, &mut outputs);
        }

        let out = outputs.get(10).unwrap();
        // Without modulator, output should be near zero (envelopes decay)
        assert!(
            out.abs() < 0.1,
            "Output should be near zero without modulator, got {}",
            out
        );
    }

    #[test]
    fn test_vocoder_output_when_both_active() {
        let mut vocoder = Vocoder::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Both carrier and modulator active
        let mut total_output = 0.0;
        for i in 0..2000 {
            let phase = i as f64 * 0.05;
            inputs.set(0, Libm::<f64>::sin(phase)); // carrier (oscillator)
            inputs.set(1, Libm::<f64>::sin(phase * 0.1)); // modulator (lower freq)
            vocoder.tick(&inputs, &mut outputs);
            total_output += outputs.get(10).unwrap().abs();
        }

        assert!(
            total_output > 1.0,
            "Should produce output when both signals active, got {}",
            total_output
        );
    }

    #[test]
    fn test_vocoder_band_count() {
        let mut vocoder_few = Vocoder::new(44100.0);
        let mut vocoder_many = Vocoder::new(44100.0);
        let mut inputs_few = PortValues::new();
        let mut inputs_many = PortValues::new();
        let mut outputs_few = PortValues::new();
        let mut outputs_many = PortValues::new();

        // Set up with different band counts
        inputs_few.set(2, 0.0); // Minimum bands (4)
        inputs_many.set(2, 1.0); // Maximum bands (16)

        // Both get same carrier and modulator
        let mut total_few = 0.0;
        let mut total_many = 0.0;

        for i in 0..1000 {
            let phase = i as f64 * 0.05;
            let carrier = Libm::<f64>::sin(phase);
            let modulator = Libm::<f64>::sin(phase * 0.2);

            inputs_few.set(0, carrier);
            inputs_few.set(1, modulator);
            inputs_many.set(0, carrier);
            inputs_many.set(1, modulator);

            vocoder_few.tick(&inputs_few, &mut outputs_few);
            vocoder_many.tick(&inputs_many, &mut outputs_many);

            total_few += outputs_few.get(10).unwrap().abs();
            total_many += outputs_many.get(10).unwrap().abs();
        }

        // Both should produce output (different character but both work)
        assert!(total_few > 0.5, "Few bands should produce output");
        assert!(total_many > 0.5, "Many bands should produce output");
    }

    #[test]
    fn test_vocoder_envelope_attack_release() {
        let mut vocoder = Vocoder::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test with different attack/release settings
        inputs.set(0, 1.0); // carrier
        inputs.set(1, 1.0); // modulator
        inputs.set(3, 0.0); // Fast attack
        inputs.set(4, 0.0); // Fast release

        // Run a few ticks to build up envelope
        for _ in 0..100 {
            vocoder.tick(&inputs, &mut outputs);
        }
        let fast_envelope = vocoder.envelopes[0];

        vocoder.reset();
        inputs.set(3, 1.0); // Slow attack

        for _ in 0..100 {
            vocoder.tick(&inputs, &mut outputs);
        }
        let slow_envelope = vocoder.envelopes[0];

        // Fast attack should build up faster
        assert!(
            fast_envelope > slow_envelope,
            "Fast attack should build envelope faster"
        );
    }

    // =========================================================================
    // Granular Tests
    // =========================================================================

    #[test]
    fn test_granular_default_reset_sample_rate() {
        let mut granular = Granular::default();
        assert_eq!(granular.sample_rate, 44100.0);

        // Feed some signal
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.5);
        granular.tick(&inputs, &mut outputs);

        // Should have written to buffer
        assert_eq!(granular.write_pos, 1);

        // Reset should clear everything
        granular.reset();
        assert_eq!(granular.write_pos, 0);
        assert!(granular.grains.iter().all(|g| !g.active));

        // Sample rate change
        granular.set_sample_rate(48000.0);
        assert_eq!(granular.sample_rate, 48000.0);

        assert_eq!(granular.type_id(), "granular");
        assert_eq!(granular.port_spec().inputs.len(), 7);
        assert_eq!(granular.port_spec().outputs.len(), 1);
    }

    #[test]
    fn test_granular_hann_window() {
        // Hann window should be 0 at edges and 1 at center
        assert!(Granular::hann_window(0.0).abs() < 0.001);
        assert!((Granular::hann_window(0.5) - 1.0).abs() < 0.001);
        assert!(Granular::hann_window(1.0).abs() < 0.001);
    }

    #[test]
    fn test_granular_records_to_buffer() {
        let mut granular = Granular::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Feed a specific pattern
        for i in 0..100 {
            inputs.set(0, i as f64 * 0.01);
            granular.tick(&inputs, &mut outputs);
        }

        // Check buffer has recorded values
        assert!((granular.buffer[50] - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_granular_freeze_stops_recording() {
        let mut granular = Granular::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Record some audio
        inputs.set(0, 1.0);
        for _ in 0..100 {
            granular.tick(&inputs, &mut outputs);
        }
        let pos_before = granular.write_pos;

        // Freeze
        inputs.set(6, 5.0); // Gate high

        // Should not advance write position
        for _ in 0..100 {
            granular.tick(&inputs, &mut outputs);
        }

        assert_eq!(granular.write_pos, pos_before);
    }

    #[test]
    fn test_granular_produces_output() {
        let mut granular = Granular::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set position to read from start of buffer where we'll write
        inputs.set(1, 0.05); // Read near the start where we're recording

        // Fill buffer with signal
        for i in 0..10000 {
            let phase = i as f64 * 0.01;
            inputs.set(0, Libm::<f64>::sin(phase));
            granular.tick(&inputs, &mut outputs);
        }

        // Continue and check output
        let mut total_output = 0.0;
        for _ in 0..5000 {
            inputs.set(0, 0.0);
            granular.tick(&inputs, &mut outputs);
            total_output += outputs.get(10).unwrap().abs();
        }

        assert!(
            total_output > 1.0,
            "Granular should produce output, got {}",
            total_output
        );
    }

    #[test]
    fn test_granular_density_affects_grain_count() {
        let mut granular_low = Granular::new(44100.0);
        let mut granular_high = Granular::new(44100.0);
        let mut inputs_low = PortValues::new();
        let mut inputs_high = PortValues::new();
        let mut outputs = PortValues::new();

        inputs_low.set(3, 0.0); // Low density
        inputs_high.set(3, 1.0); // High density

        // Fill buffers
        for i in 0..5000 {
            let sample = Libm::<f64>::sin(i as f64 * 0.05);
            inputs_low.set(0, sample);
            inputs_high.set(0, sample);
            granular_low.tick(&inputs_low, &mut outputs);
            granular_high.tick(&inputs_high, &mut outputs);
        }

        // Count active grains
        let active_low = granular_low.grains.iter().filter(|g| g.active).count();
        let active_high = granular_high.grains.iter().filter(|g| g.active).count();

        // High density should tend to have more active grains
        // (Note: due to randomness and grain lifetimes, this isn't guaranteed on every run)
        assert!(
            active_high >= active_low || (active_low == 0 && active_high == 0),
            "Higher density should produce more concurrent grains"
        );
    }

    #[test]
    fn test_granular_buffer_interpolation() {
        let granular = Granular::new(44100.0);

        // Manually set some buffer values
        let mut granular = granular;
        granular.buffer[0] = 0.0;
        granular.buffer[1] = 1.0;

        // Read at fractional position should interpolate
        let val = granular.read_buffer(0.5);
        assert!(
            (val - 0.5).abs() < 0.01,
            "Interpolation should give 0.5, got {}",
            val
        );
    }

    #[test]
    fn test_grain_default() {
        let grain = Grain::default();
        assert!(!grain.active);
        assert_eq!(grain.phase, 0.0);
        assert_eq!(grain.speed, 1.0);
    }
}
