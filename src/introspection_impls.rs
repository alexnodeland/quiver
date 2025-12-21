//! Module Introspection Implementations (Integration Layer)
//!
//! This module provides `ModuleIntrospection` implementations for all built-in modules,
//! enabling GUIs to discover and control module parameters automatically.
//!
//! Most modules are fully CV-controlled and use the default empty implementation.
//! Only modules with internal state parameters provide custom implementations.

use alloc::vec;
use alloc::vec::Vec;

use crate::introspection::{ControlType, ModuleIntrospection, ParamCurve, ParamInfo, ValueFormat};

use crate::analog::{AnalogVco, Saturator, Wavefolder};
use crate::modules::{
    Adsr, Attenuverter, BernoulliGate, Clock, Comparator, Crossfader, Crosstalk, DiodeLadderFilter,
    GroundLoop, Lfo, LogicAnd, LogicNot, LogicOr, LogicXor, Max, Min, Mixer, Multiple,
    NoiseGenerator, Offset, PrecisionAdder, Quantizer, Rectifier, RingModulator, SampleAndHold,
    Scale, SlewLimiter, StepSequencer, StereoOutput, Svf, UnitDelay, VcSwitch, Vca, Vco,
};

// =============================================================================
// CV-Controlled Modules (use default empty implementation)
// =============================================================================

// Oscillators
impl ModuleIntrospection for Vco {}
impl ModuleIntrospection for Lfo {}
impl ModuleIntrospection for AnalogVco {}

// Filters
impl ModuleIntrospection for Svf {}
impl ModuleIntrospection for DiodeLadderFilter {}

// Envelopes & Amplifiers
impl ModuleIntrospection for Adsr {}
impl ModuleIntrospection for Vca {}

// Utilities (CV-controlled)
impl ModuleIntrospection for Mixer {}
impl ModuleIntrospection for UnitDelay {}
impl ModuleIntrospection for Attenuverter {}
impl ModuleIntrospection for Multiple {}
impl ModuleIntrospection for SlewLimiter {}
impl ModuleIntrospection for SampleAndHold {}
impl ModuleIntrospection for PrecisionAdder {}
impl ModuleIntrospection for VcSwitch {}
impl ModuleIntrospection for Min {}
impl ModuleIntrospection for Max {}
impl ModuleIntrospection for Crossfader {}

// Effects (CV-controlled)
impl ModuleIntrospection for RingModulator {}
impl ModuleIntrospection for Rectifier {}
impl ModuleIntrospection for Crosstalk {}

// Logic & Random
impl ModuleIntrospection for LogicAnd {}
impl ModuleIntrospection for LogicOr {}
impl ModuleIntrospection for LogicXor {}
impl ModuleIntrospection for LogicNot {}
impl ModuleIntrospection for Comparator {}
impl ModuleIntrospection for BernoulliGate {}

// Sequencing & I/O
impl ModuleIntrospection for Clock {}
impl ModuleIntrospection for StereoOutput {}

// =============================================================================
// Modules with Parameters
// =============================================================================

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

impl ModuleIntrospection for StepSequencer {
    fn param_infos(&self) -> Vec<ParamInfo> {
        let mut params = Vec::with_capacity(16);

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

impl ModuleIntrospection for Quantizer {
    fn param_infos(&self) -> Vec<ParamInfo> {
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

impl ModuleIntrospection for GroundLoop {
    fn param_infos(&self) -> Vec<ParamInfo> {
        vec![ParamInfo::select("frequency", "Mains Frequency", 2)
            .with_default(1.0)
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

        assert!(offset.set_param_by_id("offset", -3.0));
        assert_eq!(offset.param_infos()[0].value, -3.0);
        assert!(!offset.set_param_by_id("invalid", 0.0));
    }

    #[test]
    fn test_step_sequencer_introspection() {
        let mut seq = StepSequencer::new();
        seq.set_step(0, 1.0, true);
        seq.set_step(1, -0.5, false);

        let params = seq.param_infos();
        assert_eq!(params.len(), 16);

        let step0_cv = params.iter().find(|p| p.id == "step_0_cv").unwrap();
        assert_eq!(step0_cv.value, 1.0);

        let step0_gate = params.iter().find(|p| p.id == "step_0_gate").unwrap();
        assert_eq!(step0_gate.value, 1.0);

        assert!(seq.set_param_by_id("step_2_cv", 2.5));
        assert_eq!(seq.get_step(2).unwrap().0, 2.5);
    }

    #[test]
    fn test_quantizer_introspection() {
        let mut quant = Quantizer::major();
        assert_eq!(quant.param_infos()[0].value, 1.0);

        assert!(quant.set_param_by_id("scale", 2.0));
        assert_eq!(quant.param_infos()[0].value, 2.0);
    }

    #[test]
    fn test_noise_generator_introspection() {
        let mut noise = NoiseGenerator::with_correlation(0.5);
        assert!((noise.param_infos()[0].value - 0.5).abs() < 0.001);

        assert!(noise.set_param_by_id("correlation", 0.8));
        assert!((noise.param_infos()[0].value - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_saturator_introspection() {
        let mut sat = Saturator::new(1.0);
        assert_eq!(sat.param_infos()[0].id, "drive");

        assert!(sat.set_param_by_id("drive", 5.0));
        assert_eq!(sat.param_infos()[0].value, 5.0);
    }

    #[test]
    fn test_wavefolder_introspection() {
        let mut wf = Wavefolder::new(1.0);
        assert_eq!(wf.param_infos()[0].id, "threshold");

        assert!(wf.set_param_by_id("threshold", 2.0));
        assert_eq!(wf.param_infos()[0].value, 2.0);
    }

    #[test]
    fn test_ground_loop_introspection() {
        let mut gl = GroundLoop::hz_50(44100.0);
        assert_eq!(gl.param_infos()[0].value, 0.0);

        assert!(gl.set_param_by_id("frequency", 1.0));
        assert_eq!(gl.param_infos()[0].value, 1.0);
    }

    #[test]
    fn test_cv_controlled_modules_have_no_params() {
        assert!(Vco::default().param_infos().is_empty());
        assert!(Lfo::default().param_infos().is_empty());
        assert!(Svf::default().param_infos().is_empty());
        assert!(Adsr::default().param_infos().is_empty());
        assert!(Vca::default().param_infos().is_empty());
        assert!(Clock::default().param_infos().is_empty());
        assert!(LogicAnd::default().param_infos().is_empty());
    }
}
