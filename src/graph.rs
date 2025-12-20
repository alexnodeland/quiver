//! Layer 3: Patch Graph
//!
//! This module provides the runtime graph-based patching system that allows
//! arbitrary signal routing between modules. It handles topological sorting,
//! execution ordering, and signal propagation.

use crate::port::{GraphModule, ParamId, PortId, PortSpec, PortValues};
use serde::{Deserialize, Serialize};
use slotmap::{DefaultKey, SlotMap};
use std::collections::{HashMap, VecDeque};

/// Unique identifier for a node in the patch graph
pub type NodeId = DefaultKey;

/// Unique identifier for a cable connection
pub type CableId = usize;

/// Reference to a specific port on a specific node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortRef {
    pub node: NodeId,
    pub port: PortId,
}

/// A cable connecting two ports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cable {
    pub from: PortRef,
    pub to: PortRef,
    /// Optional attenuation (0.0–1.0)
    pub attenuation: Option<f64>,
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
    CycleDetected { nodes: Vec<NodeId> },
    CompilationFailed(String),
}

impl std::fmt::Display for PatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatchError::InvalidNode => write!(f, "Invalid node"),
            PatchError::InvalidPort => write!(f, "Invalid port"),
            PatchError::InvalidCable => write!(f, "Invalid cable"),
            PatchError::CycleDetected { nodes } => {
                write!(f, "Cycle detected involving {} nodes", nodes.len())
            }
            PatchError::CompilationFailed(msg) => write!(f, "Compilation failed: {}", msg),
        }
    }
}

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
    buffers: HashMap<PortRef, f64>,

    // Configuration
    sample_rate: f64,

    // Output node
    output_node: Option<NodeId>,
}

impl Patch {
    /// Create a new empty patch
    pub fn new(sample_rate: f64) -> Self {
        Self {
            nodes: SlotMap::new(),
            cables: Vec::new(),
            execution_order: Vec::new(),
            buffers: HashMap::new(),
            sample_rate,
            output_node: None,
        }
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

        let cable = Cable {
            from,
            to,
            attenuation: None,
        };
        self.cables.push(cable);
        self.invalidate();
        Ok(self.cables.len() - 1)
    }

    /// Connect with attenuation
    pub fn connect_attenuated(
        &mut self,
        from: PortRef,
        to: PortRef,
        attenuation: f64,
    ) -> Result<CableId, PatchError> {
        self.validate_output_port(from)?;
        self.validate_input_port(to)?;

        let cable = Cable {
            from,
            to,
            attenuation: Some(attenuation.clamp(0.0, 1.0)),
        };
        self.cables.push(cable);
        self.invalidate();
        Ok(self.cables.len() - 1)
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
        let node = self.nodes.get(port_ref.node).ok_or(PatchError::InvalidNode)?;
        node.module
            .port_spec()
            .outputs
            .iter()
            .find(|p| p.id == port_ref.port)
            .ok_or(PatchError::InvalidPort)?;
        Ok(())
    }

    fn validate_input_port(&self, port_ref: PortRef) -> Result<(), PatchError> {
        let node = self.nodes.get(port_ref.node).ok_or(PatchError::InvalidNode)?;
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
        let mut in_degree: HashMap<NodeId, usize> =
            self.nodes.keys().map(|k| (k, 0)).collect();
        let mut successors: HashMap<NodeId, Vec<NodeId>> =
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
                    let attenuated = cable.attenuation.map(|a| value * a).unwrap_or(value);
                    sum += attenuated;
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
}
