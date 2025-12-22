//! QuiverEngine - Main WASM interface for Quiver audio engine

use crate::graph::{NodeId, Patch};
use crate::observer::{StateObserver, SubscriptionTarget};
use crate::port::{ports_compatible, SignalColors, SignalKind};
use crate::serialize::{ModuleRegistry, PatchDef};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use wasm_bindgen::prelude::*;

/// Main WASM interface for Quiver audio engine
#[wasm_bindgen]
pub struct QuiverEngine {
    patch: Patch,
    registry: ModuleRegistry,
    observer: StateObserver,
    sample_rate: f64,
    // MIDI state for worklet integration
    midi_note: Option<f64>,
    midi_velocity: Option<f64>,
    midi_gate: bool,
    midi_cc_values: [f64; 128],
    midi_pitch_bend_value: f64,
}

#[wasm_bindgen]
impl QuiverEngine {
    /// Create a new Quiver engine
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f64) -> Self {
        // Initialize panic hook for better error messages
        console_error_panic_hook::set_once();

        Self {
            patch: Patch::new(sample_rate),
            registry: ModuleRegistry::new(),
            observer: StateObserver::new(),
            sample_rate,
            midi_note: None,
            midi_velocity: None,
            midi_gate: false,
            midi_cc_values: [0.0; 128],
            midi_pitch_bend_value: 0.0,
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
    pub fn process_block(&mut self, num_samples: usize) -> js_sys::Float32Array {
        let output = js_sys::Float32Array::new_with_length((num_samples * 2) as u32);

        for i in 0..num_samples {
            let (left, right) = self.patch.tick();
            output.set_index((i * 2) as u32, left as f32);
            output.set_index((i * 2 + 1) as u32, right as f32);
        }

        // Collect observer updates after processing
        self.observer.collect_from_patch(&self.patch);

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

    /// Handle a MIDI Note On message
    ///
    /// This is a convenience method for AudioWorklet MIDI handling.
    /// The note and velocity are normalized to the typical synth CV ranges.
    pub fn midi_note_on(&mut self, note: u8, velocity: u8) -> Result<(), JsValue> {
        // Convert MIDI note to V/Oct (0V = C4, 1V = C5)
        let v_oct = (note as f64 - 60.0) / 12.0;
        // Convert velocity to 0-1 range
        let vel = velocity as f64 / 127.0;

        // These would typically be connected to external inputs
        // For now, just store them for retrieval
        self.midi_note = Some(v_oct);
        self.midi_velocity = Some(vel);
        self.midi_gate = true;

        Ok(())
    }

    /// Handle a MIDI Note Off message
    pub fn midi_note_off(&mut self, _note: u8, _velocity: u8) -> Result<(), JsValue> {
        self.midi_gate = false;
        Ok(())
    }

    /// Get the current MIDI note as V/Oct (for connecting to VCO)
    #[wasm_bindgen(getter)]
    pub fn midi_note(&self) -> f64 {
        self.midi_note.unwrap_or(0.0)
    }

    /// Get the current MIDI velocity (0-1)
    #[wasm_bindgen(getter)]
    pub fn midi_velocity(&self) -> f64 {
        self.midi_velocity.unwrap_or(0.0)
    }

    /// Get the current MIDI gate state
    #[wasm_bindgen(getter)]
    pub fn midi_gate(&self) -> bool {
        self.midi_gate
    }

    /// Handle a MIDI Control Change message
    pub fn midi_cc(&mut self, cc: u8, value: u8) -> Result<(), JsValue> {
        // Store CC values for retrieval
        self.midi_cc_values[cc as usize] = value as f64 / 127.0;
        Ok(())
    }

    /// Get a MIDI CC value (0-1 normalized)
    pub fn get_midi_cc(&self, cc: u8) -> f64 {
        self.midi_cc_values.get(cc as usize).copied().unwrap_or(0.0)
    }

    /// Handle a MIDI Pitch Bend message (-1 to 1)
    pub fn midi_pitch_bend(&mut self, value: f64) -> Result<(), JsValue> {
        self.midi_pitch_bend_value = value;
        Ok(())
    }

    /// Get the current pitch bend value (-1 to 1)
    #[wasm_bindgen(getter)]
    pub fn pitch_bend(&self) -> f64 {
        self.midi_pitch_bend_value
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
