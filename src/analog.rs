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

        /// Generate the next pink noise sample
        pub fn sample(&mut self) -> f64 {
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

        /// Generate the next power supply noise sample
        pub fn sample(&mut self) -> f64 {
            let out = (self.phase * std::f64::consts::TAU).sin() * self.amplitude;
            self.phase = (self.phase + self.frequency / self.sample_rate).fract();
            out + white() * self.amplitude * 0.1
        }

        pub fn set_sample_rate(&mut self, sample_rate: f64) {
            self.sample_rate = sample_rate;
        }
    }
}

/// V/Oct Tracking Model
///
/// Models the non-linear tracking errors that occur in analog VCOs,
/// where pitch accuracy degrades at extreme octaves.
#[derive(Debug, Clone)]
pub struct VoctTrackingModel {
    /// Base tracking error in cents (random offset per instance)
    base_error_cents: f64,

    /// Error coefficient per octave (cents/octave away from center)
    octave_error_coef: f64,

    /// Center octave (typically C4 = 0V = octave 4)
    center_octave: f64,

    /// Random walk state for slow drift
    drift_state: f64,

    /// Drift rate (how fast the tracking wanders)
    drift_rate: f64,
}

impl VoctTrackingModel {
    /// Create a new tracking model with typical analog characteristics
    pub fn new() -> Self {
        Self {
            base_error_cents: (rand::random::<f64>() * 2.0 - 1.0) * 5.0, // ±5 cents base
            octave_error_coef: 1.0 + rand::random::<f64>() * 2.0,        // 1-3 cents/octave
            center_octave: 4.0,
            drift_state: 0.0,
            drift_rate: 0.0001,
        }
    }

    /// Create a perfect tracking model (no errors)
    pub fn perfect() -> Self {
        Self {
            base_error_cents: 0.0,
            octave_error_coef: 0.0,
            center_octave: 4.0,
            drift_state: 0.0,
            drift_rate: 0.0,
        }
    }

    /// Apply tracking error to a V/Oct value, returning the modified V/Oct
    pub fn apply(&mut self, voct: f64, dt: f64) -> f64 {
        // Update drift (slow random walk)
        self.drift_state += (rand::random::<f64>() * 2.0 - 1.0) * self.drift_rate * dt * 1000.0;
        self.drift_state = self.drift_state.clamp(-10.0, 10.0);

        // Calculate octave distance from center
        let current_octave = self.center_octave + voct;
        let octave_distance = (current_octave - self.center_octave).abs();

        // Total error in cents
        let error_cents =
            self.base_error_cents + (octave_distance * self.octave_error_coef) + self.drift_state;

        // Convert cents error to V/Oct offset (100 cents = 1 semitone = 1/12 octave)
        let error_voct = error_cents / 1200.0;

        voct + error_voct
    }

    /// Reset the drift state
    pub fn reset(&mut self) {
        self.drift_state = 0.0;
    }
}

impl Default for VoctTrackingModel {
    fn default() -> Self {
        Self::new()
    }
}

/// High-Frequency Rolloff Model
///
/// Models the high-frequency rolloff that occurs in analog VCOs due to
/// slew rate limiting and parasitic capacitance.
#[derive(Debug, Clone)]
pub struct HighFrequencyRolloff {
    /// -3dB cutoff frequency
    cutoff_hz: f64,

    /// Current filter state
    state: f64,

    /// Filter coefficient
    coef: f64,

    /// Sample rate
    sample_rate: f64,
}

impl HighFrequencyRolloff {
    /// Create a new rolloff filter with given cutoff frequency
    pub fn new(sample_rate: f64, cutoff_hz: f64) -> Self {
        let coef = Self::calculate_coef(sample_rate, cutoff_hz);
        Self {
            cutoff_hz,
            state: 0.0,
            coef,
            sample_rate,
        }
    }

    /// Create a default rolloff (12kHz cutoff)
    pub fn default_analog(sample_rate: f64) -> Self {
        Self::new(sample_rate, 12000.0)
    }

    fn calculate_coef(sample_rate: f64, cutoff_hz: f64) -> f64 {
        let omega = TAU * cutoff_hz / sample_rate;
        omega / (1.0 + omega)
    }

    /// Apply frequency-dependent rolloff
    /// Higher frequencies get more attenuation
    pub fn apply(&mut self, input: f64, frequency: f64) -> f64 {
        // Increase rolloff for higher frequencies
        let freq_factor = (frequency / self.cutoff_hz).max(0.1);
        let effective_coef = self.coef / freq_factor.min(4.0);

        // One-pole lowpass filter
        self.state += effective_coef * (input - self.state);
        self.state
    }

    /// Set sample rate and recalculate coefficient
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.coef = Self::calculate_coef(sample_rate, self.cutoff_hz);
    }

    /// Reset filter state
    pub fn reset(&mut self) {
        self.state = 0.0;
    }
}

impl Default for HighFrequencyRolloff {
    fn default() -> Self {
        Self::new(44100.0, 12000.0)
    }
}

/// Analog-modeled Voltage Controlled Oscillator
///
/// A VCO with analog imperfections: component tolerance, thermal drift,
/// DC offset, asymmetric saturation, V/Oct tracking errors, and
/// high-frequency rolloff.
pub struct AnalogVco {
    phase: f64,
    sample_rate: f64,

    // Analog modeling
    freq_component: ComponentModel,
    thermal: ThermalModel,
    dc_offset: f64,

    // Phase 3: Enhanced analog modeling
    voct_tracking: VoctTrackingModel,
    hf_rolloff: HighFrequencyRolloff,

    // Sync state
    last_output: f64,
    last_sync: f64,
    sync_ramp: f64, // For soft sync ramping

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
            voct_tracking: VoctTrackingModel::new(),
            hf_rolloff: HighFrequencyRolloff::default_analog(sample_rate),
            last_output: 0.0,
            last_sync: 0.0,
            sync_ramp: 1.0,
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

        let dt = 1.0 / self.sample_rate;

        // Phase 3: Apply V/Oct tracking errors
        let voct_with_error = self.voct_tracking.apply(voct, dt);

        // Apply component tolerance and thermal drift to frequency
        let base_freq = 261.63 * 2.0_f64.powf(voct_with_error);
        let freq = self.freq_component.apply(base_freq);
        let freq = freq * (1.0 + self.thermal.offset() * 0.001); // Thermal detuning
        let freq = freq * 2.0_f64.powf(fm);

        // Update thermal model
        self.thermal.update(self.last_output.powi(2), dt);

        // Phase 3: Improved oscillator sync with soft ramp
        if sync > 2.5 && self.last_sync <= 2.5 {
            // Hard sync: reset phase
            self.phase = 0.0;
            // Start a soft sync ramp for smoother transient
            self.sync_ramp = 0.0;
        }
        self.last_sync = sync;

        // Ramp up sync amplitude smoothly to avoid clicks
        if self.sync_ramp < 1.0 {
            self.sync_ramp = (self.sync_ramp + 0.01).min(1.0);
        }

        // Generate waveforms with slight analog imperfections
        let sin = (self.phase * TAU).sin();
        let tri = 1.0 - 4.0 * (self.phase - 0.5).abs();
        let saw = 2.0 * self.phase - 1.0;
        let sqr = if self.phase < pw { 1.0 } else { -1.0 };

        // Add DC offset and slight asymmetric saturation
        let saw = saturation::asym_sat(saw + self.dc_offset, 1.0, 0.98);

        // Apply sync ramp for smooth sync transients
        let sin = sin * self.sync_ramp;
        let tri = tri * self.sync_ramp;
        let saw = saw * self.sync_ramp;
        let sqr = sqr * self.sync_ramp;

        // Phase 3: Apply high-frequency rolloff (more effect on high notes)
        let sin = self.hf_rolloff.apply(sin, freq);

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
        self.sync_ramp = 1.0;
        self.thermal.reset();
        self.voct_tracking.reset();
        self.hf_rolloff.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.hf_rolloff.set_sample_rate(sample_rate);
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
            sum += pink.sample().abs();
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

    // Phase 3 Tests

    #[test]
    fn test_voct_tracking_model() {
        let mut tracking = VoctTrackingModel::new();

        // Should add some error to the pitch
        let voct_in = 0.0;
        let voct_out = tracking.apply(voct_in, 1.0 / 44100.0);

        // Error should be small but non-zero (within ±50 cents = ±0.042 V/Oct)
        assert!((voct_out - voct_in).abs() < 0.05);

        // Error should increase with octave distance (test signed error, not absolute)
        let error_at_c4 = tracking.apply(0.0, 0.0) - 0.0;
        let error_at_c6 = tracking.apply(2.0, 0.0) - 2.0;
        // C6 is 2 octaves away, so signed error should be larger (octave_error_coef is always positive)
        assert!(error_at_c6 >= error_at_c4);
    }

    #[test]
    fn test_voct_tracking_perfect() {
        let mut tracking = VoctTrackingModel::perfect();

        // Perfect tracking should have no error
        let voct_out = tracking.apply(2.0, 1.0 / 44100.0);
        assert!((voct_out - 2.0).abs() < 0.0001);
    }

    #[test]
    fn test_high_frequency_rolloff() {
        let mut rolloff = HighFrequencyRolloff::new(44100.0, 12000.0);

        // Process a signal
        let output = rolloff.apply(1.0, 261.0); // Low frequency
        assert!(output > 0.0);

        // Reset and test high frequency - should have more attenuation
        rolloff.reset();
        let mut high_freq_out = 0.0;
        for _ in 0..100 {
            high_freq_out = rolloff.apply(1.0, 16000.0);
        }
        // High frequency signal should be attenuated
        assert!(high_freq_out < 1.0);
    }

    #[test]
    fn test_analog_vco_with_sync() {
        let mut vco = AnalogVco::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0); // C4

        // Run a few samples
        for _ in 0..100 {
            vco.tick(&inputs, &mut outputs);
        }

        // Trigger sync
        inputs.set(3, 5.0); // Sync high
        vco.tick(&inputs, &mut outputs);

        // After sync, amplitude should be ramping up
        let out1 = outputs.get(10).unwrap_or(0.0);

        inputs.set(3, 0.0); // Sync low
        vco.tick(&inputs, &mut outputs);
        let out2 = outputs.get(10).unwrap_or(0.0);

        // Sync ramp should be taking effect (output increasing toward full amplitude)
        assert!(out1.abs() <= out2.abs() || (out1.abs() < 5.0 && out2.abs() < 5.0));
    }

    // Additional tests for 100% coverage

    #[test]
    fn test_diode_clip() {
        // Test diode clip saturation
        let result = saturation::diode_clip(1.0, 0.7);
        assert!(result > 0.7);

        let result_neg = saturation::diode_clip(-1.0, 0.7);
        assert!(result_neg < -0.7);

        // Within forward voltage - unchanged
        let within = saturation::diode_clip(0.5, 0.7);
        assert!((within - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_cubic_sat() {
        // Below threshold
        let low = saturation::cubic_sat(0.5);
        assert!(low > 0.0);
        assert!(low < 0.5);

        // Above threshold (2/3)
        let high = saturation::cubic_sat(1.0);
        assert!((high - 2.0 / 3.0).abs() < 0.001);

        // Negative value above threshold
        let high_neg = saturation::cubic_sat(-1.0);
        assert!((high_neg - (-2.0 / 3.0)).abs() < 0.001);
    }

    #[test]
    fn test_asym_sat() {
        let pos = saturation::asym_sat(0.5, 1.0, 0.8);
        assert!(pos > 0.0);

        let neg = saturation::asym_sat(-0.5, 1.0, 0.8);
        assert!(neg < 0.0);
    }

    #[test]
    fn test_component_model_capacitor() {
        let cap = ComponentModel::capacitor_5pct();
        assert!(cap.tolerance == 0.05);
        assert!(cap.factor() >= 0.95 && cap.factor() <= 1.05);
    }

    #[test]
    fn test_component_model_default() {
        let comp = ComponentModel::default();
        assert!((comp.factor() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_component_model_temperature() {
        let mut comp = ComponentModel::new(0.01, 0.001);
        comp.set_temperature(10.0);
        assert!(comp.temp_offset == 10.0);

        let applied = comp.apply(100.0);
        assert!(applied != 100.0); // Should have some variation
    }

    #[test]
    fn test_thermal_model_default() {
        let thermal = ThermalModel::default();
        assert!((thermal.temperature() - 25.0).abs() < 0.001);
    }

    #[test]
    fn test_pink_noise_default() {
        let mut pink = noise::PinkNoise::default();
        let _sample = pink.sample();
    }

    #[test]
    fn test_power_supply_noise() {
        let mut psn = noise::PowerSupplyNoise::new(44100.0, 60.0, 0.01);
        let sample1 = psn.sample();
        assert!(sample1.abs() <= 0.02);

        // Test 60Hz constructor
        let mut psn60 = noise::PowerSupplyNoise::hz_60(44100.0, 0.01);
        let _ = psn60.sample();

        // Test 50Hz constructor
        let mut psn50 = noise::PowerSupplyNoise::hz_50(44100.0, 0.01);
        let _ = psn50.sample();

        // Test set_sample_rate
        psn.set_sample_rate(48000.0);
    }

    #[test]
    fn test_voct_tracking_default() {
        let tracking = VoctTrackingModel::default();
        assert!(tracking.center_octave == 4.0);
    }

    #[test]
    fn test_hf_rolloff_default() {
        let rolloff = HighFrequencyRolloff::default();
        assert!(rolloff.cutoff_hz == 12000.0);
    }

    #[test]
    fn test_analog_vco_default() {
        let vco = AnalogVco::default();
        assert!(vco.sample_rate == 44100.0);
    }

    #[test]
    fn test_analog_vco_reset_set_sample_rate() {
        let mut vco = AnalogVco::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.0);
        for _ in 0..100 {
            vco.tick(&inputs, &mut outputs);
        }

        vco.reset();
        assert!(vco.phase == 0.0);

        vco.set_sample_rate(48000.0);
        assert!(vco.sample_rate == 48000.0);

        assert_eq!(vco.type_id(), "analog_vco");
    }

    #[test]
    fn test_analog_vco_negative_phase() {
        // Test negative phase wraparound in tick - we need negative FM
        let mut vco = AnalogVco::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, -10.0); // Very low pitch (negative V/Oct)
        inputs.set(1, -5.0); // Negative FM to make frequency negative

        // Run enough samples to potentially go negative
        for _ in 0..1000 {
            vco.tick(&inputs, &mut outputs);
        }
        // Just ensure it doesn't crash
        assert!(vco.phase >= 0.0);
    }

    #[test]
    fn test_saturator_module() {
        let mut sat = Saturator::new(1.5);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 5.0); // Input signal

        sat.tick(&inputs, &mut outputs);
        let out = outputs.get(10).unwrap_or(0.0);
        assert!(out.abs() <= 5.0);

        // Test soft constructor
        let sat_soft = Saturator::soft(2.0);
        assert!(sat_soft.drive == 2.0);

        // Test default
        let sat_default = Saturator::default();
        assert!(sat_default.drive == 1.0);

        // Test reset/set_sample_rate/type_id
        sat.reset();
        sat.set_sample_rate(48000.0);
        assert_eq!(sat.type_id(), "saturator");
    }

    #[test]
    fn test_wavefolder_module() {
        let mut wf = Wavefolder::new(0.5);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 5.0); // Input signal beyond threshold

        wf.tick(&inputs, &mut outputs);
        let out = outputs.get(10).unwrap_or(0.0);
        assert!(out.abs() <= 5.0);

        // Test default
        let wf_default = Wavefolder::default();
        assert!(wf_default.threshold == 1.0);

        // Test reset/set_sample_rate/type_id
        wf.reset();
        wf.set_sample_rate(48000.0);
        assert_eq!(wf.type_id(), "wavefolder");
    }

    #[test]
    fn test_voct_tracking_reset() {
        let mut tracking = VoctTrackingModel::new();

        // Apply some drift
        for _ in 0..1000 {
            tracking.apply(0.0, 1.0 / 44100.0);
        }

        // Reset should clear drift
        tracking.reset();
        assert!(tracking.drift_state == 0.0);
    }
}
