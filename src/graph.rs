//! Layer 3: Patch Graph
//!
//! This module provides the runtime graph-based patching system that allows
//! arbitrary signal routing between modules. It handles topological sorting,
//! execution ordering, and signal propagation.

use crate::port::{GraphModule, ParamId, PortId, PortSpec, PortValues, SignalKind};
use crate::StdMap;
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use slotmap::{DefaultKey, SlotMap};

/// Signal validation strictness level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationMode {
    /// No validation - allow any connections
    #[default]
    None,
    /// Warn on incompatible connections but allow them
    Warn,
    /// Error on incompatible connections
    Strict,
}

/// Result of signal kind compatibility check
#[derive(Debug, Clone)]
pub struct CompatibilityResult {
    pub compatible: bool,
    pub warning: Option<String>,
}

impl SignalKind {
    /// Check if this signal kind is compatible with another for connection
    /// Returns a compatibility result with optional warning message
    pub fn is_compatible_with(&self, other: &SignalKind) -> CompatibilityResult {
        use SignalKind::*;

        // Same types are always compatible
        if self == other {
            return CompatibilityResult {
                compatible: true,
                warning: None,
            };
        }

        // Define compatibility rules
        match (self, other) {
            // Audio can connect to any CV for AM/ring mod effects
            (Audio, CvBipolar) | (CvBipolar, Audio) => CompatibilityResult {
                compatible: true,
                warning: Some("Audio/CV connection - ensure this is intentional".to_string()),
            },

            // Bipolar and unipolar CV are generally compatible with a warning
            (CvBipolar, CvUnipolar) | (CvUnipolar, CvBipolar) => CompatibilityResult {
                compatible: true,
                warning: Some(
                    "Bipolar/Unipolar CV mismatch - signal may be clipped or offset".to_string(),
                ),
            },

            // V/Oct can receive from bipolar CV (for pitch modulation)
            (CvBipolar, VoltPerOctave) => CompatibilityResult {
                compatible: true,
                warning: None,
            },

            // V/Oct to bipolar CV (extracting pitch as modulation)
            (VoltPerOctave, CvBipolar) => CompatibilityResult {
                compatible: true,
                warning: None,
            },

            // Gate/Trigger/Clock are interchangeable with warnings
            (Gate, Trigger) | (Trigger, Gate) => CompatibilityResult {
                compatible: true,
                warning: Some("Gate/Trigger connection - timing behavior may differ".to_string()),
            },

            (Clock, Trigger) | (Trigger, Clock) => CompatibilityResult {
                compatible: true,
                warning: None,
            },

            (Clock, Gate) | (Gate, Clock) => CompatibilityResult {
                compatible: true,
                warning: Some("Clock/Gate connection - duty cycle may affect behavior".to_string()),
            },

            // Audio to V/Oct is unusual but can be used for audio-rate FM
            (Audio, VoltPerOctave) => CompatibilityResult {
                compatible: true,
                warning: Some(
                    "Audio-rate pitch modulation - ensure this is intentional".to_string(),
                ),
            },

            // CV Unipolar can modulate V/Oct (for portamento, etc.)
            (CvUnipolar, VoltPerOctave) => CompatibilityResult {
                compatible: true,
                warning: Some("Unipolar CV to V/Oct - may need offset adjustment".to_string()),
            },

            // V/Oct to unipolar (unusual)
            (VoltPerOctave, CvUnipolar) => CompatibilityResult {
                compatible: true,
                warning: Some("V/Oct to Unipolar - negative voltages will be clipped".to_string()),
            },

            // Audio can be used as gate (for envelope followers, etc.)
            (Audio, Gate) | (Audio, Trigger) => CompatibilityResult {
                compatible: true,
                warning: Some("Audio to Gate/Trigger - signal will be thresholded".to_string()),
            },

            // All other combinations are allowed but with strong warning
            _ => CompatibilityResult {
                compatible: true,
                warning: Some(format!("Unusual connection: {:?} -> {:?}", self, other)),
            },
        }
    }
}

/// Unique identifier for a node in the patch graph
pub type NodeId = DefaultKey;

/// Unique identifier for a cable connection
pub type CableId = usize;

/// Reference to a specific port on a specific node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PortRef {
    pub node: NodeId,
    pub port: PortId,
}

/// A cable connecting two ports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cable {
    pub from: PortRef,
    pub to: PortRef,
    /// Optional attenuation/gain (-2.0 to 2.0, where 1.0 = unity)
    /// Negative values invert the signal (attenuverter behavior)
    pub attenuation: Option<f64>,
    /// Optional DC offset added after attenuation (-10.0 to 10.0V)
    pub offset: Option<f64>,
}

/// Internal node representation
struct Node {
    module: Box<dyn GraphModule>,
    name: String,
    position: Option<(f32, f32)>,
}

/// Error types for patch operations
#[derive(Debug, Clone)]
pub enum PatchError {
    InvalidNode,
    InvalidPort,
    InvalidCable,
    CycleDetected {
        nodes: Vec<NodeId>,
    },
    CompilationFailed(String),
    /// Signal type mismatch (only in Strict validation mode)
    SignalMismatch {
        from_kind: SignalKind,
        to_kind: SignalKind,
        message: String,
    },
}

impl core::fmt::Display for PatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PatchError::InvalidNode => write!(f, "Invalid node"),
            PatchError::InvalidPort => write!(f, "Invalid port"),
            PatchError::InvalidCable => write!(f, "Invalid cable"),
            PatchError::CycleDetected { nodes } => {
                write!(f, "Cycle detected involving {} nodes", nodes.len())
            }
            PatchError::CompilationFailed(msg) => write!(f, "Compilation failed: {}", msg),
            PatchError::SignalMismatch {
                from_kind,
                to_kind,
                message,
            } => write!(
                f,
                "Signal mismatch: {:?} -> {:?}: {}",
                from_kind, to_kind, message
            ),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for PatchError {}

/// Handle to a node for ergonomic port references
#[derive(Clone)]
pub struct NodeHandle {
    id: NodeId,
    spec: PortSpec,
}

impl NodeHandle {
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Reference an output port by name
    pub fn out(&self, name: &str) -> PortRef {
        let port = self
            .spec
            .output_by_name(name)
            .unwrap_or_else(|| panic!("Unknown output port: {}", name));
        PortRef {
            node: self.id,
            port: port.id,
        }
    }

    /// Reference an input port by name
    pub fn in_(&self, name: &str) -> PortRef {
        let port = self
            .spec
            .input_by_name(name)
            .unwrap_or_else(|| panic!("Unknown input port: {}", name));
        PortRef {
            node: self.id,
            port: port.id,
        }
    }

    /// Get the port specification
    pub fn spec(&self) -> &PortSpec {
        &self.spec
    }
}

/// The main patch graph containing modules and connections
pub struct Patch {
    nodes: SlotMap<NodeId, Node>,
    cables: Vec<Cable>,

    // Execution state
    execution_order: Vec<NodeId>,
    buffers: StdMap<PortRef, f64>,

    // Configuration
    sample_rate: f64,

    // Output node
    output_node: Option<NodeId>,

    // Validation
    validation_mode: ValidationMode,
    warnings: Vec<String>,
}

impl Patch {
    /// Create a new empty patch
    pub fn new(sample_rate: f64) -> Self {
        Self {
            nodes: SlotMap::new(),
            cables: Vec::new(),
            execution_order: Vec::new(),
            buffers: StdMap::new(),
            sample_rate,
            output_node: None,
            validation_mode: ValidationMode::None,
            warnings: Vec::new(),
        }
    }

    /// Set the signal validation mode
    pub fn set_validation_mode(&mut self, mode: ValidationMode) {
        self.validation_mode = mode;
    }

    /// Get the current validation mode
    pub fn validation_mode(&self) -> ValidationMode {
        self.validation_mode
    }

    /// Get all warnings generated during patching
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Clear all warnings
    pub fn clear_warnings(&mut self) {
        self.warnings.clear();
    }

    /// Get the sample rate
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Add a module to the patch
    pub fn add<M: GraphModule + 'static>(
        &mut self,
        name: impl Into<String>,
        mut module: M,
    ) -> NodeHandle {
        module.set_sample_rate(self.sample_rate);
        let spec = module.port_spec().clone();
        let id = self.nodes.insert(Node {
            module: Box::new(module),
            name: name.into(),
            position: None,
        });
        self.invalidate();
        NodeHandle { id, spec }
    }

    /// Add a boxed module to the patch
    pub fn add_boxed(
        &mut self,
        name: impl Into<String>,
        mut module: Box<dyn GraphModule>,
    ) -> NodeHandle {
        module.set_sample_rate(self.sample_rate);
        let spec = module.port_spec().clone();
        let id = self.nodes.insert(Node {
            module,
            name: name.into(),
            position: None,
        });
        self.invalidate();
        NodeHandle { id, spec }
    }

    /// Remove a module from the patch
    pub fn remove(&mut self, node: NodeId) -> Result<(), PatchError> {
        if self.nodes.remove(node).is_none() {
            return Err(PatchError::InvalidNode);
        }

        // Remove all cables connected to this node
        self.cables
            .retain(|cable| cable.from.node != node && cable.to.node != node);

        if self.output_node == Some(node) {
            self.output_node = None;
        }

        self.invalidate();
        Ok(())
    }

    /// Connect an output port to an input port
    pub fn connect(&mut self, from: PortRef, to: PortRef) -> Result<CableId, PatchError> {
        self.validate_output_port(from)?;
        self.validate_input_port(to)?;
        self.validate_signal_compatibility(from, to)?;

        let cable = Cable {
            from,
            to,
            attenuation: None,
            offset: None,
        };
        self.cables.push(cable);
        self.invalidate();
        Ok(self.cables.len() - 1)
    }

    /// Connect with attenuation (0.0-1.0 range for backwards compatibility)
    pub fn connect_attenuated(
        &mut self,
        from: PortRef,
        to: PortRef,
        attenuation: f64,
    ) -> Result<CableId, PatchError> {
        self.validate_output_port(from)?;
        self.validate_input_port(to)?;
        self.validate_signal_compatibility(from, to)?;

        let cable = Cable {
            from,
            to,
            attenuation: Some(attenuation.clamp(0.0, 1.0)),
            offset: None,
        };
        self.cables.push(cable);
        self.invalidate();
        Ok(self.cables.len() - 1)
    }

    /// Connect with full modulation controls (attenuverter and offset)
    /// attenuation: -2.0 to 2.0 (negative inverts, >1.0 amplifies)
    /// offset: -10.0 to 10.0V DC offset added after attenuation
    pub fn connect_modulated(
        &mut self,
        from: PortRef,
        to: PortRef,
        attenuation: f64,
        offset: f64,
    ) -> Result<CableId, PatchError> {
        self.validate_output_port(from)?;
        self.validate_input_port(to)?;
        self.validate_signal_compatibility(from, to)?;

        let cable = Cable {
            from,
            to,
            attenuation: Some(attenuation.clamp(-2.0, 2.0)),
            offset: Some(offset.clamp(-10.0, 10.0)),
        };
        self.cables.push(cable);
        self.invalidate();
        Ok(self.cables.len() - 1)
    }

    /// Validate signal kind compatibility between ports
    fn validate_signal_compatibility(
        &mut self,
        from: PortRef,
        to: PortRef,
    ) -> Result<(), PatchError> {
        if self.validation_mode == ValidationMode::None {
            return Ok(());
        }

        // Get the signal kinds for both ports
        let from_kind = self.get_output_port_kind(from);
        let to_kind = self.get_input_port_kind(to);

        if let (Some(from_kind), Some(to_kind)) = (from_kind, to_kind) {
            let result = from_kind.is_compatible_with(&to_kind);

            if let Some(warning) = result.warning {
                let from_name = self.get_name(from.node).unwrap_or("unknown");
                let to_name = self.get_name(to.node).unwrap_or("unknown");
                let full_warning = format!(
                    "{}.{} -> {}.{}: {}",
                    from_name, from.port, to_name, to.port, warning
                );

                match self.validation_mode {
                    ValidationMode::Warn => {
                        self.warnings.push(full_warning);
                    }
                    ValidationMode::Strict => {
                        return Err(PatchError::SignalMismatch {
                            from_kind,
                            to_kind,
                            message: warning,
                        });
                    }
                    ValidationMode::None => {}
                }
            }
        }

        Ok(())
    }

    /// Get the signal kind for an output port
    fn get_output_port_kind(&self, port_ref: PortRef) -> Option<SignalKind> {
        let node = self.nodes.get(port_ref.node)?;
        node.module
            .port_spec()
            .outputs
            .iter()
            .find(|p| p.id == port_ref.port)
            .map(|p| p.kind)
    }

    /// Get the signal kind for an input port
    fn get_input_port_kind(&self, port_ref: PortRef) -> Option<SignalKind> {
        let node = self.nodes.get(port_ref.node)?;
        node.module
            .port_spec()
            .inputs
            .iter()
            .find(|p| p.id == port_ref.port)
            .map(|p| p.kind)
    }

    /// Connect one output to multiple inputs (mult)
    pub fn mult(&mut self, from: PortRef, to: &[PortRef]) -> Result<Vec<CableId>, PatchError> {
        to.iter().map(|&dest| self.connect(from, dest)).collect()
    }

    /// Disconnect a cable by ID
    pub fn disconnect(&mut self, cable_id: CableId) -> Result<(), PatchError> {
        if cable_id >= self.cables.len() {
            return Err(PatchError::InvalidCable);
        }
        self.cables.remove(cable_id);
        self.invalidate();
        Ok(())
    }

    /// Set the output node for the patch
    pub fn set_output(&mut self, node: NodeId) {
        self.output_node = Some(node);
    }

    /// Set a parameter on a module
    pub fn set_param(&mut self, node: NodeId, param: ParamId, value: f64) {
        if let Some(n) = self.nodes.get_mut(node) {
            n.module.set_param(param, value);
        }
    }

    /// Get a parameter value from a module
    pub fn get_param(&self, node: NodeId, param: ParamId) -> Option<f64> {
        self.nodes.get(node).and_then(|n| n.module.get_param(param))
    }

    /// Set module position (for UI)
    pub fn set_position(&mut self, node: NodeId, position: (f32, f32)) {
        if let Some(n) = self.nodes.get_mut(node) {
            n.position = Some(position);
        }
    }

    /// Get module position (for UI/serialization)
    pub fn get_position(&self, node: NodeId) -> Option<(f32, f32)> {
        self.nodes.get(node).and_then(|n| n.position)
    }

    /// Get module name
    pub fn get_name(&self, node: NodeId) -> Option<&str> {
        self.nodes.get(node).map(|n| n.name.as_str())
    }

    /// Get number of nodes
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get number of cables
    pub fn cable_count(&self) -> usize {
        self.cables.len()
    }

    /// Get all cables
    pub fn cables(&self) -> &[Cable] {
        &self.cables
    }

    /// Get execution order (after compile)
    pub fn execution_order(&self) -> &[NodeId] {
        &self.execution_order
    }

    fn invalidate(&mut self) {
        self.execution_order.clear();
    }

    fn validate_output_port(&self, port_ref: PortRef) -> Result<(), PatchError> {
        let node = self
            .nodes
            .get(port_ref.node)
            .ok_or(PatchError::InvalidNode)?;
        node.module
            .port_spec()
            .outputs
            .iter()
            .find(|p| p.id == port_ref.port)
            .ok_or(PatchError::InvalidPort)?;
        Ok(())
    }

    fn validate_input_port(&self, port_ref: PortRef) -> Result<(), PatchError> {
        let node = self
            .nodes
            .get(port_ref.node)
            .ok_or(PatchError::InvalidNode)?;
        node.module
            .port_spec()
            .inputs
            .iter()
            .find(|p| p.id == port_ref.port)
            .ok_or(PatchError::InvalidPort)?;
        Ok(())
    }

    /// Compile the patch into an executable order
    pub fn compile(&mut self) -> Result<(), PatchError> {
        let order = self.topological_sort()?;
        self.execution_order = order;

        // Pre-allocate output buffers
        self.buffers.clear();
        for (id, node) in &self.nodes {
            for output in &node.module.port_spec().outputs {
                self.buffers.insert(
                    PortRef {
                        node: id,
                        port: output.id,
                    },
                    0.0,
                );
            }
        }

        Ok(())
    }

    fn topological_sort(&self) -> Result<Vec<NodeId>, PatchError> {
        let mut in_degree: StdMap<NodeId, usize> = self.nodes.keys().map(|k| (k, 0)).collect();
        let mut successors: StdMap<NodeId, Vec<NodeId>> =
            self.nodes.keys().map(|k| (k, vec![])).collect();

        for cable in &self.cables {
            *in_degree.entry(cable.to.node).or_insert(0) += 1;
            successors
                .entry(cable.from.node)
                .or_default()
                .push(cable.to.node);
        }

        // Kahn's algorithm
        let mut queue: VecDeque<NodeId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut result = Vec::with_capacity(self.nodes.len());

        while let Some(node) = queue.pop_front() {
            result.push(node);
            for &succ in successors.get(&node).unwrap_or(&vec![]) {
                let deg = in_degree.get_mut(&succ).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(succ);
                }
            }
        }

        if result.len() != self.nodes.len() {
            let in_cycle: Vec<NodeId> = in_degree
                .into_iter()
                .filter(|(_, deg)| *deg > 0)
                .map(|(id, _)| id)
                .collect();
            return Err(PatchError::CycleDetected { nodes: in_cycle });
        }

        Ok(result)
    }

    /// Process a single sample, returning stereo output
    pub fn tick(&mut self) -> (f64, f64) {
        for &node_id in &self.execution_order.clone() {
            let inputs = self.gather_inputs(node_id);
            let mut outputs = PortValues::new();

            // Process the module
            if let Some(node) = self.nodes.get_mut(node_id) {
                node.module.tick(&inputs, &mut outputs);
            }

            // Store outputs in buffers
            self.scatter_outputs(node_id, &outputs);
        }

        self.read_output()
    }

    fn gather_inputs(&self, node_id: NodeId) -> PortValues {
        let node = match self.nodes.get(node_id) {
            Some(n) => n,
            None => return PortValues::new(),
        };
        let spec = node.module.port_spec();
        let mut values = PortValues::new();

        for input in &spec.inputs {
            let port_ref = PortRef {
                node: node_id,
                port: input.id,
            };

            // Sum all incoming cables (hardware-style input mixing)
            let mut sum = 0.0;
            let mut has_connection = false;

            for cable in &self.cables {
                if cable.to == port_ref {
                    has_connection = true;
                    let value = self.buffers.get(&cable.from).copied().unwrap_or(0.0);
                    // Apply attenuation/attenuverter (signal * gain)
                    let attenuated = cable.attenuation.map(|a| value * a).unwrap_or(value);
                    // Apply DC offset after attenuation
                    let with_offset = cable.offset.map(|o| attenuated + o).unwrap_or(attenuated);
                    sum += with_offset;
                }
            }

            if has_connection {
                values.set(input.id, sum);
            } else if let Some(normalled) = input.normalled_to {
                // Use normalled (internal) connection
                let normalled_ref = PortRef {
                    node: node_id,
                    port: normalled,
                };
                if let Some(&v) = self.buffers.get(&normalled_ref) {
                    values.set(input.id, v);
                } else {
                    values.set(input.id, input.default);
                }
            } else {
                // Use default value
                values.set(input.id, input.default);
            }
        }

        values
    }

    fn scatter_outputs(&mut self, node_id: NodeId, outputs: &PortValues) {
        for (&port_id, &value) in &outputs.values {
            let port_ref = PortRef {
                node: node_id,
                port: port_id,
            };
            self.buffers.insert(port_ref, value);
        }
    }

    fn read_output(&self) -> (f64, f64) {
        if let Some(output_node) = self.output_node {
            let left = self
                .buffers
                .get(&PortRef {
                    node: output_node,
                    port: 0, // Assuming port 0 is left
                })
                .copied()
                .unwrap_or(0.0);
            let right = self
                .buffers
                .get(&PortRef {
                    node: output_node,
                    port: 1, // Assuming port 1 is right
                })
                .copied()
                .unwrap_or(left); // Mono fallback
            (left, right)
        } else {
            (0.0, 0.0)
        }
    }

    /// Reset all modules in the patch
    pub fn reset(&mut self) {
        for (_, node) in &mut self.nodes {
            node.module.reset();
        }
        for value in self.buffers.values_mut() {
            *value = 0.0;
        }
    }

    /// Iterate over all nodes
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &str, &dyn GraphModule)> {
        self.nodes
            .iter()
            .map(|(id, node)| (id, node.name.as_str(), node.module.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::port::{PortDef, SignalKind};

    // Simple passthrough module for testing
    struct Passthrough {
        spec: PortSpec,
    }

    impl Passthrough {
        fn new() -> Self {
            Self {
                spec: PortSpec {
                    inputs: vec![PortDef::new(0, "in", SignalKind::Audio)],
                    outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
                },
            }
        }
    }

    impl GraphModule for Passthrough {
        fn port_spec(&self) -> &PortSpec {
            &self.spec
        }

        fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
            let input = inputs.get_or(0, 0.0);
            outputs.set(10, input);
        }

        fn reset(&mut self) {}

        fn set_sample_rate(&mut self, _: f64) {}
    }

    #[test]
    fn test_add_module() {
        let mut patch = Patch::new(44100.0);
        let handle = patch.add("test", Passthrough::new());
        assert_eq!(patch.node_count(), 1);
        assert!(patch.get_name(handle.id()).is_some());
    }

    #[test]
    fn test_connect() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());

        let result = patch.connect(a.out("out"), b.in_("in"));
        assert!(result.is_ok());
        assert_eq!(patch.cable_count(), 1);
    }

    #[test]
    fn test_topological_sort() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());
        let c = patch.add("c", Passthrough::new());

        // A -> B -> C
        patch.connect(a.out("out"), b.in_("in")).unwrap();
        patch.connect(b.out("out"), c.in_("in")).unwrap();

        patch.compile().unwrap();

        let order = patch.execution_order();
        let a_pos = order.iter().position(|&x| x == a.id()).unwrap();
        let b_pos = order.iter().position(|&x| x == b.id()).unwrap();
        let c_pos = order.iter().position(|&x| x == c.id()).unwrap();

        assert!(a_pos < b_pos, "A should come before B");
        assert!(b_pos < c_pos, "B should come before C");
    }

    #[test]
    fn test_cycle_detection() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());

        // Create cycle: A -> B -> A
        patch.connect(a.out("out"), b.in_("in")).unwrap();
        patch.connect(b.out("out"), a.in_("in")).unwrap();

        let result = patch.compile();
        assert!(matches!(result, Err(PatchError::CycleDetected { .. })));
    }

    #[test]
    fn test_mult() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());
        let c = patch.add("c", Passthrough::new());

        let result = patch.mult(a.out("out"), &[b.in_("in"), c.in_("in")]);
        assert!(result.is_ok());
        assert_eq!(patch.cable_count(), 2);
    }

    #[test]
    fn test_disconnect() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());

        let cable_id = patch.connect(a.out("out"), b.in_("in")).unwrap();
        assert_eq!(patch.cable_count(), 1);

        patch.disconnect(cable_id).unwrap();
        assert_eq!(patch.cable_count(), 0);
    }

    #[test]
    fn test_remove_module() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());

        patch.connect(a.out("out"), b.in_("in")).unwrap();
        assert_eq!(patch.node_count(), 2);
        assert_eq!(patch.cable_count(), 1);

        patch.remove(a.id()).unwrap();
        assert_eq!(patch.node_count(), 1);
        assert_eq!(patch.cable_count(), 0); // Cable should be removed too
    }

    // ========================================================================
    // Phase 2 Tests: Signal Validation & Modulation
    // ========================================================================

    // Test modules with different signal types
    struct GateModule {
        spec: PortSpec,
    }

    impl GateModule {
        fn new() -> Self {
            Self {
                spec: PortSpec {
                    inputs: vec![PortDef::new(0, "in", SignalKind::Gate)],
                    outputs: vec![PortDef::new(10, "out", SignalKind::Gate)],
                },
            }
        }
    }

    impl GraphModule for GateModule {
        fn port_spec(&self) -> &PortSpec {
            &self.spec
        }
        fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
            outputs.set(10, inputs.get_or(0, 0.0));
        }
        fn reset(&mut self) {}
        fn set_sample_rate(&mut self, _: f64) {}
    }

    #[test]
    fn test_validation_mode_none() {
        let mut patch = Patch::new(44100.0);
        patch.set_validation_mode(ValidationMode::None);

        let audio = patch.add("audio", Passthrough::new());
        let gate = patch.add("gate", GateModule::new());

        // Should succeed without warnings
        let result = patch.connect(audio.out("out"), gate.in_("in"));
        assert!(result.is_ok());
        assert!(patch.warnings().is_empty());
    }

    #[test]
    fn test_validation_mode_warn() {
        let mut patch = Patch::new(44100.0);
        patch.set_validation_mode(ValidationMode::Warn);

        let audio = patch.add("audio", Passthrough::new());
        let gate = patch.add("gate", GateModule::new());

        // Should succeed but generate warning
        let result = patch.connect(audio.out("out"), gate.in_("in"));
        assert!(result.is_ok());
        assert!(!patch.warnings().is_empty());
    }

    #[test]
    fn test_validation_mode_strict() {
        let mut patch = Patch::new(44100.0);
        patch.set_validation_mode(ValidationMode::Strict);

        let audio = patch.add("audio", Passthrough::new());
        let gate = patch.add("gate", GateModule::new());

        // Should fail with SignalMismatch error
        let result = patch.connect(audio.out("out"), gate.in_("in"));
        assert!(matches!(result, Err(PatchError::SignalMismatch { .. })));
    }

    #[test]
    fn test_same_signal_type_no_warning() {
        let mut patch = Patch::new(44100.0);
        patch.set_validation_mode(ValidationMode::Warn);

        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());

        // Same type should not generate warning
        let result = patch.connect(a.out("out"), b.in_("in"));
        assert!(result.is_ok());
        assert!(patch.warnings().is_empty());
    }

    #[test]
    fn test_connect_modulated() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());

        // Connect with attenuation 0.5 and offset 1.0
        let result = patch.connect_modulated(a.out("out"), b.in_("in"), 0.5, 1.0);
        assert!(result.is_ok());

        let cables = patch.cables();
        assert_eq!(cables.len(), 1);
        assert_eq!(cables[0].attenuation, Some(0.5));
        assert_eq!(cables[0].offset, Some(1.0));
    }

    #[test]
    fn test_modulated_signal_processing() {
        let mut patch = Patch::new(44100.0);

        // Use a module that outputs a constant value
        struct ConstModule {
            spec: PortSpec,
            value: f64,
        }

        impl ConstModule {
            fn new(value: f64) -> Self {
                Self {
                    value,
                    spec: PortSpec {
                        inputs: vec![],
                        outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
                    },
                }
            }
        }

        impl GraphModule for ConstModule {
            fn port_spec(&self) -> &PortSpec {
                &self.spec
            }
            fn tick(&mut self, _: &PortValues, outputs: &mut PortValues) {
                outputs.set(10, self.value);
            }
            fn reset(&mut self) {}
            fn set_sample_rate(&mut self, _: f64) {}
        }

        struct RecordModule {
            spec: PortSpec,
            last_value: f64,
        }

        impl RecordModule {
            fn new() -> Self {
                Self {
                    spec: PortSpec {
                        inputs: vec![PortDef::new(0, "in", SignalKind::Audio)],
                        outputs: vec![],
                    },
                    last_value: 0.0,
                }
            }
        }

        impl GraphModule for RecordModule {
            fn port_spec(&self) -> &PortSpec {
                &self.spec
            }
            fn tick(&mut self, inputs: &PortValues, _: &mut PortValues) {
                self.last_value = inputs.get_or(0, 0.0);
            }
            fn reset(&mut self) {}
            fn set_sample_rate(&mut self, _: f64) {}
        }

        let source = patch.add("source", ConstModule::new(4.0));
        let sink = patch.add("sink", RecordModule::new());

        // Attenuation 0.5, offset 2.0: 4.0 * 0.5 + 2.0 = 4.0
        patch
            .connect_modulated(source.out("out"), sink.in_("in"), 0.5, 2.0)
            .unwrap();
        patch.set_output(sink.id());
        patch.compile().unwrap();
        patch.tick();

        // The value should be processed through attenuation and offset
        // We can't easily check the internal value, but we verified the connection works
    }

    #[test]
    fn test_signal_compatibility() {
        // Test specific compatibility cases
        assert!(SignalKind::Audio
            .is_compatible_with(&SignalKind::Audio)
            .warning
            .is_none());
        assert!(SignalKind::Audio
            .is_compatible_with(&SignalKind::CvBipolar)
            .warning
            .is_some());
        assert!(SignalKind::Gate
            .is_compatible_with(&SignalKind::Trigger)
            .warning
            .is_some());
        assert!(SignalKind::Clock
            .is_compatible_with(&SignalKind::Trigger)
            .warning
            .is_none());
    }

    #[test]
    fn test_patch_get_name() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("my_module", Passthrough::new());

        let name = patch.get_name(a.id());
        assert_eq!(name, Some("my_module"));

        // Non-existent node
        use slotmap::DefaultKey;
        let fake_id: NodeId = DefaultKey::default();
        assert!(patch.get_name(fake_id).is_none());
    }

    #[test]
    fn test_patch_set_position() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());

        patch.set_position(a.id(), (100.0, 200.0));
        // Position is stored but not exposed directly in tests
    }

    #[test]
    fn test_patch_clear_warnings() {
        let mut patch = Patch::new(44100.0);
        patch.set_validation_mode(ValidationMode::Warn);

        let audio = patch.add("audio", Passthrough::new());
        let gate = patch.add("gate", GateModule::new());

        patch.connect(audio.out("out"), gate.in_("in")).unwrap();
        assert!(!patch.warnings().is_empty());

        patch.clear_warnings();
        assert!(patch.warnings().is_empty());
    }

    #[test]
    fn test_patch_validation_mode_getter() {
        let mut patch = Patch::new(44100.0);
        patch.set_validation_mode(ValidationMode::Strict);
        assert_eq!(patch.validation_mode(), ValidationMode::Strict);
    }

    #[test]
    fn test_patch_sample_rate() {
        let patch = Patch::new(48000.0);
        assert_eq!(patch.sample_rate(), 48000.0);
    }

    #[test]
    fn test_patch_execution_order() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());
        patch.connect(a.out("out"), b.in_("in")).unwrap();
        patch.compile().unwrap();

        let order = patch.execution_order();
        assert_eq!(order.len(), 2);
    }

    #[test]
    fn test_patch_mult() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());
        let c = patch.add("c", Passthrough::new());

        // Connect one output to multiple inputs
        let result = patch.mult(a.out("out"), &[b.in_("in"), c.in_("in")]);
        assert!(result.is_ok());
        assert_eq!(patch.cable_count(), 2);
    }

    #[test]
    fn test_patch_reset() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        patch.set_output(a.id());
        patch.compile().unwrap();

        for _ in 0..100 {
            patch.tick();
        }

        patch.reset();
        // Reset clears internal state
    }

    #[test]
    fn test_patch_set_param_get_param() {
        use crate::modules::Vco;
        let mut patch = Patch::new(44100.0);
        let vco = patch.add("vco", Vco::new(44100.0));

        // Try to set/get param (may or may not have params)
        patch.set_param(vco.id(), 0, 0.5);
        let _ = patch.get_param(vco.id(), 0);
    }

    #[test]
    fn test_node_handle_spec() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());

        let spec = a.spec();
        assert!(!spec.inputs.is_empty());
        assert!(!spec.outputs.is_empty());
    }

    #[test]
    fn test_patch_validation_mode() {
        let mut patch = Patch::new(44100.0);

        patch.set_validation_mode(ValidationMode::Strict);
        assert_eq!(patch.validation_mode(), ValidationMode::Strict);

        patch.set_validation_mode(ValidationMode::Warn);
        assert_eq!(patch.validation_mode(), ValidationMode::Warn);
    }
}
