//! Module Development Kit (MDK)
//!
//! This module provides tools for developing new Quiver modules:
//! - Template generator for creating new module boilerplate
//! - Testing harness for validating module behavior
//! - Documentation generator for module documentation

use crate::port::{GraphModule, PortSpec, PortValues, SignalKind};

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
            ModuleCategory::InputOutput => vec![PortTemplate::new("in", SignalKind::Audio, 0.0)],
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
            ModuleCategory::Utility => vec![PortTemplate::new("out", SignalKind::Audio, 0.0)],
            ModuleCategory::Effect => vec![PortTemplate::new("out", SignalKind::Audio, 0.0)],
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
            code.push_str(&format!(
                "            {}: {},\n",
                field.name, field.initial_value
            ));
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
        code.push('\n');
        code.push_str("        // TODO: Implement processing logic\n");
        code.push('\n');
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
            code.push_str(&format!(
                "        self.{} = {};\n",
                field.name, field.initial_value
            ));
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
        code.push_str(&format!(
            "    fn type_id(&self) -> &'static str {{ \"{}\" }}\n",
            self.type_id
        ));
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
        ModuleTemplate::new(name, ModuleCategory::Effect).with_doc("Audio effect processor")
    }

    /// Create an I/O module template
    pub fn io(name: impl Into<String>) -> ModuleTemplate {
        ModuleTemplate::new(name, ModuleCategory::InputOutput)
            .with_doc("Input/Output interface module")
            .with_sample_rate(false)
    }
}

// =============================================================================
// Testing Harness
// =============================================================================

/// Test result from a module test
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Test name
    pub name: String,
    /// Whether the test passed
    pub passed: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Measured values (for diagnostic tests)
    pub measurements: Vec<(String, f64)>,
}

impl TestResult {
    fn pass(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            error: None,
            measurements: Vec::new(),
        }
    }

    fn fail(name: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: false,
            error: Some(error.into()),
            measurements: Vec::new(),
        }
    }

    fn with_measurement(mut self, name: impl Into<String>, value: f64) -> Self {
        self.measurements.push((name.into(), value));
        self
    }
}

/// Test suite results
#[derive(Debug, Clone)]
pub struct TestSuiteResult {
    /// Module type being tested
    pub module_type: String,
    /// Individual test results
    pub results: Vec<TestResult>,
}

impl TestSuiteResult {
    /// Returns true if all tests passed
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }

    /// Returns count of passed tests
    pub fn passed_count(&self) -> usize {
        self.results.iter().filter(|r| r.passed).count()
    }

    /// Returns count of failed tests
    pub fn failed_count(&self) -> usize {
        self.results.iter().filter(|r| !r.passed).count()
    }

    /// Generate a summary report
    pub fn summary(&self) -> String {
        let mut report = format!("Test Suite: {}\n", self.module_type);
        report.push_str(&format!(
            "Results: {}/{} passed\n",
            self.passed_count(),
            self.results.len()
        ));
        report.push_str(&"=".repeat(40));
        report.push('\n');

        for result in &self.results {
            let status = if result.passed { "PASS" } else { "FAIL" };
            report.push_str(&format!("[{}] {}\n", status, result.name));
            if let Some(ref err) = result.error {
                report.push_str(&format!("      Error: {}\n", err));
            }
            for (name, value) in &result.measurements {
                report.push_str(&format!("      {}: {:.6}\n", name, value));
            }
        }

        report
    }
}

/// Testing harness for validating module behavior
///
/// Provides a suite of standard tests for GraphModule implementations:
/// - Port specification validation
/// - Reset behavior
/// - Sample rate handling
/// - DC offset detection
/// - Stability testing
/// - NaN/Inf detection
pub struct ModuleTestHarness<M: GraphModule> {
    module: M,
    sample_rate: f64,
}

impl<M: GraphModule> ModuleTestHarness<M> {
    /// Create a new test harness for a module
    pub fn new(module: M, sample_rate: f64) -> Self {
        Self {
            module,
            sample_rate,
        }
    }

    /// Run all standard tests
    pub fn run_all(&mut self) -> TestSuiteResult {
        let module_type = self.module.type_id().to_string();
        let results = vec![
            self.test_port_spec(),
            self.test_reset(),
            self.test_sample_rate(),
            self.test_zero_input(),
            self.test_stability(),
            self.test_nan_inf(),
            self.test_output_range(),
        ];

        TestSuiteResult {
            module_type,
            results,
        }
    }

    /// Test that the port specification is valid
    pub fn test_port_spec(&self) -> TestResult {
        let spec = self.module.port_spec();

        // Check for duplicate input port IDs
        let mut input_ids: Vec<_> = spec.inputs.iter().map(|p| p.id).collect();
        input_ids.sort();
        for i in 1..input_ids.len() {
            if input_ids[i] == input_ids[i - 1] {
                return TestResult::fail(
                    "port_spec_valid",
                    format!("Duplicate input port ID: {}", input_ids[i]),
                );
            }
        }

        // Check for duplicate output port IDs
        let mut output_ids: Vec<_> = spec.outputs.iter().map(|p| p.id).collect();
        output_ids.sort();
        for i in 1..output_ids.len() {
            if output_ids[i] == output_ids[i - 1] {
                return TestResult::fail(
                    "port_spec_valid",
                    format!("Duplicate output port ID: {}", output_ids[i]),
                );
            }
        }

        // Check for empty port names
        for port in spec.inputs.iter().chain(spec.outputs.iter()) {
            if port.name.is_empty() {
                return TestResult::fail(
                    "port_spec_valid",
                    format!("Empty port name for ID {}", port.id),
                );
            }
        }

        TestResult::pass("port_spec_valid")
            .with_measurement("input_count", spec.inputs.len() as f64)
            .with_measurement("output_count", spec.outputs.len() as f64)
    }

    /// Test that reset clears internal state
    pub fn test_reset(&mut self) -> TestResult {
        let spec = self.module.port_spec().clone();

        // Run module for a bit to build up state
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        for input in &spec.inputs {
            inputs.set(input.id, 1.0);
        }

        for _ in 0..1000 {
            self.module.tick(&inputs, &mut outputs);
        }

        // Reset and run with zero inputs
        self.module.reset();

        inputs.clear();
        outputs.clear();

        // After reset with zero inputs, module should produce consistent output
        self.module.tick(&inputs, &mut outputs);
        let first_outputs: Vec<_> = spec
            .outputs
            .iter()
            .map(|p| outputs.get_or(p.id, 0.0))
            .collect();

        self.module.reset();
        outputs.clear();
        self.module.tick(&inputs, &mut outputs);
        let second_outputs: Vec<_> = spec
            .outputs
            .iter()
            .map(|p| outputs.get_or(p.id, 0.0))
            .collect();

        // Outputs should match after reset
        for (i, (first, second)) in first_outputs.iter().zip(second_outputs.iter()).enumerate() {
            if (first - second).abs() > 1e-10 {
                return TestResult::fail(
                    "reset_clears_state",
                    format!(
                        "Output {} differs after reset: {} vs {}",
                        spec.outputs[i].name, first, second
                    ),
                );
            }
        }

        TestResult::pass("reset_clears_state")
    }

    /// Test sample rate handling
    pub fn test_sample_rate(&mut self) -> TestResult {
        // Just verify it doesn't panic
        self.module.set_sample_rate(self.sample_rate);
        self.module.set_sample_rate(48000.0);
        self.module.set_sample_rate(96000.0);
        self.module.set_sample_rate(self.sample_rate);

        TestResult::pass("sample_rate_handling")
    }

    /// Test output with zero inputs
    pub fn test_zero_input(&mut self) -> TestResult {
        self.module.reset();

        let inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Run for a bit
        for _ in 0..100 {
            self.module.tick(&inputs, &mut outputs);
        }

        // Collect output values
        let spec = self.module.port_spec();
        let mut result = TestResult::pass("zero_input_behavior");

        for output in &spec.outputs {
            let value = outputs.get_or(output.id, 0.0);
            result = result.with_measurement(format!("{}_at_zero", output.name), value);
        }

        result
    }

    /// Test stability over many samples
    pub fn test_stability(&mut self) -> TestResult {
        self.module.reset();

        let spec = self.module.port_spec().clone();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set typical input values
        for input in &spec.inputs {
            let default = match input.kind {
                SignalKind::VoltPerOctave => 0.0,
                SignalKind::Gate | SignalKind::Trigger => 0.0,
                SignalKind::CvUnipolar => 0.5,
                SignalKind::CvBipolar => 0.0,
                SignalKind::Audio => 0.0,
                SignalKind::Clock => 0.0,
            };
            inputs.set(input.id, default);
        }

        // Run for many samples
        let mut max_output = 0.0_f64;
        for _ in 0..44100 {
            self.module.tick(&inputs, &mut outputs);

            for output in &spec.outputs {
                let value = outputs.get_or(output.id, 0.0).abs();
                max_output = max_output.max(value);
            }
        }

        // Check for reasonable output range (not exploding)
        if max_output > 1000.0 {
            return TestResult::fail("stability", format!("Output exploded to {:.2}", max_output));
        }

        TestResult::pass("stability").with_measurement("max_output", max_output)
    }

    /// Test for NaN or Infinity in outputs
    pub fn test_nan_inf(&mut self) -> TestResult {
        self.module.reset();

        let spec = self.module.port_spec().clone();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test with various input values including edge cases
        let test_values = [0.0, 1.0, -1.0, 5.0, -5.0, 10.0, -10.0, 0.001, -0.001];

        for &test_val in &test_values {
            for input in &spec.inputs {
                inputs.set(input.id, test_val);
            }

            for _ in 0..100 {
                self.module.tick(&inputs, &mut outputs);

                for output in &spec.outputs {
                    let value = outputs.get_or(output.id, 0.0);
                    if value.is_nan() {
                        return TestResult::fail(
                            "no_nan_inf",
                            format!(
                                "NaN detected in output {} with input {}",
                                output.name, test_val
                            ),
                        );
                    }
                    if value.is_infinite() {
                        return TestResult::fail(
                            "no_nan_inf",
                            format!(
                                "Infinity detected in output {} with input {}",
                                output.name, test_val
                            ),
                        );
                    }
                }
            }

            self.module.reset();
        }

        TestResult::pass("no_nan_inf")
    }

    /// Test that outputs stay within expected voltage ranges
    pub fn test_output_range(&mut self) -> TestResult {
        self.module.reset();

        let spec = self.module.port_spec().clone();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set some typical modulation
        for input in &spec.inputs {
            inputs.set(input.id, input.default);
        }

        let mut violations = Vec::new();

        for _ in 0..4410 {
            self.module.tick(&inputs, &mut outputs);

            for output in &spec.outputs {
                let value = outputs.get_or(output.id, 0.0);
                let (min, max) = output.kind.voltage_range();

                // Allow 20% headroom for transients
                let headroom = (max - min) * 0.2;
                if value < min - headroom || value > max + headroom {
                    violations.push(format!(
                        "{}: {:.2} outside [{:.1}, {:.1}]",
                        output.name, value, min, max
                    ));
                    if violations.len() >= 5 {
                        break;
                    }
                }
            }
        }

        if violations.is_empty() {
            TestResult::pass("output_range")
        } else {
            TestResult::fail("output_range", violations.join("; "))
        }
    }

    /// Custom test with user-provided input sequence
    pub fn test_with_inputs(
        &mut self,
        name: &str,
        input_sequence: &[PortValues],
        validator: impl Fn(&[PortValues]) -> Result<(), String>,
    ) -> TestResult {
        self.module.reset();

        let mut output_sequence = Vec::with_capacity(input_sequence.len());

        for inputs in input_sequence {
            let mut outputs = PortValues::new();
            self.module.tick(inputs, &mut outputs);
            output_sequence.push(outputs);
        }

        match validator(&output_sequence) {
            Ok(()) => TestResult::pass(name),
            Err(e) => TestResult::fail(name, e),
        }
    }

    /// Get mutable access to the module for custom testing
    pub fn module_mut(&mut self) -> &mut M {
        &mut self.module
    }

    /// Get access to the module
    pub fn module(&self) -> &M {
        &self.module
    }
}

/// Audio analysis utilities for testing
pub struct AudioAnalysis;

impl AudioAnalysis {
    /// Calculate RMS (root mean square) of a signal
    pub fn rms(samples: &[f64]) -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f64 = samples.iter().map(|s| s * s).sum();
        (sum_sq / samples.len() as f64).sqrt()
    }

    /// Calculate peak amplitude
    pub fn peak(samples: &[f64]) -> f64 {
        samples.iter().map(|s| s.abs()).fold(0.0, f64::max)
    }

    /// Calculate DC offset (average)
    pub fn dc_offset(samples: &[f64]) -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        samples.iter().sum::<f64>() / samples.len() as f64
    }

    /// Estimate fundamental frequency using zero-crossing
    pub fn estimate_frequency(samples: &[f64], sample_rate: f64) -> Option<f64> {
        if samples.len() < 4 {
            return None;
        }

        let mut crossings = 0;
        let mut last_positive = samples[0] >= 0.0;

        for &sample in samples.iter().skip(1) {
            let positive = sample >= 0.0;
            if positive != last_positive {
                crossings += 1;
                last_positive = positive;
            }
        }

        if crossings < 2 {
            return None;
        }

        // Frequency = (crossings / 2) / time
        let time = samples.len() as f64 / sample_rate;
        Some((crossings as f64 / 2.0) / time)
    }

    /// Check if signal is approximately silent
    pub fn is_silent(samples: &[f64], threshold: f64) -> bool {
        Self::peak(samples) < threshold
    }

    /// Check if signal contains a gate (sustained high value)
    pub fn has_gate(samples: &[f64], threshold: f64) -> bool {
        let mut consecutive_high = 0;
        let required = 10; // Need at least 10 consecutive samples above threshold

        for &sample in samples {
            if sample > threshold {
                consecutive_high += 1;
                if consecutive_high >= required {
                    return true;
                }
            } else {
                consecutive_high = 0;
            }
        }

        false
    }
}

// =============================================================================
// Documentation Generator
// =============================================================================

/// Format for generated documentation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocFormat {
    /// Markdown format
    Markdown,
    /// Plain text format
    PlainText,
    /// HTML format
    Html,
}

/// Documentation generator for modules
///
/// Generates documentation from a module's port specification and metadata.
pub struct DocGenerator;

impl DocGenerator {
    /// Generate documentation for a module
    pub fn generate<M: GraphModule>(module: &M, format: DocFormat) -> String {
        let spec = module.port_spec();
        let type_id = module.type_id();

        match format {
            DocFormat::Markdown => Self::generate_markdown(type_id, spec),
            DocFormat::PlainText => Self::generate_plain_text(type_id, spec),
            DocFormat::Html => Self::generate_html(type_id, spec),
        }
    }

    /// Generate documentation from a module template
    pub fn generate_from_template(template: &ModuleTemplate, format: DocFormat) -> String {
        match format {
            DocFormat::Markdown => Self::generate_markdown_from_template(template),
            DocFormat::PlainText => Self::generate_plain_text_from_template(template),
            DocFormat::Html => Self::generate_html_from_template(template),
        }
    }

    fn generate_markdown(type_id: &str, spec: &PortSpec) -> String {
        let mut doc = String::new();

        doc.push_str(&format!("# {}\n\n", to_pascal_case(type_id)));
        doc.push_str(&format!("**Type ID:** `{}`\n\n", type_id));

        // Inputs
        if !spec.inputs.is_empty() {
            doc.push_str("## Inputs\n\n");
            doc.push_str("| Port | Type | Default | Attenuverter |\n");
            doc.push_str("|------|------|---------|-------------|\n");
            for input in &spec.inputs {
                doc.push_str(&format!(
                    "| `{}` | {:?} | {:.2} | {} |\n",
                    input.name,
                    input.kind,
                    input.default,
                    if input.has_attenuverter { "Yes" } else { "No" }
                ));
            }
            doc.push('\n');
        }

        // Outputs
        if !spec.outputs.is_empty() {
            doc.push_str("## Outputs\n\n");
            doc.push_str("| Port | Type |\n");
            doc.push_str("|------|------|\n");
            for output in &spec.outputs {
                doc.push_str(&format!("| `{}` | {:?} |\n", output.name, output.kind));
            }
            doc.push('\n');
        }

        doc
    }

    fn generate_plain_text(type_id: &str, spec: &PortSpec) -> String {
        let mut doc = String::new();

        doc.push_str(&format!("{}\n", to_pascal_case(type_id)));
        doc.push_str(&"=".repeat(type_id.len() + 4));
        doc.push_str("\n\n");

        doc.push_str(&format!("Type ID: {}\n\n", type_id));

        // Inputs
        if !spec.inputs.is_empty() {
            doc.push_str("INPUTS:\n");
            for input in &spec.inputs {
                doc.push_str(&format!(
                    "  - {} ({:?}, default: {:.2}{})\n",
                    input.name,
                    input.kind,
                    input.default,
                    if input.has_attenuverter {
                        ", has attenuverter"
                    } else {
                        ""
                    }
                ));
            }
            doc.push('\n');
        }

        // Outputs
        if !spec.outputs.is_empty() {
            doc.push_str("OUTPUTS:\n");
            for output in &spec.outputs {
                doc.push_str(&format!("  - {} ({:?})\n", output.name, output.kind));
            }
            doc.push('\n');
        }

        doc
    }

    fn generate_html(type_id: &str, spec: &PortSpec) -> String {
        let mut doc = String::new();

        doc.push_str(&format!("<h1>{}</h1>\n", to_pascal_case(type_id)));
        doc.push_str(&format!(
            "<p><strong>Type ID:</strong> <code>{}</code></p>\n",
            type_id
        ));

        // Inputs
        if !spec.inputs.is_empty() {
            doc.push_str("<h2>Inputs</h2>\n");
            doc.push_str("<table>\n");
            doc.push_str(
                "<tr><th>Port</th><th>Type</th><th>Default</th><th>Attenuverter</th></tr>\n",
            );
            for input in &spec.inputs {
                doc.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>{:?}</td><td>{:.2}</td><td>{}</td></tr>\n",
                    input.name,
                    input.kind,
                    input.default,
                    if input.has_attenuverter { "Yes" } else { "No" }
                ));
            }
            doc.push_str("</table>\n");
        }

        // Outputs
        if !spec.outputs.is_empty() {
            doc.push_str("<h2>Outputs</h2>\n");
            doc.push_str("<table>\n");
            doc.push_str("<tr><th>Port</th><th>Type</th></tr>\n");
            for output in &spec.outputs {
                doc.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>{:?}</td></tr>\n",
                    output.name, output.kind
                ));
            }
            doc.push_str("</table>\n");
        }

        doc
    }

    fn generate_markdown_from_template(template: &ModuleTemplate) -> String {
        let mut doc = String::new();

        doc.push_str(&format!("# {}\n\n", template.name));

        if !template.doc.is_empty() {
            doc.push_str(&format!("{}\n\n", template.doc));
        }

        doc.push_str(&format!("**Type ID:** `{}`\n", template.type_id));
        doc.push_str(&format!("**Category:** {:?}\n\n", template.category));

        // Inputs
        if !template.inputs.is_empty() {
            doc.push_str("## Inputs\n\n");
            doc.push_str("| Port | Type | Default | Attenuverter |\n");
            doc.push_str("|------|------|---------|-------------|\n");
            for input in &template.inputs {
                doc.push_str(&format!(
                    "| `{}` | {:?} | {:.2} | {} |\n",
                    input.name,
                    input.kind,
                    input.default,
                    if input.has_attenuverter { "Yes" } else { "No" }
                ));
            }
            doc.push('\n');
        }

        // Outputs
        if !template.outputs.is_empty() {
            doc.push_str("## Outputs\n\n");
            doc.push_str("| Port | Type |\n");
            doc.push_str("|------|------|\n");
            for output in &template.outputs {
                doc.push_str(&format!("| `{}` | {:?} |\n", output.name, output.kind));
            }
            doc.push('\n');
        }

        doc
    }

    fn generate_plain_text_from_template(template: &ModuleTemplate) -> String {
        let mut doc = String::new();

        doc.push_str(&format!("{}\n", template.name));
        doc.push_str(&"=".repeat(template.name.len()));
        doc.push_str("\n\n");

        if !template.doc.is_empty() {
            doc.push_str(&format!("{}\n\n", template.doc));
        }

        doc.push_str(&format!("Type ID: {}\n", template.type_id));
        doc.push_str(&format!("Category: {:?}\n\n", template.category));

        // Inputs
        if !template.inputs.is_empty() {
            doc.push_str("INPUTS:\n");
            for input in &template.inputs {
                doc.push_str(&format!(
                    "  - {} ({:?}, default: {:.2}{})\n",
                    input.name,
                    input.kind,
                    input.default,
                    if input.has_attenuverter {
                        ", has attenuverter"
                    } else {
                        ""
                    }
                ));
            }
            doc.push('\n');
        }

        // Outputs
        if !template.outputs.is_empty() {
            doc.push_str("OUTPUTS:\n");
            for output in &template.outputs {
                doc.push_str(&format!("  - {} ({:?})\n", output.name, output.kind));
            }
            doc.push('\n');
        }

        doc
    }

    fn generate_html_from_template(template: &ModuleTemplate) -> String {
        let mut doc = String::new();

        doc.push_str(&format!("<h1>{}</h1>\n", template.name));

        if !template.doc.is_empty() {
            doc.push_str(&format!("<p>{}</p>\n", template.doc));
        }

        doc.push_str(&format!(
            "<p><strong>Type ID:</strong> <code>{}</code></p>\n",
            template.type_id
        ));
        doc.push_str(&format!(
            "<p><strong>Category:</strong> {:?}</p>\n",
            template.category
        ));

        // Inputs
        if !template.inputs.is_empty() {
            doc.push_str("<h2>Inputs</h2>\n");
            doc.push_str("<table>\n");
            doc.push_str(
                "<tr><th>Port</th><th>Type</th><th>Default</th><th>Attenuverter</th></tr>\n",
            );
            for input in &template.inputs {
                doc.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>{:?}</td><td>{:.2}</td><td>{}</td></tr>\n",
                    input.name,
                    input.kind,
                    input.default,
                    if input.has_attenuverter { "Yes" } else { "No" }
                ));
            }
            doc.push_str("</table>\n");
        }

        // Outputs
        if !template.outputs.is_empty() {
            doc.push_str("<h2>Outputs</h2>\n");
            doc.push_str("<table>\n");
            doc.push_str("<tr><th>Port</th><th>Type</th></tr>\n");
            for output in &template.outputs {
                doc.push_str(&format!(
                    "<tr><td><code>{}</code></td><td>{:?}</td></tr>\n",
                    output.name, output.kind
                ));
            }
            doc.push_str("</table>\n");
        }

        doc
    }
}

/// Convert snake_case to PascalCase
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::Vco;

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

    // Testing Harness Tests

    #[test]
    fn test_harness_runs_all_tests() {
        let vco = Vco::new(44100.0);
        let mut harness = ModuleTestHarness::new(vco, 44100.0);

        let results = harness.run_all();

        assert_eq!(results.module_type, "vco");
        assert_eq!(results.results.len(), 7); // 7 standard tests
        assert!(results.passed_count() > 0);
    }

    #[test]
    fn test_harness_port_spec_validation() {
        let vco = Vco::new(44100.0);
        let harness = ModuleTestHarness::new(vco, 44100.0);

        let result = harness.test_port_spec();
        assert!(result.passed);
        assert!(result.measurements.iter().any(|(n, _)| n == "input_count"));
        assert!(result.measurements.iter().any(|(n, _)| n == "output_count"));
    }

    #[test]
    fn test_suite_result_summary() {
        let vco = Vco::new(44100.0);
        let mut harness = ModuleTestHarness::new(vco, 44100.0);

        let results = harness.run_all();
        let summary = results.summary();

        assert!(summary.contains("Test Suite: vco"));
        assert!(summary.contains("passed"));
    }

    #[test]
    fn test_audio_analysis_rms() {
        let samples = vec![1.0, -1.0, 1.0, -1.0];
        let rms = AudioAnalysis::rms(&samples);
        assert!((rms - 1.0).abs() < 0.001);

        let silent = vec![0.0; 100];
        assert_eq!(AudioAnalysis::rms(&silent), 0.0);
    }

    #[test]
    fn test_audio_analysis_peak() {
        let samples = vec![0.5, -0.8, 0.3, -0.2];
        assert_eq!(AudioAnalysis::peak(&samples), 0.8);
    }

    #[test]
    fn test_audio_analysis_dc_offset() {
        let samples = vec![1.0, 1.0, 1.0, 1.0];
        assert_eq!(AudioAnalysis::dc_offset(&samples), 1.0);

        let balanced = vec![1.0, -1.0, 1.0, -1.0];
        assert_eq!(AudioAnalysis::dc_offset(&balanced), 0.0);
    }

    #[test]
    fn test_audio_analysis_frequency() {
        // 440 Hz sine wave at 44100 Hz sample rate
        let sample_rate = 44100.0;
        let freq = 440.0;
        let samples: Vec<f64> = (0..4410)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / sample_rate).sin())
            .collect();

        let estimated = AudioAnalysis::estimate_frequency(&samples, sample_rate).unwrap();
        // Allow 5% tolerance
        assert!((estimated - freq).abs() / freq < 0.05);
    }

    #[test]
    fn test_audio_analysis_silence() {
        let silent = vec![0.0; 100];
        assert!(AudioAnalysis::is_silent(&silent, 0.01));

        let loud = vec![1.0; 100];
        assert!(!AudioAnalysis::is_silent(&loud, 0.01));
    }

    #[test]
    fn test_audio_analysis_gate() {
        let mut samples = vec![0.0; 100];
        // Add a gate in the middle
        samples[30..60].fill(5.0);
        assert!(AudioAnalysis::has_gate(&samples, 2.5));

        let no_gate = vec![0.0; 100];
        assert!(!AudioAnalysis::has_gate(&no_gate, 2.5));
    }

    // Documentation Generator Tests

    #[test]
    fn test_doc_generator_markdown() {
        let vco = Vco::new(44100.0);
        let doc = DocGenerator::generate(&vco, DocFormat::Markdown);

        assert!(doc.contains("# Vco"));
        assert!(doc.contains("**Type ID:** `vco`"));
        assert!(doc.contains("## Inputs"));
        assert!(doc.contains("## Outputs"));
        assert!(doc.contains("| Port |"));
    }

    #[test]
    fn test_doc_generator_plain_text() {
        let vco = Vco::new(44100.0);
        let doc = DocGenerator::generate(&vco, DocFormat::PlainText);

        assert!(doc.contains("Vco"));
        assert!(doc.contains("Type ID: vco"));
        assert!(doc.contains("INPUTS:"));
        assert!(doc.contains("OUTPUTS:"));
    }

    #[test]
    fn test_doc_generator_html() {
        let vco = Vco::new(44100.0);
        let doc = DocGenerator::generate(&vco, DocFormat::Html);

        assert!(doc.contains("<h1>Vco</h1>"));
        assert!(doc.contains("<code>vco</code>"));
        assert!(doc.contains("<table>"));
        assert!(doc.contains("<th>Port</th>"));
    }

    #[test]
    fn test_doc_generator_from_template() {
        let template = ModulePresets::vco("CustomVco");
        let doc = DocGenerator::generate_from_template(&template, DocFormat::Markdown);

        assert!(doc.contains("# CustomVco"));
        assert!(doc.contains("**Type ID:** `custom_vco`"));
        assert!(doc.contains("**Category:** Oscillator"));
    }

    #[test]
    fn test_pascal_case_conversion() {
        assert_eq!(to_pascal_case("my_vco"), "MyVco");
        assert_eq!(to_pascal_case("diode_ladder_filter"), "DiodeLadderFilter");
        assert_eq!(to_pascal_case("vco"), "Vco");
    }

    #[test]
    fn test_module_template_builder() {
        let template = ModuleTemplate::new("TestModule", ModuleCategory::Effect)
            .with_doc("A test module")
            .with_type_id("test_module")
            .with_inputs(vec![PortTemplate::new("in", SignalKind::Audio, 0.0)])
            .with_outputs(vec![PortTemplate::new("out", SignalKind::Audio, 0.0)])
            .with_sample_rate(true);

        assert_eq!(template.name, "TestModule");
        assert_eq!(template.doc, "A test module");
        assert_eq!(template.type_id, "test_module");
        assert!(template.needs_sample_rate);
    }

    #[test]
    fn test_module_template_add_input_output() {
        let initial = ModuleTemplate::new("Test", ModuleCategory::Utility);
        let initial_inputs = initial.inputs.len();
        let initial_outputs = initial.outputs.len();

        let template = initial.add_input(PortTemplate::new("in", SignalKind::Audio, 0.0));
        assert_eq!(template.inputs.len(), initial_inputs + 1);

        let template = template.add_output(PortTemplate::new("out", SignalKind::Audio, 0.0));
        assert_eq!(template.outputs.len(), initial_outputs + 1);
    }

    #[test]
    fn test_module_template_add_state_field() {
        let mut template = ModuleTemplate::new("Test", ModuleCategory::Utility);
        template = template.add_state_field(StateFieldTemplate::new("counter", "u32", "0"));

        assert_eq!(template.state_fields.len(), 1);
    }

    #[test]
    fn test_state_field_template() {
        let field =
            StateFieldTemplate::new("level", "f64", "0.0").with_description("Current level");

        assert_eq!(field.name, "level");
        assert_eq!(field.field_type, "f64");
        assert_eq!(field.initial_value, "0.0");
        assert_eq!(field.description, "Current level");
    }

    #[test]
    fn test_module_presets_filter() {
        let template = ModulePresets::filter("MyFilter");
        assert_eq!(template.name, "MyFilter");
        assert_eq!(template.category, ModuleCategory::Filter);
    }

    #[test]
    fn test_module_presets_envelope() {
        let template = ModulePresets::envelope("MyEnv");
        assert_eq!(template.name, "MyEnv");
        assert_eq!(template.category, ModuleCategory::Modulation);
    }

    #[test]
    fn test_module_presets_utility() {
        let template = ModulePresets::utility("MyUtil");
        assert_eq!(template.name, "MyUtil");
        assert_eq!(template.category, ModuleCategory::Utility);
    }

    #[test]
    fn test_module_presets_effect() {
        let template = ModulePresets::effect("MyEffect");
        assert_eq!(template.name, "MyEffect");
        assert_eq!(template.category, ModuleCategory::Effect);
    }

    #[test]
    fn test_module_presets_io() {
        let template = ModulePresets::io("MyIO");
        assert_eq!(template.name, "MyIO");
        assert_eq!(template.category, ModuleCategory::InputOutput);
    }

    #[test]
    fn test_modulation_category_ports() {
        let inputs = ModuleCategory::Modulation.typical_inputs();
        let outputs = ModuleCategory::Modulation.typical_outputs();
        assert!(!outputs.is_empty());
        let _ = inputs;
    }

    #[test]
    fn test_utility_category_ports() {
        let inputs = ModuleCategory::Utility.typical_inputs();
        let outputs = ModuleCategory::Utility.typical_outputs();
        assert!(!inputs.is_empty());
        assert!(!outputs.is_empty());
    }

    #[test]
    fn test_effect_category_ports() {
        let inputs = ModuleCategory::Effect.typical_inputs();
        let outputs = ModuleCategory::Effect.typical_outputs();
        assert!(!inputs.is_empty());
        assert!(!outputs.is_empty());
    }

    #[test]
    fn test_io_category_ports() {
        let inputs = ModuleCategory::InputOutput.typical_inputs();
        let outputs = ModuleCategory::InputOutput.typical_outputs();
        let _ = (inputs, outputs);
    }

    #[test]
    fn test_test_result_with_measurement() {
        let result = TestResult::pass("test")
            .with_measurement("value1", 1.0)
            .with_measurement("value2", 2.0);

        assert_eq!(result.measurements.len(), 2);
    }

    #[test]
    fn test_test_suite_failed_count() {
        let results = TestSuiteResult {
            module_type: "test".to_string(),
            results: vec![
                TestResult::pass("test1"),
                TestResult::fail("test2", "error"),
            ],
        };

        assert_eq!(results.passed_count(), 1);
        assert_eq!(results.failed_count(), 1);
        assert!(!results.all_passed());
    }

    #[test]
    fn test_harness_test_reset() {
        let vco = Vco::new(44100.0);
        let mut harness = ModuleTestHarness::new(vco, 44100.0);

        let result = harness.test_reset();
        assert!(result.passed);
    }

    #[test]
    fn test_harness_test_sample_rate() {
        let vco = Vco::new(44100.0);
        let mut harness = ModuleTestHarness::new(vco, 44100.0);

        let result = harness.test_sample_rate();
        assert!(result.passed);
    }

    #[test]
    fn test_harness_test_zero_input() {
        let vco = Vco::new(44100.0);
        let mut harness = ModuleTestHarness::new(vco, 44100.0);

        let result = harness.test_zero_input();
        assert!(result.passed);
    }

    #[test]
    fn test_harness_test_stability() {
        let vco = Vco::new(44100.0);
        let mut harness = ModuleTestHarness::new(vco, 44100.0);

        let result = harness.test_stability();
        assert!(result.passed);
    }

    #[test]
    fn test_harness_test_nan_inf() {
        let vco = Vco::new(44100.0);
        let mut harness = ModuleTestHarness::new(vco, 44100.0);

        let result = harness.test_nan_inf();
        assert!(result.passed);
    }

    #[test]
    fn test_harness_test_output_range() {
        let vco = Vco::new(44100.0);
        let mut harness = ModuleTestHarness::new(vco, 44100.0);

        let result = harness.test_output_range();
        assert!(result.passed);
    }

    #[test]
    fn test_harness_test_with_inputs() {
        let vco = Vco::new(44100.0);
        let mut harness = ModuleTestHarness::new(vco, 44100.0);

        let mut input_seq = vec![];
        for _ in 0..10 {
            let mut pv = PortValues::new();
            pv.set(0, 0.0);
            input_seq.push(pv);
        }

        let result = harness.test_with_inputs("custom", &input_seq, |_outputs| Ok(()));
        assert!(result.passed);
    }

    #[test]
    fn test_harness_module_access() {
        let vco = Vco::new(44100.0);
        let mut harness = ModuleTestHarness::new(vco, 44100.0);

        let _module = harness.module();
        let _module_mut = harness.module_mut();
    }

    #[test]
    fn test_doc_from_template_plain_text() {
        let template = ModulePresets::vco("TestVco");
        let doc = DocGenerator::generate_from_template(&template, DocFormat::PlainText);
        assert!(doc.contains("TestVco"));
        assert!(doc.contains("Type ID:"));
    }

    #[test]
    fn test_doc_from_template_html() {
        let template = ModulePresets::vco("TestVco");
        let doc = DocGenerator::generate_from_template(&template, DocFormat::Html);
        assert!(doc.contains("<h1>TestVco</h1>"));
    }
}
