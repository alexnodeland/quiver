//! Core DSP Modules
//!
//! This module provides the essential building blocks for synthesis:
//! oscillators, filters, envelopes, amplifiers, and utilities.

use crate::port::{GraphModule, ParamDef, ParamId, PortDef, PortSpec, PortValues, SignalKind};
use std::f64::consts::{PI, TAU};

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
        let base_freq = 261.63 * 2.0_f64.powf(voct);
        let freq = base_freq * 2.0_f64.powf(fm);

        // Hard sync on rising edge
        if sync > 2.5 && self.last_sync <= 2.5 {
            self.phase = 0.0;
        }
        self.last_sync = sync;

        // Generate waveforms (±5V range)
        let sin = (self.phase * TAU).sin() * 5.0;
        let tri = (1.0 - 4.0 * (self.phase - 0.5).abs()) * 5.0;
        let saw = (2.0 * self.phase - 1.0) * 5.0;
        let sqr = if self.phase < pw { 5.0 } else { -5.0 };

        outputs.set(10, sin);
        outputs.set(11, tri);
        outputs.set(12, saw);
        outputs.set(13, sqr);

        // Advance phase
        self.phase = (self.phase + freq / self.sample_rate).fract();
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
        let freq = 0.01 * (3000.0_f64).powf(rate_cv.clamp(0.0, 1.0));

        // Reset on trigger
        if reset > 2.5 && self.last_reset <= 2.5 {
            self.phase = 0.0;
        }
        self.last_reset = reset;

        // Generate waveforms scaled by depth (±5V * depth)
        let scale = 5.0 * depth;
        let sin = (self.phase * TAU).sin() * scale;
        let tri = (1.0 - 4.0 * (self.phase - 0.5).abs()) * scale;
        let saw = (2.0 * self.phase - 1.0) * scale;
        let sqr = if self.phase < 0.5 { scale } else { -scale };
        let sin_uni = ((self.phase * TAU).sin() * 0.5 + 0.5) * depth * 10.0;

        outputs.set(10, sin);
        outputs.set(11, tri);
        outputs.set(12, saw);
        outputs.set(13, sqr);
        outputs.set(14, sin_uni);

        self.phase = (self.phase + freq / self.sample_rate).fract();
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
/// highpass, and notch outputs. Features cutoff, resonance, and FM inputs.
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

        // Map cutoff CV (0-1) to frequency (20 Hz - 20 kHz, exponential)
        let cutoff_hz = 20.0 * (1000.0_f64).powf(cutoff_cv.clamp(0.0, 1.0));
        let f = 2.0 * (PI * cutoff_hz / self.sample_rate).sin();
        let f = f.min(0.99); // Prevent instability
        let q = 1.0 - res * 0.9; // Resonance: higher res = lower damping

        // SVF topology
        let high = input - self.low - q * self.band;
        self.band += f * high;
        self.low += f * self.band;
        let notch = high + self.low;

        outputs.set(10, self.low);
        outputs.set(11, self.band);
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
        0.001 * (10000.0_f64).powf(cv.clamp(0.0, 1.0))
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
    offset: f64,
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
            self.rows[i] = rand::random::<f64>() * 2.0 - 1.0;
            self.running_sum += self.rows[i];
        }

        self.running_sum / 16.0
    }
}

/// Noise Generator
///
/// Generates white and pink noise signals.
pub struct NoiseGenerator {
    pink: PinkNoiseState,
    spec: PortSpec,
}

impl NoiseGenerator {
    pub fn new() -> Self {
        Self {
            pink: PinkNoiseState::new(),
            spec: PortSpec {
                inputs: vec![],
                outputs: vec![
                    PortDef::new(10, "white", SignalKind::Audio),
                    PortDef::new(11, "pink", SignalKind::Audio),
                ],
            },
        }
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

    fn tick(&mut self, _inputs: &PortValues, outputs: &mut PortValues) {
        let white = rand::random::<f64>() * 2.0 - 1.0;
        let pink = self.pink.sample();

        outputs.set(10, white * 5.0);
        outputs.set(11, pink * 5.0);
    }

    fn reset(&mut self) {
        self.pink = PinkNoiseState::new();
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "noise"
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
        let time = 0.001 + cv.clamp(0.0, 1.0).powi(2) * 10.0; // 1ms to 10s
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
            self.current += diff.min(rate * 10.0); // Scale for voltage range
        } else if diff < 0.0 {
            // Falling
            let rate = self.cv_to_rate(fall_cv);
            self.current += diff.max(-rate * 10.0);
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
    scale: Scale,
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
        let octave = (total_semitones / 12.0).floor();
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
        20.0 * (15.0_f64).powf(cv / 10.0)
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
        let div2_phase = (self.phase * 0.5).fract();
        let div4_phase = (self.phase * 0.25).fract();
        let div2_out = if div2_phase < pulse_width { 5.0 } else { 0.0 };
        let div4_out = if div4_phase < pulse_width { 5.0 } else { 0.0 };

        outputs.set(10, main_out);
        outputs.set(11, div2_out);
        outputs.set(12, div4_out);

        // Advance phase
        self.phase = (self.phase + freq / self.sample_rate).fract();
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

        inputs.set(0, 3.14159);
        mult.tick(&inputs, &mut outputs);

        // All 4 outputs should have the same value
        assert!((outputs.get(10).unwrap() - 3.14159).abs() < 0.0001);
        assert!((outputs.get(11).unwrap() - 3.14159).abs() < 0.0001);
        assert!((outputs.get(12).unwrap() - 3.14159).abs() < 0.0001);
        assert!((outputs.get(13).unwrap() - 3.14159).abs() < 0.0001);
    }
}
