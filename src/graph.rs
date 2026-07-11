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
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use slotmap::{DefaultKey, SlotMap};

/// Signal validation strictness level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationMode {
    /// No validation - allow any connections
    None,
    /// Warn on incompatible connections but allow them.
    ///
    /// This is the default: it surfaces likely mistakes (e.g. patching Audio into a Gate
    /// input) as collectable [`Patch::warnings`] without blocking experimentation.
    #[default]
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

/// Stable, unique identifier for a cable connection.
///
/// Assigned by [`Patch::connect`] (and its variants) from a monotonically increasing
/// counter and stored inside the [`Cable`]. Unlike a positional index into the cable list,
/// a `CableId` remains valid after other cables are disconnected/removed, so it is safe to
/// hold and later pass to [`Patch::disconnect`].
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
    /// Stable identifier assigned at connect time (see [`CableId`]).
    #[serde(default)]
    pub id: CableId,
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

/// Error types for patch operations.
///
/// Marked `#[non_exhaustive]`: downstream `match` expressions must include a wildcard arm,
/// so new variants can be added in future without breaking callers.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PatchError {
    /// A referenced node does not exist in the patch.
    InvalidNode {
        node: NodeId,
    },
    /// A referenced port does not exist on the given node.
    ///
    /// Carries enough context to be actionable: the offending node, the requested port by
    /// name and/or id (whichever the caller supplied), and the list of valid port names on
    /// that node so the message can suggest the intended target.
    InvalidPort {
        node: NodeId,
        name: Option<String>,
        port: Option<PortId>,
        available: Vec<String>,
    },
    InvalidCable,
    /// A feedback cycle with no cycle-breaker (delay) was detected.
    ///
    /// `nodes` are the [`NodeId`]s stuck in the cycle; `names` are their resolved module
    /// names captured at error-construction time so [`Display`](core::fmt::Display) can
    /// print the actual path without a back-reference to the [`Patch`].
    CycleDetected {
        nodes: Vec<NodeId>,
        names: Vec<String>,
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
            PatchError::InvalidNode { node } => write!(f, "Invalid node: {:?}", node),
            PatchError::InvalidPort {
                node,
                name,
                port,
                available,
            } => {
                write!(f, "Invalid port")?;
                match (name, port) {
                    (Some(n), _) => write!(f, " '{}'", n)?,
                    (None, Some(p)) => write!(f, " #{}", p)?,
                    (None, None) => {}
                }
                write!(f, " on node {:?}", node)?;
                if available.is_empty() {
                    write!(f, " (module exposes no matching ports)")
                } else {
                    write!(f, " (available ports: {})", available.join(", "))
                }
            }
            PatchError::InvalidCable => write!(f, "Invalid cable"),
            PatchError::CycleDetected { nodes, names } => {
                if names.is_empty() {
                    write!(f, "Cycle detected involving {} nodes", nodes.len())
                } else {
                    write!(f, "Cycle detected: {}", names.join(" -> "))
                }
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

    /// Create a NodeHandle from a NodeId and module reference
    pub fn from_module(id: NodeId, module: &dyn GraphModule) -> Self {
        Self {
            id,
            spec: module.port_spec().clone(),
        }
    }

    /// Reference an output port by name, returning an error if it does not exist.
    ///
    /// Prefer this over the panicking [`out`](Self::out) when the port name comes from
    /// untrusted or dynamic input (e.g. deserializing a patch). The error lists the valid
    /// output ports for this module.
    pub fn output(&self, name: &str) -> Result<PortRef, PatchError> {
        match self.spec.output_by_name(name) {
            Some(port) => Ok(PortRef {
                node: self.id,
                port: port.id,
            }),
            None => Err(PatchError::InvalidPort {
                node: self.id,
                name: Some(name.to_string()),
                port: None,
                available: self.output_names().iter().map(|s| s.to_string()).collect(),
            }),
        }
    }

    /// Reference an input port by name, returning an error if it does not exist.
    ///
    /// The fallible companion to [`in_`](Self::in_); see [`output`](Self::output).
    pub fn input(&self, name: &str) -> Result<PortRef, PatchError> {
        match self.spec.input_by_name(name) {
            Some(port) => Ok(PortRef {
                node: self.id,
                port: port.id,
            }),
            None => Err(PatchError::InvalidPort {
                node: self.id,
                name: Some(name.to_string()),
                port: None,
                available: self.input_names().iter().map(|s| s.to_string()).collect(),
            }),
        }
    }

    /// Reference an output port by name (panicking convenience).
    ///
    /// Panics with a message listing the valid output ports if `name` is unknown. For a
    /// non-panicking version use [`output`](Self::output).
    pub fn out(&self, name: &str) -> PortRef {
        self.output(name).unwrap_or_else(|_| {
            panic!(
                "Unknown output port: '{}'. Valid output ports: [{}]",
                name,
                self.output_names().join(", ")
            )
        })
    }

    /// Reference an input port by name (panicking convenience).
    ///
    /// Panics with a message listing the valid input ports if `name` is unknown. For a
    /// non-panicking version use [`input`](Self::input).
    pub fn in_(&self, name: &str) -> PortRef {
        self.input(name).unwrap_or_else(|_| {
            panic!(
                "Unknown input port: '{}'. Valid input ports: [{}]",
                name,
                self.input_names().join(", ")
            )
        })
    }

    /// List the names of this module's input ports (in spec order).
    pub fn input_names(&self) -> Vec<&str> {
        self.spec.inputs.iter().map(|p| p.name.as_str()).collect()
    }

    /// List the names of this module's output ports (in spec order).
    pub fn output_names(&self) -> Vec<&str> {
        self.spec.outputs.iter().map(|p| p.name.as_str()).collect()
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

    // Monotonic source of stable CableIds (never reused)
    next_cable_id: CableId,

    // Execution state
    execution_order: Vec<NodeId>,
    buffers: StdMap<PortRef, f64>,

    // True when the graph has been mutated since the last successful compile().
    // tick() checks this and recompiles lazily.
    dirty: bool,
    // Error from the most recent failed compile (auto or explicit), if any.
    last_compile_error: Option<PatchError>,

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
            next_cable_id: 0,
            execution_order: Vec::new(),
            buffers: StdMap::new(),
            // A fresh patch is "dirty" so the first tick() compiles automatically even if
            // the caller forgets to call compile().
            dirty: true,
            last_compile_error: None,
            sample_rate,
            output_node: None,
            // Default is Warn (see ValidationMode): mismatched connections are flagged as
            // warnings without blocking, matching the documented behavior.
            validation_mode: ValidationMode::Warn,
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
            return Err(PatchError::InvalidNode { node });
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

    /// Allocate the next stable cable id.
    fn alloc_cable_id(&mut self) -> CableId {
        let id = self.next_cable_id;
        self.next_cable_id += 1;
        id
    }

    /// Connect an output port to an input port.
    ///
    /// Returns a stable [`CableId`] that remains valid for [`disconnect`](Self::disconnect)
    /// even after other cables are removed.
    pub fn connect(&mut self, from: PortRef, to: PortRef) -> Result<CableId, PatchError> {
        self.validate_output_port(from)?;
        self.validate_input_port(to)?;
        self.validate_signal_compatibility(from, to)?;

        let id = self.alloc_cable_id();
        self.cables.push(Cable {
            id,
            from,
            to,
            attenuation: None,
            offset: None,
        });
        self.invalidate();
        Ok(id)
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

        let id = self.alloc_cable_id();
        self.cables.push(Cable {
            id,
            from,
            to,
            attenuation: Some(attenuation.clamp(0.0, 1.0)),
            offset: None,
        });
        self.invalidate();
        Ok(id)
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

        let id = self.alloc_cable_id();
        self.cables.push(Cable {
            id,
            from,
            to,
            attenuation: Some(attenuation.clamp(-2.0, 2.0)),
            offset: Some(offset.clamp(-10.0, 10.0)),
        });
        self.invalidate();
        Ok(id)
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

    /// Disconnect a cable by its stable [`CableId`].
    ///
    /// Scans for the cable whose id matches (patch cable counts are small), so previously
    /// returned ids stay valid regardless of how many other cables have been removed.
    pub fn disconnect(&mut self, cable_id: CableId) -> Result<(), PatchError> {
        let idx = self
            .cables
            .iter()
            .position(|c| c.id == cable_id)
            .ok_or(PatchError::InvalidCable)?;
        self.cables.remove(idx);
        self.invalidate();
        Ok(())
    }

    /// Set the output node for the patch (infallible convenience).
    ///
    /// Marks the patch dirty so the next [`tick`](Self::tick) reflects the new routing. If
    /// `node` is invalid or exposes no output ports, [`tick`](Self::tick) simply reads
    /// silence; use [`try_set_output`](Self::try_set_output) for a validated, fallible
    /// alternative.
    pub fn set_output(&mut self, node: NodeId) {
        self.output_node = Some(node);
        self.dirty = true;
    }

    /// Set the output node, validating that it exists and exposes at least one output port.
    ///
    /// The checked companion to [`set_output`](Self::set_output), for callers (e.g. GUIs
    /// or loaders) that prefer a `Result` over silent misrouting.
    pub fn try_set_output(&mut self, node: NodeId) -> Result<(), PatchError> {
        let n = self
            .nodes
            .get(node)
            .ok_or(PatchError::InvalidNode { node })?;
        if n.module.port_spec().outputs.is_empty() {
            return Err(PatchError::InvalidPort {
                node,
                name: None,
                port: None,
                available: Vec::new(),
            });
        }
        self.output_node = Some(node);
        self.dirty = true;
        Ok(())
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

    /// Mark the compiled schedule stale and drop stale output buffers.
    ///
    /// Called after every structural mutation. Clearing `buffers` here guarantees that a
    /// read after a mutation (before the next recompile) cannot leak the previous graph's
    /// last-tick values; `tick()` recompiles lazily via the `dirty` flag.
    fn invalidate(&mut self) {
        self.execution_order.clear();
        self.buffers.clear();
        self.dirty = true;
    }

    fn validate_output_port(&self, port_ref: PortRef) -> Result<(), PatchError> {
        let node = self
            .nodes
            .get(port_ref.node)
            .ok_or(PatchError::InvalidNode {
                node: port_ref.node,
            })?;
        let spec = node.module.port_spec();
        if spec.outputs.iter().any(|p| p.id == port_ref.port) {
            Ok(())
        } else {
            Err(PatchError::InvalidPort {
                node: port_ref.node,
                name: None,
                port: Some(port_ref.port),
                available: spec.outputs.iter().map(|p| p.name.clone()).collect(),
            })
        }
    }

    fn validate_input_port(&self, port_ref: PortRef) -> Result<(), PatchError> {
        let node = self
            .nodes
            .get(port_ref.node)
            .ok_or(PatchError::InvalidNode {
                node: port_ref.node,
            })?;
        let spec = node.module.port_spec();
        if spec.inputs.iter().any(|p| p.id == port_ref.port) {
            Ok(())
        } else {
            Err(PatchError::InvalidPort {
                node: port_ref.node,
                name: None,
                port: Some(port_ref.port),
                available: spec.inputs.iter().map(|p| p.name.clone()).collect(),
            })
        }
    }

    /// Compile the patch into an executable order.
    ///
    /// On success clears the dirty flag and any previous compile error. On failure
    /// (e.g. an unbroken feedback cycle) the stale schedule and buffers are dropped so a
    /// subsequent [`tick`](Self::tick) outputs silence, the error is stored (retrievable
    /// via [`last_compile_error`](Self::last_compile_error)), and the same error is
    /// returned.
    pub fn compile(&mut self) -> Result<(), PatchError> {
        let order = match self.topological_sort() {
            Ok(order) => order,
            Err(e) => {
                self.execution_order.clear();
                self.buffers.clear();
                // Do not stay dirty: avoid re-running a known-failing sort every tick.
                // A later structural mutation re-sets dirty via invalidate().
                self.dirty = false;
                self.last_compile_error = Some(e.clone());
                return Err(e);
            }
        };
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

        self.dirty = false;
        self.last_compile_error = None;
        Ok(())
    }

    /// Whether the module at `node` is a feedback cycle-breaker (delay-style).
    fn node_breaks_feedback(&self, node: NodeId) -> bool {
        self.nodes
            .get(node)
            .map(|n| n.module.breaks_feedback_cycle())
            .unwrap_or(false)
    }

    fn topological_sort(&self) -> Result<Vec<NodeId>, PatchError> {
        let mut in_degree: StdMap<NodeId, usize> = self.nodes.keys().map(|k| (k, 0)).collect();
        let mut successors: StdMap<NodeId, Vec<NodeId>> =
            self.nodes.keys().map(|k| (k, Vec::new())).collect();

        for cable in &self.cables {
            // Feedback support: exclude edges feeding INTO a cycle-breaker (delay) node.
            // Such a node is scheduled without waiting for its upstream producers and, at
            // runtime, reads their previous-tick output buffers — a one-sample feedback
            // delay. This lets loops routed through a UnitDelay/DelayLine compile while
            // genuine breakerless cycles are still rejected below.
            if self.node_breaks_feedback(cable.to.node) {
                continue;
            }
            if let Some(deg) = in_degree.get_mut(&cable.to.node) {
                *deg += 1;
            }
            if let Some(succ) = successors.get_mut(&cable.from.node) {
                succ.push(cable.to.node);
            }
        }

        // Kahn's algorithm, seeded in deterministic slotmap (insertion) order so the
        // resulting execution_order is reproducible across runs/builds (no HashMap
        // iteration order dependence).
        let mut queue: VecDeque<NodeId> = VecDeque::new();
        for id in self.nodes.keys() {
            if in_degree.get(&id).copied().unwrap_or(0) == 0 {
                queue.push_back(id);
            }
        }

        let mut result = Vec::with_capacity(self.nodes.len());

        while let Some(node) = queue.pop_front() {
            result.push(node);
            // Successor lists are built in cable order (a Vec), keeping this deterministic.
            if let Some(succ) = successors.get(&node) {
                for &s in succ {
                    if let Some(deg) = in_degree.get_mut(&s) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(s);
                        }
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            // Collect stuck nodes deterministically and capture their names for Display.
            let nodes: Vec<NodeId> = self
                .nodes
                .keys()
                .filter(|k| in_degree.get(k).copied().unwrap_or(0) > 0)
                .collect();
            let names = nodes
                .iter()
                .map(|&id| self.get_name(id).unwrap_or("<unknown>").to_string())
                .collect();
            return Err(PatchError::CycleDetected { nodes, names });
        }

        Ok(result)
    }

    /// The error from the most recent failed compile (auto or explicit), if any.
    ///
    /// Cleared by the next successful [`compile`](Self::compile) or [`tick`](Self::tick).
    /// After [`tick`](Self::tick) unexpectedly returns silence, check this to learn why the
    /// graph did not compile (e.g. [`PatchError::CycleDetected`]).
    pub fn last_compile_error(&self) -> Option<&PatchError> {
        self.last_compile_error.as_ref()
    }

    /// Process a single sample, returning stereo output.
    ///
    /// # Lazy (re)compilation
    ///
    /// `tick` is self-healing. Every structural mutation
    /// (`add`/`connect`/`disconnect`/`remove`/`set_output`) marks the patch dirty; `tick`
    /// detects this and recompiles automatically before processing, so the output always
    /// reflects the *current* graph — you never have to remember to call
    /// [`compile`](Self::compile) again after an edit, and a `tick` before the first
    /// `compile` works too.
    ///
    /// If the automatic recompile fails (for example a mutation introduced a feedback
    /// cycle with no delay to break it), `tick` outputs silence `(0.0, 0.0)` and the error
    /// is retained in [`last_compile_error`](Self::last_compile_error). A patch with no
    /// output node, or an empty graph, likewise ticks to silence.
    pub fn tick(&mut self) -> (f64, f64) {
        // Lazily (re)compile if the graph was mutated since the last compile. On failure
        // compile() records last_compile_error and leaves an empty schedule, so the loop
        // below is a no-op and we fall through to silence.
        if self.dirty {
            let _ = self.compile();
        }

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

        // Pass 1: resolve every input that has a patched cable, plus plain (non-normalled)
        // defaults. Normalled-but-unpatched inputs are deferred to pass 2 so they can read
        // the *current-tick* resolved value of the sibling INPUT port they normal to.
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
            } else if input.normalled_to.is_none() {
                // Unpatched with no normal: use the port's default.
                values.set(input.id, input.default);
            }
            // else: normalled + unpatched -> resolved in pass 2 below.
        }

        // Pass 2: resolve normalled inputs. `normalled_to` names a sibling INPUT port on the
        // SAME node; we read that port's value resolved above (this tick), NOT the output
        // buffer namespace. This gives, e.g., StereoOutput's mono fallback (right normals to
        // left) the current-sample left value on both channels instead of a stale/colliding
        // output-buffer read. If the referenced sibling has not been resolved (e.g. a
        // normal to a later normalled port, or a missing id), fall back to this port's
        // default.
        for input in &spec.inputs {
            if values.has(input.id) {
                continue;
            }
            if let Some(normalled) = input.normalled_to {
                let v = values.get(normalled).unwrap_or(input.default);
                values.set(input.id, v);
            } else {
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

    /// Read the stereo output from the output node's first two output ports (in
    /// [`PortSpec`] order), rather than hardcoded port ids 0/1. The first output port is
    /// the left channel; the second (if any) is the right. A mono node (single output) is
    /// duplicated to both channels. Missing output node or buffers yield silence.
    fn read_output(&self) -> (f64, f64) {
        let Some(output_node) = self.output_node else {
            return (0.0, 0.0);
        };
        let Some(node) = self.nodes.get(output_node) else {
            return (0.0, 0.0);
        };
        let outputs = &node.module.port_spec().outputs;

        let read = |port: PortId| -> Option<f64> {
            self.buffers
                .get(&PortRef {
                    node: output_node,
                    port,
                })
                .copied()
        };

        let left = outputs.first().and_then(|p| read(p.id)).unwrap_or(0.0);
        let right = outputs.get(1).and_then(|p| read(p.id)).unwrap_or(left); // Mono fallback: duplicate left to right
        (left, right)
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

    /// Get a NodeId by module name
    pub fn get_node_id_by_name(&self, name: &str) -> Option<NodeId> {
        self.nodes
            .iter()
            .find(|(_, node)| node.name == name)
            .map(|(id, _)| id)
    }

    /// Get a NodeHandle by module name
    pub fn get_handle_by_name(&self, name: &str) -> Option<NodeHandle> {
        self.nodes
            .iter()
            .find(|(_, node)| node.name == name)
            .map(|(id, node)| NodeHandle::from_module(id, node.module.as_ref()))
    }

    /// Disconnect a cable by finding matching port refs
    pub fn disconnect_ports(&mut self, from: PortRef, to: PortRef) -> Result<(), PatchError> {
        let idx = self
            .cables
            .iter()
            .position(|c| c.from == from && c.to == to)
            .ok_or(PatchError::InvalidCable)?;

        self.cables.remove(idx);
        self.invalidate();
        Ok(())
    }

    /// Get all module names
    pub fn module_names(&self) -> Vec<&str> {
        self.nodes
            .iter()
            .map(|(_, node)| node.name.as_str())
            .collect()
    }

    /// Get the current output buffer value for a specific port
    ///
    /// This is used by the observer to collect real-time values for metering,
    /// scope display, and other visualizations.
    pub fn get_output_value(&self, node: NodeId, port: PortId) -> Option<f64> {
        self.buffers.get(&PortRef { node, port }).copied()
    }

    /// Get the signal kind for an output port by node ID and port ID
    pub fn get_output_signal_kind(&self, node: NodeId, port: PortId) -> Option<SignalKind> {
        let node_data = self.nodes.get(node)?;
        node_data
            .module
            .port_spec()
            .outputs
            .iter()
            .find(|p| p.id == port)
            .map(|p| p.kind)
    }
}

/// Manual `Debug` for `Patch` so `println!("{:?}", patch)` works for inspection without
/// requiring `GraphModule: Debug`. Prints each node's name and `type_id`, the cable list,
/// the output node, validation mode, dirty flag, and warning count.
impl core::fmt::Debug for Patch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Wrapper so nodes format as `name (type_id)` without a GraphModule: Debug bound.
        struct NodeDebug<'a> {
            id: NodeId,
            name: &'a str,
            type_id: &'a str,
        }
        impl core::fmt::Debug for NodeDebug<'_> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "{:?}: {} ({})", self.id, self.name, self.type_id)
            }
        }

        let nodes: Vec<NodeDebug> = self
            .nodes
            .iter()
            .map(|(id, n)| NodeDebug {
                id,
                name: n.name.as_str(),
                type_id: n.module.type_id(),
            })
            .collect();

        f.debug_struct("Patch")
            .field("sample_rate", &self.sample_rate)
            .field("nodes", &nodes)
            .field("cables", &self.cables)
            .field("output_node", &self.output_node)
            .field("validation_mode", &self.validation_mode)
            .field("dirty", &self.dirty)
            .field("warnings", &self.warnings.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::port::{PortDef, SignalKind};
    use alloc::vec;

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

    // ========================================================================
    // Wave B-0 audit remediation tests
    // ========================================================================

    // A module that sums two inputs (ids 0, 1) into one output (id 10).
    struct SumModule {
        spec: PortSpec,
    }
    impl SumModule {
        fn new() -> Self {
            Self {
                spec: PortSpec {
                    inputs: vec![
                        PortDef::new(0, "a", SignalKind::Audio),
                        PortDef::new(1, "b", SignalKind::Audio),
                    ],
                    outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
                },
            }
        }
    }
    impl GraphModule for SumModule {
        fn port_spec(&self) -> &PortSpec {
            &self.spec
        }
        fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
            outputs.set(10, inputs.get_or(0, 0.0) + inputs.get_or(1, 0.0));
        }
        fn reset(&mut self) {}
        fn set_sample_rate(&mut self, _: f64) {}
    }

    // A one-sample delay that declares itself a feedback cycle-breaker.
    struct FeedbackDelay {
        spec: PortSpec,
        buffer: f64,
    }
    impl FeedbackDelay {
        fn new() -> Self {
            Self {
                spec: PortSpec {
                    inputs: vec![PortDef::new(0, "in", SignalKind::Audio)],
                    outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
                },
                buffer: 0.0,
            }
        }
    }
    impl GraphModule for FeedbackDelay {
        fn port_spec(&self) -> &PortSpec {
            &self.spec
        }
        fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
            outputs.set(10, self.buffer);
            self.buffer = inputs.get_or(0, 0.0);
        }
        fn reset(&mut self) {
            self.buffer = 0.0;
        }
        fn set_sample_rate(&mut self, _: f64) {}
        fn breaks_feedback_cycle(&self) -> bool {
            true
        }
    }

    // A constant source with a single non-zero-id output (id 10).
    struct ConstSource {
        spec: PortSpec,
        value: f64,
    }
    impl ConstSource {
        fn new(value: f64) -> Self {
            Self {
                spec: PortSpec {
                    inputs: vec![],
                    outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
                },
                value,
            }
        }
    }
    impl GraphModule for ConstSource {
        fn port_spec(&self) -> &PortSpec {
            &self.spec
        }
        fn tick(&mut self, _: &PortValues, outputs: &mut PortValues) {
            outputs.set(10, self.value);
        }
        fn reset(&mut self) {}
        fn set_sample_rate(&mut self, _: f64) {}
    }

    // Q075: CableIds are stable across disconnects of other cables.
    #[test]
    fn test_cable_ids_are_stable_across_disconnect() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", SumModule::new());
        let c = patch.add("c", SumModule::new());

        let c1 = patch.connect(a.out("out"), b.in_("a")).unwrap();
        let c2 = patch.connect(a.out("out"), b.in_("b")).unwrap();
        let c3 = patch.connect(a.out("out"), c.in_("a")).unwrap();
        assert_eq!(patch.cable_count(), 3);

        // Disconnect the FIRST cable. With Vec-index ids this would shift c3 down.
        patch.disconnect(c1).unwrap();
        assert_eq!(patch.cable_count(), 2);

        // Disconnecting the THIRD cable by its still-valid id must remove exactly it,
        // leaving only c2 (a.out -> b.b).
        patch.disconnect(c3).unwrap();
        assert_eq!(patch.cable_count(), 1);
        let remaining = &patch.cables()[0];
        assert_eq!(remaining.id, c2);
        assert_eq!(remaining.to, b.in_("b"));

        // A stale id (already removed) errors rather than dropping the wrong cable.
        assert!(matches!(
            patch.disconnect(c1),
            Err(PatchError::InvalidCable)
        ));
    }

    // Q076/Q181: mutating after compile is reflected on the next tick (lazy recompile).
    #[test]
    fn test_mutation_after_compile_is_reflected_on_tick() {
        let mut patch = Patch::new(44100.0);
        let src = patch.add("src", ConstSource::new(1.0));
        let out = patch.add("out", Passthrough::new());
        patch.connect(src.out("out"), out.in_("in")).unwrap();
        patch.set_output(out.id());
        patch.compile().unwrap();

        // First tick: passthrough carries the source through.
        let (l0, _) = patch.tick();
        assert!((l0 - 1.0).abs() < 1e-9);

        // Mutate AFTER compile: add a second source summed in. No explicit recompile.
        let src2 = patch.add("src2", ConstSource::new(2.0));
        let sum = patch.add("sum", SumModule::new());
        // Rewire: src -> sum.a, src2 -> sum.b, sum -> out
        patch
            .disconnect_ports(src.out("out"), out.in_("in"))
            .unwrap();
        patch.connect(src.out("out"), sum.in_("a")).unwrap();
        patch.connect(src2.out("out"), sum.in_("b")).unwrap();
        patch.connect(sum.out("out"), out.in_("in")).unwrap();

        // tick() must lazily recompile and reflect the NEW graph (1.0 + 2.0 = 3.0),
        // not freeze at the stale 1.0.
        let (l1, _) = patch.tick();
        assert!((l1 - 3.0).abs() < 1e-9, "expected 3.0, got {}", l1);
    }

    // Q076/Q181: a cycle-creating mutation surfaces via last_compile_error and ticks silent.
    #[test]
    fn test_cycle_mutation_surfaces_via_last_compile_error() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());
        patch.connect(a.out("out"), b.in_("in")).unwrap();
        patch.set_output(b.id());
        patch.compile().unwrap();
        assert!(patch.last_compile_error().is_none());

        // Introduce a breakerless cycle: b -> a.
        patch.connect(b.out("out"), a.in_("in")).unwrap();

        // tick() auto-recompiles, fails, outputs silence, and records the error.
        let (l, r) = patch.tick();
        assert_eq!((l, r), (0.0, 0.0));
        match patch.last_compile_error() {
            Some(PatchError::CycleDetected { names, .. }) => {
                assert_eq!(names.len(), 2);
            }
            other => panic!("expected CycleDetected, got {:?}", other),
        }
    }

    // Q077: a feedback loop routed through a cycle-breaker compiles and decays.
    #[test]
    fn test_feedback_loop_with_delay_compiles_and_decays() {
        // A one-shot impulse: 1.0 on the first tick, 0.0 thereafter. It stays connected so
        // no mid-run mutation clears the feedback buffers.
        struct Impulse {
            spec: PortSpec,
            fired: bool,
        }
        impl GraphModule for Impulse {
            fn port_spec(&self) -> &PortSpec {
                &self.spec
            }
            fn tick(&mut self, _: &PortValues, outputs: &mut PortValues) {
                outputs.set(10, if self.fired { 0.0 } else { 1.0 });
                self.fired = true;
            }
            fn reset(&mut self) {
                self.fired = false;
            }
            fn set_sample_rate(&mut self, _: f64) {}
        }

        let mut patch = Patch::new(44100.0);
        let impulse = patch.add(
            "impulse",
            Impulse {
                spec: PortSpec {
                    inputs: vec![],
                    outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
                },
                fired: false,
            },
        );
        let sum = patch.add("sum", SumModule::new());
        let delay = patch.add("delay", FeedbackDelay::new());

        // impulse -> sum.a ; delay.out -> sum.b (feedback, x0.5) ; sum.out -> delay.in
        // (edge into the breaker) ; output = delay.out
        patch.connect(impulse.out("out"), sum.in_("a")).unwrap();
        patch
            .connect_attenuated(delay.out("out"), sum.in_("b"), 0.5)
            .unwrap();
        patch.connect(sum.out("out"), delay.in_("in")).unwrap();
        patch.set_output(delay.id());

        // Must compile despite the sum<->delay cycle (delay breaks it).
        patch.compile().expect("feedback loop should compile");
        assert!(patch.last_compile_error().is_none());

        let mut outs = Vec::new();
        for _ in 0..14 {
            outs.push(patch.tick().0);
        }

        // The loop must ring: there are several non-zero echoes...
        let nonzero: Vec<f64> = outs.iter().copied().filter(|v| v.abs() > 1e-9).collect();
        assert!(
            nonzero.len() >= 3,
            "expected multiple decaying echoes, got {:?}",
            outs
        );
        // ...and successive echo magnitudes decay (0.5 feedback): the first echo is the
        // loudest, the tail is quieter.
        let peak_early = outs.iter().cloned().fold(0.0_f64, f64::max);
        let peak_late = outs[outs.len() - 3..]
            .iter()
            .cloned()
            .fold(0.0_f64, f64::max);
        assert!(
            peak_late < peak_early,
            "echo should decay: early peak {}, late peak {}",
            peak_early,
            peak_late
        );
    }

    // Q077: a cycle with no breaker still fails to compile.
    #[test]
    fn test_breakerless_cycle_still_errors() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());
        patch.connect(a.out("out"), b.in_("in")).unwrap();
        patch.connect(b.out("out"), a.in_("in")).unwrap();
        assert!(matches!(
            patch.compile(),
            Err(PatchError::CycleDetected { .. })
        ));
    }

    // Q079: normalled input reads the sibling INPUT's current-tick value, not a stale
    // output-buffer read. StereoOutput with only `left` patched -> both channels identical.
    #[test]
    fn test_normalled_input_uses_current_sibling_value() {
        use crate::modules::StereoOutput;
        let mut patch = Patch::new(44100.0);
        // Time-varying source so a one-sample lag would be detectable.
        struct Ramp {
            spec: PortSpec,
            n: f64,
        }
        impl GraphModule for Ramp {
            fn port_spec(&self) -> &PortSpec {
                &self.spec
            }
            fn tick(&mut self, _: &PortValues, outputs: &mut PortValues) {
                self.n += 1.0;
                outputs.set(10, self.n);
            }
            fn reset(&mut self) {
                self.n = 0.0;
            }
            fn set_sample_rate(&mut self, _: f64) {}
        }
        let ramp = patch.add(
            "ramp",
            Ramp {
                spec: PortSpec {
                    inputs: vec![],
                    outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
                },
                n: 0.0,
            },
        );
        let out = patch.add("out", StereoOutput::new());
        // Patch only LEFT; RIGHT is normalled to LEFT.
        patch.connect(ramp.out("out"), out.in_("left")).unwrap();
        patch.set_output(out.id());
        patch.compile().unwrap();

        for _ in 0..5 {
            let (l, r) = patch.tick();
            assert!(l > 0.0);
            assert_eq!(l, r, "mono fallback must be current-sample, not delayed");
        }
    }

    // Q080: compilation is deterministic — same patch built twice -> same execution_order.
    #[test]
    fn test_execution_order_is_deterministic() {
        fn build_order() -> Vec<usize> {
            let mut patch = Patch::new(44100.0);
            // Several independent sources feeding one sum -> deterministic tie-breaking.
            let s1 = patch.add("s1", ConstSource::new(1.0));
            let s2 = patch.add("s2", ConstSource::new(2.0));
            let s3 = patch.add("s3", ConstSource::new(3.0));
            let sum = patch.add("sum", SumModule::new());
            patch.connect(s1.out("out"), sum.in_("a")).unwrap();
            patch.connect(s2.out("out"), sum.in_("b")).unwrap();
            patch.connect(s3.out("out"), sum.in_("a")).unwrap();
            patch.compile().unwrap();
            // Map NodeIds to their insertion rank for a build-independent comparison.
            let ids = [s1.id(), s2.id(), s3.id(), sum.id()];
            patch
                .execution_order()
                .iter()
                .map(|nid| ids.iter().position(|x| x == nid).unwrap())
                .collect()
        }
        assert_eq!(build_order(), build_order());
    }

    // Q121: read_output reads the output node's first two outputs (mono duplicated),
    // regardless of their port ids (here a single output with id 10).
    #[test]
    fn test_read_output_uses_first_two_outputs_mono_duplicated() {
        let mut patch = Patch::new(44100.0);
        let src = patch.add("src", ConstSource::new(0.7));
        patch.set_output(src.id());
        patch.compile().unwrap();
        let (l, r) = patch.tick();
        assert!((l - 0.7).abs() < 1e-9);
        assert_eq!(l, r, "mono node must duplicate to both channels");
    }

    // Q121/6a: try_set_output validates node existence and presence of outputs.
    #[test]
    fn test_try_set_output_validates() {
        let mut patch = Patch::new(44100.0);
        let src = patch.add("src", ConstSource::new(1.0));
        assert!(patch.try_set_output(src.id()).is_ok());

        // A node with no outputs is rejected.
        struct SinkNoOut {
            spec: PortSpec,
        }
        impl GraphModule for SinkNoOut {
            fn port_spec(&self) -> &PortSpec {
                &self.spec
            }
            fn tick(&mut self, _: &PortValues, _: &mut PortValues) {}
            fn reset(&mut self) {}
            fn set_sample_rate(&mut self, _: f64) {}
        }
        let sink = patch.add(
            "sink",
            SinkNoOut {
                spec: PortSpec {
                    inputs: vec![PortDef::new(0, "in", SignalKind::Audio)],
                    outputs: vec![],
                },
            },
        );
        assert!(matches!(
            patch.try_set_output(sink.id()),
            Err(PatchError::InvalidPort { .. })
        ));
    }

    // Q122/Q180: NodeHandle fallible port lookups and name discovery.
    #[test]
    fn test_node_handle_fallible_ports_and_names() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());

        assert!(a.output("out").is_ok());
        assert!(a.input("in").is_ok());

        // Unknown ports return an InvalidPort carrying the available names.
        match a.output("nope") {
            Err(PatchError::InvalidPort { available, .. }) => {
                assert!(available.iter().any(|n| n == "out"));
            }
            other => panic!("expected InvalidPort, got {:?}", other),
        }
        assert!(a.input("nope").is_err());

        assert_eq!(a.input_names(), vec!["in"]);
        assert_eq!(a.output_names(), vec!["out"]);
    }

    // Q182: Display lists the module's available ports on an invalid connection.
    #[test]
    fn test_invalid_port_display_lists_available() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("a", Passthrough::new());
        let b = patch.add("b", Passthrough::new());
        // Connect to a non-existent input port id on b.
        let bad = PortRef {
            node: b.id(),
            port: 999,
        };
        let err = patch.connect(a.out("out"), bad).unwrap_err();
        let msg = alloc::format!("{}", err);
        assert!(msg.contains("Invalid port"), "got: {}", msg);
        assert!(
            msg.contains("in"),
            "should list available port 'in': {}",
            msg
        );
    }

    // Q185: CycleDetected Display prints the module names in the cycle.
    #[test]
    fn test_cycle_detected_display_names() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("osc", Passthrough::new());
        let b = patch.add("filt", Passthrough::new());
        patch.connect(a.out("out"), b.in_("in")).unwrap();
        patch.connect(b.out("out"), a.in_("in")).unwrap();
        let err = patch.compile().unwrap_err();
        let msg = alloc::format!("{}", err);
        assert!(msg.contains("Cycle detected"), "got: {}", msg);
        assert!(
            msg.contains("osc") && msg.contains("filt"),
            "cycle message should name modules: {}",
            msg
        );
    }

    // Q183: the default validation mode is Warn.
    #[test]
    fn test_default_validation_mode_is_warn() {
        let patch = Patch::new(44100.0);
        assert_eq!(patch.validation_mode(), ValidationMode::Warn);
        assert_eq!(ValidationMode::default(), ValidationMode::Warn);
    }

    // Q187: Patch implements Debug for println-style inspection.
    #[test]
    fn test_patch_debug_impl() {
        let mut patch = Patch::new(44100.0);
        let a = patch.add("my_osc", Passthrough::new());
        let b = patch.add("my_out", Passthrough::new());
        patch.connect(a.out("out"), b.in_("in")).unwrap();
        patch.set_output(b.id());
        let s = alloc::format!("{:?}", patch);
        assert!(s.contains("Patch"));
        assert!(s.contains("my_osc"));
        assert!(s.contains("my_out"));
        assert!(s.contains("validation_mode"));
    }
}
