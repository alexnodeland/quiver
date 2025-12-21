//! Layer 2: Signal Conventions and Port System
//!
//! This module defines the signal types, port definitions, and type-erased interfaces
//! that bridge the typed combinator layer with the graph-based patching system.

use crate::StdMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use libm::Libm;
use serde::{Deserialize, Serialize};

/// Unique identifier for a port within a module
pub type PortId = u32;

/// Unique identifier for a parameter within a module
pub type ParamId = u32;

/// Semantic signal classification following hardware modular conventions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalKind {
    /// Audio signal, AC-coupled, typically ±5V peak
    Audio,

    /// Bipolar control voltage, ±5V (LFO, pitch bend, modulation)
    CvBipolar,

    /// Unipolar control voltage, 0–10V (envelope, velocity, expression)
    CvUnipolar,

    /// Pitch CV following 1V/octave standard
    /// Reference: 0V = C4 (middle C, 261.63 Hz)
    VoltPerOctave,

    /// Gate signal, binary state: 0V (low) or +5V (high)
    /// Remains high while note/event is active
    Gate,

    /// Trigger signal, short pulse (~1–10ms) at +5V
    /// Used for instantaneous events
    Trigger,

    /// Clock signal, regular trigger pulses at tempo
    Clock,
}

impl SignalKind {
    /// Returns the typical voltage range (min, max) for this signal type
    pub fn voltage_range(&self) -> (f64, f64) {
        match self {
            SignalKind::Audio => (-5.0, 5.0),
            SignalKind::CvBipolar => (-5.0, 5.0),
            SignalKind::CvUnipolar => (0.0, 10.0),
            SignalKind::VoltPerOctave => (-5.0, 5.0), // ~C-1 to C9
            SignalKind::Gate => (0.0, 5.0),
            SignalKind::Trigger => (0.0, 5.0),
            SignalKind::Clock => (0.0, 5.0),
        }
    }

    /// Whether multiple signals of this kind should be summed when connected
    pub fn is_summable(&self) -> bool {
        matches!(
            self,
            SignalKind::Audio
                | SignalKind::CvBipolar
                | SignalKind::CvUnipolar
                | SignalKind::VoltPerOctave
        )
    }

    /// Threshold voltage for high/low detection
    pub fn gate_threshold(&self) -> Option<f64> {
        match self {
            SignalKind::Gate | SignalKind::Trigger | SignalKind::Clock => Some(2.5),
            _ => None,
        }
    }
}

/// Definition of a single port (input or output)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDef {
    /// Unique identifier within the module
    pub id: PortId,

    /// Human-readable name (e.g., "cutoff", "voct", "out")
    pub name: String,

    /// Signal type for validation and UI hints
    pub kind: SignalKind,

    /// Default value when no cable connected
    pub default: f64,

    /// For inputs: internal source when unpatched (normalled connection)
    pub normalled_to: Option<PortId>,

    /// Whether this input has an associated attenuverter control
    pub has_attenuverter: bool,
}

impl PortDef {
    pub fn new(id: PortId, name: impl Into<String>, kind: SignalKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            default: 0.0,
            normalled_to: None,
            has_attenuverter: false,
        }
    }

    pub fn with_default(mut self, default: f64) -> Self {
        self.default = default;
        self
    }

    pub fn with_attenuverter(mut self) -> Self {
        self.has_attenuverter = true;
        self
    }

    pub fn normalled_to(mut self, port: PortId) -> Self {
        self.normalled_to = Some(port);
        self
    }
}

/// Specification of all ports for a module
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortSpec {
    pub inputs: Vec<PortDef>,
    pub outputs: Vec<PortDef>,
}

impl PortSpec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn input_by_name(&self, name: &str) -> Option<&PortDef> {
        self.inputs.iter().find(|p| p.name == name)
    }

    pub fn output_by_name(&self, name: &str) -> Option<&PortDef> {
        self.outputs.iter().find(|p| p.name == name)
    }

    pub fn input_by_id(&self, id: PortId) -> Option<&PortDef> {
        self.inputs.iter().find(|p| p.id == id)
    }

    pub fn output_by_id(&self, id: PortId) -> Option<&PortDef> {
        self.outputs.iter().find(|p| p.id == id)
    }
}

/// Runtime port values container
#[derive(Debug, Clone, Default)]
pub struct PortValues {
    pub values: StdMap<PortId, f64>,
}

impl PortValues {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, id: PortId) -> Option<f64> {
        self.values.get(&id).copied()
    }

    pub fn get_or(&self, id: PortId, default: f64) -> f64 {
        self.values.get(&id).copied().unwrap_or(default)
    }

    pub fn set(&mut self, id: PortId, value: f64) {
        self.values.insert(id, value);
    }

    /// Accumulate (sum) a value into a port (for input mixing)
    pub fn accumulate(&mut self, id: PortId, value: f64) {
        *self.values.entry(id).or_insert(0.0) += value;
    }

    pub fn has(&self, id: PortId) -> bool {
        self.values.contains_key(&id)
    }

    pub fn clear(&mut self) {
        self.values.clear();
    }
}

/// Block-oriented port values for efficient processing
pub struct BlockPortValues {
    buffers: StdMap<PortId, Vec<f64>>,
    block_size: usize,
}

impl BlockPortValues {
    pub fn new(block_size: usize) -> Self {
        Self {
            buffers: StdMap::new(),
            block_size,
        }
    }

    pub fn block_size(&self) -> usize {
        self.block_size
    }

    pub fn get_buffer(&self, port: PortId) -> Option<&[f64]> {
        self.buffers.get(&port).map(|v| v.as_slice())
    }

    pub fn get_buffer_mut(&mut self, port: PortId) -> &mut Vec<f64> {
        self.buffers
            .entry(port)
            .or_insert_with(|| vec![0.0; self.block_size])
    }

    pub fn frame(&self, index: usize) -> PortValues {
        let mut values = PortValues::new();
        for (&port, buffer) in &self.buffers {
            if index < buffer.len() {
                values.set(port, buffer[index]);
            }
        }
        values
    }

    pub fn set_frame(&mut self, index: usize, values: PortValues) {
        for (&port, &value) in &values.values {
            let buffer = self.get_buffer_mut(port);
            if index < buffer.len() {
                buffer[index] = value;
            }
        }
    }

    pub fn clear(&mut self) {
        for buffer in self.buffers.values_mut() {
            buffer.fill(0.0);
        }
    }
}

/// Parameter range mapping for modulated parameters
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ParamRange {
    /// Linear mapping from normalized (0–1) to (min, max)
    Linear { min: f64, max: f64 },

    /// Exponential mapping, useful for frequency/time controls
    Exponential { min: f64, max: f64 },

    /// V/Oct: input is in volts, output is frequency multiplier
    VoltPerOctave { base_freq: f64 },
}

impl ParamRange {
    pub fn apply(&self, normalized: f64) -> f64 {
        match self {
            ParamRange::Linear { min, max } => min + normalized.clamp(0.0, 1.0) * (max - min),
            ParamRange::Exponential { min, max } => {
                let clamped = normalized.clamp(0.0, 1.0);
                if *min <= 0.0 {
                    // Handle edge case where min is zero or negative
                    clamped * max
                } else {
                    min * Libm::<f64>::pow(max / min, clamped)
                }
            }
            ParamRange::VoltPerOctave { base_freq } => {
                base_freq * Libm::<f64>::pow(2.0, normalized)
            }
        }
    }
}

/// A parameter that combines a base value (knob) with CV modulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModulatedParam {
    /// Base value from panel knob (typically 0.0–1.0 normalized)
    pub base: f64,

    /// Incoming CV voltage (set during tick)
    pub cv: f64,

    /// Attenuverter setting (-1.0 to 1.0)
    /// Positive: CV adds to base
    /// Negative: CV subtracts from base (inverted)
    pub attenuverter: f64,

    /// Output range mapping
    pub range: ParamRange,
}

impl ModulatedParam {
    pub fn new(range: ParamRange) -> Self {
        Self {
            base: 0.5,
            cv: 0.0,
            attenuverter: 1.0,
            range,
        }
    }

    pub fn with_base(mut self, base: f64) -> Self {
        self.base = base;
        self
    }

    /// Compute the effective parameter value
    pub fn value(&self) -> f64 {
        let modulated = self.base + (self.cv * self.attenuverter);
        self.range.apply(modulated)
    }

    /// Update CV from port value
    pub fn set_cv(&mut self, cv: f64) {
        self.cv = cv;
    }
}

/// Parameter definition for UI binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDef {
    pub id: ParamId,
    pub name: String,
    pub default: f64,
    pub range: ParamRange,
}

/// Type-erased module interface for graph-based patching
pub trait GraphModule: Send + Sync {
    /// Returns the module's port specification
    fn port_spec(&self) -> &PortSpec;

    /// Process one sample given port values
    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues);

    /// Process a block of samples (optional optimization)
    fn process_block(
        &mut self,
        inputs: &BlockPortValues,
        outputs: &mut BlockPortValues,
        frames: usize,
    ) {
        for i in 0..frames {
            let in_frame = inputs.frame(i);
            let mut out_frame = PortValues::new();
            self.tick(&in_frame, &mut out_frame);
            outputs.set_frame(i, out_frame);
        }
    }

    /// Reset internal state
    fn reset(&mut self);

    /// Set sample rate
    fn set_sample_rate(&mut self, sample_rate: f64);

    /// Get parameter definitions for UI binding
    fn params(&self) -> &[ParamDef] {
        &[]
    }

    /// Get a parameter value
    fn get_param(&self, _id: ParamId) -> Option<f64> {
        None
    }

    /// Set a parameter value
    fn set_param(&mut self, _id: ParamId, _value: f64) {}

    /// Get module type identifier for serialization
    fn type_id(&self) -> &'static str {
        "unknown"
    }

    /// Serialize module state (alloc feature only)
    #[cfg(feature = "alloc")]
    fn serialize_state(&self) -> Option<serde_json::Value> {
        None
    }

    /// Deserialize module state (alloc feature only)
    #[cfg(feature = "alloc")]
    fn deserialize_state(
        &mut self,
        _state: &serde_json::Value,
    ) -> Result<(), alloc::string::String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_kind_ranges() {
        assert_eq!(SignalKind::Audio.voltage_range(), (-5.0, 5.0));
        assert_eq!(SignalKind::Gate.voltage_range(), (0.0, 5.0));
        assert_eq!(SignalKind::CvUnipolar.voltage_range(), (0.0, 10.0));
    }

    #[test]
    fn test_signal_kind_summable() {
        assert!(SignalKind::Audio.is_summable());
        assert!(SignalKind::CvBipolar.is_summable());
        assert!(!SignalKind::Gate.is_summable());
        assert!(!SignalKind::Trigger.is_summable());
    }

    #[test]
    fn test_port_values() {
        let mut pv = PortValues::new();
        pv.set(0, 1.0);
        pv.set(1, 2.0);
        assert_eq!(pv.get(0), Some(1.0));
        assert_eq!(pv.get(1), Some(2.0));
        assert_eq!(pv.get(2), None);
        assert_eq!(pv.get_or(2, 5.0), 5.0);

        pv.accumulate(0, 0.5);
        assert_eq!(pv.get(0), Some(1.5));
    }

    #[test]
    fn test_param_range_linear() {
        let range = ParamRange::Linear {
            min: 0.0,
            max: 100.0,
        };
        assert!((range.apply(0.0) - 0.0).abs() < 1e-10);
        assert!((range.apply(0.5) - 50.0).abs() < 1e-10);
        assert!((range.apply(1.0) - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_param_range_exponential() {
        let range = ParamRange::Exponential {
            min: 20.0,
            max: 20000.0,
        };
        assert!((range.apply(0.0) - 20.0).abs() < 1e-10);
        assert!((range.apply(1.0) - 20000.0).abs() < 1e-10);
    }

    #[test]
    fn test_param_range_voct() {
        let range = ParamRange::VoltPerOctave { base_freq: 261.63 };
        // 0V = C4 = 261.63 Hz
        assert!((range.apply(0.0) - 261.63).abs() < 0.01);
        // +1V = C5 = 523.26 Hz
        assert!((range.apply(1.0) - 523.26).abs() < 0.01);
    }

    #[test]
    fn test_modulated_param() {
        let mut param = ModulatedParam::new(ParamRange::Linear {
            min: 0.0,
            max: 100.0,
        })
        .with_base(0.5);

        // No CV: should return base * range
        assert!((param.value() - 50.0).abs() < 1e-10);

        // Add CV
        param.set_cv(0.2);
        assert!((param.value() - 70.0).abs() < 1e-10);

        // Invert attenuverter
        param.attenuverter = -1.0;
        assert!((param.value() - 30.0).abs() < 1e-10);
    }

    #[test]
    fn test_signal_kind_gate_threshold() {
        assert!(SignalKind::Gate.gate_threshold().is_some());
        assert!(SignalKind::Trigger.gate_threshold().is_some());
        assert!(SignalKind::Audio.gate_threshold().is_none());
    }

    #[test]
    fn test_port_def_with_default_and_attenuverter() {
        let port = PortDef::new(0, "test", SignalKind::CvUnipolar)
            .with_default(5.0)
            .with_attenuverter();

        assert!((port.default - 5.0).abs() < 0.001);
        assert!(port.has_attenuverter);
    }

    #[test]
    fn test_port_def_normalled_to() {
        let port = PortDef::new(0, "test", SignalKind::CvUnipolar).normalled_to(1);
        assert_eq!(port.normalled_to, Some(1));
    }

    #[test]
    fn test_port_spec_lookup() {
        let spec = PortSpec {
            inputs: vec![
                PortDef::new(0, "in1", SignalKind::Audio),
                PortDef::new(1, "in2", SignalKind::CvBipolar),
            ],
            outputs: vec![
                PortDef::new(10, "out1", SignalKind::Audio),
                PortDef::new(11, "out2", SignalKind::Gate),
            ],
        };

        assert!(spec.input_by_name("in1").is_some());
        assert!(spec.input_by_name("nonexistent").is_none());
        assert!(spec.output_by_name("out1").is_some());
        assert!(spec.output_by_name("nonexistent").is_none());

        assert!(spec.input_by_id(0).is_some());
        assert!(spec.input_by_id(99).is_none());
        assert!(spec.output_by_id(10).is_some());
        assert!(spec.output_by_id(99).is_none());
    }

    #[test]
    fn test_port_values_has() {
        let mut pv = PortValues::new();
        assert!(!pv.has(0));
        pv.set(0, 1.0);
        assert!(pv.has(0));
    }

    #[test]
    fn test_port_values_clear() {
        let mut pv = PortValues::new();
        pv.set(0, 1.0);
        pv.set(1, 2.0);
        pv.clear();
        assert!(!pv.has(0));
        assert!(!pv.has(1));
    }

    #[test]
    fn test_block_port_values() {
        let mut bpv = BlockPortValues::new(64);
        assert_eq!(bpv.block_size(), 64);

        // Get mutable buffer (creates buffer for port 0)
        let buf_mut = bpv.get_buffer_mut(0);
        assert_eq!(buf_mut.len(), 64);
        buf_mut[0] = 1.0;

        // Now we can read it
        assert_eq!(bpv.get_buffer(0).unwrap()[0], 1.0);

        // Frame operations
        let mut frame_vals = PortValues::new();
        frame_vals.set(0, 99.0);
        bpv.set_frame(1, frame_vals);

        // Clear
        bpv.clear();
    }

    #[test]
    fn test_signal_kind_clock() {
        let range = SignalKind::Clock.voltage_range();
        assert_eq!(range, (0.0, 5.0));
        assert!(!SignalKind::Clock.is_summable());
    }

    #[test]
    fn test_param_range_exponential_clamped() {
        let range = ParamRange::Exponential {
            min: 20.0,
            max: 20000.0,
        };
        // Test with values outside 0-1
        let below = range.apply(-0.5);
        assert!((below - 20.0).abs() < 1e-10);

        let above = range.apply(1.5);
        assert!((above - 20000.0).abs() < 1e-10);
    }
}
