//! Module Development Kit (MDK)
//!
//! This module provides tools for developing new Quiver modules:
//! - Template generator for creating new module boilerplate
//! - Testing harness for validating module behavior
//! - Documentation generator for module documentation

use crate::port::SignalKind;

/// Module category for template generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCategory {
    /// Oscillator modules (VCO, LFO, noise sources)
    Oscillator,
    /// Filter modules (VCF, EQ, waveshaper)
    Filter,
    /// Envelope and modulation sources (ADSR, LFO, S&H)
    Modulation,
    /// Utility modules (mixer, attenuator, logic)
    Utility,
    /// Effect modules (delay, reverb, distortion)
    Effect,
    /// Input/Output modules (audio I/O, MIDI, CV)
    InputOutput,
}

impl ModuleCategory {
    /// Returns typical input ports for this category
    pub fn typical_inputs(&self) -> Vec<PortTemplate> {
        match self {
            ModuleCategory::Oscillator => vec![
                PortTemplate::new("voct", SignalKind::VoltPerOctave, 0.0),
                PortTemplate::new("fm", SignalKind::CvBipolar, 0.0).with_attenuverter(),
                PortTemplate::new("sync", SignalKind::Gate, 0.0),
            ],
            ModuleCategory::Filter => vec![
                PortTemplate::new("in", SignalKind::Audio, 0.0),
                PortTemplate::new("cutoff", SignalKind::CvUnipolar, 0.5).with_attenuverter(),
                PortTemplate::new("resonance", SignalKind::CvUnipolar, 0.0).with_attenuverter(),
            ],
            ModuleCategory::Modulation => vec![
                PortTemplate::new("rate", SignalKind::CvUnipolar, 0.5).with_attenuverter(),
                PortTemplate::new("depth", SignalKind::CvUnipolar, 1.0),
                PortTemplate::new("trigger", SignalKind::Trigger, 0.0),
            ],
            ModuleCategory::Utility => vec![
                PortTemplate::new("in", SignalKind::Audio, 0.0),
                PortTemplate::new("cv", SignalKind::CvBipolar, 0.0).with_attenuverter(),
            ],
            ModuleCategory::Effect => vec![
                PortTemplate::new("in", SignalKind::Audio, 0.0),
                PortTemplate::new("mix", SignalKind::CvUnipolar, 0.5).with_attenuverter(),
                PortTemplate::new("param", SignalKind::CvUnipolar, 0.5).with_attenuverter(),
            ],
            ModuleCategory::InputOutput => vec![
                PortTemplate::new("in", SignalKind::Audio, 0.0),
            ],
        }
    }

    /// Returns typical output ports for this category
    pub fn typical_outputs(&self) -> Vec<PortTemplate> {
        match self {
            ModuleCategory::Oscillator => vec![
                PortTemplate::new("sin", SignalKind::Audio, 0.0),
                PortTemplate::new("saw", SignalKind::Audio, 0.0),
                PortTemplate::new("sqr", SignalKind::Audio, 0.0),
                PortTemplate::new("tri", SignalKind::Audio, 0.0),
            ],
            ModuleCategory::Filter => vec![
                PortTemplate::new("lp", SignalKind::Audio, 0.0),
                PortTemplate::new("bp", SignalKind::Audio, 0.0),
                PortTemplate::new("hp", SignalKind::Audio, 0.0),
            ],
            ModuleCategory::Modulation => vec![
                PortTemplate::new("out", SignalKind::CvBipolar, 0.0),
                PortTemplate::new("gate", SignalKind::Gate, 0.0),
            ],
            ModuleCategory::Utility => vec![
                PortTemplate::new("out", SignalKind::Audio, 0.0),
            ],
            ModuleCategory::Effect => vec![
                PortTemplate::new("out", SignalKind::Audio, 0.0),
            ],
            ModuleCategory::InputOutput => vec![
                PortTemplate::new("left", SignalKind::Audio, 0.0),
                PortTemplate::new("right", SignalKind::Audio, 0.0),
            ],
        }
    }
}

/// Port template for module generation
#[derive(Debug, Clone)]
pub struct PortTemplate {
    /// Port name
    pub name: String,
    /// Signal kind
    pub kind: SignalKind,
    /// Default value
    pub default: f64,
    /// Whether this port has an attenuverter
    pub has_attenuverter: bool,
    /// Normalled connection target (port name)
    pub normalled_to: Option<String>,
}

impl PortTemplate {
    pub fn new(name: impl Into<String>, kind: SignalKind, default: f64) -> Self {
        Self {
            name: name.into(),
            kind,
            default,
            has_attenuverter: false,
            normalled_to: None,
        }
    }

    pub fn with_attenuverter(mut self) -> Self {
        self.has_attenuverter = true;
        self
    }

    pub fn normalled_to(mut self, target: impl Into<String>) -> Self {
        self.normalled_to = Some(target.into());
        self
    }
}

/// State field template for module generation
#[derive(Debug, Clone)]
pub struct StateFieldTemplate {
    /// Field name
    pub name: String,
    /// Rust type (e.g., "f64", "bool", "usize")
    pub field_type: String,
    /// Initial value expression
    pub initial_value: String,
    /// Description for documentation
    pub description: String,
}

impl StateFieldTemplate {
    pub fn new(
        name: impl Into<String>,
        field_type: impl Into<String>,
        initial_value: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            field_type: field_type.into(),
            initial_value: initial_value.into(),
            description: String::new(),
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
}

/// Module template for code generation
#[derive(Debug, Clone)]
pub struct ModuleTemplate {
    /// Module name (PascalCase)
    pub name: String,
    /// Module type_id (snake_case)
    pub type_id: String,
    /// Module category
    pub category: ModuleCategory,
    /// Documentation string
    pub doc: String,
    /// Input ports
    pub inputs: Vec<PortTemplate>,
    /// Output ports
    pub outputs: Vec<PortTemplate>,
    /// State fields
    pub state_fields: Vec<StateFieldTemplate>,
    /// Whether this module needs sample_rate
    pub needs_sample_rate: bool,
}

impl ModuleTemplate {
    /// Create a new module template with defaults for the given category
    pub fn new(name: impl Into<String>, category: ModuleCategory) -> Self {
        let name = name.into();
        let type_id = to_snake_case(&name);
        Self {
            name,
            type_id,
            category,
            doc: String::new(),
            inputs: category.typical_inputs(),
            outputs: category.typical_outputs(),
            state_fields: Vec::new(),
            needs_sample_rate: matches!(
                category,
                ModuleCategory::Oscillator | ModuleCategory::Filter | ModuleCategory::Effect
            ),
        }
    }

    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = doc.into();
        self
    }

    pub fn with_type_id(mut self, type_id: impl Into<String>) -> Self {
        self.type_id = type_id.into();
        self
    }

    pub fn with_inputs(mut self, inputs: Vec<PortTemplate>) -> Self {
        self.inputs = inputs;
        self
    }

    pub fn with_outputs(mut self, outputs: Vec<PortTemplate>) -> Self {
        self.outputs = outputs;
        self
    }

    pub fn add_input(mut self, port: PortTemplate) -> Self {
        self.inputs.push(port);
        self
    }

    pub fn add_output(mut self, port: PortTemplate) -> Self {
        self.outputs.push(port);
        self
    }

    pub fn add_state_field(mut self, field: StateFieldTemplate) -> Self {
        self.state_fields.push(field);
        self
    }

    pub fn with_sample_rate(mut self, needs: bool) -> Self {
        self.needs_sample_rate = needs;
        self
    }

    /// Generate Rust source code for the module
    pub fn generate_code(&self) -> String {
        let mut code = String::new();

        // Module documentation
        if !self.doc.is_empty() {
            code.push_str(&format!("/// {}\n", self.doc));
        } else {
            code.push_str(&format!("/// {} module\n", self.name));
        }
        code.push_str("///\n");
        code.push_str("/// # Inputs\n");
        for input in &self.inputs {
            code.push_str(&format!(
                "/// - `{}`: {:?}{}\n",
                input.name,
                input.kind,
                if input.has_attenuverter {
                    " (with attenuverter)"
                } else {
                    ""
                }
            ));
        }
        code.push_str("///\n");
        code.push_str("/// # Outputs\n");
        for output in &self.outputs {
            code.push_str(&format!("/// - `{}`: {:?}\n", output.name, output.kind));
        }

        // Struct definition
        code.push_str(&format!("pub struct {} {{\n", self.name));

        // State fields
        for field in &self.state_fields {
            if !field.description.is_empty() {
                code.push_str(&format!("    /// {}\n", field.description));
            }
            code.push_str(&format!("    {}: {},\n", field.name, field.field_type));
        }

        // Sample rate field (if needed)
        if self.needs_sample_rate {
            code.push_str("    sample_rate: f64,\n");
        }

        // Port spec field
        code.push_str("    spec: PortSpec,\n");
        code.push_str("}\n\n");

        // Constructor
        code.push_str(&format!("impl {} {{\n", self.name));
        if self.needs_sample_rate {
            code.push_str("    pub fn new(sample_rate: f64) -> Self {\n");
        } else {
            code.push_str("    pub fn new() -> Self {\n");
        }
        code.push_str("        Self {\n");

        // Initialize state fields
        for field in &self.state_fields {
            code.push_str(&format!("            {}: {},\n", field.name, field.initial_value));
        }

        if self.needs_sample_rate {
            code.push_str("            sample_rate,\n");
        }

        // Initialize port spec
        code.push_str("            spec: PortSpec {\n");

        // Inputs
        code.push_str("                inputs: vec![\n");
        for (i, input) in self.inputs.iter().enumerate() {
            let mut port_def = format!(
                "                    PortDef::new({}, \"{}\", SignalKind::{:?})",
                i, input.name, input.kind
            );
            if input.default != 0.0 {
                port_def.push_str(&format!(".with_default({:.1})", input.default));
            }
            if input.has_attenuverter {
                port_def.push_str(".with_attenuverter()");
            }
            port_def.push_str(",\n");
            code.push_str(&port_def);
        }
        code.push_str("                ],\n");

        // Outputs (IDs start at 10 to leave room for inputs)
        code.push_str("                outputs: vec![\n");
        for (i, output) in self.outputs.iter().enumerate() {
            code.push_str(&format!(
                "                    PortDef::new({}, \"{}\", SignalKind::{:?}),\n",
                10 + i,
                output.name,
                output.kind
            ));
        }
        code.push_str("                ],\n");
        code.push_str("            },\n");
        code.push_str("        }\n");
        code.push_str("    }\n");
        code.push_str("}\n\n");

        // Default impl (if no sample_rate needed)
        if !self.needs_sample_rate {
            code.push_str(&format!("impl Default for {} {{\n", self.name));
            code.push_str("    fn default() -> Self {\n");
            code.push_str("        Self::new()\n");
            code.push_str("    }\n");
            code.push_str("}\n\n");
        } else {
            code.push_str(&format!("impl Default for {} {{\n", self.name));
            code.push_str("    fn default() -> Self {\n");
            code.push_str("        Self::new(44100.0)\n");
            code.push_str("    }\n");
            code.push_str("}\n\n");
        }

        // GraphModule impl
        code.push_str(&format!("impl GraphModule for {} {{\n", self.name));

        // port_spec
        code.push_str("    fn port_spec(&self) -> &PortSpec {\n");
        code.push_str("        &self.spec\n");
        code.push_str("    }\n\n");

        // tick
        code.push_str("    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {\n");
        code.push_str("        // Read inputs\n");
        for (i, input) in self.inputs.iter().enumerate() {
            let var_name = input.name.replace('-', "_");
            code.push_str(&format!(
                "        let {} = inputs.get_or({}, {:.1});\n",
                var_name, i, input.default
            ));
        }
        code.push_str("\n");
        code.push_str("        // TODO: Implement processing logic\n");
        code.push_str("\n");
        code.push_str("        // Write outputs\n");
        for (i, output) in self.outputs.iter().enumerate() {
            code.push_str(&format!(
                "        outputs.set({}, 0.0); // {}\n",
                10 + i,
                output.name
            ));
        }
        code.push_str("    }\n\n");

        // reset
        code.push_str("    fn reset(&mut self) {\n");
        for field in &self.state_fields {
            code.push_str(&format!("        self.{} = {};\n", field.name, field.initial_value));
        }
        if self.state_fields.is_empty() {
            code.push_str("        // Reset internal state\n");
        }
        code.push_str("    }\n\n");

        // set_sample_rate
        code.push_str("    fn set_sample_rate(&mut self, sample_rate: f64) {\n");
        if self.needs_sample_rate {
            code.push_str("        self.sample_rate = sample_rate;\n");
        } else {
            code.push_str("        let _ = sample_rate;\n");
        }
        code.push_str("    }\n\n");

        // type_id
        code.push_str("    fn type_id(&self) -> &'static str {\n");
        code.push_str(&format!("        \"{}\"\n", self.type_id));
        code.push_str("    }\n");

        code.push_str("}\n");

        code
    }

    /// Generate a minimal module with just the essential boilerplate
    pub fn generate_minimal(&self) -> String {
        let mut code = String::new();

        code.push_str(&format!("/// {} module\n", self.name));
        code.push_str(&format!("pub struct {} {{\n", self.name));
        if self.needs_sample_rate {
            code.push_str("    sample_rate: f64,\n");
        }
        code.push_str("    spec: PortSpec,\n");
        code.push_str("}\n\n");

        code.push_str(&format!("impl {} {{\n", self.name));
        if self.needs_sample_rate {
            code.push_str("    pub fn new(sample_rate: f64) -> Self {\n");
            code.push_str("        Self {\n");
            code.push_str("            sample_rate,\n");
        } else {
            code.push_str("    pub fn new() -> Self {\n");
            code.push_str("        Self {\n");
        }
        code.push_str("            spec: PortSpec::default(),\n");
        code.push_str("        }\n");
        code.push_str("    }\n");
        code.push_str("}\n\n");

        code.push_str(&format!("impl GraphModule for {} {{\n", self.name));
        code.push_str("    fn port_spec(&self) -> &PortSpec { &self.spec }\n");
        code.push_str(
            "    fn tick(&mut self, _inputs: &PortValues, _outputs: &mut PortValues) {}\n",
        );
        code.push_str("    fn reset(&mut self) {}\n");
        if self.needs_sample_rate {
            code.push_str(
                "    fn set_sample_rate(&mut self, sample_rate: f64) { self.sample_rate = sample_rate; }\n",
            );
        } else {
            code.push_str("    fn set_sample_rate(&mut self, _: f64) {}\n");
        }
        code.push_str(&format!("    fn type_id(&self) -> &'static str {{ \"{}\" }}\n", self.type_id));
        code.push_str("}\n");

        code
    }
}

/// Convert PascalCase to snake_case
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

/// Template presets for common module types
pub struct ModulePresets;

impl ModulePresets {
    /// Create a simple VCO template
    pub fn vco(name: impl Into<String>) -> ModuleTemplate {
        ModuleTemplate::new(name, ModuleCategory::Oscillator)
            .with_doc("Voltage-controlled oscillator with multiple waveform outputs")
            .add_state_field(
                StateFieldTemplate::new("phase", "f64", "0.0")
                    .with_description("Current oscillator phase (0.0 to 1.0)"),
            )
            .add_state_field(
                StateFieldTemplate::new("last_sync", "f64", "0.0")
                    .with_description("Previous sync input for edge detection"),
            )
    }

    /// Create a simple filter template
    pub fn filter(name: impl Into<String>) -> ModuleTemplate {
        ModuleTemplate::new(name, ModuleCategory::Filter)
            .with_doc("State variable filter with lowpass, bandpass, and highpass outputs")
            .add_state_field(
                StateFieldTemplate::new("lp_state", "f64", "0.0")
                    .with_description("Lowpass state variable"),
            )
            .add_state_field(
                StateFieldTemplate::new("bp_state", "f64", "0.0")
                    .with_description("Bandpass state variable"),
            )
    }

    /// Create a simple envelope template
    pub fn envelope(name: impl Into<String>) -> ModuleTemplate {
        ModuleTemplate::new(name, ModuleCategory::Modulation)
            .with_doc("Envelope generator with attack, decay, sustain, and release")
            .with_inputs(vec![
                PortTemplate::new("gate", SignalKind::Gate, 0.0),
                PortTemplate::new("attack", SignalKind::CvUnipolar, 0.1).with_attenuverter(),
                PortTemplate::new("decay", SignalKind::CvUnipolar, 0.2).with_attenuverter(),
                PortTemplate::new("sustain", SignalKind::CvUnipolar, 0.7).with_attenuverter(),
                PortTemplate::new("release", SignalKind::CvUnipolar, 0.3).with_attenuverter(),
            ])
            .with_outputs(vec![
                PortTemplate::new("out", SignalKind::CvUnipolar, 0.0),
                PortTemplate::new("eoc", SignalKind::Trigger, 0.0),
            ])
            .add_state_field(
                StateFieldTemplate::new("stage", "EnvelopeStage", "EnvelopeStage::Idle")
                    .with_description("Current envelope stage"),
            )
            .add_state_field(
                StateFieldTemplate::new("level", "f64", "0.0")
                    .with_description("Current envelope level"),
            )
    }

    /// Create a simple utility template (pass-through with gain)
    pub fn utility(name: impl Into<String>) -> ModuleTemplate {
        ModuleTemplate::new(name, ModuleCategory::Utility)
            .with_doc("Utility module for signal processing")
            .with_sample_rate(false)
    }

    /// Create a simple effect template
    pub fn effect(name: impl Into<String>) -> ModuleTemplate {
        ModuleTemplate::new(name, ModuleCategory::Effect)
            .with_doc("Audio effect processor")
    }

    /// Create an I/O module template
    pub fn io(name: impl Into<String>) -> ModuleTemplate {
        ModuleTemplate::new(name, ModuleCategory::InputOutput)
            .with_doc("Input/Output interface module")
            .with_sample_rate(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_template_generation() {
        let template = ModuleTemplate::new("MyVco", ModuleCategory::Oscillator)
            .with_doc("A custom VCO module");

        let code = template.generate_code();

        assert!(code.contains("pub struct MyVco"));
        assert!(code.contains("impl GraphModule for MyVco"));
        assert!(code.contains("fn tick("));
        assert!(code.contains("fn reset("));
        assert!(code.contains("fn type_id("));
        assert!(code.contains("\"my_vco\""));
    }

    #[test]
    fn test_snake_case_conversion() {
        assert_eq!(to_snake_case("MyVco"), "my_vco");
        assert_eq!(to_snake_case("DiodeLadderFilter"), "diode_ladder_filter");
        assert_eq!(to_snake_case("VCA"), "v_c_a");
    }

    #[test]
    fn test_module_preset_vco() {
        let template = ModulePresets::vco("CustomVco");
        assert_eq!(template.name, "CustomVco");
        assert_eq!(template.category, ModuleCategory::Oscillator);
        assert!(template.needs_sample_rate);
        assert!(!template.state_fields.is_empty());
    }

    #[test]
    fn test_port_template() {
        let port = PortTemplate::new("cutoff", SignalKind::CvUnipolar, 0.5)
            .with_attenuverter()
            .normalled_to("freq");

        assert_eq!(port.name, "cutoff");
        assert!(port.has_attenuverter);
        assert_eq!(port.normalled_to, Some("freq".to_string()));
    }

    #[test]
    fn test_category_typical_ports() {
        let osc_inputs = ModuleCategory::Oscillator.typical_inputs();
        assert!(osc_inputs.iter().any(|p| p.name == "voct"));

        let filter_inputs = ModuleCategory::Filter.typical_inputs();
        assert!(filter_inputs.iter().any(|p| p.name == "cutoff"));

        let filter_outputs = ModuleCategory::Filter.typical_outputs();
        assert!(filter_outputs.iter().any(|p| p.name == "lp"));
    }

    #[test]
    fn test_minimal_generation() {
        let template = ModuleTemplate::new("SimpleModule", ModuleCategory::Utility);
        let code = template.generate_minimal();

        assert!(code.contains("pub struct SimpleModule"));
        assert!(code.contains("impl GraphModule for SimpleModule"));
        // Minimal should be shorter
        assert!(code.len() < template.generate_code().len());
    }
}
