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
        let delay_samples = (delay_ms * self.sample_rate / 1000.0)
            .clamp(1.0, (self.buffer.len() - 1) as f64);

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
        let buffer_size = ((Self::MAX_MOD_DELAY_MS + Self::BASE_DELAY_MS) * sample_rate / 1000.0)
            as usize
            + 10;
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
        let buffer_size = ((Self::MAX_MOD_DELAY_MS + Self::BASE_DELAY_MS) * sample_rate / 1000.0)
            as usize
            + 10;
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
}
