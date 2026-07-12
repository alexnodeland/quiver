//! Analog Modeling Primitives
//!
//! This module provides primitives for modeling analog circuit behavior:
//! saturation, soft clipping, component variation, thermal drift, and noise.

use crate::modules::common::C4_HZ;
use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use crate::rng;
use alloc::vec;
use core::f64::consts::TAU;
use libm::Libm;

/// Saturation and soft clipping functions
pub mod saturation {
    use libm::Libm;

    /// Hyperbolic tangent saturation (tube-like warmth).
    ///
    /// Higher `drive` values increase harmonic content. The input is pre-scaled
    /// (by `tanh(drive)/drive`) before the `tanh(x*drive)/tanh(drive)`
    /// normalization so the small-signal gain at the origin is exactly unity —
    /// low-level signals pass through transparently instead of being boosted (a
    /// naive `tanh(x*drive)/tanh(drive)` has origin slope `drive/tanh(drive) > 1`,
    /// e.g. 1.31 at `drive = 1`). The transfer function therefore reduces to
    /// `tanh(x*tanh(drive))/tanh(drive)`, and the result is clamped to `[-1, 1]`.
    pub fn tanh_sat(x: f64, drive: f64) -> f64 {
        let t = Libm::<f64>::tanh(drive).max(0.001);
        (Libm::<f64>::tanh(x * t) / t).clamp(-1.0, 1.0)
    }

    /// Soft clipping with adjustable knee
    ///
    /// Signals below threshold pass through unchanged;
    /// signals above are compressed.
    pub fn soft_clip(x: f64, threshold: f64) -> f64 {
        if Libm::<f64>::fabs(x) < threshold {
            x
        } else {
            let sign = if x >= 0.0 { 1.0 } else { -1.0 };
            let excess = Libm::<f64>::fabs(x) - threshold;
            sign * (threshold + excess / (1.0 + excess))
        }
    }

    /// Asymmetric saturation (generates even harmonics)
    ///
    /// Different drive for positive and negative half-cycles
    /// creates even harmonics, giving a warmer, tube-like character.
    pub fn asym_sat(x: f64, pos_drive: f64, neg_drive: f64) -> f64 {
        if x >= 0.0 {
            Libm::<f64>::tanh(x * pos_drive)
        } else {
            Libm::<f64>::tanh(x * neg_drive)
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

        while Libm::<f64>::fabs(y) > threshold && iterations < max_iterations {
            if y > threshold {
                y = 2.0 * threshold - y;
            } else if y < -threshold {
                y = -2.0 * threshold - y;
            }
            iterations += 1;
        }
        y
    }

    /// Cubic soft saturation.
    ///
    /// The classic `x - x³/3` soft-clip curve. Its zero-slope maximum of `2/3`
    /// occurs at `x = 1` (not `2/3`), so the branch switches to the clamp at
    /// `|x| = 1`. This joins the curve to the clamp value `2/3` continuously in
    /// both value and slope — using the wrong threshold `2/3` leaves a ~0.099
    /// step discontinuity at the knee that injects clicks/harmonics.
    pub fn cubic_sat(x: f64) -> f64 {
        if Libm::<f64>::fabs(x) < 1.0 {
            x - x * x * x / 3.0
        } else {
            let sign = if x >= 0.0 { 1.0 } else { -1.0 };
            sign * 2.0 / 3.0
        }
    }

    /// Gentle asymmetric saturation with output gain compensation.
    ///
    /// Applies a mild asymmetric `tanh` shaper (slightly different positive and
    /// negative drive) for subtle even-harmonic warmth, then compensates the
    /// gain so a unit-amplitude input keeps its positive peak (under ~3% level
    /// loss). A raw `tanh` shaper (e.g. `asym_sat(x, 1.0, 0.98)`) would squash
    /// the `±1` peaks to `±0.76` — a ~24% level loss with almost no asymmetry.
    pub fn gentle_asym_sat(x: f64) -> f64 {
        // Gentle drives keep the waveform nearly linear; the small pos/neg
        // difference produces mild even harmonics without a large asymmetry.
        const POS_DRIVE: f64 = 0.3;
        const NEG_DRIVE: f64 = 0.27;
        // Normalize so a +1 input maps back to ~+1 output (tanh(POS_DRIVE) is
        // the uncompensated positive peak).
        let comp = 1.0 / Libm::<f64>::tanh(POS_DRIVE);
        asym_sat(x, POS_DRIVE, NEG_DRIVE) * comp
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
            instance_offset: rng::random_bipolar() * tolerance,
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

    /// Create a default thermal model, calibrated as a phenomenological analog
    /// warm-up model.
    ///
    /// The ODE is `temp' = energy*heat_rate - (temp-ambient)*cool_rate`:
    /// - `ambient = 25 °C` — room-temperature reference.
    /// - `cool_rate = 0.025 /s` → thermal time constant `tau = 1/cool_rate = 40 s`,
    ///   i.e. tens of seconds to warm up and settle, like a real oscillator core
    ///   (the previous `0.001` gave `tau ≈ 1000 s ≈ 16 min`, inaudible).
    /// - `heat_rate = 0.4 °C per unit signal-energy·second` → with a typical
    ///   oscillator signal energy (mean-square ≈ 0.2) the equilibrium offset is
    ///   `energy * heat_rate / cool_rate ≈ 3 °C`, which at `1 cent/°C` (see
    ///   [`detune_ratio`](Self::detune_ratio)) is a few cents of drift — audible
    ///   but bounded.
    pub fn default_analog() -> Self {
        Self::new(25.0, 0.4, 0.025)
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

    /// Pitch detuning caused by the current thermal offset, as a frequency
    /// multiplier.
    ///
    /// Analog oscillators drift on the order of a cent or so per °C; this model
    /// uses `1 cent/°C`, so a few °C of warm-up yields a few cents of detune.
    /// The result is bounded because the thermal offset itself is bounded by
    /// `energy * heat_rate / cool_rate`. `1 cent = 2^(1/1200)`.
    pub fn detune_ratio(&self) -> f64 {
        const DETUNE_CENTS_PER_DEGC: f64 = 1.0;
        let cents = self.offset() * DETUNE_CENTS_PER_DEGC;
        Libm::<f64>::pow(2.0, cents / 1200.0)
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
    use crate::rng;
    use core::f64::consts::TAU;
    use libm::Libm;

    /// White noise (flat spectrum)
    pub fn white() -> f64 {
        rng::random_bipolar()
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
                self.rows[i] = rng::random_bipolar();
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
            let out = Libm::<f64>::sin(self.phase * TAU) * self.amplitude;
            let new_phase = self.phase + self.frequency / self.sample_rate;
            self.phase = new_phase - Libm::<f64>::floor(new_phase);
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

    /// Ornstein-Uhlenbeck drift state (slow pitch wander), in cents
    drift_state: f64,

    /// OU mean-reversion time constant, in seconds (correlation time of the
    /// drift). Tens of seconds gives a slow, musical wander.
    drift_tau: f64,

    /// OU noise amplitude, in cents per sqrt(second). With the OU process
    /// `dX = -(X/tau) dt + sigma*sqrt(dt)*noise`, the stationary standard
    /// deviation is `sigma * sqrt(Var(noise) * tau / 2)`. The noise source is
    /// uniform `[-1, 1]` (variance `1/3`), so `std = sigma * sqrt(tau / 6)` —
    /// about 3 cents for `tau = 30 s`, `sigma = 1.34`.
    drift_sigma: f64,
}

impl VoctTrackingModel {
    /// Create a new tracking model with typical analog characteristics
    pub fn new() -> Self {
        Self {
            base_error_cents: rng::random_bipolar() * 5.0, // ±5 cents base
            octave_error_coef: 1.0 + rng::random() * 2.0,  // 1-3 cents/octave
            center_octave: 4.0,
            drift_state: 0.0,
            drift_tau: 30.0,   // seconds (mean-reversion correlation time)
            drift_sigma: 1.34, // ~3 cents stationary std with uniform noise
        }
    }

    /// Create a perfect tracking model (no errors)
    pub fn perfect() -> Self {
        Self {
            base_error_cents: 0.0,
            octave_error_coef: 0.0,
            center_octave: 4.0,
            drift_state: 0.0,
            drift_tau: 30.0,
            drift_sigma: 0.0,
        }
    }

    /// Apply tracking error to a V/Oct value, returning the modified V/Oct
    pub fn apply(&mut self, voct: f64, dt: f64) -> f64 {
        // Slow pitch drift modeled as an Ornstein-Uhlenbeck process (a
        // mean-reverting random walk): dX = -(X/tau) dt + sigma*sqrt(dt)*noise.
        // The sqrt(dt) noise scaling makes the wander sample-rate invariant in
        // wall-clock time (unlike a dt-scaled random walk, whose diffusion rate
        // is proportional to 1/sample_rate).
        let noise = rng::random_bipolar();
        self.drift_state += -self.drift_state / self.drift_tau * dt
            + self.drift_sigma * Libm::<f64>::sqrt(dt) * noise;
        self.drift_state = self.drift_state.clamp(-25.0, 25.0);

        // Signed octave distance from center. A real analog V/Oct error is a
        // scale-factor error, so pitch runs sharp above the center octave and
        // flat below it — a monotone signed deviation, not the non-physical
        // symmetric V-shape produced by using the absolute distance.
        let current_octave = self.center_octave + voct;
        let octave_distance = current_octave - self.center_octave;

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

    /// Sample rate
    sample_rate: f64,
}

impl HighFrequencyRolloff {
    /// Create a new rolloff filter with given cutoff frequency
    pub fn new(sample_rate: f64, cutoff_hz: f64) -> Self {
        Self {
            cutoff_hz,
            state: 0.0,
            sample_rate,
        }
    }

    /// Create a default rolloff (12kHz cutoff)
    pub fn default_analog(sample_rate: f64) -> Self {
        Self::new(sample_rate, 12000.0)
    }

    /// One-pole low-pass coefficient for a given cutoff, clamped into `(0, 1)`.
    ///
    /// The recursion `y += a*(x - y)` has pole `1 - a` and is stable only for
    /// `0 < a < 1`; `omega/(1+omega)` is naturally in `(0, 1)` for `omega > 0`,
    /// and the clamp is a hard safety bound against degenerate inputs.
    fn calculate_coef(sample_rate: f64, cutoff_hz: f64) -> f64 {
        let omega = TAU * cutoff_hz / sample_rate;
        (omega / (1.0 + omega)).clamp(1.0e-6, 0.999)
    }

    /// Apply frequency-dependent rolloff.
    ///
    /// Higher oscillator frequencies experience progressively more
    /// high-frequency rolloff (slew-rate limiting, parasitic capacitance). This
    /// is modeled by *lowering the effective cutoff* as the played frequency
    /// rises above the nominal cutoff, then recomputing a stable one-pole
    /// coefficient — never by dividing the coefficient (which drove it past the
    /// stability bound and diverged for every note below ~3.8 kHz).
    pub fn apply(&mut self, input: f64, frequency: f64) -> f64 {
        let freq_ratio = (frequency / self.cutoff_hz).max(0.0);
        let effective_cutoff = self.cutoff_hz / (1.0 + freq_ratio);
        let coef = Self::calculate_coef(self.sample_rate, effective_cutoff);

        // One-pole lowpass filter; `coef` is guaranteed in (0, 1) so this is
        // unconditionally stable.
        self.state += coef * (input - self.state);
        self.state
    }

    /// Set sample rate
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
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
    prev_sync: f64,
    sync_ramp: f64, // For soft sync ramping

    spec: PortSpec,
}

impl AnalogVco {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
            freq_component: ComponentModel::new(0.02, 0.0001), // 2% tolerance
            thermal: ThermalModel::default_analog(),
            dc_offset: rng::random_bipolar() * 0.01,
            voct_tracking: VoctTrackingModel::new(),
            hf_rolloff: HighFrequencyRolloff::default_analog(sample_rate),
            last_output: 0.0,
            prev_sync: 0.0,
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

        // Apply component tolerance and thermal drift to frequency.
        // Q008: anchor at the exact shared C4 reference (`C4_HZ`), not an
        // imprecise 261.63 literal, so all oscillators share one tuning source.
        let base_freq = C4_HZ * Libm::<f64>::pow(2.0, voct_with_error);
        let freq = self.freq_component.apply(base_freq);
        let freq = freq * self.thermal.detune_ratio(); // Thermal detuning (a few cents warmed)
        let freq = freq * Libm::<f64>::pow(2.0, fm);

        // Update thermal model
        self.thermal.update(self.last_output * self.last_output, dt);

        // Phase 3: Improved oscillator sync with soft ramp
        if sync > 2.5 && self.prev_sync <= 2.5 {
            // Hard sync: reset phase
            self.phase = 0.0;
            // Start a soft sync ramp for smoother transient
            self.sync_ramp = 0.0;
        }
        self.prev_sync = sync;

        // Ramp up sync amplitude smoothly to avoid clicks
        if self.sync_ramp < 1.0 {
            self.sync_ramp = (self.sync_ramp + 0.01).min(1.0);
        }

        // Generate waveforms with slight analog imperfections
        let sin = Libm::<f64>::sin(self.phase * TAU);
        let tri = 1.0 - 4.0 * Libm::<f64>::fabs(self.phase - 0.5);
        let saw = 2.0 * self.phase - 1.0;
        let sqr = if self.phase < pw { 1.0 } else { -1.0 };

        // Add DC offset and gentle, gain-compensated asymmetric saturation for
        // subtle even-harmonic warmth without squashing the peak level.
        let saw = saturation::gentle_asym_sat(saw + self.dc_offset);

        // Apply sync ramp for smooth sync transients
        let sin = sin * self.sync_ramp;
        let tri = tri * self.sync_ramp;
        let saw = saw * self.sync_ramp;
        let sqr = sqr * self.sync_ramp;

        // Phase 3: Apply high-frequency rolloff (more effect on high notes)
        let sin = self.hf_rolloff.apply(sin, freq);

        self.last_output = saw;
        let new_phase = self.phase + freq / self.sample_rate;
        self.phase = new_phase - Libm::<f64>::floor(new_phase);
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
        self.prev_sync = 0.0;
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
    pub(crate) drive: f64,
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

// `Wavefolder` now lives in `crate::modules::nonlinear`; re-exported here so
// `crate::analog::Wavefolder` (registry, presets, introspection) keeps working.
pub use crate::modules::Wavefolder;

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

        // Very high drive at a unit input: strongly saturated but still bounded.
        // (With unity origin gain, a unit input maps near tanh(1) ≈ 0.76 rather
        // than being pushed to the ±1 rail.)
        let saturated = saturation::tanh_sat(1.0, 10.0);
        assert!(saturated > 0.5 && saturated <= 1.0);
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

    #[test]
    fn test_analog_vco_pitch_anchored_at_c4() {
        // Q008: with the analog imperfections neutralized, AnalogVco at 0 V must
        // oscillate at the shared C4 reference (C4_HZ), confirming the pitch
        // path is anchored to the single tuning constant, not a stray literal.
        // (The 0.029-cent difference vs 261.63 is below what a zero-crossing
        // count resolves; this guards against gross regressions and the literal
        // drifting away from C4_HZ.)
        let sr = 44100.0;
        let mut vco = AnalogVco::new(sr);
        // Neutralize component tolerance, V/Oct tracking error, thermal drift, DC.
        vco.freq_component = ComponentModel::perfect();
        vco.voct_tracking = VoctTrackingModel::perfect();
        vco.thermal = ThermalModel::new(25.0, 0.0, 0.025); // heat_rate 0 => no drift
        vco.dc_offset = 0.0;

        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.0); // 0 V = C4

        // Count rising zero-crossings of the saw output over one second.
        let n = sr as usize;
        let mut prev = 0.0;
        let mut crossings = 0usize;
        for i in 0..n {
            vco.tick(&inputs, &mut outputs);
            let s = outputs.get(12).unwrap();
            if i > 0 && prev < 0.0 && s >= 0.0 {
                crossings += 1;
            }
            prev = s;
        }
        let measured_hz = crossings as f64; // cycles per one-second window
        assert!(
            (measured_hz - C4_HZ).abs() < 2.0,
            "AnalogVco 0 V pitch not anchored at C4: measured {measured_hz} Hz, \
             expected {C4_HZ} Hz"
        );
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

    // ---- Wave B audit remediation tests ----

    // Q043: the HF rolloff one-pole coefficient must stay in (0, 1) so the
    // AnalogVco sin output no longer diverges to ±inf on ordinary notes.
    #[test]
    fn test_hf_rolloff_stable_low_freq() {
        let mut rolloff = HighFrequencyRolloff::new(44100.0, 12000.0);
        let mut out = 0.0;
        // Alternating drive at a low note (261.63 Hz) — the worst case that
        // previously drove the coefficient to ~6.3 and blew up in ~10 ms.
        for i in 0..100_000 {
            let input = if i % 2 == 0 { 1.0 } else { -1.0 };
            out = rolloff.apply(input, 261.63);
            assert!(out.is_finite(), "rolloff diverged at sample {i}: {out}");
        }
        assert!(out.abs() <= 1.0);
    }

    // Q043: AnalogVco sin (and all outputs) stay finite and bounded across the
    // full musical range, including the notes that previously diverged.
    #[test]
    fn test_analog_vco_stable_across_frequencies() {
        rng::seed(0xC0FFEE);
        // C4 (0 V), ~1 kHz, and ~8 kHz expressed in V/Oct (261.63 * 2^voct).
        for &voct in &[0.0, 1.9342, 4.9344] {
            let mut vco = AnalogVco::new(44100.0);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(0, voct);

            for n in 0..100_000 {
                vco.tick(&inputs, &mut outputs);
                for &port in &[10, 11, 12, 13] {
                    let v = outputs.get(port).unwrap();
                    assert!(v.is_finite(), "port {port} @ voct {voct} sample {n}: {v}");
                    assert!(v.abs() <= 6.0, "port {port} @ voct {voct} sample {n}: {v}");
                }
            }
        }
    }

    // Q044: cubic_sat is continuous at the knee (|x| = 1), no ~0.099 step.
    #[test]
    fn test_cubic_sat_knee_continuity() {
        let below = saturation::cubic_sat(0.999_999);
        let above = saturation::cubic_sat(1.000_001);
        assert!(
            (below - above).abs() < 1.0e-6,
            "knee discontinuity: below={below}, above={above}"
        );
        // The curve joins the clamp value exactly.
        assert!((saturation::cubic_sat(1.0) - 2.0 / 3.0).abs() < 1.0e-12);
        // Symmetric knee on the negative side too.
        let nb = saturation::cubic_sat(-0.999_999);
        let na = saturation::cubic_sat(-1.000_001);
        assert!((nb - na).abs() < 1.0e-6);
    }

    // Q045: the drift is an Ornstein-Uhlenbeck process whose wander is
    // sample-rate invariant in wall-clock time (sqrt(dt) noise scaling).
    #[test]
    fn test_voct_drift_sample_rate_invariance() {
        // Ensemble variance of the drift after a fixed wall-time, over many
        // independent instances. In the (short) pure-diffusion regime this is
        // sigma_eff^2 * t, which must not depend on the sample rate.
        fn ensemble_var(sample_rate: f64, wall_time: f64, trials: usize) -> f64 {
            let dt = 1.0 / sample_rate;
            let n = (wall_time * sample_rate) as usize;
            let mut vals = alloc::vec::Vec::with_capacity(trials);
            for _ in 0..trials {
                let mut m = VoctTrackingModel::new();
                m.drift_state = 0.0;
                for _ in 0..n {
                    m.apply(0.0, dt);
                }
                vals.push(m.drift_state);
            }
            let mean: f64 = vals.iter().sum::<f64>() / trials as f64;
            vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / trials as f64
        }

        rng::seed(0x5EED_1234);
        let var_low = ensemble_var(44_100.0, 0.4, 80);
        let var_high = ensemble_var(96_000.0, 0.4, 80);

        assert!(var_low.is_finite() && var_high.is_finite());
        // Both should be near the theoretical sigma_eff^2 * t ≈ 0.24.
        assert!(
            (0.08..0.7).contains(&var_low),
            "var_low out of expected band: {var_low}"
        );
        assert!(
            (0.08..0.7).contains(&var_high),
            "var_high out of expected band: {var_high}"
        );
        // And comparable to each other (sample-rate invariant).
        let ratio = var_low / var_high;
        assert!(
            (0.4..2.5).contains(&ratio),
            "sample-rate dependent: ratio={ratio}, low={var_low}, high={var_high}"
        );
    }

    // Q045: the drift never runs away — it stays well within the clamp.
    #[test]
    fn test_voct_drift_bounded() {
        rng::seed(0xD817);
        let mut m = VoctTrackingModel::new();
        let dt = 1.0 / 44_100.0;
        let mut max_abs = 0.0_f64;
        for _ in 0..1_000_000 {
            m.apply(0.0, dt);
            assert!(m.drift_state.is_finite());
            assert!(m.drift_state.abs() <= 25.0);
            max_abs = max_abs.max(m.drift_state.abs());
        }
        // With ~3 cent stationary std, ~23 s of audio should wander a few cents
        // but nowhere near the ±25 clamp — confirms it is neither frozen nor
        // runaway.
        assert!(max_abs > 0.5, "drift is frozen: max_abs={max_abs}");
        assert!(max_abs < 25.0, "drift hit the clamp: max_abs={max_abs}");
    }

    // Q046: tracking error is signed (sharp above center, flat below), not a
    // symmetric V-shape.
    #[test]
    fn test_voct_tracking_signed_error() {
        let mut tracking = VoctTrackingModel::perfect();
        tracking.octave_error_coef = 3.0; // known positive scale error
                                          // dt = 0 keeps the drift fixed for a clean comparison.
        let e_below = tracking.apply(-2.0, 0.0) - (-2.0);
        let e_center = tracking.apply(0.0, 0.0);
        let e_above = tracking.apply(2.0, 0.0) - 2.0;

        assert!(
            e_below < 0.0,
            "should be flat (negative) below center: {e_below}"
        );
        assert!(
            e_center.abs() < 1.0e-9,
            "should be ~0 at center: {e_center}"
        );
        assert!(
            e_above > 0.0,
            "should be sharp (positive) above center: {e_above}"
        );
        // Monotone increasing — no V-shape kink.
        assert!(e_below < e_center && e_center < e_above);
    }

    // Q047: tanh_sat has unity small-signal gain at the origin (no low-level
    // boost), is monotonic, and stays bounded.
    #[test]
    fn test_tanh_sat_unity_origin_gain() {
        for &drive in &[0.25, 0.5, 1.0, 2.0, 4.0] {
            let h = 1.0e-6;
            let deriv =
                (saturation::tanh_sat(h, drive) - saturation::tanh_sat(-h, drive)) / (2.0 * h);
            assert!(
                (deriv - 1.0).abs() < 1.0e-3,
                "origin gain != 1 at drive={drive}: {deriv}"
            );
            // Monotonic increasing over the operating range.
            assert!(saturation::tanh_sat(0.6, drive) > saturation::tanh_sat(0.3, drive));
            // Bounded to [-1, 1], even for out-of-range inputs.
            assert!(saturation::tanh_sat(1.0, drive).abs() <= 1.0);
            assert!(saturation::tanh_sat(-1.0, drive).abs() <= 1.0);
            assert!(saturation::tanh_sat(100.0, drive).abs() <= 1.0);
            assert!(saturation::tanh_sat(-100.0, drive).abs() <= 1.0);
        }
    }

    // Q048: the thermal model produces an audible-but-bounded detune trajectory
    // that warms up over tens of seconds.
    #[test]
    fn test_thermal_detune_trajectory() {
        let mut thermal = ThermalModel::default_analog();
        // Coarse dt (still << tau = 40 s) keeps the test fast; forward Euler is
        // effectively exact here.
        let dt = 1.0 / 1000.0;
        // Representative saw-like signal energy (mean square ≈ 0.19).
        let energy = 0.19;

        // Tests always build with std, so the inherent `log2` is available.
        let cents = |t: &ThermalModel| t.detune_ratio().log2() * 1200.0;

        // Cold start: no detune.
        assert!(cents(&thermal).abs() < 0.01);

        // Warm up for one time constant (~40 s).
        for _ in 0..(40.0 / dt) as usize {
            thermal.update(energy, dt);
        }
        let cents_1tau = cents(&thermal);

        // Continue out to ~5 time constants (near equilibrium).
        for _ in 0..(160.0 / dt) as usize {
            thermal.update(energy, dt);
        }
        let cents_eq = cents(&thermal);

        // Audible: at least ~1 cent of drift once warmed.
        assert!(cents_eq > 1.0, "detune inaudible: {cents_eq} cents");
        // Bounded: only a few cents, never a wild pitch shift.
        assert!(cents_eq < 8.0, "detune unbounded: {cents_eq} cents");
        // First-order warm-up: one time constant reaches ~63% of equilibrium.
        let ratio = cents_1tau / cents_eq;
        assert!(
            (0.5..0.8).contains(&ratio),
            "warm-up time constant off: ratio={ratio}"
        );
        assert!(cents_1tau < cents_eq);
    }

    // Q049: the AnalogVco saw shaper preserves peak level (<3% loss) while
    // keeping a mild even-harmonic asymmetry.
    #[test]
    fn test_gentle_asym_sat_peak_level() {
        let pos_peak = saturation::gentle_asym_sat(1.0);
        // Positive peak within 3% of unity (previously ~24% loss).
        assert!(
            (pos_peak - 1.0).abs() < 0.03,
            "peak level loss too large: {pos_peak}"
        );
        assert!(pos_peak.abs() <= 1.05, "peak overshoots: {pos_peak}");

        let neg_peak = saturation::gentle_asym_sat(-1.0);
        // Some, but mild, asymmetry (the source of even harmonics).
        let asymmetry = (pos_peak + neg_peak).abs();
        assert!(asymmetry > 0.01, "no even-harmonic asymmetry: {asymmetry}");
        assert!(asymmetry < 0.2, "asymmetry not mild: {asymmetry}");

        // Sign-preserving and monotonic through the origin.
        assert!(saturation::gentle_asym_sat(0.5) > 0.0);
        assert!(saturation::gentle_asym_sat(-0.5) < 0.0);
        assert!(saturation::gentle_asym_sat(0.6) > saturation::gentle_asym_sat(0.3));
    }
}
