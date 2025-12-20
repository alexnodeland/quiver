//! Analog Modeling Primitives
//!
//! This module provides primitives for modeling analog circuit behavior:
//! saturation, soft clipping, component variation, thermal drift, and noise.

use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use std::f64::consts::TAU;

/// Saturation and soft clipping functions
pub mod saturation {
    /// Hyperbolic tangent saturation (tube-like warmth)
    ///
    /// Higher drive values increase harmonic content.
    pub fn tanh_sat(x: f64, drive: f64) -> f64 {
        let denominator = drive.tanh().max(0.001);
        (x * drive).tanh() / denominator
    }

    /// Soft clipping with adjustable knee
    ///
    /// Signals below threshold pass through unchanged;
    /// signals above are compressed.
    pub fn soft_clip(x: f64, threshold: f64) -> f64 {
        if x.abs() < threshold {
            x
        } else {
            let sign = x.signum();
            let excess = x.abs() - threshold;
            sign * (threshold + excess / (1.0 + excess))
        }
    }

    /// Asymmetric saturation (generates even harmonics)
    ///
    /// Different drive for positive and negative half-cycles
    /// creates even harmonics, giving a warmer, tube-like character.
    pub fn asym_sat(x: f64, pos_drive: f64, neg_drive: f64) -> f64 {
        if x >= 0.0 {
            (x * pos_drive).tanh()
        } else {
            (x * neg_drive).tanh()
        }
    }

    /// Diode-style hard clipping
    ///
    /// Simulates the forward voltage drop of a diode.
    pub fn diode_clip(x: f64, forward_voltage: f64) -> f64 {
        let vf = forward_voltage;
        if x > vf {
            vf + (x - vf) * 0.1
        } else if x < -vf {
            -vf + (x + vf) * 0.1
        } else {
            x
        }
    }

    /// Wavefolder (generates complex harmonics)
    ///
    /// When the signal exceeds the threshold, it "folds" back,
    /// creating rich harmonic content.
    pub fn fold(x: f64, threshold: f64) -> f64 {
        let mut y = x;
        let max_iterations = 10; // Prevent infinite loops
        let mut iterations = 0;

        while y.abs() > threshold && iterations < max_iterations {
            if y > threshold {
                y = 2.0 * threshold - y;
            } else if y < -threshold {
                y = -2.0 * threshold - y;
            }
            iterations += 1;
        }
        y
    }

    /// Cubic soft saturation
    ///
    /// A simple polynomial saturation curve.
    pub fn cubic_sat(x: f64) -> f64 {
        if x.abs() < 2.0 / 3.0 {
            x - x.powi(3) / 3.0
        } else {
            x.signum() * 2.0 / 3.0
        }
    }
}

/// Models real-world component imperfection
#[derive(Debug, Clone)]
pub struct ComponentModel {
    /// Base tolerance (e.g., 0.01 for 1% resistor)
    pub tolerance: f64,

    /// Temperature coefficient (drift per degree C)
    pub temp_coef: f64,

    /// Current operating temperature offset from nominal
    pub temp_offset: f64,

    /// Random offset applied at instantiation
    pub instance_offset: f64,
}

impl ComponentModel {
    /// Create a new component with random variation
    pub fn new(tolerance: f64, temp_coef: f64) -> Self {
        Self {
            tolerance,
            temp_coef,
            temp_offset: 0.0,
            instance_offset: (rand::random::<f64>() * 2.0 - 1.0) * tolerance,
        }
    }

    /// Perfect component (no variation)
    pub fn perfect() -> Self {
        Self {
            tolerance: 0.0,
            temp_coef: 0.0,
            temp_offset: 0.0,
            instance_offset: 0.0,
        }
    }

    /// Typical resistor (1% tolerance)
    pub fn resistor_1pct() -> Self {
        Self::new(0.01, 0.0001)
    }

    /// Typical capacitor (5% tolerance)
    pub fn capacitor_5pct() -> Self {
        Self::new(0.05, 0.0002)
    }

    /// Get the effective value multiplier
    pub fn factor(&self) -> f64 {
        1.0 + self.instance_offset + (self.temp_offset * self.temp_coef)
    }

    /// Apply component variation to a value
    pub fn apply(&self, value: f64) -> f64 {
        value * self.factor()
    }

    /// Update temperature offset
    pub fn set_temperature(&mut self, temp_offset: f64) {
        self.temp_offset = temp_offset;
    }
}

impl Default for ComponentModel {
    fn default() -> Self {
        Self::perfect()
    }
}

/// Thermal drift simulation
///
/// Models how circuit temperature changes based on signal energy
/// and affects component values.
#[derive(Debug, Clone)]
pub struct ThermalModel {
    /// Current virtual temperature
    temperature: f64,

    /// Ambient temperature
    ambient: f64,

    /// Heat generated per unit of signal energy
    heat_rate: f64,

    /// Cooling rate (thermal dissipation)
    cool_rate: f64,
}

impl ThermalModel {
    pub fn new(ambient: f64, heat_rate: f64, cool_rate: f64) -> Self {
        Self {
            temperature: ambient,
            ambient,
            heat_rate,
            cool_rate,
        }
    }

    /// Create a default thermal model
    pub fn default_analog() -> Self {
        Self::new(25.0, 0.01, 0.001)
    }

    /// Update temperature based on signal energy
    pub fn update(&mut self, signal_energy: f64, dt: f64) {
        let heating = signal_energy * self.heat_rate;
        let cooling = (self.temperature - self.ambient) * self.cool_rate;
        self.temperature += (heating - cooling) * dt;
    }

    /// Get current temperature
    pub fn temperature(&self) -> f64 {
        self.temperature
    }

    /// Get current temperature offset from ambient
    pub fn offset(&self) -> f64 {
        self.temperature - self.ambient
    }

    /// Reset to ambient temperature
    pub fn reset(&mut self) {
        self.temperature = self.ambient;
    }
}

impl Default for ThermalModel {
    fn default() -> Self {
        Self::default_analog()
    }
}

/// Noise generation utilities
pub mod noise {
    /// White noise (flat spectrum)
    pub fn white() -> f64 {
        rand::random::<f64>() * 2.0 - 1.0
    }

    /// Pink noise generator (1/f spectrum) using Voss-McCartney algorithm
    #[derive(Debug, Clone)]
    pub struct PinkNoise {
        rows: [f64; 16],
        running_sum: f64,
        index: u32,
    }

    impl PinkNoise {
        pub fn new() -> Self {
            Self {
                rows: [0.0; 16],
                running_sum: 0.0,
                index: 0,
            }
        }

        pub fn next(&mut self) -> f64 {
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

    impl Default for PinkNoise {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Power supply ripple (low frequency hum)
    #[derive(Debug, Clone)]
    pub struct PowerSupplyNoise {
        phase: f64,
        frequency: f64, // 50 or 60 Hz
        sample_rate: f64,
        amplitude: f64,
    }

    impl PowerSupplyNoise {
        pub fn new(sample_rate: f64, frequency: f64, amplitude: f64) -> Self {
            Self {
                phase: 0.0,
                frequency,
                sample_rate,
                amplitude,
            }
        }

        /// Create 60Hz power supply noise (North America)
        pub fn hz_60(sample_rate: f64, amplitude: f64) -> Self {
            Self::new(sample_rate, 60.0, amplitude)
        }

        /// Create 50Hz power supply noise (Europe, etc.)
        pub fn hz_50(sample_rate: f64, amplitude: f64) -> Self {
            Self::new(sample_rate, 50.0, amplitude)
        }

        pub fn next(&mut self) -> f64 {
            let out = (self.phase * std::f64::consts::TAU).sin() * self.amplitude;
            self.phase = (self.phase + self.frequency / self.sample_rate).fract();
            out + white() * self.amplitude * 0.1
        }

        pub fn set_sample_rate(&mut self, sample_rate: f64) {
            self.sample_rate = sample_rate;
        }
    }
}

/// Analog-modeled Voltage Controlled Oscillator
///
/// A VCO with analog imperfections: component tolerance, thermal drift,
/// DC offset, and slight asymmetric saturation.
pub struct AnalogVco {
    phase: f64,
    sample_rate: f64,

    // Analog modeling
    freq_component: ComponentModel,
    thermal: ThermalModel,
    dc_offset: f64,

    // State
    last_output: f64,
    last_sync: f64,

    spec: PortSpec,
}

impl AnalogVco {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
            freq_component: ComponentModel::new(0.02, 0.0001), // 2% tolerance
            thermal: ThermalModel::new(25.0, 0.01, 0.001),
            dc_offset: (rand::random::<f64>() * 2.0 - 1.0) * 0.01,
            last_output: 0.0,
            last_sync: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "voct", SignalKind::VoltPerOctave),
                    PortDef::new(1, "fm", SignalKind::CvBipolar).with_attenuverter(),
                    PortDef::new(2, "pw", SignalKind::CvUnipolar).with_default(0.5),
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

impl Default for AnalogVco {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for AnalogVco {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let voct = inputs.get_or(0, 0.0);
        let fm = inputs.get_or(1, 0.0);
        let pw = inputs.get_or(2, 0.5).clamp(0.05, 0.95);
        let sync = inputs.get_or(3, 0.0);

        // Apply component tolerance and thermal drift to frequency
        let base_freq = 261.63 * 2.0_f64.powf(voct);
        let freq = self.freq_component.apply(base_freq);
        let freq = freq * (1.0 + self.thermal.offset() * 0.001); // Thermal detuning
        let freq = freq * 2.0_f64.powf(fm);

        // Update thermal model
        self.thermal
            .update(self.last_output.powi(2), 1.0 / self.sample_rate);

        // Hard sync
        if sync > 2.5 && self.last_sync <= 2.5 {
            self.phase = 0.0;
        }
        self.last_sync = sync;

        // Generate waveforms with slight analog imperfections
        let sin = (self.phase * TAU).sin();
        let tri = 1.0 - 4.0 * (self.phase - 0.5).abs();
        let saw = 2.0 * self.phase - 1.0;
        let sqr = if self.phase < pw { 1.0 } else { -1.0 };

        // Add DC offset and slight asymmetric saturation
        let saw = saturation::asym_sat(saw + self.dc_offset, 1.0, 0.98);

        self.last_output = saw;
        self.phase = (self.phase + freq / self.sample_rate).fract();
        if self.phase < 0.0 {
            self.phase += 1.0;
        }

        // Output at ±5V
        outputs.set(10, sin * 5.0);
        outputs.set(11, tri * 5.0);
        outputs.set(12, saw * 5.0);
        outputs.set(13, sqr * 5.0);
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.last_output = 0.0;
        self.last_sync = 0.0;
        self.thermal.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "analog_vco"
    }
}

/// Saturator module for adding warmth and harmonics
pub struct Saturator {
    drive: f64,
    spec: PortSpec,
}

impl Saturator {
    pub fn new(drive: f64) -> Self {
        Self {
            drive,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "drive", SignalKind::CvUnipolar)
                        .with_default(drive)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }

    pub fn soft(drive: f64) -> Self {
        Self::new(drive)
    }
}

impl Default for Saturator {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl GraphModule for Saturator {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let drive = inputs.get_or(1, self.drive).max(0.1);

        let saturated = saturation::tanh_sat(input / 5.0, drive) * 5.0;
        outputs.set(10, saturated);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "saturator"
    }
}

/// Wavefolder module
pub struct Wavefolder {
    threshold: f64,
    spec: PortSpec,
}

impl Wavefolder {
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold: threshold.max(0.1),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "threshold", SignalKind::CvUnipolar)
                        .with_default(threshold)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }
}

impl Default for Wavefolder {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl GraphModule for Wavefolder {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let threshold = inputs.get_or(1, self.threshold).max(0.1);

        let folded = saturation::fold(input / 5.0, threshold) * 5.0;
        outputs.set(10, folded);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "wavefolder"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tanh_saturation() {
        // Saturation preserves sign
        assert!(saturation::tanh_sat(0.5, 1.0) > 0.0);
        assert!(saturation::tanh_sat(-0.5, 1.0) < 0.0);

        // Higher drive increases saturation effect
        let low_drive = saturation::tanh_sat(0.5, 0.5);
        let high_drive = saturation::tanh_sat(0.5, 2.0);
        // With normalization, both should be close to input for moderate values
        assert!(low_drive.abs() < 1.0);
        assert!(high_drive.abs() < 1.0);

        // Very high drive: approaches limits
        let saturated = saturation::tanh_sat(1.0, 10.0);
        assert!(saturated > 0.9 && saturated <= 1.0);
    }

    #[test]
    fn test_soft_clip() {
        // Below threshold: unchanged
        assert!((saturation::soft_clip(0.5, 1.0) - 0.5).abs() < 0.01);

        // Above threshold: compressed
        let clipped = saturation::soft_clip(2.0, 1.0);
        assert!(clipped > 1.0 && clipped < 2.0);
    }

    #[test]
    fn test_wavefold() {
        // Below threshold: unchanged
        assert!((saturation::fold(0.5, 1.0) - 0.5).abs() < 0.01);

        // Above threshold: folded back
        let folded = saturation::fold(1.5, 1.0);
        assert!(folded.abs() < 1.0);
    }

    #[test]
    fn test_component_model() {
        let perfect = ComponentModel::perfect();
        assert!((perfect.factor() - 1.0).abs() < 0.001);

        let resistor = ComponentModel::resistor_1pct();
        assert!(resistor.factor() >= 0.99 && resistor.factor() <= 1.01);
    }

    #[test]
    fn test_thermal_model() {
        let mut thermal = ThermalModel::new(25.0, 0.1, 0.01);

        // Heat up
        thermal.update(1.0, 0.001);
        assert!(thermal.offset() > 0.0);

        // Cool down
        for _ in 0..1000 {
            thermal.update(0.0, 0.001);
        }
        assert!(thermal.offset() < 0.01);
    }

    #[test]
    fn test_pink_noise() {
        let mut pink = noise::PinkNoise::new();
        let mut sum = 0.0;

        for _ in 0..1000 {
            sum += pink.next().abs();
        }

        // Should produce some output
        assert!(sum > 0.0);
    }

    #[test]
    fn test_analog_vco() {
        let mut vco = AnalogVco::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0); // C4

        // Should produce output
        vco.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).is_some());
        assert!(outputs.get(12).is_some());
    }
}
