//! QuiverEngine - Main WASM interface for Quiver audio engine

use crate::graph::{NodeId, Patch};
use crate::io::{AtomicF64, ExternalInput};
use crate::observer::{StateObserver, SubscriptionTarget};
use crate::port::{ports_compatible, SignalColors, SignalKind};
use crate::serialize::{ModuleRegistry, PatchDef};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use wasm_bindgen::prelude::*;

/// Main WASM interface for Quiver audio engine
#[wasm_bindgen]
pub struct QuiverEngine {
    patch: Patch,
    registry: ModuleRegistry,
    observer: StateObserver,
    sample_rate: f64,
    // MIDI state with atomic values for routing to ExternalInput modules
    midi_voct: Arc<AtomicF64>,
    midi_velocity_val: Arc<AtomicF64>,
    midi_gate_val: Arc<AtomicF64>,
    midi_pitch_bend_val: Arc<AtomicF64>,
    midi_mod_wheel: Arc<AtomicF64>,
    midi_cc_values: [Arc<AtomicF64>; 128],
    // Track which MIDI inputs have been created
    midi_inputs_created: bool,
}

#[wasm_bindgen]
impl QuiverEngine {
    /// Create a new Quiver engine
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f64) -> Self {
        // Initialize panic hook for better error messages
        console_error_panic_hook::set_once();

        // Initialize CC array with Arc<AtomicF64>
        let midi_cc_values: [Arc<AtomicF64>; 128] =
            core::array::from_fn(|_| Arc::new(AtomicF64::new(0.0)));

        Self {
            patch: Patch::new(sample_rate),
            registry: ModuleRegistry::new(),
            observer: StateObserver::new(),
            sample_rate,
            midi_voct: Arc::new(AtomicF64::new(0.0)),
            midi_velocity_val: Arc::new(AtomicF64::new(0.0)),
            midi_gate_val: Arc::new(AtomicF64::new(0.0)),
            midi_pitch_bend_val: Arc::new(AtomicF64::new(0.0)),
            midi_mod_wheel: Arc::new(AtomicF64::new(0.0)),
            midi_cc_values,
            midi_inputs_created: false,
        }
    }

    /// Get the sample rate
    #[wasm_bindgen(getter)]
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    // =========================================================================
    // Catalog API
    // =========================================================================

    /// Get the full module catalog
    pub fn get_catalog(&self) -> Result<JsValue, JsValue> {
        let catalog = self.registry.catalog();
        serde_wasm_bindgen::to_value(&catalog).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Search modules by query string
    pub fn search_modules(&self, query: &str) -> Result<JsValue, JsValue> {
        let results = self.registry.search(query);
        serde_wasm_bindgen::to_value(&results).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Get modules by category
    pub fn get_modules_by_category(&self, category: &str) -> Result<JsValue, JsValue> {
        let results = self.registry.by_category(category);
        serde_wasm_bindgen::to_value(&results).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Get all categories
    pub fn get_categories(&self) -> Result<JsValue, JsValue> {
        let categories = self.registry.categories();
        serde_wasm_bindgen::to_value(&categories).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    // =========================================================================
    // Signal Semantics API
    // =========================================================================

    /// Get default signal colors
    pub fn get_signal_colors(&self) -> Result<JsValue, JsValue> {
        let colors = SignalColors::default();
        serde_wasm_bindgen::to_value(&colors).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Check port compatibility between two signal kinds
    pub fn check_compatibility(&self, from: &str, to: &str) -> Result<JsValue, JsValue> {
        let from_kind = parse_signal_kind(from)?;
        let to_kind = parse_signal_kind(to)?;
        let compat = ports_compatible(from_kind, to_kind);
        serde_wasm_bindgen::to_value(&compat).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    // =========================================================================
    // Patch Operations
    // =========================================================================

    /// Load a patch from JSON
    pub fn load_patch(&mut self, patch_json: JsValue) -> Result<(), JsValue> {
        let patch_def: PatchDef = serde_wasm_bindgen::from_value(patch_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        self.patch = Patch::from_def(&patch_def, &self.registry, self.sample_rate)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        Ok(())
    }

    /// Save the current patch to JSON
    pub fn save_patch(&self, name: &str) -> Result<JsValue, JsValue> {
        let patch_def = self.patch.to_def(name);
        serde_wasm_bindgen::to_value(&patch_def).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Validate a patch definition
    pub fn validate_patch(&self, patch_json: JsValue) -> Result<JsValue, JsValue> {
        let patch_def: PatchDef = serde_wasm_bindgen::from_value(patch_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let result = patch_def.validate_with_registry(&self.registry);
        serde_wasm_bindgen::to_value(&result).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Clear the current patch
    pub fn clear_patch(&mut self) {
        self.patch = Patch::new(self.sample_rate);
    }

    // =========================================================================
    // Module Operations
    // =========================================================================

    /// Add a module to the patch
    pub fn add_module(&mut self, type_id: &str, name: &str) -> Result<(), JsValue> {
        let module = self
            .registry
            .instantiate(type_id, self.sample_rate)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module type: {}", type_id)))?;

        self.patch.add_boxed(name, module);
        Ok(())
    }

    /// Remove a module from the patch
    pub fn remove_module(&mut self, name: &str) -> Result<(), JsValue> {
        let node_id = self
            .get_node_id_by_name(name)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", name)))?;

        self.patch
            .remove(node_id)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))
    }

    /// Set module position for UI layout
    pub fn set_module_position(&mut self, name: &str, x: f32, y: f32) -> Result<(), JsValue> {
        let node_id = self
            .get_node_id_by_name(name)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", name)))?;

        self.patch.set_position(node_id, (x, y));
        Ok(())
    }

    /// Get module position
    pub fn get_module_position(&self, name: &str) -> Result<JsValue, JsValue> {
        let node_id = self
            .get_node_id_by_name(name)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", name)))?;

        let position = self.patch.get_position(node_id);
        serde_wasm_bindgen::to_value(&position).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Get the number of modules in the patch
    pub fn module_count(&self) -> usize {
        self.patch.node_count()
    }

    /// Get the number of cables in the patch
    pub fn cable_count(&self) -> usize {
        self.patch.cable_count()
    }

    /// Set the output module (required for audio output)
    ///
    /// The specified module's outputs will be read as the patch's stereo output.
    /// Port 0 is left channel, port 1 is right channel.
    pub fn set_output(&mut self, name: &str) -> Result<(), JsValue> {
        let node_id = self
            .get_node_id_by_name(name)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", name)))?;

        self.patch.set_output(node_id);
        Ok(())
    }

    // =========================================================================
    // Cable Operations
    // =========================================================================

    /// Connect two ports (format: "module.port")
    pub fn connect(&mut self, from: &str, to: &str) -> Result<(), JsValue> {
        let (from_module, from_port) = parse_port_ref(from)?;
        let (to_module, to_port) = parse_port_ref(to)?;

        let from_handle = self
            .patch
            .get_handle_by_name(from_module)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", from_module)))?;
        let to_handle = self
            .patch
            .get_handle_by_name(to_module)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", to_module)))?;

        self.patch
            .connect(from_handle.out(from_port), to_handle.in_(to_port))
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        Ok(())
    }

    /// Connect with attenuation
    pub fn connect_attenuated(
        &mut self,
        from: &str,
        to: &str,
        attenuation: f64,
    ) -> Result<(), JsValue> {
        let (from_module, from_port) = parse_port_ref(from)?;
        let (to_module, to_port) = parse_port_ref(to)?;

        let from_handle = self
            .patch
            .get_handle_by_name(from_module)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", from_module)))?;
        let to_handle = self
            .patch
            .get_handle_by_name(to_module)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", to_module)))?;

        self.patch
            .connect_attenuated(
                from_handle.out(from_port),
                to_handle.in_(to_port),
                attenuation,
            )
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        Ok(())
    }

    /// Connect with full modulation (attenuation and offset)
    pub fn connect_modulated(
        &mut self,
        from: &str,
        to: &str,
        attenuation: f64,
        offset: f64,
    ) -> Result<(), JsValue> {
        let (from_module, from_port) = parse_port_ref(from)?;
        let (to_module, to_port) = parse_port_ref(to)?;

        let from_handle = self
            .patch
            .get_handle_by_name(from_module)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", from_module)))?;
        let to_handle = self
            .patch
            .get_handle_by_name(to_module)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", to_module)))?;

        self.patch
            .connect_modulated(
                from_handle.out(from_port),
                to_handle.in_(to_port),
                attenuation,
                offset,
            )
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        Ok(())
    }

    /// Disconnect a cable by index
    pub fn disconnect_by_index(&mut self, cable_index: usize) -> Result<(), JsValue> {
        self.patch
            .disconnect(cable_index)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))
    }

    /// Disconnect two ports (format: "module.port")
    pub fn disconnect(&mut self, from: &str, to: &str) -> Result<(), JsValue> {
        let (from_module, from_port) = parse_port_ref(from)?;
        let (to_module, to_port) = parse_port_ref(to)?;

        let from_handle = self
            .patch
            .get_handle_by_name(from_module)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", from_module)))?;
        let to_handle = self
            .patch
            .get_handle_by_name(to_module)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", to_module)))?;

        self.patch
            .disconnect_ports(from_handle.out(from_port), to_handle.in_(to_port))
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))
    }

    /// Get all module names in the patch
    pub fn get_module_names(&self) -> Result<JsValue, JsValue> {
        let names = self.patch.module_names();
        serde_wasm_bindgen::to_value(&names).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    // =========================================================================
    // Parameter Operations
    // =========================================================================

    /// Get parameters for a module
    ///
    /// Note: This returns metadata about the module's type from the registry,
    /// not the current parameter values. Use get_param for values.
    pub fn get_params(&self, node_name: &str) -> Result<JsValue, JsValue> {
        // Find the module to get its type
        let type_id = self
            .patch
            .nodes()
            .find(|(_, name, _)| *name == node_name)
            .map(|(_, _, module)| module.type_id())
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", node_name)))?;

        // Get metadata from registry which includes port spec with param info
        let metadata = self
            .registry
            .get_metadata(type_id)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module type: {}", type_id)))?;

        // Return the port spec which contains param definitions
        serde_wasm_bindgen::to_value(&metadata.port_spec)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Set a parameter value by numeric index
    pub fn set_param(
        &mut self,
        node_name: &str,
        param_index: u32,
        value: f64,
    ) -> Result<(), JsValue> {
        let node_id = self
            .get_node_id_by_name(node_name)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", node_name)))?;

        self.patch.set_param(node_id, param_index, value);
        Ok(())
    }

    /// Get a parameter value
    pub fn get_param(&self, node_name: &str, param_index: u32) -> Result<f64, JsValue> {
        let node_id = self
            .get_node_id_by_name(node_name)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", node_name)))?;

        self.patch
            .get_param(node_id, param_index)
            .ok_or_else(|| JsValue::from_str(&format!("Param {} not found", param_index)))
    }

    /// Set a parameter value by name
    ///
    /// This is a convenience method that looks up the parameter index by name.
    pub fn set_param_by_name(
        &mut self,
        node_name: &str,
        param_name: &str,
        value: f64,
    ) -> Result<(), JsValue> {
        // Find the module and get its param definitions
        let param_id = self
            .patch
            .nodes()
            .find(|(_, name, _)| *name == node_name)
            .and_then(|(_, _, module)| {
                module
                    .params()
                    .iter()
                    .find(|p| p.name == param_name)
                    .map(|p| p.id)
            })
            .ok_or_else(|| {
                JsValue::from_str(&format!(
                    "Unknown parameter '{}' on module '{}'",
                    param_name, node_name
                ))
            })?;

        // Set the parameter
        let node_id = self
            .get_node_id_by_name(node_name)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module: {}", node_name)))?;
        self.patch.set_param(node_id, param_id, value);
        Ok(())
    }

    // =========================================================================
    // Real-Time Bridge API
    // =========================================================================

    /// Subscribe to real-time value updates
    pub fn subscribe(&mut self, targets: JsValue) -> Result<(), JsValue> {
        let targets: Vec<SubscriptionTarget> = serde_wasm_bindgen::from_value(targets)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        self.observer.add_subscriptions(targets);
        Ok(())
    }

    /// Unsubscribe from real-time value updates
    pub fn unsubscribe(&mut self, target_ids: JsValue) -> Result<(), JsValue> {
        let ids: Vec<String> = serde_wasm_bindgen::from_value(target_ids)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        self.observer.remove_subscriptions(&ids);
        Ok(())
    }

    /// Clear all subscriptions
    pub fn clear_subscriptions(&mut self) {
        self.observer.clear_subscriptions();
    }

    /// Poll for pending updates (called from requestAnimationFrame)
    pub fn poll_updates(&mut self) -> Result<JsValue, JsValue> {
        let updates = self.observer.drain_updates();
        serde_wasm_bindgen::to_value(&updates).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Get the number of pending updates
    pub fn pending_update_count(&self) -> usize {
        self.observer.pending_count()
    }

    // =========================================================================
    // Audio Processing
    // =========================================================================

    /// Process a single sample and return stereo output [left, right]
    pub fn tick(&mut self) -> Box<[f64]> {
        let (left, right) = self.patch.tick();
        Box::new([left, right])
    }

    /// Process a block of samples and return interleaved stereo Float32Array
    ///
    /// Output is safety-clamped to ±10V to prevent speaker/hearing damage
    /// from runaway signals or edge cases.
    ///
    /// This method also collects samples for observer subscriptions (Level, Scope,
    /// Spectrum) to enable accurate block-based metering.
    pub fn process_block(&mut self, num_samples: usize) -> js_sys::Float32Array {
        const SAFETY_LIMIT: f64 = 10.0; // Max output voltage

        let output = js_sys::Float32Array::new_with_length((num_samples * 2) as u32);

        // Collect samples for observer during processing
        let mut observer_samples: Vec<(f32, f32)> = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let (left, right) = self.patch.tick();
            // Safety clamp to prevent dangerous audio levels
            let left_safe = left.clamp(-SAFETY_LIMIT, SAFETY_LIMIT);
            let right_safe = right.clamp(-SAFETY_LIMIT, SAFETY_LIMIT);
            output.set_index((i * 2) as u32, left_safe as f32);
            output.set_index((i * 2 + 1) as u32, right_safe as f32);

            // Collect for observer
            observer_samples.push((left_safe as f32, right_safe as f32));
        }

        // Pass full block to observer for accurate metering
        self.observer.collect_block(&observer_samples, &self.patch);

        output
    }

    /// Reset all module state
    pub fn reset(&mut self) {
        self.patch.reset();
    }

    /// Compile the patch (required after adding/removing modules or cables)
    pub fn compile(&mut self) -> Result<(), JsValue> {
        self.patch
            .compile()
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))
    }

    // =========================================================================
    // MIDI Support for Worklet Integration
    // =========================================================================

    /// Create MIDI input modules in the patch
    ///
    /// This adds four modules to the patch that output MIDI CV signals:
    /// - `midi_voct`: V/Oct pitch output (0V = C4)
    /// - `midi_gate`: Gate signal (0V or 5V)
    /// - `midi_velocity`: Velocity CV (0-10V)
    /// - `midi_pitch_bend`: Pitch bend CV (±2 semitones as V/Oct)
    ///
    /// These modules are automatically updated when MIDI messages are received
    /// via `midi_note_on`, `midi_note_off`, and `midi_pitch_bend`.
    ///
    /// Call this once after creating the engine, before building your patch.
    pub fn create_midi_input(&mut self) -> Result<(), JsValue> {
        if self.midi_inputs_created {
            return Err(JsValue::from_str("MIDI inputs already created"));
        }

        // Create ExternalInput modules connected to our atomic values
        let voct_input = ExternalInput::voct(Arc::clone(&self.midi_voct));
        let gate_input = ExternalInput::gate(Arc::clone(&self.midi_gate_val));
        let velocity_input = ExternalInput::cv(Arc::clone(&self.midi_velocity_val));
        let pitch_bend_input = ExternalInput::cv_bipolar(Arc::clone(&self.midi_pitch_bend_val));
        let mod_wheel_input = ExternalInput::cv(Arc::clone(&self.midi_mod_wheel));

        // Add them to the patch
        self.patch.add_boxed("midi_voct", Box::new(voct_input));
        self.patch.add_boxed("midi_gate", Box::new(gate_input));
        self.patch
            .add_boxed("midi_velocity", Box::new(velocity_input));
        self.patch
            .add_boxed("midi_pitch_bend", Box::new(pitch_bend_input));
        self.patch
            .add_boxed("midi_mod_wheel", Box::new(mod_wheel_input));

        self.midi_inputs_created = true;
        Ok(())
    }

    /// Create a MIDI CC input module
    ///
    /// Creates an external input module for a specific MIDI CC number.
    /// The module is named `midi_cc_{cc}` and outputs 0-10V CV.
    pub fn create_midi_cc_input(&mut self, cc: u8) -> Result<(), JsValue> {
        if cc >= 128 {
            return Err(JsValue::from_str("CC number must be 0-127"));
        }

        let name = format!("midi_cc_{}", cc);
        let cc_input = ExternalInput::cv(Arc::clone(&self.midi_cc_values[cc as usize]));
        self.patch.add_boxed(&name, Box::new(cc_input));
        Ok(())
    }

    /// Handle a MIDI Note On message
    ///
    /// Updates the MIDI input modules with the new note values.
    /// The note is converted to V/Oct (0V = C4, 1V = C5).
    /// Velocity is normalized to 0-10V range.
    pub fn midi_note_on(&mut self, note: u8, velocity: u8) -> Result<(), JsValue> {
        // Convert MIDI note to V/Oct (0V = C4, 1V = C5)
        let v_oct = (note as f64 - 60.0) / 12.0;
        // Convert velocity to 0-10V range (modular standard)
        let vel = (velocity as f64 / 127.0) * 10.0;

        // Update atomic values - these automatically propagate to ExternalInput modules
        self.midi_voct.set(v_oct);
        self.midi_velocity_val.set(vel);
        self.midi_gate_val.set(5.0); // Gate high (5V)

        Ok(())
    }

    /// Handle a MIDI Note Off message
    ///
    /// Sets the gate to 0V. Note and velocity values are preserved.
    pub fn midi_note_off(&mut self, _note: u8, _velocity: u8) -> Result<(), JsValue> {
        self.midi_gate_val.set(0.0); // Gate low
        Ok(())
    }

    /// Get the current MIDI note as V/Oct (for connecting to VCO)
    #[wasm_bindgen(getter)]
    pub fn midi_note(&self) -> f64 {
        self.midi_voct.get()
    }

    /// Get the current MIDI velocity (0-10V)
    #[wasm_bindgen(getter)]
    pub fn midi_velocity(&self) -> f64 {
        self.midi_velocity_val.get()
    }

    /// Get the current MIDI gate state
    #[wasm_bindgen(getter)]
    pub fn midi_gate(&self) -> bool {
        self.midi_gate_val.get() > 2.5 // Threshold at 2.5V
    }

    /// Handle a MIDI Control Change message
    ///
    /// Updates the CC value. If a CC input module was created with
    /// `create_midi_cc_input`, it will automatically receive this value.
    /// CC 1 (mod wheel) also updates the `midi_mod_wheel` input.
    pub fn midi_cc(&mut self, cc: u8, value: u8) -> Result<(), JsValue> {
        if cc >= 128 {
            return Err(JsValue::from_str("CC number must be 0-127"));
        }

        // Convert to 0-10V range
        let cv_value = (value as f64 / 127.0) * 10.0;
        self.midi_cc_values[cc as usize].set(cv_value);

        // Special handling for mod wheel (CC 1)
        if cc == 1 {
            self.midi_mod_wheel.set(cv_value);
        }

        Ok(())
    }

    /// Get a MIDI CC value (0-10V)
    pub fn get_midi_cc(&self, cc: u8) -> f64 {
        self.midi_cc_values
            .get(cc as usize)
            .map(|v| v.get())
            .unwrap_or(0.0)
    }

    /// Handle a MIDI Pitch Bend message
    ///
    /// Value should be in the range -1.0 to 1.0.
    /// This is converted to V/Oct (±2 semitones by default).
    pub fn midi_pitch_bend(&mut self, value: f64) -> Result<(), JsValue> {
        // Convert to V/Oct: ±2 semitones = ±2/12 V = ±0.167V
        let bend_semitones = 2.0; // Standard pitch bend range
        let v_oct = value * (bend_semitones / 12.0);
        self.midi_pitch_bend_val.set(v_oct);
        Ok(())
    }

    /// Get the current pitch bend value as V/Oct
    #[wasm_bindgen(getter)]
    pub fn pitch_bend(&self) -> f64 {
        self.midi_pitch_bend_val.get()
    }

    // =========================================================================
    // Port Information
    // =========================================================================

    /// Get port specification for a module type
    pub fn get_port_spec(&self, type_id: &str) -> Result<JsValue, JsValue> {
        let metadata = self
            .registry
            .get_metadata(type_id)
            .ok_or_else(|| JsValue::from_str(&format!("Unknown module type: {}", type_id)))?;

        serde_wasm_bindgen::to_value(&metadata.port_spec)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    // =========================================================================
    // Helper Methods (non-WASM)
    // =========================================================================

    /// Get NodeId by module name (delegates to Patch)
    fn get_node_id_by_name(&self, name: &str) -> Option<NodeId> {
        self.patch.get_node_id_by_name(name)
    }
}

// Helper functions

fn parse_port_ref(s: &str) -> Result<(&str, &str), JsValue> {
    let parts: Vec<&str> = s.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err(JsValue::from_str(&format!(
            "Invalid port reference: {} (expected 'module.port')",
            s
        )));
    }
    Ok((parts[0], parts[1]))
}

fn parse_signal_kind(s: &str) -> Result<SignalKind, JsValue> {
    match s {
        "audio" => Ok(SignalKind::Audio),
        "cv_bipolar" => Ok(SignalKind::CvBipolar),
        "cv_unipolar" => Ok(SignalKind::CvUnipolar),
        "volt_per_octave" => Ok(SignalKind::VoltPerOctave),
        "gate" => Ok(SignalKind::Gate),
        "trigger" => Ok(SignalKind::Trigger),
        "clock" => Ok(SignalKind::Clock),
        _ => Err(JsValue::from_str(&format!("Unknown signal kind: {}", s))),
    }
}
