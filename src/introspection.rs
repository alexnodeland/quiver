//! GUI Introspection API (Phase 1)
//!
//! This module provides types and traits for exposing module metadata to UIs,
//! enabling automatic generation of appropriate controls (knobs, sliders, etc.).

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::port::GraphModule;

// =============================================================================
// Parameter Value Formatting
// =============================================================================

/// How to format parameter values for display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ValueFormat {
    /// Decimal number with specified precision
    Decimal { places: u8 },
    /// Frequency in Hz/kHz
    Frequency,
    /// Time in ms/s
    Time,
    /// Level in decibels
    Decibels,
    /// Percentage (0-100%)
    Percent,
    /// Musical note name (C4, D#5, etc.)
    NoteName,
    /// Ratio (1:2, 3:1, etc.)
    Ratio,
}

impl Default for ValueFormat {
    fn default() -> Self {
        ValueFormat::Decimal { places: 2 }
    }
}

impl ValueFormat {
    /// Format a value according to this format specification
    pub fn format(&self, value: f64) -> String {
        match self {
            ValueFormat::Decimal { places } => {
                format!("{:.prec$}", value, prec = *places as usize)
            }
            ValueFormat::Frequency => {
                if value >= 1000.0 {
                    format!("{:.2} kHz", value / 1000.0)
                } else {
                    format!("{:.1} Hz", value)
                }
            }
            ValueFormat::Time => {
                if value >= 1.0 {
                    format!("{:.2} s", value)
                } else {
                    format!("{:.1} ms", value * 1000.0)
                }
            }
            ValueFormat::Decibels => {
                format!("{:.1} dB", value)
            }
            ValueFormat::Percent => {
                format!("{:.0}%", value * 100.0)
            }
            ValueFormat::NoteName => {
                // Convert voltage to MIDI note number (0V = C4 = 60)
                let midi_note = ((value * 12.0) + 60.0).round() as i32;
                let note_names = [
                    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                ];
                let note = note_names[(midi_note.rem_euclid(12)) as usize];
                let octave = (midi_note / 12) - 1;
                format!("{}{}", note, octave)
            }
            ValueFormat::Ratio => {
                if value >= 1.0 {
                    format!("{:.1}:1", value)
                } else if value > 0.0 {
                    format!("1:{:.1}", 1.0 / value)
                } else {
                    "0:1".into()
                }
            }
        }
    }
}

// =============================================================================
// Parameter Curve (Value Scaling)
// =============================================================================

/// How parameter values are scaled between min and max
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ParamCurve {
    /// Linear interpolation between min and max
    #[default]
    Linear,
    /// Exponential scaling (good for frequency, time)
    Exponential,
    /// Logarithmic scaling (good for dB, perception)
    Logarithmic,
    /// Discrete steps
    Stepped { steps: u32 },
}

impl ParamCurve {
    /// Apply the curve to a normalized (0-1) value, returning the actual value
    pub fn apply(&self, normalized: f64, min: f64, max: f64) -> f64 {
        let n = normalized.clamp(0.0, 1.0);
        match self {
            ParamCurve::Linear => min + n * (max - min),
            ParamCurve::Exponential => {
                if min <= 0.0 {
                    n * max
                } else {
                    min * libm::Libm::<f64>::pow(max / min, n)
                }
            }
            ParamCurve::Logarithmic => {
                // Inverse of exponential
                let log_min = if min > 0.0 {
                    libm::Libm::<f64>::log10(min)
                } else {
                    0.0
                };
                let log_max = libm::Libm::<f64>::log10(max.max(0.001));
                libm::Libm::<f64>::pow(10.0, log_min + n * (log_max - log_min))
            }
            ParamCurve::Stepped { steps } => {
                let step_size = (max - min) / (*steps as f64);
                let step_index = (n * (*steps as f64)).floor() as u32;
                min + (step_index.min(*steps - 1) as f64) * step_size
            }
        }
    }

    /// Convert an actual value to normalized (0-1) based on this curve
    pub fn normalize(&self, value: f64, min: f64, max: f64) -> f64 {
        if (max - min).abs() < 1e-10 {
            return 0.0;
        }

        match self {
            ParamCurve::Linear => ((value - min) / (max - min)).clamp(0.0, 1.0),
            ParamCurve::Exponential => {
                if min <= 0.0 || value <= 0.0 {
                    ((value - min) / (max - min)).clamp(0.0, 1.0)
                } else {
                    let log_ratio =
                        libm::Libm::<f64>::log(value / min) / libm::Libm::<f64>::log(max / min);
                    log_ratio.clamp(0.0, 1.0)
                }
            }
            ParamCurve::Logarithmic => {
                let log_min = if min > 0.0 {
                    libm::Libm::<f64>::log10(min)
                } else {
                    0.0
                };
                let log_max = libm::Libm::<f64>::log10(max.max(0.001));
                let log_val = libm::Libm::<f64>::log10(value.max(0.001));
                ((log_val - log_min) / (log_max - log_min)).clamp(0.0, 1.0)
            }
            ParamCurve::Stepped { steps } => {
                let step_size = (max - min) / (*steps as f64);
                let step_index = ((value - min) / step_size).round() as u32;
                (step_index as f64 / *steps as f64).clamp(0.0, 1.0)
            }
        }
    }
}

// =============================================================================
// Control Type
// =============================================================================

/// Suggested UI control type for a parameter
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum ControlType {
    /// Rotary knob (most common for synth parameters)
    #[default]
    Knob,
    /// Linear slider (vertical or horizontal)
    Slider,
    /// On/off toggle switch
    Toggle,
    /// Dropdown or segmented selector for discrete options
    Select,
}

// =============================================================================
// Parameter Information
// =============================================================================

/// Complete parameter descriptor for UI generation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct ParamInfo {
    /// Unique identifier within module (e.g., "frequency", "resonance")
    pub id: String,
    /// Display name (e.g., "Frequency", "Resonance")
    pub name: String,
    /// Current value
    pub value: f64,
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// Default value
    pub default: f64,
    /// Value scaling curve
    pub curve: ParamCurve,
    /// Suggested control type
    pub control: ControlType,
    /// Unit for display (Hz, ms, dB, %, etc.)
    pub unit: Option<String>,
    /// Value formatting hint
    pub format: ValueFormat,
}

impl ParamInfo {
    /// Create a new parameter info with sensible defaults
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            value: 0.5,
            min: 0.0,
            max: 1.0,
            default: 0.5,
            curve: ParamCurve::Linear,
            control: ControlType::Knob,
            unit: None,
            format: ValueFormat::default(),
        }
    }

    /// Set the value range
    pub fn with_range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// Set the default value
    pub fn with_default(mut self, default: f64) -> Self {
        self.default = default;
        self.value = default;
        self
    }

    /// Set the current value
    pub fn with_value(mut self, value: f64) -> Self {
        self.value = value;
        self
    }

    /// Set the curve type
    pub fn with_curve(mut self, curve: ParamCurve) -> Self {
        self.curve = curve;
        self
    }

    /// Set the control type
    pub fn with_control(mut self, control: ControlType) -> Self {
        self.control = control;
        self
    }

    /// Set the unit string
    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    /// Set the value format
    pub fn with_format(mut self, format: ValueFormat) -> Self {
        self.format = format;
        self
    }

    /// Create a frequency parameter (20Hz - 20kHz, exponential)
    pub fn frequency(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self::new(id, name)
            .with_range(20.0, 20000.0)
            .with_default(1000.0)
            .with_curve(ParamCurve::Exponential)
            .with_unit("Hz")
            .with_format(ValueFormat::Frequency)
    }

    /// Create a time parameter (1ms - 10s, exponential)
    pub fn time(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self::new(id, name)
            .with_range(0.001, 10.0)
            .with_default(0.1)
            .with_curve(ParamCurve::Exponential)
            .with_unit("s")
            .with_format(ValueFormat::Time)
    }

    /// Create a level/gain parameter (-60dB to +12dB)
    pub fn decibels(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self::new(id, name)
            .with_range(-60.0, 12.0)
            .with_default(0.0)
            .with_curve(ParamCurve::Linear)
            .with_unit("dB")
            .with_format(ValueFormat::Decibels)
    }

    /// Create a percentage parameter (0-100%)
    pub fn percent(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self::new(id, name)
            .with_range(0.0, 1.0)
            .with_default(0.5)
            .with_curve(ParamCurve::Linear)
            .with_format(ValueFormat::Percent)
    }

    /// Create a toggle parameter (0 or 1)
    pub fn toggle(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self::new(id, name)
            .with_range(0.0, 1.0)
            .with_default(0.0)
            .with_curve(ParamCurve::Stepped { steps: 2 })
            .with_control(ControlType::Toggle)
    }

    /// Create a selector parameter with N options
    pub fn select(id: impl Into<String>, name: impl Into<String>, options: u32) -> Self {
        Self::new(id, name)
            .with_range(0.0, (options - 1) as f64)
            .with_default(0.0)
            .with_curve(ParamCurve::Stepped { steps: options })
            .with_control(ControlType::Select)
            .with_format(ValueFormat::Decimal { places: 0 })
    }

    /// Get the normalized (0-1) value
    pub fn normalized(&self) -> f64 {
        self.curve.normalize(self.value, self.min, self.max)
    }

    /// Set value from normalized (0-1) input
    pub fn set_normalized(&mut self, normalized: f64) {
        self.value = self.curve.apply(normalized, self.min, self.max);
    }

    /// Format the current value for display
    pub fn format_value(&self) -> String {
        self.format.format(self.value)
    }
}

// =============================================================================
// Module Introspection Trait
// =============================================================================

/// Trait for modules to expose their parameters to UIs
///
/// This trait extends `GraphModule` to provide parameter metadata
/// that UIs can use to automatically generate appropriate controls.
pub trait ModuleIntrospection: GraphModule {
    /// Get all parameter descriptors for this module
    ///
    /// Returns a list of `ParamInfo` describing each controllable parameter.
    /// The order should be consistent and reflect a logical grouping.
    fn param_infos(&self) -> Vec<ParamInfo> {
        Vec::new()
    }

    /// Get a specific parameter by its ID
    fn get_param_info(&self, id: &str) -> Option<ParamInfo> {
        self.param_infos().into_iter().find(|p| p.id == id)
    }

    /// Set a parameter value by its ID
    ///
    /// Returns true if the parameter was found and set, false otherwise.
    fn set_param_by_id(&mut self, _id: &str, _value: f64) -> bool {
        false
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_format_decimal() {
        let fmt = ValueFormat::Decimal { places: 2 };
        assert_eq!(fmt.format(3.14159), "3.14");
    }

    #[test]
    fn test_value_format_frequency() {
        let fmt = ValueFormat::Frequency;
        assert_eq!(fmt.format(440.0), "440.0 Hz");
        assert_eq!(fmt.format(2500.0), "2.50 kHz");
    }

    #[test]
    fn test_value_format_time() {
        let fmt = ValueFormat::Time;
        assert_eq!(fmt.format(0.1), "100.0 ms");
        assert_eq!(fmt.format(2.5), "2.50 s");
    }

    #[test]
    fn test_value_format_decibels() {
        let fmt = ValueFormat::Decibels;
        assert_eq!(fmt.format(-12.0), "-12.0 dB");
    }

    #[test]
    fn test_value_format_percent() {
        let fmt = ValueFormat::Percent;
        assert_eq!(fmt.format(0.5), "50%");
        assert_eq!(fmt.format(1.0), "100%");
    }

    #[test]
    fn test_value_format_note_name() {
        let fmt = ValueFormat::NoteName;
        assert_eq!(fmt.format(0.0), "C4"); // 0V = C4
        assert_eq!(fmt.format(1.0), "C5"); // 1V = C5
    }

    #[test]
    fn test_value_format_ratio() {
        let fmt = ValueFormat::Ratio;
        assert_eq!(fmt.format(2.0), "2.0:1");
        assert_eq!(fmt.format(0.5), "1:2.0");
    }

    #[test]
    fn test_param_curve_linear() {
        let curve = ParamCurve::Linear;
        assert!((curve.apply(0.0, 0.0, 100.0) - 0.0).abs() < 0.01);
        assert!((curve.apply(0.5, 0.0, 100.0) - 50.0).abs() < 0.01);
        assert!((curve.apply(1.0, 0.0, 100.0) - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_param_curve_exponential() {
        let curve = ParamCurve::Exponential;
        let val = curve.apply(0.5, 20.0, 20000.0);
        // At 50%, exponential should give geometric mean
        let expected = (20.0_f64 * 20000.0).sqrt();
        assert!((val - expected).abs() < 1.0);
    }

    #[test]
    fn test_param_curve_stepped() {
        let curve = ParamCurve::Stepped { steps: 4 };
        // With 4 steps over range 0-3, step_size = 0.75
        // step_index = floor(normalized * steps)
        assert!((curve.apply(0.0, 0.0, 3.0) - 0.0).abs() < 0.01); // step 0
        assert!((curve.apply(0.25, 0.0, 3.0) - 0.75).abs() < 0.01); // step 1
        assert!((curve.apply(0.5, 0.0, 3.0) - 1.5).abs() < 0.01); // step 2
        assert!((curve.apply(0.75, 0.0, 3.0) - 2.25).abs() < 0.01); // step 3
    }

    #[test]
    fn test_param_curve_normalize_linear() {
        let curve = ParamCurve::Linear;
        assert!((curve.normalize(50.0, 0.0, 100.0) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_param_info_creation() {
        let param = ParamInfo::new("freq", "Frequency")
            .with_range(20.0, 20000.0)
            .with_default(440.0)
            .with_curve(ParamCurve::Exponential)
            .with_unit("Hz")
            .with_format(ValueFormat::Frequency);

        assert_eq!(param.id, "freq");
        assert_eq!(param.name, "Frequency");
        assert_eq!(param.min, 20.0);
        assert_eq!(param.max, 20000.0);
        assert_eq!(param.default, 440.0);
        assert_eq!(param.value, 440.0);
        assert_eq!(param.unit, Some("Hz".to_string()));
    }

    #[test]
    fn test_param_info_frequency_preset() {
        let param = ParamInfo::frequency("cutoff", "Cutoff");
        assert_eq!(param.min, 20.0);
        assert_eq!(param.max, 20000.0);
        assert_eq!(param.curve, ParamCurve::Exponential);
    }

    #[test]
    fn test_param_info_toggle_preset() {
        let param = ParamInfo::toggle("sync", "Hard Sync");
        assert_eq!(param.control, ControlType::Toggle);
        assert!(matches!(param.curve, ParamCurve::Stepped { steps: 2 }));
    }

    #[test]
    fn test_param_info_select_preset() {
        let param = ParamInfo::select("waveform", "Waveform", 4);
        assert_eq!(param.control, ControlType::Select);
        assert!(matches!(param.curve, ParamCurve::Stepped { steps: 4 }));
    }

    #[test]
    fn test_param_info_normalized() {
        let mut param = ParamInfo::new("test", "Test")
            .with_range(0.0, 100.0)
            .with_value(50.0);

        assert!((param.normalized() - 0.5).abs() < 0.01);

        param.set_normalized(0.25);
        assert!((param.value - 25.0).abs() < 0.01);
    }

    #[test]
    fn test_param_info_format_value() {
        let param = ParamInfo::frequency("freq", "Frequency").with_value(1000.0);
        assert_eq!(param.format_value(), "1.00 kHz");
    }

    #[test]
    fn test_param_info_serialization() {
        let param = ParamInfo::frequency("cutoff", "Cutoff");
        let json = serde_json::to_string(&param).unwrap();
        assert!(json.contains("cutoff"));
        assert!(json.contains("exponential"));

        let parsed: ParamInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "cutoff");
    }
}
