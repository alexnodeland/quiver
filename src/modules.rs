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
                PortDef::new(i as u32, format!("ch{}", i), SignalKind::Audio)
                    .with_attenuverter()
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

    fn next(&mut self) -> f64 {
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
        let pink = self.pink.next();

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
}
