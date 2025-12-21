//! Module Introspection Implementations (Integration Layer)
//!
//! This module provides `ModuleIntrospection` implementations for all built-in modules,
//! enabling GUIs to discover and control module parameters automatically.
//!
//! Each implementation exposes the module's controllable parameters with appropriate:
//! - Value ranges (min/max)
//! - Scaling curves (linear, exponential, stepped)
//! - Control types (knob, slider, toggle, select)
//! - Value formatting (frequency, time, dB, percent, etc.)

use alloc::vec;
use alloc::vec::Vec;

use crate::introspection::{ControlType, ModuleIntrospection, ParamCurve, ParamInfo, ValueFormat};

// Import all modules
use crate::modules::{
    Adsr, Attenuverter, BernoulliGate, Clock, Comparator, Crossfader, Crosstalk,
    DiodeLadderFilter, GroundLoop, Lfo, LogicAnd, LogicNot, LogicOr, LogicXor, Max, Min, Mixer,
    Multiple, NoiseGenerator, Offset, PrecisionAdder, Quantizer, Rectifier, RingModulator,
    SampleAndHold, Scale, SlewLimiter, StepSequencer, StereoOutput, Svf, UnitDelay, Vca,
    VcSwitch, Vco,
};

// Import analog modules
use crate::analog::{AnalogVco, Saturator, Wavefolder};

// =============================================================================
// Oscillators
// =============================================================================

impl ModuleIntrospection for Vco {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // VCO has no internal parameters - all control is via CV inputs
        // The V/Oct, FM, PW, and Sync inputs define its behavior
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for Lfo {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // LFO has no internal parameters - rate and depth are CV-controlled
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

// =============================================================================
// Filters
// =============================================================================

impl ModuleIntrospection for Svf {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // SVF has no internal parameters - cutoff, resonance, FM, and keytrack are CV-controlled
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for DiodeLadderFilter {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Diode ladder has no internal parameters - all control via CV inputs
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

// =============================================================================
// Envelopes
// =============================================================================

impl ModuleIntrospection for Adsr {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // ADSR has no internal parameters - A/D/S/R times are CV-controlled via inputs
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

// =============================================================================
// Amplifiers
// =============================================================================

impl ModuleIntrospection for Vca {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // VCA has no internal parameters - gain is CV-controlled
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

// =============================================================================
// Utilities
// =============================================================================

impl ModuleIntrospection for Mixer {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Mixer channels have attenuverters on inputs (CV-controlled)
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for Offset {
    fn param_infos(&self) -> Vec<ParamInfo> {
        vec![ParamInfo::new("offset", "Offset")
            .with_range(-10.0, 10.0)
            .with_default(0.0)
            .with_value(self.offset)
            .with_curve(ParamCurve::Linear)
            .with_control(ControlType::Knob)
            .with_unit("V")
            .with_format(ValueFormat::Decimal { places: 2 })]
    }

    fn set_param_by_id(&mut self, id: &str, value: f64) -> bool {
        match id {
            "offset" => {
                self.set_offset(value);
                true
            }
            _ => false,
        }
    }
}

impl ModuleIntrospection for UnitDelay {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Unit delay has no adjustable parameters - it's always one sample
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for Attenuverter {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Attenuverter level is CV-controlled via input
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for Multiple {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Multiple/Mult has no parameters - just copies signal
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for SlewLimiter {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Slew limiter rise/fall times are CV-controlled
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for SampleAndHold {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Sample and hold has no adjustable parameters
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for PrecisionAdder {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Precision adder has no adjustable parameters
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for VcSwitch {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // VC Switch is controlled entirely by CV input
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for Min {
    fn param_infos(&self) -> Vec<ParamInfo> {
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for Max {
    fn param_infos(&self) -> Vec<ParamInfo> {
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

// =============================================================================
// Sources
// =============================================================================

impl ModuleIntrospection for NoiseGenerator {
    fn param_infos(&self) -> Vec<ParamInfo> {
        vec![ParamInfo::percent("correlation", "Stereo Correlation")
            .with_default(0.3)
            .with_value(self.correlation)]
    }

    fn set_param_by_id(&mut self, id: &str, value: f64) -> bool {
        match id {
            "correlation" => {
                self.correlation = value.clamp(0.0, 1.0);
                true
            }
            _ => false,
        }
    }
}

// =============================================================================
// Sequencing
// =============================================================================

impl ModuleIntrospection for StepSequencer {
    fn param_infos(&self) -> Vec<ParamInfo> {
        let mut params = Vec::with_capacity(16);

        // 8 step CV values
        for i in 0..8 {
            if let Some((voltage, _gate)) = self.get_step(i) {
                params.push(
                    ParamInfo::new(
                        alloc::format!("step_{}_cv", i),
                        alloc::format!("Step {} CV", i + 1),
                    )
                    .with_range(-5.0, 5.0)
                    .with_default(0.0)
                    .with_value(voltage)
                    .with_curve(ParamCurve::Linear)
                    .with_control(ControlType::Slider)
                    .with_unit("V")
                    .with_format(ValueFormat::NoteName),
                );
            }
        }

        // 8 step gate toggles
        for i in 0..8 {
            if let Some((_voltage, gate)) = self.get_step(i) {
                params.push(
                    ParamInfo::toggle(
                        alloc::format!("step_{}_gate", i),
                        alloc::format!("Step {} Gate", i + 1),
                    )
                    .with_value(if gate { 1.0 } else { 0.0 }),
                );
            }
        }

        params
    }

    fn set_param_by_id(&mut self, id: &str, value: f64) -> bool {
        // Parse step_N_cv or step_N_gate format
        if let Some(rest) = id.strip_prefix("step_") {
            if let Some((num_str, param_type)) = rest.split_once('_') {
                if let Ok(step_idx) = num_str.parse::<usize>() {
                    if step_idx < 8 {
                        if let Some((current_cv, current_gate)) = self.get_step(step_idx) {
                            match param_type {
                                "cv" => {
                                    self.set_step(step_idx, value, current_gate);
                                    return true;
                                }
                                "gate" => {
                                    self.set_step(step_idx, current_cv, value > 0.5);
                                    return true;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        false
    }
}

impl ModuleIntrospection for Clock {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Clock BPM is CV-controlled via input
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

// =============================================================================
// Effects
// =============================================================================

impl ModuleIntrospection for RingModulator {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Ring mod has no internal parameters
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for Crossfader {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Crossfader position is CV-controlled
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for Rectifier {
    fn param_infos(&self) -> Vec<ParamInfo> {
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

// =============================================================================
// Analog Modeling
// =============================================================================

impl ModuleIntrospection for Crosstalk {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Crosstalk amount and HF emphasis are CV-controlled
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for GroundLoop {
    fn param_infos(&self) -> Vec<ParamInfo> {
        vec![ParamInfo::select("frequency", "Mains Frequency", 2)
            .with_default(1.0) // Default to 60Hz
            .with_value(if self.frequency == 60.0 { 1.0 } else { 0.0 })]
    }

    fn set_param_by_id(&mut self, id: &str, value: f64) -> bool {
        match id {
            "frequency" => {
                self.frequency = if value > 0.5 { 60.0 } else { 50.0 };
                true
            }
            _ => false,
        }
    }
}

impl ModuleIntrospection for AnalogVco {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // AnalogVco has analog imperfections modeled but no user-adjustable params
        // The imperfections are per-instance variations
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for Saturator {
    fn param_infos(&self) -> Vec<ParamInfo> {
        vec![ParamInfo::new("drive", "Drive")
            .with_range(1.0, 10.0)
            .with_default(1.0)
            .with_value(self.drive)
            .with_curve(ParamCurve::Exponential)
            .with_control(ControlType::Knob)
            .with_format(ValueFormat::Ratio)]
    }

    fn set_param_by_id(&mut self, id: &str, value: f64) -> bool {
        match id {
            "drive" => {
                self.drive = value.clamp(1.0, 10.0);
                true
            }
            _ => false,
        }
    }
}

impl ModuleIntrospection for Wavefolder {
    fn param_infos(&self) -> Vec<ParamInfo> {
        vec![ParamInfo::new("threshold", "Fold Threshold")
            .with_range(0.1, 5.0)
            .with_default(1.0)
            .with_value(self.threshold)
            .with_curve(ParamCurve::Exponential)
            .with_control(ControlType::Knob)
            .with_unit("V")
            .with_format(ValueFormat::Decimal { places: 2 })]
    }

    fn set_param_by_id(&mut self, id: &str, value: f64) -> bool {
        match id {
            "threshold" => {
                self.threshold = value.clamp(0.1, 5.0);
                true
            }
            _ => false,
        }
    }
}

// =============================================================================
// Logic Modules
// =============================================================================

impl ModuleIntrospection for LogicAnd {
    fn param_infos(&self) -> Vec<ParamInfo> {
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for LogicOr {
    fn param_infos(&self) -> Vec<ParamInfo> {
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for LogicXor {
    fn param_infos(&self) -> Vec<ParamInfo> {
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for LogicNot {
    fn param_infos(&self) -> Vec<ParamInfo> {
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for Comparator {
    fn param_infos(&self) -> Vec<ParamInfo> {
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

// =============================================================================
// Random
// =============================================================================

impl ModuleIntrospection for BernoulliGate {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Probability is CV-controlled via input
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

// =============================================================================
// I/O Modules
// =============================================================================

impl ModuleIntrospection for StereoOutput {
    fn param_infos(&self) -> Vec<ParamInfo> {
        Vec::new()
    }

    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

impl ModuleIntrospection for Quantizer {
    fn param_infos(&self) -> Vec<ParamInfo> {
        // Map scale enum to numeric value for UI
        let scale_value = match self.scale {
            Scale::Chromatic => 0.0,
            Scale::Major => 1.0,
            Scale::Minor => 2.0,
            Scale::PentatonicMajor => 3.0,
            Scale::PentatonicMinor => 4.0,
            Scale::Dorian => 5.0,
            Scale::Mixolydian => 6.0,
            Scale::Blues => 7.0,
        };

        vec![ParamInfo::select("scale", "Scale", 8).with_value(scale_value)]
    }

    fn set_param_by_id(&mut self, id: &str, value: f64) -> bool {
        match id {
            "scale" => {
                let scale = match value as u8 {
                    0 => Scale::Chromatic,
                    1 => Scale::Major,
                    2 => Scale::Minor,
                    3 => Scale::PentatonicMajor,
                    4 => Scale::PentatonicMinor,
                    5 => Scale::Dorian,
                    6 => Scale::Mixolydian,
                    7 => Scale::Blues,
                    _ => return false,
                };
                self.set_scale(scale);
                true
            }
            _ => false,
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_introspection() {
        let mut offset = Offset::new(2.5);

        let params = offset.param_infos();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].id, "offset");
        assert_eq!(params[0].value, 2.5);

        // Test setting parameter
        assert!(offset.set_param_by_id("offset", -3.0));
        let params = offset.param_infos();
        assert_eq!(params[0].value, -3.0);

        // Test invalid parameter
        assert!(!offset.set_param_by_id("invalid", 0.0));
    }

    #[test]
    fn test_step_sequencer_introspection() {
        let mut seq = StepSequencer::new();
        seq.set_step(0, 1.0, true);
        seq.set_step(1, -0.5, false);

        let params = seq.param_infos();
        assert_eq!(params.len(), 16); // 8 CV + 8 gate params

        // Find step 0 CV
        let step0_cv = params.iter().find(|p| p.id == "step_0_cv").unwrap();
        assert_eq!(step0_cv.value, 1.0);

        // Find step 0 gate
        let step0_gate = params.iter().find(|p| p.id == "step_0_gate").unwrap();
        assert_eq!(step0_gate.value, 1.0);

        // Find step 1 gate (should be off)
        let step1_gate = params.iter().find(|p| p.id == "step_1_gate").unwrap();
        assert_eq!(step1_gate.value, 0.0);

        // Test setting step CV
        assert!(seq.set_param_by_id("step_2_cv", 2.5));
        if let Some((cv, _)) = seq.get_step(2) {
            assert_eq!(cv, 2.5);
        }

        // Test setting step gate
        assert!(seq.set_param_by_id("step_2_gate", 0.0));
        if let Some((_, gate)) = seq.get_step(2) {
            assert!(!gate);
        }
    }

    #[test]
    fn test_quantizer_introspection() {
        let mut quant = Quantizer::major();

        let params = quant.param_infos();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].id, "scale");
        assert_eq!(params[0].value, 1.0); // Major = 1

        // Change to minor
        assert!(quant.set_param_by_id("scale", 2.0));
        let params = quant.param_infos();
        assert_eq!(params[0].value, 2.0);
    }

    #[test]
    fn test_noise_generator_introspection() {
        let mut noise = NoiseGenerator::with_correlation(0.5);

        let params = noise.param_infos();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].id, "correlation");
        assert!((params[0].value - 0.5).abs() < 0.001);

        // Change correlation
        assert!(noise.set_param_by_id("correlation", 0.8));
        let params = noise.param_infos();
        assert!((params[0].value - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_saturator_introspection() {
        let mut sat = Saturator::new(1.0);

        let params = sat.param_infos();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].id, "drive");
        assert_eq!(params[0].min, 1.0);
        assert_eq!(params[0].max, 10.0);

        // Set drive
        assert!(sat.set_param_by_id("drive", 5.0));
        let params = sat.param_infos();
        assert_eq!(params[0].value, 5.0);
    }

    #[test]
    fn test_wavefolder_introspection() {
        let mut wf = Wavefolder::new(1.0);

        let params = wf.param_infos();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].id, "threshold");

        // Set threshold
        assert!(wf.set_param_by_id("threshold", 2.0));
        let params = wf.param_infos();
        assert_eq!(params[0].value, 2.0);
    }

    #[test]
    fn test_ground_loop_introspection() {
        let mut gl = GroundLoop::hz_50(44100.0);

        let params = gl.param_infos();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].id, "frequency");
        assert_eq!(params[0].value, 0.0); // 50Hz = 0

        // Switch to 60Hz
        assert!(gl.set_param_by_id("frequency", 1.0));
        let params = gl.param_infos();
        assert_eq!(params[0].value, 1.0);
    }

    #[test]
    fn test_cv_controlled_modules_have_no_params() {
        // Modules that are entirely CV-controlled should return empty params
        assert!(Vco::default().param_infos().is_empty());
        assert!(Lfo::default().param_infos().is_empty());
        assert!(Svf::default().param_infos().is_empty());
        assert!(Adsr::default().param_infos().is_empty());
        assert!(Vca::default().param_infos().is_empty());
        assert!(Clock::default().param_infos().is_empty());
        assert!(LogicAnd::default().param_infos().is_empty());
    }
}
