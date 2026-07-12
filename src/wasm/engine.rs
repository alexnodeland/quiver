//! QuiverEngine - Main WASM interface for Quiver audio engine

use crate::graph::{CableId, NodeId, Patch};
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

/// Well-known module names for the engine-owned MIDI CV sources (see
/// [`QuiverEngine::add_midi_inputs`]). Cable from these to drive audio from MIDI.
pub const MIDI_VOCT_MODULE: &str = "midi_voct";
pub const MIDI_GATE_MODULE: &str = "midi_gate";
pub const MIDI_VELOCITY_MODULE: &str = "midi_velocity";
pub const MIDI_MOD_MODULE: &str = "midi_mod";
pub const MIDI_BEND_MODULE: &str = "midi_bend";

/// Shared atomic handles for the engine-owned MIDI CV sources.
///
/// Each handle is a clone of the `Arc<AtomicF64>` held inside an [`ExternalInput`]
/// module injected into the patch by [`QuiverEngine::add_midi_inputs`]. Writing to
/// a handle (from `midi_note_on`, `midi_cc`, ...) changes the value the matching
/// in-patch module outputs on the next tick, so MIDI actually affects audio once
/// the user cables the source into their graph.
struct MidiInputs {
    /// Pitch as V/Oct (0V = C4 / MIDI note 60).
    voct: Arc<AtomicF64>,
    /// Gate: 5.0 while a note is held, 0.0 otherwise.
    gate: Arc<AtomicF64>,
    /// Note velocity normalized to 0..1.
    velocity: Arc<AtomicF64>,
    /// Mod wheel (CC1) normalized to 0..1.
    modulation: Arc<AtomicF64>,
    /// Pitch bend as a V/Oct offset (±2 semitones at full deflection).
    bend: Arc<AtomicF64>,
}

impl MidiInputs {
    fn new() -> Self {
        Self {
            voct: Arc::new(AtomicF64::new(0.0)),
            gate: Arc::new(AtomicF64::new(0.0)),
            velocity: Arc::new(AtomicF64::new(0.0)),
            modulation: Arc::new(AtomicF64::new(0.0)),
            bend: Arc::new(AtomicF64::new(0.0)),
        }
    }
}

/// Main WASM interface for Quiver audio engine
#[wasm_bindgen]
pub struct QuiverEngine {
    patch: Patch,
    registry: ModuleRegistry,
    observer: StateObserver,
    sample_rate: f64,

    // Preallocated block-processing buffers (Q093). Reused across `process_block`
    // calls and grown on demand (never shrunk), so steady-state rendering does no
    // per-block heap or per-sample JS allocation.
    block_left: Vec<f64>,
    block_right: Vec<f64>,
    block_interleaved: Vec<f32>,

    // Observer decimation (Q093): collect observer state once every `observer_interval`
    // blocks instead of on every render quantum. `observer_countdown` counts blocks
    // down to the next collection (0 = collect on this block).
    observer_interval: u32,
    observer_countdown: u32,

    // Engine-owned MIDI CV source handles, shared with in-patch ExternalInput
    // modules created by `add_midi_inputs` (Q096).
    midi: MidiInputs,

    // MIDI state mirrored for the scalar getters (`midi_note`, `midi_velocity`, ...).
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
            block_left: Vec::new(),
            block_right: Vec::new(),
            block_interleaved: Vec::new(),
            // Default: collect observer state every 8th block. UIs poll at ~display
            // rate, so per-block collection is wasted work; raise/lower with
            // `set_observer_interval`.
            observer_interval: 8,
            observer_countdown: 0,
            midi: MidiInputs::new(),
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

    /// Connect two ports (format: "module.port").
    ///
    /// Returns the new cable's stable [`CableId`](crate::graph::CableId) as a number.
    /// Hold onto it and pass it to [`disconnect_cable`](Self::disconnect_cable) to
    /// remove exactly this connection later, even after other cables change.
    pub fn connect(&mut self, from: &str, to: &str) -> Result<usize, JsValue> {
        let (from_ref, to_ref) = self.resolve_ports(from, to)?;
        self.patch
            .connect(from_ref, to_ref)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))
    }

    /// Connect with attenuation. Returns the new cable's stable `CableId`.
    pub fn connect_attenuated(
        &mut self,
        from: &str,
        to: &str,
        attenuation: f64,
    ) -> Result<usize, JsValue> {
        let (from_ref, to_ref) = self.resolve_ports(from, to)?;
        self.patch
            .connect_attenuated(from_ref, to_ref, attenuation)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))
    }

    /// Connect with full modulation (attenuation and offset).
    /// Returns the new cable's stable `CableId`.
    pub fn connect_modulated(
        &mut self,
        from: &str,
        to: &str,
        attenuation: f64,
        offset: f64,
    ) -> Result<usize, JsValue> {
        let (from_ref, to_ref) = self.resolve_ports(from, to)?;
        self.patch
            .connect_modulated(from_ref, to_ref, attenuation, offset)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))
    }

    /// Disconnect a cable by its stable [`CableId`](crate::graph::CableId).
    ///
    /// This is the id returned by [`connect`](Self::connect) and friends. It stays
    /// valid regardless of how many other cables have been removed since.
    pub fn disconnect_cable(&mut self, cable_id: usize) -> Result<(), JsValue> {
        self.patch
            .disconnect(cable_id as CableId)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))
    }

    /// Disconnect the cable at the given position in the cable list.
    ///
    /// Convenience for callers that track cables positionally. Resolves the position
    /// to the cable's stable [`CableId`](crate::graph::CableId) and removes it, so the
    /// underlying removal is id-based (never off-by-one after prior removals).
    pub fn disconnect_by_index(&mut self, cable_index: usize) -> Result<(), JsValue> {
        let cable_id = self
            .patch
            .cables()
            .get(cable_index)
            .map(|c| c.id)
            .ok_or_else(|| JsValue::from_str(&format!("Invalid cable index: {}", cable_index)))?;
        self.patch
            .disconnect(cable_id)
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

    /// Process a single sample and return stereo output as a `Float64Array`
    /// `[left, right]`.
    pub fn tick(&mut self) -> Box<[f64]> {
        let (left, right) = self.patch.tick();
        Box::new([left, right])
    }

    /// Process a block of `num_samples` frames and return the interleaved stereo
    /// result as a `Float32Array` of length `num_samples * 2` (`[l0, r0, l1, r1, ...]`).
    ///
    /// # Zero-allocation
    ///
    /// The engine keeps preallocated, reused L/R and interleaved buffers (grown on
    /// demand). Rendering uses the allocation-free [`Patch::tick_block`], so a
    /// steady-state render quantum performs no per-sample or per-block heap
    /// allocation. Output is safety-clamped to ±10V to prevent speaker/hearing
    /// damage from runaway signals.
    ///
    /// # Ownership rule (important)
    ///
    /// The returned `Float32Array` is a **view into WASM linear memory**, valid only
    /// until the next call into this engine (which reuses/grows the buffer) or
    /// `free`. Read it immediately — e.g. copy into your own array with
    /// `Array.from(...)` or `myBuffer.set(...)` — before calling any other engine
    /// method. Do not retain the returned object.
    pub fn process_block(&mut self, num_samples: usize) -> js_sys::Float32Array {
        const SAFETY_LIMIT: f64 = 10.0; // Max output voltage

        // Grow (never shrink) the reused buffers to fit this block.
        if self.block_left.len() < num_samples {
            self.block_left.resize(num_samples, 0.0);
            self.block_right.resize(num_samples, 0.0);
        }
        let interleaved_len = num_samples * 2;
        if self.block_interleaved.len() < interleaved_len {
            self.block_interleaved.resize(interleaved_len, 0.0);
        }

        // Allocation-free block render into the reused L/R slices.
        self.patch.tick_block(
            &mut self.block_left[..num_samples],
            &mut self.block_right[..num_samples],
        );

        // Interleave + safety-clamp into the reused f32 buffer.
        for i in 0..num_samples {
            let left = self.block_left[i].clamp(-SAFETY_LIMIT, SAFETY_LIMIT) as f32;
            let right = self.block_right[i].clamp(-SAFETY_LIMIT, SAFETY_LIMIT) as f32;
            self.block_interleaved[i * 2] = left;
            self.block_interleaved[i * 2 + 1] = right;
        }

        // Decimated observer collection (Q093): collect only every `observer_interval`
        // blocks via a countdown (avoids per-sample work; cheap no-op with no
        // subscriptions). Countdown-based rather than modulo to stay MSRV-friendly.
        if self.observer_interval <= 1 || self.observer_countdown == 0 {
            self.observer.collect_from_patch(&self.patch);
            self.observer_countdown = self.observer_interval.saturating_sub(1);
        } else {
            self.observer_countdown -= 1;
        }

        // SAFETY: `Float32Array::view` returns a view into WASM memory backed by
        // `block_interleaved`. It is valid until the next engine call reuses/grows
        // the buffer (documented ownership rule above); callers read it synchronously.
        unsafe { js_sys::Float32Array::view(&self.block_interleaved[..interleaved_len]) }
    }

    /// Set how often the state observer collects values, in blocks.
    ///
    /// `1` collects on every [`process_block`](Self::process_block); higher values
    /// decimate collection (default `8`). Clamped to a minimum of 1.
    pub fn set_observer_interval(&mut self, blocks: u32) {
        self.observer_interval = blocks.max(1);
        // Collect promptly under the new cadence.
        self.observer_countdown = 0;
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

    /// Inject the engine-owned MIDI CV source modules into the current patch.
    ///
    /// Adds five [`ExternalInput`](crate::io::ExternalInput) modules the user can
    /// cable from to make MIDI actually drive audio:
    ///
    /// | Module name      | Signal          | Fed by                         |
    /// |------------------|-----------------|--------------------------------|
    /// | `midi_voct`      | V/Oct           | `midi_note_on` (pitch)         |
    /// | `midi_gate`      | Gate (0/5V)     | `midi_note_on` / `midi_note_off` |
    /// | `midi_velocity`  | CV unipolar 0–1 | `midi_note_on` (velocity)      |
    /// | `midi_mod`       | CV unipolar 0–1 | `midi_cc(1, ...)` (mod wheel)  |
    /// | `midi_bend`      | CV bipolar V/Oct| `midi_pitch_bend`              |
    ///
    /// Each exposes a single `out` port (e.g. cable `midi_voct.out` -> `vco.voct`).
    /// Idempotent: modules already present (by name) are left untouched, so it is
    /// safe to call after building or loading a patch. Marks the patch dirty.
    ///
    /// Note: these modules are engine-managed and are not in the module registry, so
    /// a patch saved while they are present cannot be re-instantiated by
    /// `load_patch` on a fresh engine — call `add_midi_inputs()` again after loading.
    pub fn add_midi_inputs(&mut self) {
        if self.patch.get_node_id_by_name(MIDI_VOCT_MODULE).is_none() {
            self.patch.add_boxed(
                MIDI_VOCT_MODULE,
                Box::new(ExternalInput::voct(Arc::clone(&self.midi.voct))),
            );
        }
        if self.patch.get_node_id_by_name(MIDI_GATE_MODULE).is_none() {
            self.patch.add_boxed(
                MIDI_GATE_MODULE,
                Box::new(ExternalInput::gate(Arc::clone(&self.midi.gate))),
            );
        }
        if self
            .patch
            .get_node_id_by_name(MIDI_VELOCITY_MODULE)
            .is_none()
        {
            self.patch.add_boxed(
                MIDI_VELOCITY_MODULE,
                Box::new(ExternalInput::cv(Arc::clone(&self.midi.velocity))),
            );
        }
        if self.patch.get_node_id_by_name(MIDI_MOD_MODULE).is_none() {
            self.patch.add_boxed(
                MIDI_MOD_MODULE,
                Box::new(ExternalInput::cv(Arc::clone(&self.midi.modulation))),
            );
        }
        if self.patch.get_node_id_by_name(MIDI_BEND_MODULE).is_none() {
            self.patch.add_boxed(
                MIDI_BEND_MODULE,
                Box::new(ExternalInput::cv_bipolar(Arc::clone(&self.midi.bend))),
            );
        }
    }

    /// Handle a MIDI Note On message.
    ///
    /// Updates both the scalar getters and the shared `midi_voct` / `midi_gate` /
    /// `midi_velocity` CV sources (see [`add_midi_inputs`](Self::add_midi_inputs)),
    /// so a cabled patch responds on the next processed sample.
    pub fn midi_note_on(&mut self, note: u8, velocity: u8) -> Result<(), JsValue> {
        // Convert MIDI note to V/Oct (0V = C4, 1V = C5).
        let v_oct = (note as f64 - 60.0) / 12.0;
        // Convert velocity to 0-1 range.
        let vel = velocity as f64 / 127.0;

        self.midi_note = Some(v_oct);
        self.midi_velocity = Some(vel);
        self.midi_gate = true;

        // Drive the in-patch CV sources.
        self.midi.voct.set(v_oct);
        self.midi.velocity.set(vel);
        self.midi.gate.set(5.0);

        Ok(())
    }

    /// Handle a MIDI Note Off message. Closes the shared gate CV source.
    pub fn midi_note_off(&mut self, _note: u8, _velocity: u8) -> Result<(), JsValue> {
        self.midi_gate = false;
        self.midi.gate.set(0.0);
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

    /// Handle a MIDI Control Change message.
    ///
    /// All CCs are stored for retrieval via [`get_midi_cc`](Self::get_midi_cc). CC1
    /// (mod wheel) additionally drives the shared `midi_mod` CV source.
    pub fn midi_cc(&mut self, cc: u8, value: u8) -> Result<(), JsValue> {
        let normalized = value as f64 / 127.0;
        self.midi_cc_values[cc as usize] = normalized;
        if cc == 1 {
            self.midi.modulation.set(normalized);
        }
        Ok(())
    }

    /// Get a MIDI CC value (0-1 normalized)
    pub fn get_midi_cc(&self, cc: u8) -> f64 {
        self.midi_cc_values.get(cc as usize).copied().unwrap_or(0.0)
    }

    /// Handle a MIDI Pitch Bend message (`value` in -1..1).
    ///
    /// Drives the shared `midi_bend` CV source as a V/Oct offset of ±2 semitones at
    /// full deflection. The [`pitch_bend`](Self::pitch_bend) getter still returns the
    /// raw -1..1 value.
    pub fn midi_pitch_bend(&mut self, value: f64) -> Result<(), JsValue> {
        self.midi_pitch_bend_value = value;
        // ±2 semitones = ±(2/12) V.
        self.midi.bend.set(value * (2.0 / 12.0));
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

    /// Resolve two `"module.port"` references into concrete [`PortRef`]s.
    ///
    /// Uses the fallible [`NodeHandle::output`](crate::graph::NodeHandle::output) /
    /// [`input`](crate::graph::NodeHandle::input) so a bad port name yields a clean
    /// `JsValue` error (listing the valid ports) instead of a panic/`unreachable`.
    fn resolve_ports(
        &self,
        from: &str,
        to: &str,
    ) -> Result<(crate::graph::PortRef, crate::graph::PortRef), JsValue> {
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

        let from_ref = from_handle
            .output(from_port)
            .map_err(|e| JsValue::from_str(&format!("{}", e)))?;
        let to_ref = to_handle
            .input(to_port)
            .map_err(|e| JsValue::from_str(&format!("{}", e)))?;
        Ok((from_ref, to_ref))
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
