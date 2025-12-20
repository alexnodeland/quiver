//! Serialization and Persistence
//!
//! This module provides types and utilities for saving and loading patches,
//! including module registry and patch definitions.

use crate::analog::{AnalogVco, Saturator, Wavefolder};
use crate::graph::{NodeHandle, Patch, PatchError};
use crate::modules::*;
use crate::port::{GraphModule, PortSpec};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub parameters: HashMap<String, f64>,
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
            parameters: HashMap::new(),
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
    factories: HashMap<String, ModuleFactory>,
    metadata: HashMap<String, ModuleMetadata>,
}

impl ModuleRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        let mut registry = Self {
            factories: HashMap::new(),
            metadata: HashMap::new(),
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
            .map(|(_, node_name, module)| {
                ModuleDef {
                    name: node_name.to_string(),
                    module_type: module.type_id().to_string(),
                    position: None, // TODO: store positions
                    state: module.serialize_state(),
                }
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
            parameters: HashMap::new(),
        }
    }

    /// Load a patch from a definition
    pub fn from_def(
        def: &PatchDef,
        registry: &ModuleRegistry,
        sample_rate: f64,
    ) -> Result<Self, PatchError> {
        let mut patch = Patch::new(sample_rate);
        let mut name_to_handle: HashMap<String, NodeHandle> = HashMap::new();

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
    fn test_patch_roundtrip() {
        let registry = ModuleRegistry::new();

        // Create a simple patch
        let mut patch = Patch::new(44100.0);
        let vco = patch.add("vco", Vco::new(44100.0));
        let vcf = patch.add("vcf", Svf::new(44100.0));
        let output = patch.add("output", StereoOutput::new());

        patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
        patch.connect(vcf.out("lp"), output.in_("left")).unwrap();
        patch.set_output(output.id());
        patch.compile().unwrap();

        // Serialize
        let def = patch.to_def("Test");
        let json = def.to_json().unwrap();

        // Deserialize
        let loaded_def = PatchDef::from_json(&json).unwrap();
        let loaded_patch = Patch::from_def(&loaded_def, &registry, 44100.0).unwrap();

        // Verify
        assert_eq!(loaded_patch.node_count(), 3);
        assert_eq!(loaded_patch.cable_count(), 2);
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

    #[test]
    fn test_module_registry_default() {
        let registry = ModuleRegistry::default();
        assert!(registry.list_modules().count() > 0);
    }

    #[test]
    fn test_module_registry_list_by_category() {
        let registry = ModuleRegistry::new();
        let oscillators: Vec<_> = registry.list_by_category("Oscillators").collect();
        assert!(!oscillators.is_empty());
    }

    #[test]
    fn test_module_registry_instantiate_all() {
        let registry = ModuleRegistry::new();
        for meta in registry.list_modules() {
            let instance = registry.instantiate(&meta.type_id, 44100.0);
            assert!(
                instance.is_some(),
                "Failed to instantiate: {}",
                meta.type_id
            );
        }
    }

    #[test]
    fn test_patch_from_def_unknown_module() {
        let registry = ModuleRegistry::new();
        let def = PatchDef {
            version: 1,
            name: "Test".to_string(),
            author: None,
            description: None,
            tags: vec![],
            modules: vec![ModuleDef::new("unknown", "nonexistent_module")],
            cables: vec![],
            parameters: HashMap::new(),
        };

        let result = Patch::from_def(&def, &registry, 44100.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_patch_from_def_with_offset_only() {
        let registry = ModuleRegistry::new();
        let def = PatchDef {
            version: 1,
            name: "Test".to_string(),
            author: None,
            description: None,
            tags: vec![],
            modules: vec![
                ModuleDef::new("vco", "vco"),
                ModuleDef::new("output", "stereo_output"),
            ],
            cables: vec![CableDef {
                from: "vco.saw".to_string(),
                to: "output.left".to_string(),
                attenuation: None,
                offset: Some(0.5),
            }],
            parameters: HashMap::new(),
        };

        let result = Patch::from_def(&def, &registry, 44100.0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_patch_from_def_with_attenuated_connection() {
        let registry = ModuleRegistry::new();
        let def = PatchDef {
            version: 1,
            name: "Test".to_string(),
            author: None,
            description: None,
            tags: vec![],
            modules: vec![
                ModuleDef::new("vco", "vco"),
                ModuleDef::new("output", "stereo_output"),
            ],
            cables: vec![CableDef {
                from: "vco.saw".to_string(),
                to: "output.left".to_string(),
                attenuation: Some(0.5),
                offset: None,
            }],
            parameters: HashMap::new(),
        };

        let result = Patch::from_def(&def, &registry, 44100.0);
        assert!(result.is_ok());
    }
}
