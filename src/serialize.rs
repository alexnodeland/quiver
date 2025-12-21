//! Serialization and Persistence
//!
//! This module provides types and utilities for saving and loading patches,
//! including module registry and patch definitions.

use crate::analog::{AnalogVco, Saturator, Wavefolder};
use crate::graph::{NodeHandle, Patch, PatchError};
use crate::modules::*;
use crate::port::{GraphModule, PortSpec};
use crate::StdMap;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Serializable patch definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchDef {
    /// Schema version for forward compatibility
    pub version: u32,

    /// Patch metadata
    pub name: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,

    /// Module instances
    pub modules: Vec<ModuleDef>,

    /// Cable connections
    pub cables: Vec<CableDef>,

    /// Parameter values (key: "module_name.param_id")
    pub parameters: StdMap<String, f64>,
}

impl PatchDef {
    /// Create a new empty patch definition
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: 1,
            name: name.into(),
            author: None,
            description: None,
            tags: vec![],
            modules: vec![],
            cables: vec![],
            parameters: StdMap::new(),
        }
    }

    /// Set the author
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Set the description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add a tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl Default for PatchDef {
    fn default() -> Self {
        Self::new("Untitled")
    }
}

/// Serializable module definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDef {
    /// Unique instance name
    pub name: String,

    /// Module type identifier
    pub module_type: String,

    /// UI position (optional)
    pub position: Option<(f32, f32)>,

    /// Module-specific state
    pub state: Option<serde_json::Value>,
}

impl ModuleDef {
    pub fn new(name: impl Into<String>, module_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            module_type: module_type.into(),
            position: None,
            state: None,
        }
    }

    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.position = Some((x, y));
        self
    }
}

/// Serializable cable definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CableDef {
    /// Source: "module_name.port_name"
    pub from: String,

    /// Destination: "module_name.port_name"
    pub to: String,

    /// Optional attenuation/gain (-2.0 to 2.0)
    pub attenuation: Option<f64>,

    /// Optional DC offset (-10.0 to 10.0V)
    pub offset: Option<f64>,
}

impl CableDef {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            attenuation: None,
            offset: None,
        }
    }

    pub fn with_attenuation(mut self, attenuation: f64) -> Self {
        self.attenuation = Some(attenuation);
        self
    }

    pub fn with_offset(mut self, offset: f64) -> Self {
        self.offset = Some(offset);
        self
    }

    pub fn with_modulation(mut self, attenuation: f64, offset: f64) -> Self {
        self.attenuation = Some(attenuation);
        self.offset = Some(offset);
        self
    }
}

/// Module factory function type
pub type ModuleFactory = Box<dyn Fn(f64) -> Box<dyn GraphModule> + Send + Sync>;

/// Metadata about a registered module type
#[derive(Debug, Clone)]
pub struct ModuleMetadata {
    pub type_id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub port_spec: PortSpec,
}

/// Registry of available module types for instantiation
pub struct ModuleRegistry {
    factories: StdMap<String, ModuleFactory>,
    metadata: StdMap<String, ModuleMetadata>,
}

impl ModuleRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        let mut registry = Self {
            factories: StdMap::new(),
            metadata: StdMap::new(),
        };

        // Register built-in modules
        registry.register_builtin();
        registry
    }

    fn register_builtin(&mut self) {
        // Oscillators
        self.register_factory(
            "vco",
            "VCO",
            "Oscillators",
            "Voltage-controlled oscillator with multiple waveforms",
            |sr| Box::new(Vco::new(sr)),
        );

        self.register_factory(
            "analog_vco",
            "Analog VCO",
            "Oscillators",
            "VCO with analog modeling (drift, saturation)",
            |sr| Box::new(AnalogVco::new(sr)),
        );

        self.register_factory(
            "lfo",
            "LFO",
            "Modulation",
            "Low-frequency oscillator for modulation",
            |sr| Box::new(Lfo::new(sr)),
        );

        // Filters
        self.register_factory(
            "svf",
            "SVF",
            "Filters",
            "State-variable filter with LP/BP/HP/Notch outputs",
            |sr| Box::new(Svf::new(sr)),
        );

        // Envelopes
        self.register_factory(
            "adsr",
            "ADSR",
            "Envelopes",
            "Attack-Decay-Sustain-Release envelope generator",
            |sr| Box::new(Adsr::new(sr)),
        );

        // Amplifiers
        self.register_factory(
            "vca",
            "VCA",
            "Utilities",
            "Voltage-controlled amplifier",
            |_| Box::new(Vca::new()),
        );

        // Utilities
        self.register_factory(
            "mixer",
            "Mixer",
            "Utilities",
            "4-channel audio mixer",
            |_| Box::new(Mixer::new(4)),
        );

        self.register_factory(
            "offset",
            "Offset",
            "Utilities",
            "DC offset / voltage source",
            |_| Box::new(Offset::new(0.0)),
        );

        self.register_factory(
            "unit_delay",
            "Unit Delay",
            "Utilities",
            "Single-sample delay for feedback",
            |_| Box::new(UnitDelay::new()),
        );

        // Sources
        self.register_factory(
            "noise",
            "Noise",
            "Sources",
            "White and pink noise generator",
            |_| Box::new(NoiseGenerator::new()),
        );

        // Sequencing
        self.register_factory(
            "step_sequencer",
            "Step Sequencer",
            "Sequencing",
            "8-step CV/gate sequencer",
            |_| Box::new(StepSequencer::new()),
        );

        // Output
        self.register_factory(
            "stereo_output",
            "Stereo Output",
            "I/O",
            "Final stereo audio output",
            |_| Box::new(StereoOutput::new()),
        );

        // Analog modeling
        self.register_factory(
            "saturator",
            "Saturator",
            "Effects",
            "Soft saturation / overdrive",
            |_| Box::new(Saturator::default()),
        );

        self.register_factory(
            "wavefolder",
            "Wavefolder",
            "Effects",
            "Wavefolder for complex harmonics",
            |_| Box::new(Wavefolder::default()),
        );

        // Utility modules
        self.register_factory(
            "sample_and_hold",
            "Sample & Hold",
            "Utilities",
            "Sample input value on trigger",
            |_| Box::new(SampleAndHold::new()),
        );

        self.register_factory(
            "slew_limiter",
            "Slew Limiter",
            "Utilities",
            "Limits rate of change (portamento/glide)",
            |sr| Box::new(SlewLimiter::new(sr)),
        );

        self.register_factory(
            "quantizer",
            "Quantizer",
            "Utilities",
            "Quantize V/Oct to musical scales",
            |_| Box::new(Quantizer::new(Scale::Chromatic)),
        );

        self.register_factory(
            "clock",
            "Clock",
            "Sequencing",
            "Master clock with tempo control",
            |sr| Box::new(Clock::new(sr)),
        );

        self.register_factory(
            "attenuverter",
            "Attenuverter",
            "Utilities",
            "Attenuate, invert, and offset signals",
            |_| Box::new(Attenuverter::new()),
        );

        self.register_factory(
            "multiple",
            "Multiple",
            "Utilities",
            "Signal splitter (1 input to 4 outputs)",
            |_| Box::new(Multiple::new()),
        );

        // Phase 2 Modules

        self.register_factory(
            "ring_mod",
            "Ring Modulator",
            "Effects",
            "Multiplies two signals for metallic/bell sounds",
            |_| Box::new(RingModulator::new()),
        );

        self.register_factory(
            "crossfader",
            "Crossfader/Panner",
            "Utilities",
            "Crossfade between inputs or pan stereo",
            |_| Box::new(Crossfader::new()),
        );

        self.register_factory(
            "logic_and",
            "Logic AND",
            "Logic",
            "Output high when both inputs are high",
            |_| Box::new(LogicAnd::new()),
        );

        self.register_factory(
            "logic_or",
            "Logic OR",
            "Logic",
            "Output high when either input is high",
            |_| Box::new(LogicOr::new()),
        );

        self.register_factory(
            "logic_xor",
            "Logic XOR",
            "Logic",
            "Output high when exactly one input is high",
            |_| Box::new(LogicXor::new()),
        );

        self.register_factory(
            "logic_not",
            "Logic NOT",
            "Logic",
            "Invert gate signal",
            |_| Box::new(LogicNot::new()),
        );

        self.register_factory(
            "comparator",
            "Comparator",
            "Logic",
            "Compare two CVs, output gates for greater/less/equal",
            |_| Box::new(Comparator::new()),
        );

        self.register_factory(
            "rectifier",
            "Rectifier",
            "Effects",
            "Full-wave and half-wave rectification",
            |_| Box::new(Rectifier::new()),
        );

        self.register_factory(
            "precision_adder",
            "Precision Adder",
            "Utilities",
            "High-precision CV adder for V/Oct signals",
            |_| Box::new(PrecisionAdder::new()),
        );

        self.register_factory(
            "vc_switch",
            "VC Switch",
            "Utilities",
            "Voltage-controlled signal router",
            |_| Box::new(VcSwitch::new()),
        );

        self.register_factory(
            "bernoulli_gate",
            "Bernoulli Gate",
            "Random",
            "Probabilistic trigger router",
            |_| Box::new(BernoulliGate::new()),
        );

        self.register_factory(
            "min",
            "Min",
            "Utilities",
            "Output minimum of two signals",
            |_| Box::new(Min::new()),
        );

        self.register_factory(
            "max",
            "Max",
            "Utilities",
            "Output maximum of two signals",
            |_| Box::new(Max::new()),
        );

        // Phase 3 Modules

        self.register_factory(
            "diode_ladder",
            "Diode Ladder Filter",
            "Filters",
            "24dB/oct ladder filter with diode saturation",
            |sr| Box::new(DiodeLadderFilter::new(sr)),
        );

        self.register_factory(
            "crosstalk",
            "Crosstalk",
            "Analog Modeling",
            "Channel crosstalk simulation",
            |sr| Box::new(Crosstalk::new(sr)),
        );

        self.register_factory(
            "ground_loop",
            "Ground Loop",
            "Analog Modeling",
            "Ground loop hum simulation (50/60 Hz)",
            |sr| Box::new(GroundLoop::new(sr)),
        );
    }

    /// Register a module factory with metadata
    pub fn register_factory<F>(
        &mut self,
        type_id: &str,
        name: &str,
        category: &str,
        description: &str,
        factory: F,
    ) where
        F: Fn(f64) -> Box<dyn GraphModule> + Send + Sync + 'static,
    {
        // Get port spec from a temporary instance
        let temp_instance = factory(44100.0);
        let port_spec = temp_instance.port_spec().clone();

        self.factories
            .insert(type_id.to_string(), Box::new(factory));

        self.metadata.insert(
            type_id.to_string(),
            ModuleMetadata {
                type_id: type_id.to_string(),
                name: name.to_string(),
                category: category.to_string(),
                description: description.to_string(),
                port_spec,
            },
        );
    }

    /// Instantiate a module by type ID
    pub fn instantiate(&self, type_id: &str, sample_rate: f64) -> Option<Box<dyn GraphModule>> {
        self.factories.get(type_id).map(|f| f(sample_rate))
    }

    /// List all registered module types
    pub fn list_modules(&self) -> impl Iterator<Item = &ModuleMetadata> {
        self.metadata.values()
    }

    /// Get metadata for a specific module type
    pub fn get_metadata(&self, type_id: &str) -> Option<&ModuleMetadata> {
        self.metadata.get(type_id)
    }

    /// List modules in a specific category
    pub fn list_by_category<'a>(
        &'a self,
        category: &'a str,
    ) -> impl Iterator<Item = &'a ModuleMetadata> {
        self.metadata
            .values()
            .filter(move |m| m.category == category)
    }

    /// Get all unique categories
    pub fn categories(&self) -> Vec<String> {
        let mut cats: Vec<_> = self.metadata.values().map(|m| m.category.clone()).collect();
        cats.sort();
        cats.dedup();
        cats
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension methods for Patch to support serialization
impl Patch {
    /// Convert patch to a serializable definition
    pub fn to_def(&self, name: &str) -> PatchDef {
        let modules: Vec<ModuleDef> = self
            .nodes()
            .map(|(node_id, node_name, module)| ModuleDef {
                name: node_name.to_string(),
                module_type: module.type_id().to_string(),
                position: self.get_position(node_id),
                state: module.serialize_state(),
            })
            .collect();

        let cables: Vec<CableDef> = self
            .cables()
            .iter()
            .filter_map(|cable| {
                // Find node names and port names
                let from_name = self.get_name(cable.from.node)?;
                let to_name = self.get_name(cable.to.node)?;

                // Find port names from the modules
                let (_, _, from_module) = self.nodes().find(|(id, _, _)| *id == cable.from.node)?;
                let (_, _, to_module) = self.nodes().find(|(id, _, _)| *id == cable.to.node)?;

                let from_port = from_module
                    .port_spec()
                    .outputs
                    .iter()
                    .find(|p| p.id == cable.from.port)
                    .map(|p| p.name.as_str())?;

                let to_port = to_module
                    .port_spec()
                    .inputs
                    .iter()
                    .find(|p| p.id == cable.to.port)
                    .map(|p| p.name.as_str())?;

                Some(CableDef {
                    from: format!("{}.{}", from_name, from_port),
                    to: format!("{}.{}", to_name, to_port),
                    attenuation: cable.attenuation,
                    offset: cable.offset,
                })
            })
            .collect();

        PatchDef {
            version: 1,
            name: name.to_string(),
            author: None,
            description: None,
            tags: vec![],
            modules,
            cables,
            parameters: StdMap::new(),
        }
    }

    /// Load a patch from a definition
    pub fn from_def(
        def: &PatchDef,
        registry: &ModuleRegistry,
        sample_rate: f64,
    ) -> Result<Self, PatchError> {
        let mut patch = Patch::new(sample_rate);
        let mut name_to_handle: StdMap<String, NodeHandle> = StdMap::new();

        // Instantiate modules
        for module_def in &def.modules {
            let module = registry
                .instantiate(&module_def.module_type, sample_rate)
                .ok_or_else(|| {
                    PatchError::CompilationFailed(format!(
                        "Unknown module type: {}",
                        module_def.module_type
                    ))
                })?;

            let handle = patch.add_boxed(&module_def.name, module);

            // Set position if available
            if let Some((x, y)) = module_def.position {
                patch.set_position(handle.id(), (x, y));
            }

            name_to_handle.insert(module_def.name.clone(), handle);
        }

        // Create cables
        for cable_def in &def.cables {
            let (from_module, from_port) = parse_port_ref(&cable_def.from)?;
            let (to_module, to_port) = parse_port_ref(&cable_def.to)?;

            let from_handle = name_to_handle.get(from_module).ok_or_else(|| {
                PatchError::CompilationFailed(format!("Unknown module: {}", from_module))
            })?;

            let to_handle = name_to_handle.get(to_module).ok_or_else(|| {
                PatchError::CompilationFailed(format!("Unknown module: {}", to_module))
            })?;

            match (cable_def.attenuation, cable_def.offset) {
                (Some(attenuation), Some(offset)) => {
                    patch.connect_modulated(
                        from_handle.out(from_port),
                        to_handle.in_(to_port),
                        attenuation,
                        offset,
                    )?;
                }
                (Some(attenuation), None) => {
                    patch.connect_attenuated(
                        from_handle.out(from_port),
                        to_handle.in_(to_port),
                        attenuation,
                    )?;
                }
                (None, Some(offset)) => {
                    patch.connect_modulated(
                        from_handle.out(from_port),
                        to_handle.in_(to_port),
                        1.0, // Unity gain
                        offset,
                    )?;
                }
                (None, None) => {
                    patch.connect(from_handle.out(from_port), to_handle.in_(to_port))?;
                }
            }
        }

        // Find and set output node (look for stereo_output)
        if let Some(handle) = name_to_handle.get("output") {
            patch.set_output(handle.id());
        } else if let Some(handle) = name_to_handle.values().find(|h| {
            h.spec()
                .outputs
                .iter()
                .any(|p| p.name == "left" || p.name == "right")
        }) {
            patch.set_output(handle.id());
        }

        patch.compile()?;
        Ok(patch)
    }
}

fn parse_port_ref(s: &str) -> Result<(&str, &str), PatchError> {
    let parts: Vec<&str> = s.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err(PatchError::CompilationFailed(format!(
            "Invalid port reference: {}",
            s
        )));
    }
    Ok((parts[0], parts[1]))
}

// =============================================================================
// Patch Validation
// =============================================================================

/// A validation error with path and message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// JSON path to the error location (e.g., "modules[0].name")
    pub path: String,
    /// Human-readable error message
    pub message: String,
}

impl ValidationError {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl core::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

/// Result of patch validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether the patch is valid
    pub valid: bool,
    /// List of validation errors (empty if valid)
    pub errors: Vec<ValidationError>,
}

impl ValidationResult {
    pub fn ok() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
        }
    }

    pub fn with_errors(errors: Vec<ValidationError>) -> Self {
        Self {
            valid: errors.is_empty(),
            errors,
        }
    }
}

impl PatchDef {
    /// Validate the patch definition without loading it
    ///
    /// This performs structural validation to catch errors early before
    /// attempting to instantiate modules. For full semantic validation
    /// (e.g., checking that port names exist), use `validate_with_registry`.
    pub fn validate(&self) -> ValidationResult {
        let mut errors = Vec::new();

        // Validate version
        if self.version < 1 {
            errors.push(ValidationError::new(
                "version",
                "Version must be a positive integer",
            ));
        }

        // Validate name
        if self.name.is_empty() {
            errors.push(ValidationError::new("name", "Name must be a non-empty string"));
        }

        // Collect module names for duplicate checking
        let mut module_names = alloc::collections::BTreeSet::new();

        // Validate modules
        for (i, module) in self.modules.iter().enumerate() {
            let path = format!("modules[{}]", i);

            if module.name.is_empty() {
                errors.push(ValidationError::new(
                    format!("{}.name", path),
                    "Module name must be a non-empty string",
                ));
            } else if !module_names.insert(&module.name) {
                errors.push(ValidationError::new(
                    format!("{}.name", path),
                    format!("Duplicate module name: {}", module.name),
                ));
            }

            if module.module_type.is_empty() {
                errors.push(ValidationError::new(
                    format!("{}.module_type", path),
                    "Module type must be a non-empty string",
                ));
            }
        }

        // Validate cables
        for (i, cable) in self.cables.iter().enumerate() {
            let path = format!("cables[{}]", i);

            // Validate port reference format
            if !is_valid_port_ref(&cable.from) {
                errors.push(ValidationError::new(
                    format!("{}.from", path),
                    "From must be a port reference in format 'module_name.port_name'",
                ));
            }

            if !is_valid_port_ref(&cable.to) {
                errors.push(ValidationError::new(
                    format!("{}.to", path),
                    "To must be a port reference in format 'module_name.port_name'",
                ));
            }

            // Validate attenuation range
            if let Some(attenuation) = cable.attenuation {
                if !(-2.0..=2.0).contains(&attenuation) {
                    errors.push(ValidationError::new(
                        format!("{}.attenuation", path),
                        "Attenuation must be between -2.0 and 2.0",
                    ));
                }
            }

            // Validate offset range
            if let Some(offset) = cable.offset {
                if !(-10.0..=10.0).contains(&offset) {
                    errors.push(ValidationError::new(
                        format!("{}.offset", path),
                        "Offset must be between -10.0 and 10.0",
                    ));
                }
            }
        }

        ValidationResult::with_errors(errors)
    }

    /// Validate the patch definition with registry context
    ///
    /// This performs full semantic validation including checking that:
    /// - All module types exist in the registry
    /// - All port references point to existing modules
    /// - All port names exist on their respective modules
    pub fn validate_with_registry(&self, registry: &ModuleRegistry) -> ValidationResult {
        // First do structural validation
        let mut result = self.validate();
        if !result.valid {
            return result;
        }

        let mut errors = Vec::new();

        // Collect module names for reference checking
        let module_names: alloc::collections::BTreeSet<_> =
            self.modules.iter().map(|m| m.name.as_str()).collect();

        // Validate module types exist
        for (i, module) in self.modules.iter().enumerate() {
            if registry.get_metadata(&module.module_type).is_none() {
                errors.push(ValidationError::new(
                    format!("modules[{}].module_type", i),
                    format!("Unknown module type: {}", module.module_type),
                ));
            }
        }

        // Validate cable references
        for (i, cable) in self.cables.iter().enumerate() {
            let path = format!("cables[{}]", i);

            // Check source module exists
            if let Ok((from_module, from_port)) = parse_port_ref(&cable.from) {
                if !module_names.contains(from_module) {
                    errors.push(ValidationError::new(
                        format!("{}.from", path),
                        format!("Unknown module: {}", from_module),
                    ));
                } else {
                    // Check source port exists
                    if let Some(module_def) = self.modules.iter().find(|m| m.name == from_module) {
                        if let Some(metadata) = registry.get_metadata(&module_def.module_type) {
                            if metadata.port_spec.output_by_name(from_port).is_none() {
                                errors.push(ValidationError::new(
                                    format!("{}.from", path),
                                    format!(
                                        "Unknown output port '{}' on module '{}'",
                                        from_port, from_module
                                    ),
                                ));
                            }
                        }
                    }
                }
            }

            // Check destination module exists
            if let Ok((to_module, to_port)) = parse_port_ref(&cable.to) {
                if !module_names.contains(to_module) {
                    errors.push(ValidationError::new(
                        format!("{}.to", path),
                        format!("Unknown module: {}", to_module),
                    ));
                } else {
                    // Check destination port exists
                    if let Some(module_def) = self.modules.iter().find(|m| m.name == to_module) {
                        if let Some(metadata) = registry.get_metadata(&module_def.module_type) {
                            if metadata.port_spec.input_by_name(to_port).is_none() {
                                errors.push(ValidationError::new(
                                    format!("{}.to", path),
                                    format!(
                                        "Unknown input port '{}' on module '{}'",
                                        to_port, to_module
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }

        if errors.is_empty() {
            result
        } else {
            result.valid = false;
            result.errors.extend(errors);
            result
        }
    }
}

/// Check if a string is a valid port reference (module.port format)
fn is_valid_port_ref(s: &str) -> bool {
    let parts: Vec<&str> = s.splitn(2, '.').collect();
    if parts.len() != 2 {
        return false;
    }

    // Check that both parts are non-empty and contain valid characters
    let valid_chars = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    };

    valid_chars(parts[0]) && valid_chars(parts[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_def_serialization() {
        let def = PatchDef::new("Test Patch")
            .with_author("Test Author")
            .with_description("A test patch")
            .with_tag("test");

        let json = def.to_json().unwrap();
        let loaded = PatchDef::from_json(&json).unwrap();

        assert_eq!(loaded.name, "Test Patch");
        assert_eq!(loaded.author, Some("Test Author".to_string()));
    }

    #[test]
    fn test_cable_def() {
        let cable = CableDef::new("vco.saw", "vcf.in").with_attenuation(0.5);
        assert_eq!(cable.from, "vco.saw");
        assert_eq!(cable.to, "vcf.in");
        assert_eq!(cable.attenuation, Some(0.5));
    }

    #[test]
    fn test_patch_def_default() {
        let def = PatchDef::default();
        assert_eq!(def.name, "Untitled");
    }

    #[test]
    fn test_module_def_with_position() {
        let def = ModuleDef::new("vco1", "vco").with_position(100.0, 200.0);
        assert_eq!(def.position, Some((100.0, 200.0)));
    }

    #[test]
    fn test_cable_def_with_offset() {
        let cable = CableDef::new("a.out", "b.in").with_offset(2.5);
        assert_eq!(cable.offset, Some(2.5));
    }

    #[test]
    fn test_cable_def_with_modulation() {
        let cable = CableDef::new("a.out", "b.in").with_modulation(0.5, 1.0);
        assert_eq!(cable.attenuation, Some(0.5));
        assert_eq!(cable.offset, Some(1.0));
    }

    // =============================================================================
    // Validation Tests
    // =============================================================================

    #[test]
    fn test_valid_patch_validation() {
        let mut def = PatchDef::new("Test Patch");
        def.modules.push(ModuleDef::new("vco1", "vco"));
        def.modules.push(ModuleDef::new("output", "stereo_output"));
        def.cables.push(CableDef::new("vco1.saw", "output.left"));

        let result = def.validate();
        assert!(result.valid, "Expected valid patch, got errors: {:?}", result.errors);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_empty_name_validation() {
        let mut def = PatchDef::new("");
        def.modules.push(ModuleDef::new("vco1", "vco"));

        let result = def.validate();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "name"));
    }

    #[test]
    fn test_duplicate_module_name_validation() {
        let mut def = PatchDef::new("Test");
        def.modules.push(ModuleDef::new("vco1", "vco"));
        def.modules.push(ModuleDef::new("vco1", "vco")); // Duplicate!

        let result = def.validate();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.message.contains("Duplicate")));
    }

    #[test]
    fn test_invalid_port_reference_validation() {
        let mut def = PatchDef::new("Test");
        def.modules.push(ModuleDef::new("vco1", "vco"));
        def.cables.push(CableDef::new("invalid", "also_invalid")); // Missing dots

        let result = def.validate();
        assert!(!result.valid);
        assert!(result.errors.len() >= 2);
    }

    #[test]
    fn test_attenuation_range_validation() {
        let mut def = PatchDef::new("Test");
        def.modules.push(ModuleDef::new("vco1", "vco"));
        def.cables.push(CableDef::new("a.out", "b.in").with_attenuation(5.0)); // Out of range

        let result = def.validate();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path.contains("attenuation")));
    }

    #[test]
    fn test_offset_range_validation() {
        let mut def = PatchDef::new("Test");
        def.modules.push(ModuleDef::new("vco1", "vco"));
        def.cables.push(CableDef::new("a.out", "b.in").with_offset(15.0)); // Out of range

        let result = def.validate();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path.contains("offset")));
    }

    #[test]
    fn test_validate_with_registry_unknown_module_type() {
        let registry = ModuleRegistry::new();

        let mut def = PatchDef::new("Test");
        def.modules.push(ModuleDef::new("foo", "nonexistent_type"));

        let result = def.validate_with_registry(&registry);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.message.contains("Unknown module type")));
    }

    #[test]
    fn test_validate_with_registry_unknown_module_reference() {
        let registry = ModuleRegistry::new();

        let mut def = PatchDef::new("Test");
        def.modules.push(ModuleDef::new("vco1", "vco"));
        def.cables.push(CableDef::new("nonexistent.out", "vco1.voct"));

        let result = def.validate_with_registry(&registry);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.message.contains("Unknown module")));
    }

    #[test]
    fn test_validate_with_registry_unknown_port() {
        let registry = ModuleRegistry::new();

        let mut def = PatchDef::new("Test");
        def.modules.push(ModuleDef::new("vco1", "vco"));
        def.modules.push(ModuleDef::new("output", "stereo_output"));
        def.cables.push(CableDef::new("vco1.nonexistent_port", "output.left"));

        let result = def.validate_with_registry(&registry);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.message.contains("Unknown output port")));
    }

    #[test]
    fn test_validate_with_registry_valid_patch() {
        let registry = ModuleRegistry::new();

        let mut def = PatchDef::new("Valid Patch");
        def.modules.push(ModuleDef::new("vco1", "vco"));
        def.modules.push(ModuleDef::new("output", "stereo_output"));
        def.cables.push(CableDef::new("vco1.saw", "output.left"));
        def.cables.push(CableDef::new("vco1.sin", "output.right"));

        let result = def.validate_with_registry(&registry);
        assert!(result.valid, "Expected valid patch, got errors: {:?}", result.errors);
    }

    #[test]
    fn test_is_valid_port_ref() {
        assert!(is_valid_port_ref("vco1.out"));
        assert!(is_valid_port_ref("module_name.port_name"));
        assert!(is_valid_port_ref("a.b"));
        assert!(is_valid_port_ref("my-module.my-port"));

        assert!(!is_valid_port_ref("nodot"));
        assert!(!is_valid_port_ref(".startswithdot"));
        assert!(!is_valid_port_ref("endswithdot."));
        assert!(!is_valid_port_ref(""));
        assert!(!is_valid_port_ref("has spaces.port"));
    }

    #[test]
    fn test_validation_error_display() {
        let error = ValidationError::new("modules[0].name", "Name is empty");
        let display = format!("{}", error);
        assert_eq!(display, "modules[0].name: Name is empty");
    }
}
