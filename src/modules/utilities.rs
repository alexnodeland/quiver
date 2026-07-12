//! Utility, logic, CV, and sequencing modules.

use super::common::{EdgeDetector, GATE_HIGH_V, GATE_THRESHOLD_V};
use crate::port::{GraphModule, ParamDef, ParamId, PortDef, PortSpec, PortValues, SignalKind};
use crate::rng;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use core::f64::consts::TAU;
use libm::Libm;

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

    // `offset` is genuine internal state (not an input port); bridge it to introspection.
    crate::impl_introspect!();
}

/// Hysteresis band (in semitones) applied by the pitch quantizers so a CV
/// hovering on a note boundary does not chatter between two notes (Q041).
const NOTE_HYSTERESIS_SEMITONES: f64 = 0.3;

/// Apply per-note hysteresis to a quantizer.
///
/// A new candidate note is only committed once the input has moved
/// `hysteresis_semitones` *past* the midpoint between the last committed note
/// and the candidate; otherwise the previous note is held. All voltages are
/// V/Oct (`1.0` == 12 semitones); `last` is the previously committed output,
/// `None` on the first sample. This removes the boundary chatter described in
/// Q041 while leaving clean, decisive note changes untouched.
fn hysteretic_note(
    last: Option<f64>,
    input_v: f64,
    candidate_v: f64,
    hysteresis_semitones: f64,
) -> f64 {
    match last {
        None => candidate_v,
        Some(last_v) => {
            if candidate_v == last_v {
                return last_v;
            }
            let in_s = input_v * 12.0;
            let last_s = last_v * 12.0;
            let cand_s = candidate_v * 12.0;
            let boundary = (last_s + cand_s) * 0.5;
            let commit = if cand_s > last_s {
                in_s >= boundary + hysteresis_semitones
            } else {
                in_s <= boundary - hysteresis_semitones
            };
            if commit {
                candidate_v
            } else {
                last_v
            }
        }
    }
}

/// Scale Quantizer
///
/// Quantizes CV input to musical scale notes.
/// Supports major, minor, pentatonic, and chromatic scales.
pub struct ScaleQuantizer {
    /// Last committed output voltage, for note-change triggers and hysteresis.
    last_output: Option<f64>,
    /// Optional microtuning override (Q146): scale degrees in cents within one
    /// octave `[0, 1200)`, sorted. When non-empty it replaces the built-in 12-TET
    /// enum tables. Always present (heap-backed `Vec`), but only populated via the
    /// alloc-gated [`set_custom_scale`](Self::set_custom_scale) /
    /// [`load_scala`](Self::load_scala) setters.
    custom_cents: Vec<f64>,
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
            last_output: None,
            custom_cents: Vec::new(),
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
        let octave = note.div_euclid(12);
        let semitone = note.rem_euclid(12);

        // Find the closest scale note, also considering the scale root wrapped
        // into the NEXT octave (`s + 12`). Without carrying that +12 (Q034), a
        // top-of-octave input whose nearest note is the next root drops ~an
        // octave instead of snapping up. Mirrors `Quantizer::quantize`.
        let mut closest = scale[0] as i32;
        let mut min_dist = i32::MAX;

        for &s in scale {
            let s = s as i32;
            let dist = (semitone - s).abs();
            if dist < min_dist {
                min_dist = dist;
                closest = s;
            }
            let dist_wrap = (semitone - (s + 12)).abs();
            if dist_wrap < min_dist {
                min_dist = dist_wrap;
                closest = s + 12;
            }
        }

        octave * 12 + closest
    }

    /// Whether a microtuning custom scale is currently active (Q146).
    pub fn has_custom_scale(&self) -> bool {
        !self.custom_cents.is_empty()
    }

    /// Install a custom microtuning scale (Q146): `cents` are scale degrees within
    /// one octave, in cents (`0.0` is the root). The list is sorted and reduced
    /// into `[0, 1200)` internally, so callers need not pre-sort. Passing an empty
    /// slice clears the override and restores the built-in 12-TET scales.
    ///
    /// Non-real-time: allocates. Alloc-tier only.
    #[cfg(feature = "alloc")]
    pub fn set_custom_scale(&mut self, cents: &[f64]) {
        let mut degrees: Vec<f64> = cents
            .iter()
            .map(|&c| {
                let mut r = Libm::<f64>::fmod(c, 1200.0);
                if r < 0.0 {
                    r += 1200.0;
                }
                r
            })
            .collect();
        degrees.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        degrees.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        self.custom_cents = degrees;
    }

    /// Clear any custom microtuning scale, restoring the built-in 12-TET scales.
    #[cfg(feature = "alloc")]
    pub fn clear_custom_scale(&mut self) {
        self.custom_cents.clear();
    }

    /// Load a Scala (`.scl`) file body as the custom microtuning scale (Q146).
    ///
    /// On success the parsed scale's octave-reduced degrees become the active
    /// scale (see [`set_custom_scale`](Self::set_custom_scale)). On a malformed
    /// file the current scale is left unchanged and the parse error is returned.
    ///
    /// Non-real-time: allocates. Alloc-tier only.
    #[cfg(feature = "alloc")]
    pub fn load_scala(&mut self, source: &str) -> Result<(), crate::scala::ScalaError> {
        let scale = crate::scala::ScalaScale::parse(source)?;
        self.set_custom_scale(&scale.degrees_within_octave());
        Ok(())
    }

    /// Quantize a cents value to the nearest degree of a custom scale (Q146).
    ///
    /// `degrees` are sorted degrees within `[0, 1200)`; the search also considers
    /// each degree wrapped into the next octave so a pitch near the top of the
    /// octave snaps up to the next root rather than dropping an octave (mirrors
    /// [`quantize_to_scale`](Self::quantize_to_scale)).
    fn quantize_custom_cents(input_cents: f64, degrees: &[f64]) -> f64 {
        if degrees.is_empty() {
            return input_cents;
        }
        let octave = Libm::<f64>::floor(input_cents / 1200.0);
        let within = input_cents - octave * 1200.0;

        let mut closest = degrees[0];
        let mut min_dist = f64::MAX;
        for &d in degrees {
            let dist = Libm::<f64>::fabs(within - d);
            if dist < min_dist {
                min_dist = dist;
                closest = d;
            }
            let dist_wrap = Libm::<f64>::fabs(within - (d + 1200.0));
            if dist_wrap < min_dist {
                min_dist = dist_wrap;
                closest = d + 1200.0;
            }
        }

        octave * 1200.0 + closest
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

        // A custom microtuning scale (Q146) overrides the built-in 12-TET enum
        // tables and quantizes in cents rather than integer semitones.
        let candidate_voct = if !self.custom_cents.is_empty() {
            let root_cents = root as f64 * 100.0;
            let input_cents = input * 1200.0 - root_cents;
            let q_cents = Self::quantize_custom_cents(input_cents, &self.custom_cents);
            (q_cents + root_cents) / 1200.0
        } else {
            // Convert V/Oct to semitones from C4
            let semitones_from_c4 = Libm::<f64>::round(input * 12.0) as i32;

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
            (quantized + root) as f64 / 12.0
        };

        // Commit the note through hysteresis so a CV parked on a boundary does
        // not chatter (Q041), and fire the trigger only on an actual committed
        // note change rather than continuously while quantization is active.
        let prev = self.last_output;
        let output_voct = hysteretic_note(prev, input, candidate_voct, NOTE_HYSTERESIS_SEMITONES);
        let trigger = match prev {
            Some(p) if (p - output_voct).abs() > 1e-9 => GATE_HIGH_V,
            _ => 0.0,
        };
        self.last_output = Some(output_voct);

        outputs.set(10, output_voct);
        outputs.set(11, trigger);
    }

    fn reset(&mut self) {
        self.last_output = None;
    }

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
    /// Pulse count baked into the current `pattern`, so the pulses control is
    /// no longer inert when the step count is unchanged (Q037).
    last_pulses: usize,
    /// Rising-edge detector for the clock input (canonical 2.5V, Q129).
    clock_edge: EdgeDetector,
    /// Rising-edge detector for the reset input (canonical 2.5V, Q129).
    reset_edge: EdgeDetector,
    /// Whether the current pattern cycle has already fired its accent (Q042).
    cycle_accented: bool,
    spec: PortSpec,
}

impl Euclidean {
    pub fn new(_sample_rate: f64) -> Self {
        Self {
            step: 0,
            pattern: vec![true; 16],
            last_pulses: 16,
            clock_edge: EdgeDetector::new(),
            reset_edge: EdgeDetector::new(),
            cycle_accented: false,
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

        // Regenerate the pattern whenever the step count OR the pulse count
        // changes, so the pulses (density) control is live (Q037).
        if self.pattern.len() != steps || self.last_pulses != pulses {
            self.pattern = Self::generate_pattern(steps, pulses);
            self.last_pulses = pulses;
        }

        // Reset on a rising edge at the canonical gate threshold (Q129).
        if self.reset_edge.rising(reset) {
            self.step = 0;
            self.cycle_accented = false;
        }

        // Detect a clock rising edge at the canonical gate threshold (Q129).
        let trigger = self.clock_edge.rising(clock);

        let mut out = 0.0;
        let mut accent = 0.0;

        if trigger {
            // Rotation now spans the full 0..steps range (Q042); it shifts which
            // pattern slot is read at this sequence position.
            let rotation = ((rotation_cv * steps as f64) as usize).min(steps - 1);

            // A new pattern cycle begins at step 0: re-arm the accent.
            if self.step == 0 {
                self.cycle_accented = false;
            }

            let rotated_step = (self.step + rotation) % steps;

            if self.pattern[rotated_step] {
                out = GATE_HIGH_V;
                // Accent the active downbeat of the (rotated) pattern: the first
                // real pulse of the cycle, so the accent always coincides with a
                // pulse instead of firing on a pre-rotation counter that may land
                // on a rest (Q042).
                if !self.cycle_accented {
                    accent = GATE_HIGH_V;
                    self.cycle_accented = true;
                }
            }

            self.step = (self.step + 1) % steps;
        }

        outputs.set(10, out);
        outputs.set(11, accent);
    }

    fn reset(&mut self) {
        self.step = 0;
        self.cycle_accented = false;
        self.clock_edge.reset();
        self.reset_edge.reset();
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "euclidean"
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

        let clock_rising = clock > GATE_THRESHOLD_V && self.last_clock <= GATE_THRESHOLD_V;
        let reset_rising = reset > GATE_THRESHOLD_V && self.last_reset <= GATE_THRESHOLD_V;

        let mut trigger = 0.0;

        if reset_rising {
            self.current = 0;
            trigger = GATE_HIGH_V;
        } else if clock_rising {
            self.current = (self.current + 1) % 8;
            trigger = GATE_HIGH_V;
        }

        self.last_clock = clock;
        self.last_reset = reset;

        let cv = self.steps[self.current];
        let gate = if self.gates[self.current] && clock > GATE_THRESHOLD_V {
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

    // Step CV/gate values are internal state (no ports); bridge to introspection.
    crate::impl_introspect!();
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
    trigger_edge: EdgeDetector,
    spec: PortSpec,
}

impl SampleAndHold {
    pub fn new() -> Self {
        Self {
            held_value: 0.0,
            trigger_edge: EdgeDetector::new(),
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
        if self.trigger_edge.rising(trigger) {
            self.held_value = input;
        }

        outputs.set(10, self.held_value);
    }

    fn reset(&mut self) {
        self.held_value = 0.0;
        self.trigger_edge.reset();
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
    /// Last committed output voltage, for boundary hysteresis (Q041).
    last_output: Option<f64>,
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
            last_output: None,
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
        let candidate = self.quantize(input);
        // Hold the note through hysteresis so a CV parked on a boundary does not
        // chatter between adjacent scale degrees (Q041).
        let committed = hysteretic_note(
            self.last_output,
            input,
            candidate,
            NOTE_HYSTERESIS_SEMITONES,
        );
        self.last_output = Some(committed);
        outputs.set(10, committed);
    }

    fn reset(&mut self) {
        self.last_output = None;
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "quantizer"
    }

    // `scale` is internal state (no scale port); bridge to introspection.
    crate::impl_introspect!();
}

/// Clock Generator
///
/// Generates clock pulses at a specified tempo (BPM).
pub struct Clock {
    phase: f64,
    /// Integer count of completed main cycles, used to derive the divided
    /// outputs so they actually divide the tempo (Q035).
    cycle: u64,
    sample_rate: f64,
    spec: PortSpec,
}

impl Clock {
    /// Bpm-control CV that yields exactly 120 BPM through [`Clock::cv_to_bpm`].
    ///
    /// Since `cv_to_bpm(cv) = 20 * 15^(cv/10)`, solving `20 * 15^(cv/10) = 120`
    /// gives `cv = 10 * ln(6) / ln(15) ≈ 6.6164` (Q038 — the old `1.2` default
    /// produced only ~27.5 BPM).
    const DEFAULT_BPM_CV: f64 = 6.616_418_958_920_283;

    pub fn new(sample_rate: f64) -> Self {
        Self {
            phase: 0.0,
            cycle: 0,
            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "bpm", SignalKind::CvUnipolar)
                        .with_default(Self::DEFAULT_BPM_CV) // 120 BPM when scaled
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
        let bpm_cv = inputs.get_or(0, Self::DEFAULT_BPM_CV); // Default 120 BPM
        let reset = inputs.get_or(1, 0.0);

        let bpm = Self::cv_to_bpm(bpm_cv);
        let freq = bpm / 60.0; // Hz

        // Reset on trigger
        if reset > GATE_THRESHOLD_V {
            self.phase = 0.0;
            self.cycle = 0;
        }

        // Main clock output (short pulse at start of each cycle)
        let pulse_width = 0.1; // 10% duty cycle
        let in_pulse = self.phase < pulse_width;
        let main_out = if in_pulse { GATE_HIGH_V } else { 0.0 };

        // Divided outputs derived from the integer cycle counter so they
        // genuinely divide the tempo (Q035): div2 fires on even cycles, div4 on
        // every fourth cycle, both with the same pulse-width window as main.
        // Bitwise masks (both divisors are powers of two) keep this MSRV-1.78
        // safe, avoiding the newer `u64::is_multiple_of`.
        let div2_out = if in_pulse && (self.cycle & 1) == 0 {
            GATE_HIGH_V
        } else {
            0.0
        };
        let div4_out = if in_pulse && (self.cycle & 3) == 0 {
            GATE_HIGH_V
        } else {
            0.0
        };

        outputs.set(10, main_out);
        outputs.set(11, div2_out);
        outputs.set(12, div4_out);

        // Advance phase, incrementing the cycle counter on each wrap.
        let new_phase = self.phase + freq / self.sample_rate;
        let wraps = Libm::<f64>::floor(new_phase);
        if wraps > 0.0 {
            self.cycle = self.cycle.wrapping_add(wraps as u64);
        }
        self.phase = new_phase - wraps;
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.cycle = 0;
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
        let a = inputs.get_or(0, 0.0) > GATE_THRESHOLD_V;
        let b = inputs.get_or(1, 0.0) > GATE_THRESHOLD_V;

        outputs.set(10, if a && b { GATE_HIGH_V } else { 0.0 });
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
        let a = inputs.get_or(0, 0.0) > GATE_THRESHOLD_V;
        let b = inputs.get_or(1, 0.0) > GATE_THRESHOLD_V;

        outputs.set(10, if a || b { GATE_HIGH_V } else { 0.0 });
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
        let a = inputs.get_or(0, 0.0) > GATE_THRESHOLD_V;
        let b = inputs.get_or(1, 0.0) > GATE_THRESHOLD_V;

        outputs.set(10, if a ^ b { GATE_HIGH_V } else { 0.0 });
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
        let input = inputs.get_or(0, 0.0) > GATE_THRESHOLD_V;
        outputs.set(10, if input { 0.0 } else { GATE_HIGH_V });
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
    /// Last committed comparison state: `1` = gt, `-1` = lt, `0` = eq. Used for
    /// true stateful hysteresis so a signal dithering around B does not toggle
    /// every sample (Q041).
    state: i8,
    spec: PortSpec,
}

impl Comparator {
    /// Deadband half-width defining the equality region (volts).
    const DEADBAND_V: f64 = 0.01;
    /// Extra margin, beyond the deadband edge, the input must cross to flip
    /// state. A dither smaller than this can no longer cause chatter.
    const HYSTERESIS_V: f64 = 0.02;

    pub fn new() -> Self {
        Self {
            state: 0,
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
        let d = a - b;

        let t = Self::DEADBAND_V;
        let hy = Self::HYSTERESIS_V;

        // Stateful hysteresis: turning an output ON needs the input to cross the
        // deadband edge plus the hysteresis margin; turning it OFF happens back
        // at the deadband edge. Once committed, a dither smaller than `hy`
        // cannot flip the state, eliminating boundary chatter (Q041).
        let mut gt = self.state == 1;
        let mut lt = self.state == -1;

        if gt {
            if d < t {
                gt = false;
            }
        } else if d >= t + hy {
            gt = true;
        }

        if lt {
            if d > -t {
                lt = false;
            }
        } else if d <= -t - hy {
            lt = true;
        }

        // `gt` and `lt` are mutually exclusive: their ON conditions require
        // |d| >= t + hy of opposite sign.
        self.state = if gt {
            1
        } else if lt {
            -1
        } else {
            0
        };

        outputs.set(10, if gt { GATE_HIGH_V } else { 0.0 });
        outputs.set(11, if lt { GATE_HIGH_V } else { 0.0 });
        outputs.set(12, if self.state == 0 { GATE_HIGH_V } else { 0.0 });
    }

    fn reset(&mut self) {
        self.state = 0;
    }

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

        let select_b = cv > GATE_THRESHOLD_V;

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
    /// Latched gate A state, persisted in the struct because the engine hands
    /// `tick` a fresh output buffer each sample (Q036).
    gate_a: f64,
    /// Latched gate B state (see `gate_a`).
    gate_b: f64,
    spec: PortSpec,
}

impl BernoulliGate {
    pub fn new() -> Self {
        Self {
            last_trigger: 0.0,
            gate_a: 0.0,
            gate_b: 0.0,
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

        let rising_edge = trigger > GATE_THRESHOLD_V && self.last_trigger <= GATE_THRESHOLD_V;
        self.last_trigger = trigger;

        // Default: no trigger output
        let mut trig_a = 0.0;
        let mut trig_b = 0.0;

        if rising_edge {
            // Random decision based on probability
            let rand_val: f64 = rng::random();
            if rand_val < prob {
                trig_a = GATE_HIGH_V;
            } else {
                trig_b = GATE_HIGH_V;
            }
        }

        // Trigger outputs (momentary)
        outputs.set(10, trig_a);
        outputs.set(11, trig_b);

        // Gate outputs track which side was last triggered and latch until the
        // other side is triggered. State lives in struct fields (Q036) because
        // the output buffer is not persisted across ticks by the engine.
        if trig_a > 0.0 {
            self.gate_a = GATE_HIGH_V;
            self.gate_b = 0.0;
        } else if trig_b > 0.0 {
            self.gate_a = 0.0;
            self.gate_b = GATE_HIGH_V;
        }

        outputs.set(12, self.gate_a);
        outputs.set(13, self.gate_b);
    }

    fn reset(&mut self) {
        self.last_trigger = 0.0;
        self.gate_a = 0.0;
        self.gate_b = 0.0;
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
    /// Note captured on the current gate's rising edge, removed on its falling
    /// edge so held notes are actually released (Q040).
    captured_note: Option<f64>,
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
            captured_note: None,
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

        // Handle gate input (note capture/release).
        // A note is added on the gate's rising edge and released on its falling
        // edge (Q040), so the held set reflects the currently-held note rather
        // than growing monotonically to the 8-note cap.
        if gate > GATE_THRESHOLD_V && self.prev_gate <= GATE_THRESHOLD_V {
            // Rising edge - add note and remember it for the matching release.
            self.add_note(v_oct);
            self.captured_note = Some(v_oct);
        } else if gate <= GATE_THRESHOLD_V && self.prev_gate > GATE_THRESHOLD_V {
            // Falling edge - remove the note captured on the rising edge.
            if let Some(note) = self.captured_note.take() {
                self.remove_note(note);
            }
        }
        self.prev_gate = gate;

        // Handle reset - also clears the held-note buffer (Q040).
        if reset > GATE_THRESHOLD_V && self.prev_reset <= GATE_THRESHOLD_V {
            self.current_step = 0;
            self.direction_up = true;
            self.held_notes = [0.0; 8];
            self.num_notes = 0;
            self.captured_note = None;
        }
        self.prev_reset = reset;

        // Handle clock (advance sequence)
        let mut trigger_out = 0.0;
        let clock_rising =
            clock > GATE_THRESHOLD_V && self.prev_clock <= GATE_THRESHOLD_V && self.num_notes > 0;

        if clock_rising {
            self.gate_out = GATE_HIGH_V;
            // Start trigger pulse
            self.trigger_countdown = (Self::TRIGGER_MS * self.sample_rate / 1000.0) as usize;
            trigger_out = GATE_HIGH_V;
        }
        self.prev_clock = clock;

        // Update trigger
        if self.trigger_countdown > 0 {
            self.trigger_countdown -= 1;
            trigger_out = GATE_HIGH_V;
        }

        // Gate follows clock (simplified - stays high while clock is high)
        if clock <= GATE_THRESHOLD_V {
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
        self.captured_note = None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::common::SAFE_AUDIO_LIMIT;

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

        // Populate the held chord directly. (A single gate+pitch input can only
        // hold one note at a time now that releases remove notes — Q040 — so a
        // three-note chord is set up via add_note.)
        arp.add_note(0.0); // C4
        arp.add_note(0.333); // E4
        arp.add_note(0.583); // G4
        inputs.set(1, 0.0); // Gate low throughout

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
    #[test]
    fn test_mixer_summation_bounded() {
        // Mixer should not produce unbounded output when summing multiple channels
        let mut mixer = Mixer::new(4);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // 4 channels at full scale
        for i in 0..4 {
            inputs.set(i as u32, 5.0);
        }

        mixer.tick(&inputs, &mut outputs);
        let out = outputs.get(100).unwrap_or(0.0);

        // Note: This test documents current behavior (20V output)
        // If mixer adds limiting, update this test
        assert!(
            out.abs() <= SAFE_AUDIO_LIMIT * 2.0,
            "Mixer output {} is very high - consider adding limiting",
            out
        );
    }

    // ---- Q034: ScaleQuantizer octave-wrap ----

    #[test]
    fn test_scale_quantizer_octave_wrap_minor_11() {
        // Semitone 11 (B above the root) must snap UP to 12, not drop to 0.
        assert_eq!(
            ScaleQuantizer::quantize_to_scale(11, &ScaleQuantizer::MINOR),
            12
        );
    }

    #[test]
    fn test_scale_quantizer_monotonic_sweep() {
        // A chromatic sweep across two octaves must map to a monotonically
        // nondecreasing sequence for every scale — the old code dropped
        // top-of-octave notes ~an octave (non-monotonic).
        for scale in [
            &ScaleQuantizer::MINOR[..],
            &ScaleQuantizer::PENT_MAJOR[..],
            &ScaleQuantizer::BLUES[..],
        ] {
            let mut prev = i32::MIN;
            for note in 0..=24 {
                let q = ScaleQuantizer::quantize_to_scale(note, scale);
                assert!(
                    q >= prev,
                    "non-monotonic: note {} -> {} after {}",
                    note,
                    q,
                    prev
                );
                prev = q;
            }
        }
    }

    // ---- Q041: quantizer / comparator hysteresis ----

    #[test]
    fn test_quantizer_hysteresis_no_chatter() {
        // A slow ramp across two semitone boundaries with a tiny dither must
        // cross each boundary exactly once (two committed note changes), not
        // chatter every sample.
        let mut quant = Quantizer::new(Scale::Chromatic);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        let n = 4000;
        let mut changes = 0;
        let mut prev: Option<f64> = None;
        for i in 0..=n {
            let base = 2.0 * i as f64 / n as f64; // semitones, 0..2
            let dither = if i % 2 == 0 { 0.1 } else { -0.1 };
            inputs.set(0, (base + dither) / 12.0);
            quant.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            if let Some(p) = prev {
                if (out - p).abs() > 1e-9 {
                    changes += 1;
                }
            }
            prev = Some(out);
        }
        assert_eq!(changes, 2, "expected exactly two boundary crossings");
    }

    #[test]
    fn test_scale_quantizer_trigger_once_per_boundary() {
        // The change-trigger must fire once per committed note change, not
        // continuously while quantization is active.
        let mut sq = ScaleQuantizer::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(2, 0.0); // chromatic scale

        let n = 4000;
        let mut triggers = 0;
        for i in 0..=n {
            let base = 2.0 * i as f64 / n as f64;
            let dither = if i % 2 == 0 { 0.1 } else { -0.1 };
            inputs.set(0, (base + dither) / 12.0);
            sq.tick(&inputs, &mut outputs);
            if outputs.get(11).unwrap() > 2.5 {
                triggers += 1;
            }
        }
        assert_eq!(triggers, 2, "trigger should fire once per boundary");
    }

    #[test]
    fn test_comparator_hysteresis_no_chatter() {
        // A signal dithering around B (amplitude between the deadband and the
        // hysteresis margin) must not toggle the outputs.
        let mut cmp = Comparator::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 0.0); // B = 0

        // Establish equality first.
        inputs.set(0, 0.0);
        cmp.tick(&inputs, &mut outputs);

        let mut gt_high = 0;
        let mut lt_high = 0;
        for i in 0..200 {
            let a = if i % 2 == 0 { 0.02 } else { -0.02 };
            inputs.set(0, a);
            cmp.tick(&inputs, &mut outputs);
            if outputs.get(10).unwrap() > 2.5 {
                gt_high += 1;
            }
            if outputs.get(11).unwrap() > 2.5 {
                lt_high += 1;
            }
            assert!(outputs.get(12).unwrap() > 2.5, "should stay equal");
        }
        assert_eq!(gt_high, 0, "gt should never fire on sub-band dither");
        assert_eq!(lt_high, 0, "lt should never fire on sub-band dither");
    }

    #[test]
    fn test_comparator_still_compares() {
        // Decisive inputs still resolve correctly through the hysteresis.
        let mut cmp = Comparator::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 3.0);
        inputs.set(1, 1.0);
        cmp.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap() > 2.5);

        inputs.set(0, 1.0);
        inputs.set(1, 3.0);
        cmp.tick(&inputs, &mut outputs);
        assert!(outputs.get(11).unwrap() > 2.5);

        inputs.set(0, 2.0);
        inputs.set(1, 2.0);
        cmp.tick(&inputs, &mut outputs);
        assert!(outputs.get(12).unwrap() > 2.5);
    }

    // ---- Q035 / Q038: Clock divided outputs and default tempo ----

    #[test]
    fn test_clock_divided_outputs() {
        // Over N main cycles, div2 pulses N/2 times and div4 pulses N/4 times.
        let mut clock = Clock::new(1000.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 5.0); // medium tempo

        let (mut main_c, mut div2_c, mut div4_c) = (0, 0, 0);
        let (mut pm, mut p2, mut p4) = (0.0, 0.0, 0.0);
        for _ in 0..100_000 {
            clock.tick(&inputs, &mut outputs);
            let m = outputs.get(10).unwrap();
            let d2 = outputs.get(11).unwrap();
            let d4 = outputs.get(12).unwrap();
            let m_rise = m > 2.5 && pm <= 2.5;
            if m_rise {
                main_c += 1;
            }
            if d2 > 2.5 && p2 <= 2.5 {
                div2_c += 1;
            }
            if d4 > 2.5 && p4 <= 2.5 {
                div4_c += 1;
            }
            pm = m;
            p2 = d2;
            p4 = d4;
            if m_rise && main_c == 8 {
                break;
            }
        }
        assert_eq!(main_c, 8, "main should pulse every cycle");
        assert_eq!(div2_c, 4, "div2 should pulse at half rate");
        assert_eq!(div4_c, 2, "div4 should pulse at quarter rate");
    }

    #[test]
    fn test_clock_default_tempo_120_bpm() {
        // With no bpm input, the port default must yield ~120 BPM.
        let mut clock = Clock::default();
        let inputs = PortValues::new(); // empty -> use port default
        let mut outputs = PortValues::new();

        let mut edges = Vec::new();
        let mut prev = 0.0;
        for i in 0..100_000 {
            clock.tick(&inputs, &mut outputs);
            let m = outputs.get(10).unwrap();
            if m > 2.5 && prev <= 2.5 {
                edges.push(i);
            }
            prev = m;
            if edges.len() >= 2 {
                break;
            }
        }
        assert!(edges.len() >= 2, "expected at least two clock pulses");
        let period = (edges[1] - edges[0]) as f64;
        let bpm = 60.0 * 44100.0 / period;
        assert!(
            (bpm - 120.0).abs() < 1.0,
            "default tempo {} BPM should be ~120",
            bpm
        );
    }

    // ---- Q036: BernoulliGate latched gates ----

    #[test]
    fn test_bernoulli_gate_latches() {
        let mut bg = BernoulliGate::new();
        let mut inputs = PortValues::new();

        // Deterministic route to A: 100% probability.
        inputs.set(1, 10.0);

        // A fresh output buffer every tick (as the engine provides) — the latch
        // must survive without reading back the output buffer.
        let tick = |bg: &mut BernoulliGate, inputs: &PortValues| {
            let mut o = PortValues::new();
            bg.tick(inputs, &mut o);
            o
        };

        inputs.set(0, 0.0);
        tick(&mut bg, &inputs);
        inputs.set(0, 5.0);
        let o = tick(&mut bg, &inputs); // rising edge -> A
        assert!(o.get(12).unwrap() > 2.5, "gate_a should latch high");
        assert!(o.get(13).unwrap() < 2.5);

        // Hold across many non-trigger ticks (gate low, no rising edge).
        inputs.set(0, 0.0);
        for _ in 0..20 {
            let o = tick(&mut bg, &inputs);
            assert!(o.get(12).unwrap() > 2.5, "gate_a must stay latched");
            assert!(o.get(13).unwrap() < 2.5);
        }

        // Now route to B: 0% probability, new rising edge.
        inputs.set(1, 0.0);
        inputs.set(0, 0.0);
        tick(&mut bg, &inputs);
        inputs.set(0, 5.0);
        let o = tick(&mut bg, &inputs); // rising edge -> B
        assert!(o.get(12).unwrap() < 2.5, "gate_a should release");
        assert!(o.get(13).unwrap() > 2.5, "gate_b should latch high");
    }

    // ---- Q037 / Q042 / Q129: Euclidean ----

    #[test]
    fn test_euclidean_pulses_control_live() {
        // Changing pulses at a constant step count must change the pattern.
        let mut euc = Euclidean::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 0.5); // steps -> 9

        inputs.set(2, 0.25); // pulses -> 2
        euc.tick(&inputs, &mut outputs);
        let active_low = euc.pattern.iter().filter(|&&x| x).count();
        assert_eq!(active_low, 2);

        inputs.set(2, 0.75); // pulses -> 6
        euc.tick(&inputs, &mut outputs);
        let active_high = euc.pattern.iter().filter(|&&x| x).count();
        assert_eq!(active_high, 6);
        assert_ne!(active_low, active_high, "pulses control must be live");
    }

    #[test]
    fn test_euclidean_accent_on_rotated_pulse() {
        // With rotation, the accent must fire exactly once per cycle and always
        // coincide with an actual pulse.
        let mut euc = Euclidean::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 0.4003); // steps -> 8
        inputs.set(2, 0.5); // pulses -> 4
        inputs.set(3, 0.3); // rotation -> 2

        let mut accents = 0;
        for _ in 0..8 {
            inputs.set(0, 5.0); // clock high (rising)
            euc.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            let accent = outputs.get(11).unwrap();
            if accent > 2.5 {
                accents += 1;
                assert!(out > 2.5, "accent must coincide with a pulse");
            }
            inputs.set(0, 0.0); // clock low
            euc.tick(&inputs, &mut outputs);
        }
        assert_eq!(accents, 1, "exactly one accent per cycle");
    }

    #[test]
    fn test_euclidean_gate_threshold() {
        // Clock pulses below the canonical 2.5V threshold must be ignored;
        // 5V pulses must produce output (Q129).
        let mut euc = Euclidean::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 0.4003); // steps -> 8
        inputs.set(2, 1.0); // all pulses active

        // 1.0V clock: never crosses threshold -> no pulses.
        let mut low_pulses = 0;
        for _ in 0..8 {
            inputs.set(0, 1.0);
            euc.tick(&inputs, &mut outputs);
            if outputs.get(10).unwrap() > 2.5 {
                low_pulses += 1;
            }
            inputs.set(0, 0.0);
            euc.tick(&inputs, &mut outputs);
        }
        assert_eq!(low_pulses, 0, "1.0V clock must not trigger");

        // 5V clock: produces pulses.
        let mut high_pulses = 0;
        for _ in 0..8 {
            inputs.set(0, 5.0);
            euc.tick(&inputs, &mut outputs);
            if outputs.get(10).unwrap() > 2.5 {
                high_pulses += 1;
            }
            inputs.set(0, 0.0);
            euc.tick(&inputs, &mut outputs);
        }
        assert!(high_pulses > 0, "5V clock must trigger");
    }

    // ---- Q040: Arpeggiator note release ----

    #[test]
    fn test_arpeggiator_releases_notes() {
        let mut arp = Arpeggiator::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Press a note (rising edge).
        inputs.set(0, 0.25);
        inputs.set(1, 5.0);
        arp.tick(&inputs, &mut outputs);
        assert_eq!(arp.num_notes, 1);

        // Hold: still one note.
        for _ in 0..5 {
            arp.tick(&inputs, &mut outputs);
        }
        assert_eq!(arp.num_notes, 1);

        // Release (falling edge) removes the note.
        inputs.set(1, 0.0);
        arp.tick(&inputs, &mut outputs);
        assert_eq!(arp.num_notes, 0, "release must remove the held note");

        // Another press/release cycle keeps the count correct.
        inputs.set(0, 0.5);
        inputs.set(1, 5.0);
        arp.tick(&inputs, &mut outputs);
        assert_eq!(arp.num_notes, 1);
        inputs.set(1, 0.0);
        arp.tick(&inputs, &mut outputs);
        assert_eq!(arp.num_notes, 0);
    }

    #[test]
    fn test_arpeggiator_reset_clears_held_notes() {
        let mut arp = Arpeggiator::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Hold a note (gate stays high).
        inputs.set(0, 0.0);
        inputs.set(1, 5.0);
        arp.tick(&inputs, &mut outputs);
        assert_eq!(arp.num_notes, 1);

        // Reset input (rising edge) empties the held set.
        inputs.set(5, 5.0);
        arp.tick(&inputs, &mut outputs);
        assert_eq!(arp.num_notes, 0, "reset must clear held notes");
    }

    // ================================================================
    // Q146: microtuning / custom scales on ScaleQuantizer
    // ================================================================

    #[cfg(feature = "alloc")]
    #[test]
    fn test_custom_scale_snaps_to_degrees() {
        // Whole-tone scale: degrees every 200 cents.
        let mut sq = ScaleQuantizer::new(44100.0);
        assert!(!sq.has_custom_scale());
        sq.set_custom_scale(&[0.0, 200.0, 400.0, 600.0, 800.0, 1000.0]);
        assert!(sq.has_custom_scale());

        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Input just above the 200-cent degree (200 cents = 1/6 V) should snap to
        // it, not to a chromatic semitone.
        // 210 cents = 0.175 V.
        inputs.set(0, 0.175);
        // Run a few ticks so hysteresis commits.
        for _ in 0..4 {
            sq.tick(&inputs, &mut outputs);
        }
        let out_v = outputs.get(10).unwrap();
        // Expect ~200 cents = 0.16667 V.
        assert!(
            (out_v - 200.0 / 1200.0).abs() < 1e-6,
            "custom scale should snap to 200 cents, got {} cents",
            out_v * 1200.0
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn test_load_scala_and_quantize() {
        let scl = "\
whole tone
6
200.0
400.0
600.0
800.0
1000.0
1200.0
";
        let mut sq = ScaleQuantizer::new(44100.0);
        sq.load_scala(scl).unwrap();
        assert!(sq.has_custom_scale());

        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        // 390 cents -> nearest whole-tone degree is 400 cents.
        inputs.set(0, 390.0 / 1200.0);
        for _ in 0..4 {
            sq.tick(&inputs, &mut outputs);
        }
        let out_v = outputs.get(10).unwrap();
        assert!(
            (out_v - 400.0 / 1200.0).abs() < 1e-6,
            "expected 400 cents, got {} cents",
            out_v * 1200.0
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn test_clear_custom_scale_restores_enum() {
        let mut sq = ScaleQuantizer::new(44100.0);
        sq.set_custom_scale(&[0.0, 200.0, 400.0]);
        assert!(sq.has_custom_scale());
        sq.clear_custom_scale();
        assert!(!sq.has_custom_scale());
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn test_load_scala_malformed_leaves_scale_unchanged() {
        let mut sq = ScaleQuantizer::new(44100.0);
        sq.set_custom_scale(&[0.0, 500.0]);
        // Malformed: declares 3 pitches but supplies one.
        let err = sq.load_scala("bad\n3\n100.0\n");
        assert!(err.is_err());
        // Previous custom scale is retained.
        assert!(sq.has_custom_scale());
    }

    // ---- Q161: Quantizer with negative V/Oct (notes below C4) ----

    #[test]
    fn test_quantizer_negative_voct_chromatic() {
        // A fresh quantizer per input avoids boundary hysteresis carrying over.
        let cases = [
            (-0.5, -0.5),                 // exactly F#3 -> stays
            (-1.0, -1.0),                 // C3 -> octave floor, stays
            (-13.0 / 12.0, -13.0 / 12.0), // B2 -> negative octave, chromatic passthrough
            (-1.0 / 24.0, 0.0),           // half a semitone below C4 -> wraps UP to C4
        ];
        for (input, expected) in cases {
            let mut q = Quantizer::new(Scale::Chromatic);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(0, input);
            q.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            assert!(
                (out - expected).abs() < 1e-9,
                "chromatic {input}V -> {out}V, expected {expected}V"
            );
        }
    }

    #[test]
    fn test_quantizer_negative_voct_major_scale() {
        // -0.5V is F#3; in a major scale it snaps to the nearest degree, F3
        // (-7/12 V), proving the negative-octave scale-wrap path works.
        let mut q = Quantizer::new(Scale::Major);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, -0.5);
        q.tick(&inputs, &mut outputs);
        let out = outputs.get(10).unwrap();
        assert!(
            (out - (-7.0 / 12.0)).abs() < 1e-9,
            "major -0.5V should snap to F3 (-7/12 V), got {out}V"
        );

        // -1.0V is exactly C3, a scale degree, so it is preserved.
        let mut q2 = Quantizer::new(Scale::Major);
        inputs.set(0, -1.0);
        q2.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - (-1.0)).abs() < 1e-9);
    }

    #[test]
    fn test_scale_quantizer_negative_voct() {
        // Chromatic passthrough below C4 with the div_euclid/rem_euclid path.
        let cases = [(-1.0, -1.0), (-13.0 / 12.0, -13.0 / 12.0)];
        for (input, expected) in cases {
            let mut sq = ScaleQuantizer::new(44100.0);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(0, input);
            inputs.set(2, 0.0); // chromatic
            sq.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            assert!(
                (out - expected).abs() < 1e-9,
                "scale-quantizer chromatic {input}V -> {out}V, expected {expected}V"
            );
        }

        // Minor scale, -0.5V (F#3) snaps to F3 (-7/12 V) below C4.
        let mut sq = ScaleQuantizer::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, -0.5);
        inputs.set(2, 0.3); // scale index 2 == minor
        sq.tick(&inputs, &mut outputs);
        let out = outputs.get(10).unwrap();
        assert!(
            (out - (-7.0 / 12.0)).abs() < 1e-9,
            "minor -0.5V should snap to F3 (-7/12 V), got {out}V"
        );
    }

    // ---- Q157: Euclidean + ScaleQuantizer reset / sample-rate ----

    #[test]
    fn test_euclidean_reset_and_sample_rate() {
        let mut euc = Euclidean::new(44100.0);
        assert_eq!(euc.type_id(), "euclidean");
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        // Advance the sequencer a few clock pulses so `step` moves off zero.
        for _ in 0..3 {
            inputs.set(0, 5.0);
            euc.tick(&inputs, &mut outputs);
            inputs.set(0, 0.0);
            euc.tick(&inputs, &mut outputs);
        }
        assert!(euc.step != 0, "clock pulses should advance the step");
        euc.reset();
        assert_eq!(euc.step, 0);
        assert!(!euc.cycle_accented);
        // set_sample_rate is a no-op but must not panic and keep it usable.
        euc.set_sample_rate(48000.0);
        inputs.set(0, 5.0);
        euc.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap().is_finite());
    }

    #[test]
    fn test_scale_quantizer_reset_and_sample_rate() {
        let mut sq = ScaleQuantizer::new(44100.0);
        assert_eq!(sq.type_id(), "scale_quantizer");
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.25); // some note
        sq.tick(&inputs, &mut outputs);
        assert!(sq.last_output.is_some());
        sq.reset();
        assert!(sq.last_output.is_none());
        sq.set_sample_rate(48000.0);
        sq.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap().is_finite());
    }
}
