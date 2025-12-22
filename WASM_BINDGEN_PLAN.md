# WASM-Bindgen Implementation Plan

This document provides a detailed implementation plan for adding wasm-bindgen support to Quiver, enabling the browser-based architecture described in `GUI_FRAMEWORK_PLAN.md`.

## Current State Analysis

### Already Implemented (Foundation Complete)

| Component | File | Status |
|-----------|------|--------|
| Introspection Types | `src/introspection.rs` | `ParamInfo`, `ParamCurve`, `ControlType`, `ValueFormat` with serde |
| Module Introspection | `src/introspection_impls.rs` | `ModuleIntrospection` for all 36 modules |
| Real-Time Observer | `src/observer.rs` | `StateObserver`, `ObservableValue`, `SubscriptionTarget` with serde |
| Serialization | `src/serialize.rs` | `PatchDef`, `CatalogResponse`, `ModuleRegistry` with serde |
| Signal Semantics | `src/port.rs` | `SignalKind`, `SignalColors`, `Compatibility` with serde |
| TypeScript Types | `packages/@quiver/types` | Complete type definitions matching Rust |
| React Utilities | `packages/@quiver/react` | React Flow mappings and helpers |

### Missing for WASM (Implementation Required)

1. **Cargo.toml** - `wasm` feature flag with wasm-bindgen dependencies
2. **Tsify Derives** - `#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]` on types
3. **WASM Module** - `src/wasm/` with QuiverEngine wrapper
4. **Build Configuration** - wasm-pack setup
5. **AudioWorklet** - Browser audio thread integration

---

## Implementation Phases

### Phase 1: Feature Flag & Dependencies

**Objective:** Configure Cargo.toml for WASM target compilation.

**File: `Cargo.toml`**

```toml
[features]
default = ["std"]
std = ["alloc", "serde/std", "slotmap/std", "rand"]
alloc = ["serde_json"]
simd = []

# WASM target (browser)
wasm = [
    "alloc",
    "wasm-bindgen",
    "tsify",
    "serde-wasm-bindgen",
    "js-sys",
    "web-sys",
    "console_error_panic_hook",
]

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
tsify = { version = "0.4", features = ["js"] }
serde-wasm-bindgen = "0.6"
js-sys = "0.3"
web-sys = { version = "0.3", features = ["console"] }
console_error_panic_hook = "0.1"
```

**Deliverables:**
- [ ] Add wasm feature flag to Cargo.toml
- [ ] Add conditional dependencies
- [ ] Verify `cargo check --features wasm --target wasm32-unknown-unknown` passes

---

### Phase 2: Tsify Derives for Type Generation

**Objective:** Add `#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]` to all GUI-facing types so TypeScript types are auto-generated from Rust.

**Files to modify:**

#### `src/introspection.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct ParamInfo { /* ... */ }

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ParamCurve { /* ... */ }

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum ControlType { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ValueFormat { /* ... */ }
```

#### `src/observer.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ObservableValue { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum SubscriptionTarget { /* ... */ }
```

#### `src/serialize.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct PatchDef { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct ModuleDef { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct CableDef { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CatalogResponse { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct ModuleCatalogEntry { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct PortSummary { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct ValidationError { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct ValidationResult { /* ... */ }
```

#### `src/port.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub enum SignalKind { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct SignalColors { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct PortInfo { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum Compatibility { /* ... */ }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct PortDef { /* ... */ }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct PortSpec { /* ... */ }
```

**Deliverables:**
- [ ] Add tsify derives to `src/introspection.rs`
- [ ] Add tsify derives to `src/observer.rs`
- [ ] Add tsify derives to `src/serialize.rs`
- [ ] Add tsify derives to `src/port.rs`
- [ ] Run `cargo check --features wasm --target wasm32-unknown-unknown`

---

### Phase 3: WASM Engine Module

**Objective:** Create the `src/wasm/` module with `QuiverEngine` wrapper exposing all APIs to JavaScript.

**File Structure:**
```
src/wasm/
├── mod.rs          # Module exports
├── engine.rs       # QuiverEngine wasm_bindgen wrapper
└── error.rs        # JsError conversions
```

#### `src/wasm/mod.rs`

```rust
//! WASM bindings for Quiver
//!
//! This module provides the JavaScript-facing API for running Quiver
//! in a browser environment via WebAssembly.

#[cfg(feature = "wasm")]
mod engine;
#[cfg(feature = "wasm")]
mod error;

#[cfg(feature = "wasm")]
pub use engine::QuiverEngine;
#[cfg(feature = "wasm")]
pub use error::QuiverError;

// Re-export wasm_bindgen for convenience
#[cfg(feature = "wasm")]
pub use wasm_bindgen::prelude::*;
```

#### `src/wasm/error.rs`

```rust
use wasm_bindgen::prelude::*;
use alloc::string::String;

/// Error type for WASM bindings
#[wasm_bindgen]
pub struct QuiverError {
    message: String,
}

#[wasm_bindgen]
impl QuiverError {
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }
}

impl From<crate::graph::PatchError> for QuiverError {
    fn from(e: crate::graph::PatchError) -> Self {
        Self {
            message: alloc::format!("{:?}", e),
        }
    }
}

impl From<String> for QuiverError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for QuiverError {
    fn from(message: &str) -> Self {
        Self {
            message: message.into(),
        }
    }
}
```

#### `src/wasm/engine.rs`

```rust
use wasm_bindgen::prelude::*;
use crate::graph::Patch;
use crate::serialize::{ModuleRegistry, PatchDef, CatalogResponse};
use crate::observer::{StateObserver, SubscriptionTarget, ObservableValue};
use crate::introspection::{ModuleIntrospection, ParamInfo};
use crate::port::{SignalColors, Compatibility, ports_compatible, SignalKind};
use alloc::vec::Vec;
use alloc::string::String;

/// Main WASM interface for Quiver audio engine
#[wasm_bindgen]
pub struct QuiverEngine {
    patch: Patch,
    registry: ModuleRegistry,
    observer: StateObserver,
    sample_rate: f64,
}

#[wasm_bindgen]
impl QuiverEngine {
    /// Create a new Quiver engine
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f64) -> Self {
        // Initialize panic hook for better error messages
        #[cfg(feature = "wasm")]
        console_error_panic_hook::set_once();

        Self {
            patch: Patch::new(sample_rate),
            registry: ModuleRegistry::new(),
            observer: StateObserver::new(),
            sample_rate,
        }
    }

    /// Get the sample rate
    #[wasm_bindgen(getter)]
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    // =========================================================================
    // Catalog API (Phase 3)
    // =========================================================================

    /// Get the full module catalog
    pub fn get_catalog(&self) -> Result<JsValue, JsValue> {
        let catalog = self.registry.catalog();
        serde_wasm_bindgen::to_value(&catalog)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Search modules by query string
    pub fn search_modules(&self, query: &str) -> Result<JsValue, JsValue> {
        let results = self.registry.search(query);
        serde_wasm_bindgen::to_value(&results)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Get modules by category
    pub fn get_modules_by_category(&self, category: &str) -> Result<JsValue, JsValue> {
        let results = self.registry.by_category(category);
        serde_wasm_bindgen::to_value(&results)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    // =========================================================================
    // Signal Semantics API (Phase 2)
    // =========================================================================

    /// Get default signal colors
    pub fn get_signal_colors(&self) -> Result<JsValue, JsValue> {
        let colors = SignalColors::default();
        serde_wasm_bindgen::to_value(&colors)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Check port compatibility between two signal kinds
    pub fn check_compatibility(&self, from: &str, to: &str) -> Result<JsValue, JsValue> {
        let from_kind = parse_signal_kind(from)?;
        let to_kind = parse_signal_kind(to)?;
        let compat = ports_compatible(from_kind, to_kind);
        serde_wasm_bindgen::to_value(&compat)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    // =========================================================================
    // Patch Operations
    // =========================================================================

    /// Load a patch from JSON
    pub fn load_patch(&mut self, patch_json: JsValue) -> Result<(), JsValue> {
        let patch_def: PatchDef = serde_wasm_bindgen::from_value(patch_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        self.patch = Patch::from_def(&patch_def, &self.registry, self.sample_rate)
            .map_err(|e| JsValue::from_str(&alloc::format!("{:?}", e)))?;

        Ok(())
    }

    /// Save the current patch to JSON
    pub fn save_patch(&self, name: &str) -> Result<JsValue, JsValue> {
        let patch_def = self.patch.to_def(name);
        serde_wasm_bindgen::to_value(&patch_def)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Validate a patch definition
    pub fn validate_patch(&self, patch_json: JsValue) -> Result<JsValue, JsValue> {
        let patch_def: PatchDef = serde_wasm_bindgen::from_value(patch_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let result = patch_def.validate_with_registry(&self.registry);
        serde_wasm_bindgen::to_value(&result)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    // =========================================================================
    // Module Operations
    // =========================================================================

    /// Add a module to the patch
    pub fn add_module(&mut self, type_id: &str, name: &str) -> Result<(), JsValue> {
        let module = self.registry.instantiate(type_id, self.sample_rate)
            .ok_or_else(|| JsValue::from_str(&alloc::format!("Unknown module type: {}", type_id)))?;

        self.patch.add_boxed(name, module);
        Ok(())
    }

    /// Remove a module from the patch
    pub fn remove_module(&mut self, name: &str) -> Result<(), JsValue> {
        self.patch.remove_by_name(name)
            .map_err(|e| JsValue::from_str(&alloc::format!("{:?}", e)))
    }

    /// Set module position for UI layout
    pub fn set_module_position(&mut self, name: &str, x: f32, y: f32) -> Result<(), JsValue> {
        let node_id = self.patch.get_node_id_by_name(name)
            .ok_or_else(|| JsValue::from_str(&alloc::format!("Unknown module: {}", name)))?;

        self.patch.set_position(node_id, (x, y));
        Ok(())
    }

    // =========================================================================
    // Cable Operations
    // =========================================================================

    /// Connect two ports
    pub fn connect(&mut self, from: &str, to: &str) -> Result<(), JsValue> {
        // Parse "module.port" format
        let (from_module, from_port) = parse_port_ref(from)?;
        let (to_module, to_port) = parse_port_ref(to)?;

        let from_handle = self.patch.get_handle_by_name(from_module)
            .ok_or_else(|| JsValue::from_str(&alloc::format!("Unknown module: {}", from_module)))?;
        let to_handle = self.patch.get_handle_by_name(to_module)
            .ok_or_else(|| JsValue::from_str(&alloc::format!("Unknown module: {}", to_module)))?;

        self.patch.connect(from_handle.out(from_port), to_handle.in_(to_port))
            .map_err(|e| JsValue::from_str(&alloc::format!("{:?}", e)))
    }

    /// Disconnect two ports
    pub fn disconnect(&mut self, from: &str, to: &str) -> Result<(), JsValue> {
        let (from_module, from_port) = parse_port_ref(from)?;
        let (to_module, to_port) = parse_port_ref(to)?;

        let from_handle = self.patch.get_handle_by_name(from_module)
            .ok_or_else(|| JsValue::from_str(&alloc::format!("Unknown module: {}", from_module)))?;
        let to_handle = self.patch.get_handle_by_name(to_module)
            .ok_or_else(|| JsValue::from_str(&alloc::format!("Unknown module: {}", to_module)))?;

        self.patch.disconnect(from_handle.out(from_port), to_handle.in_(to_port))
            .map_err(|e| JsValue::from_str(&alloc::format!("{:?}", e)))
    }

    // =========================================================================
    // Introspection API (Phase 1)
    // =========================================================================

    /// Get parameters for a module
    pub fn get_params(&self, node_name: &str) -> Result<JsValue, JsValue> {
        let params = self.patch.get_module_params(node_name)
            .map_err(|e| JsValue::from_str(&alloc::format!("{:?}", e)))?;

        serde_wasm_bindgen::to_value(&params)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Set a parameter value
    pub fn set_param(&mut self, node_name: &str, param_id: &str, value: f64) -> Result<(), JsValue> {
        self.patch.set_module_param(node_name, param_id, value)
            .map_err(|e| JsValue::from_str(&alloc::format!("{:?}", e)))
    }

    // =========================================================================
    // Real-Time Bridge API (Phase 4)
    // =========================================================================

    /// Subscribe to real-time value updates
    pub fn subscribe(&mut self, targets: JsValue) -> Result<(), JsValue> {
        let targets: Vec<SubscriptionTarget> = serde_wasm_bindgen::from_value(targets)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        self.observer.add_subscriptions(targets);
        Ok(())
    }

    /// Unsubscribe from real-time value updates
    pub fn unsubscribe(&mut self, target_keys: JsValue) -> Result<(), JsValue> {
        let keys: Vec<String> = serde_wasm_bindgen::from_value(target_keys)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        self.observer.remove_subscriptions(&keys);
        Ok(())
    }

    /// Poll for pending updates (called from requestAnimationFrame)
    pub fn poll_updates(&mut self) -> Result<JsValue, JsValue> {
        let updates = self.observer.drain_updates();
        serde_wasm_bindgen::to_value(&updates)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    // =========================================================================
    // Audio Processing
    // =========================================================================

    /// Process a block of audio samples
    /// Returns stereo output as Float32Array [left0, right0, left1, right1, ...]
    pub fn process(&mut self, buffer_size: usize) -> Result<js_sys::Float32Array, JsValue> {
        // Process the patch
        let output = self.patch.process_block(buffer_size);

        // Collect updates for subscribed values
        self.observer.collect_updates_from_patch(&self.patch);

        // Convert to interleaved stereo Float32Array
        let stereo_output = js_sys::Float32Array::new_with_length((buffer_size * 2) as u32);

        for i in 0..buffer_size {
            stereo_output.set_index((i * 2) as u32, output.left[i] as f32);
            stereo_output.set_index((i * 2 + 1) as u32, output.right[i] as f32);
        }

        Ok(stereo_output)
    }

    /// Reset all module state
    pub fn reset(&mut self) {
        self.patch.reset();
    }

    /// Compile the patch (required after adding/removing modules or cables)
    pub fn compile(&mut self) -> Result<(), JsValue> {
        self.patch.compile()
            .map_err(|e| JsValue::from_str(&alloc::format!("{:?}", e)))
    }
}

// Helper functions

fn parse_port_ref(s: &str) -> Result<(&str, &str), JsValue> {
    let parts: Vec<&str> = s.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err(JsValue::from_str(&alloc::format!("Invalid port reference: {}", s)));
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
        _ => Err(JsValue::from_str(&alloc::format!("Unknown signal kind: {}", s))),
    }
}
```

#### Update `src/lib.rs`

```rust
// Add WASM module export
#[cfg(feature = "wasm")]
pub mod wasm;
```

**Deliverables:**
- [ ] Create `src/wasm/mod.rs`
- [ ] Create `src/wasm/error.rs`
- [ ] Create `src/wasm/engine.rs`
- [ ] Update `src/lib.rs` to export wasm module
- [ ] Add missing methods to `Patch` (`get_module_params`, `set_module_param`, etc.)
- [ ] Verify compilation with `cargo check --features wasm --target wasm32-unknown-unknown`

---

### Phase 4: Missing Patch Methods

**Objective:** Add helper methods to `Patch` that are needed by the WASM bindings.

**File: `src/graph.rs`**

Add the following methods to the `Patch` impl:

```rust
impl Patch {
    /// Get module params by node name
    pub fn get_module_params(&self, name: &str) -> Result<Vec<ParamInfo>, PatchError> {
        let node_id = self.get_node_id_by_name(name)
            .ok_or_else(|| PatchError::NodeNotFound(name.to_string()))?;

        let module = self.get_module(node_id)?;
        Ok(module.param_infos())
    }

    /// Set a parameter value by node name and param id
    pub fn set_module_param(&mut self, name: &str, param_id: &str, value: f64) -> Result<bool, PatchError> {
        let node_id = self.get_node_id_by_name(name)
            .ok_or_else(|| PatchError::NodeNotFound(name.to_string()))?;

        let module = self.get_module_mut(node_id)?;
        Ok(module.set_param_by_id(param_id, value))
    }

    /// Get node ID by name
    pub fn get_node_id_by_name(&self, name: &str) -> Option<NodeId> {
        // Implementation depends on internal structure
    }

    /// Get handle by name
    pub fn get_handle_by_name(&self, name: &str) -> Option<NodeHandle> {
        // Implementation depends on internal structure
    }

    /// Remove a node by name
    pub fn remove_by_name(&mut self, name: &str) -> Result<(), PatchError> {
        let node_id = self.get_node_id_by_name(name)
            .ok_or_else(|| PatchError::NodeNotFound(name.to_string()))?;
        self.remove(node_id)
    }

    /// Process a block of samples
    pub fn process_block(&mut self, buffer_size: usize) -> StereoOutput {
        // Process and return stereo output
    }
}
```

**Deliverables:**
- [ ] Add `get_module_params` method
- [ ] Add `set_module_param` method
- [ ] Add `get_node_id_by_name` method
- [ ] Add `get_handle_by_name` method
- [ ] Add `remove_by_name` method
- [ ] Add `process_block` returning stereo output
- [ ] Unit tests for new methods

---

### Phase 5: Observer Integration with Patch

**Objective:** Add method to StateObserver to collect updates from patch processing.

**File: `src/observer.rs`**

```rust
impl StateObserver {
    /// Collect observable values from the patch after processing
    pub fn collect_updates_from_patch(&mut self, patch: &Patch) {
        for target in &self.subscriptions {
            match target {
                SubscriptionTarget::Param { node_id, param_id } => {
                    if let Ok(params) = patch.get_module_params(node_id) {
                        if let Some(param) = params.iter().find(|p| &p.id == param_id) {
                            self.push_update(ObservableValue::Param {
                                node_id: node_id.clone(),
                                param_id: param_id.clone(),
                                value: param.value,
                            });
                        }
                    }
                }
                SubscriptionTarget::Level { node_id, port_id } => {
                    // Get samples from port and calculate levels
                    // Implementation depends on patch internals
                }
                // ... other subscription types
                _ => {}
            }
        }
    }
}
```

**Deliverables:**
- [ ] Add `collect_updates_from_patch` method to StateObserver
- [ ] Implement level metering collection
- [ ] Implement gate state collection
- [ ] Unit tests

---

### Phase 6: Build Configuration

**Objective:** Set up wasm-pack for building the WASM package.

**File: `Cargo.toml` (additions)**

```toml
[lib]
crate-type = ["cdylib", "rlib"]

[profile.release]
opt-level = "z"
lto = true

[package.metadata.wasm-pack.profile.release]
wasm-opt = ["-O4"]
```

**File: `packages/@quiver/wasm/package.json`**

```json
{
  "name": "@quiver/wasm",
  "version": "0.1.0",
  "description": "Quiver modular synthesizer WASM bindings",
  "main": "quiver.js",
  "types": "quiver.d.ts",
  "files": [
    "quiver.js",
    "quiver.d.ts",
    "quiver_bg.wasm",
    "quiver_bg.js"
  ],
  "scripts": {
    "build": "wasm-pack build ../../.. --target web --features wasm --out-dir packages/@quiver/wasm"
  },
  "peerDependencies": {
    "@quiver/types": "^0.1.0"
  }
}
```

**Build command:**
```bash
wasm-pack build --target web --features wasm --out-dir packages/@quiver/wasm
```

**Deliverables:**
- [ ] Update Cargo.toml with crate-type and optimization settings
- [ ] Create packages/@quiver/wasm/package.json
- [ ] Add build script to root package.json
- [ ] Verify `wasm-pack build` succeeds

---

### Phase 7: React Hooks for WASM

**Objective:** Add React hooks to `@quiver/react` for WASM bridge integration.

**File: `packages/@quiver/react/src/hooks.ts`**

```typescript
import { useEffect, useState, useRef, useCallback, useMemo } from 'react';
import type {
  ObservableValue,
  SubscriptionTarget,
  ParamInfo,
  CatalogResponse,
} from '@quiver/types';
import {
  getObservableValueKey,
  getSubscriptionTargetKey,
} from '@quiver/types';

// Import the WASM engine type
import type { QuiverEngine } from '@quiver/wasm';

/**
 * Hook for subscribing to real-time Quiver value updates
 */
export function useQuiverUpdates(
  engine: QuiverEngine | null,
  targets: SubscriptionTarget[]
): Map<string, ObservableValue> {
  const [values, setValues] = useState<Map<string, ObservableValue>>(new Map());
  const targetsRef = useRef(targets);

  useEffect(() => {
    if (!engine) return;

    engine.subscribe(targets);
    targetsRef.current = targets;

    let animationId: number;
    const poll = () => {
      try {
        const updates = engine.poll_updates();
        if (updates && updates.length > 0) {
          setValues(prev => {
            const next = new Map(prev);
            for (const update of updates) {
              next.set(getObservableValueKey(update), update);
            }
            return next;
          });
        }
      } catch (e) {
        console.error('Error polling Quiver updates:', e);
      }
      animationId = requestAnimationFrame(poll);
    };
    animationId = requestAnimationFrame(poll);

    return () => {
      cancelAnimationFrame(animationId);
      engine.unsubscribe(targets.map(t => getSubscriptionTargetKey(t)));
    };
  }, [engine, JSON.stringify(targets)]);

  return values;
}

/**
 * Hook for a single parameter value with setter
 */
export function useQuiverParam(
  engine: QuiverEngine | null,
  nodeId: string,
  paramId: string
): [number, (value: number) => void] {
  const [value, setValue] = useState(0);

  const targets = useMemo(
    () => [{ type: 'param' as const, node_id: nodeId, param_id: paramId }],
    [nodeId, paramId]
  );

  const updates = useQuiverUpdates(engine, targets);

  useEffect(() => {
    const key = `param:${nodeId}:${paramId}`;
    const update = updates.get(key);
    if (update?.type === 'param') {
      setValue(update.value);
    }
  }, [updates, nodeId, paramId]);

  const setParam = useCallback(
    (newValue: number) => {
      if (!engine) return;
      try {
        engine.set_param(nodeId, paramId, newValue);
        setValue(newValue);
      } catch (e) {
        console.error('Error setting param:', e);
      }
    },
    [engine, nodeId, paramId]
  );

  return [value, setParam];
}

/**
 * Hook for level meter values
 */
export function useQuiverLevel(
  engine: QuiverEngine | null,
  nodeId: string,
  portId: number
): { rmsDb: number; peakDb: number } {
  const targets = useMemo(
    () => [{ type: 'level' as const, node_id: nodeId, port_id: portId }],
    [nodeId, portId]
  );

  const updates = useQuiverUpdates(engine, targets);

  const key = `level:${nodeId}:${portId}`;
  const update = updates.get(key);

  if (update?.type === 'level') {
    return { rmsDb: update.rms_db, peakDb: update.peak_db };
  }

  return { rmsDb: -Infinity, peakDb: -Infinity };
}

/**
 * Hook for module catalog
 */
export function useQuiverCatalog(
  engine: QuiverEngine | null
): CatalogResponse | null {
  const [catalog, setCatalog] = useState<CatalogResponse | null>(null);

  useEffect(() => {
    if (!engine) return;

    try {
      const cat = engine.get_catalog();
      setCatalog(cat);
    } catch (e) {
      console.error('Error getting catalog:', e);
    }
  }, [engine]);

  return catalog;
}

/**
 * Hook for module parameters
 */
export function useQuiverModuleParams(
  engine: QuiverEngine | null,
  nodeId: string
): ParamInfo[] {
  const [params, setParams] = useState<ParamInfo[]>([]);

  useEffect(() => {
    if (!engine || !nodeId) return;

    try {
      const p = engine.get_params(nodeId);
      setParams(p);
    } catch (e) {
      console.error('Error getting params:', e);
      setParams([]);
    }
  }, [engine, nodeId]);

  return params;
}
```

**Deliverables:**
- [ ] Add `hooks.ts` to @quiver/react
- [ ] Export hooks from index.ts
- [ ] Add @quiver/wasm as peer dependency
- [ ] Update package.json

---

### Phase 8: AudioWorklet Integration

**Objective:** Create AudioWorkletProcessor for real-time audio in the browser.

**File: `packages/@quiver/wasm/src/worklet.ts`**

```typescript
/**
 * AudioWorkletProcessor for Quiver
 *
 * This runs in the audio thread and calls the WASM engine's process method.
 */

// This file needs to be bundled separately as an AudioWorklet
class QuiverProcessor extends AudioWorkletProcessor {
  private engine: any = null;
  private wasmReady = false;

  constructor() {
    super();

    this.port.onmessage = async (event) => {
      if (event.data.type === 'init') {
        // Initialize WASM engine with sample rate
        try {
          const { QuiverEngine } = await import('./quiver.js');
          this.engine = new QuiverEngine(sampleRate);
          this.wasmReady = true;
          this.port.postMessage({ type: 'ready' });
        } catch (e) {
          this.port.postMessage({ type: 'error', error: String(e) });
        }
      } else if (event.data.type === 'load_patch') {
        if (this.engine) {
          try {
            this.engine.load_patch(event.data.patch);
            this.engine.compile();
            this.port.postMessage({ type: 'patch_loaded' });
          } catch (e) {
            this.port.postMessage({ type: 'error', error: String(e) });
          }
        }
      } else if (event.data.type === 'set_param') {
        if (this.engine) {
          try {
            this.engine.set_param(
              event.data.nodeId,
              event.data.paramId,
              event.data.value
            );
          } catch (e) {
            // Log but don't crash audio thread
            console.error('Error setting param:', e);
          }
        }
      }
    };
  }

  process(
    inputs: Float32Array[][],
    outputs: Float32Array[][],
    parameters: Record<string, Float32Array>
  ): boolean {
    if (!this.wasmReady || !this.engine) {
      return true; // Keep processor alive
    }

    const output = outputs[0];
    if (!output || output.length < 2) {
      return true;
    }

    try {
      // Process 128 samples (standard AudioWorklet quantum)
      const stereoOutput = this.engine.process(128);

      // Deinterleave stereo output
      for (let i = 0; i < 128; i++) {
        output[0][i] = stereoOutput[i * 2];
        output[1][i] = stereoOutput[i * 2 + 1];
      }
    } catch (e) {
      // Silence on error
      output[0].fill(0);
      output[1].fill(0);
    }

    return true;
  }
}

registerProcessor('quiver-processor', QuiverProcessor);
```

**File: `packages/@quiver/wasm/src/audio-context.ts`**

```typescript
/**
 * Helper for setting up Quiver audio in the browser
 */

export interface QuiverAudioContext {
  audioContext: AudioContext;
  workletNode: AudioWorkletNode;
  connect: (destination: AudioNode) => void;
  disconnect: () => void;
  loadPatch: (patch: any) => Promise<void>;
  setParam: (nodeId: string, paramId: string, value: number) => void;
}

export async function createQuiverAudioContext(): Promise<QuiverAudioContext> {
  const audioContext = new AudioContext();

  // Load the worklet module
  await audioContext.audioWorklet.addModule(
    new URL('./worklet.js', import.meta.url)
  );

  // Create worklet node
  const workletNode = new AudioWorkletNode(audioContext, 'quiver-processor', {
    numberOfInputs: 0,
    numberOfOutputs: 1,
    outputChannelCount: [2],
  });

  // Wait for WASM initialization
  await new Promise<void>((resolve, reject) => {
    workletNode.port.onmessage = (event) => {
      if (event.data.type === 'ready') {
        resolve();
      } else if (event.data.type === 'error') {
        reject(new Error(event.data.error));
      }
    };
    workletNode.port.postMessage({ type: 'init' });
  });

  return {
    audioContext,
    workletNode,
    connect: (destination: AudioNode) => {
      workletNode.connect(destination);
    },
    disconnect: () => {
      workletNode.disconnect();
    },
    loadPatch: async (patch: any) => {
      return new Promise((resolve, reject) => {
        const handler = (event: MessageEvent) => {
          if (event.data.type === 'patch_loaded') {
            workletNode.port.removeEventListener('message', handler);
            resolve();
          } else if (event.data.type === 'error') {
            workletNode.port.removeEventListener('message', handler);
            reject(new Error(event.data.error));
          }
        };
        workletNode.port.addEventListener('message', handler);
        workletNode.port.postMessage({ type: 'load_patch', patch });
      });
    },
    setParam: (nodeId: string, paramId: string, value: number) => {
      workletNode.port.postMessage({
        type: 'set_param',
        nodeId,
        paramId,
        value,
      });
    },
  };
}
```

**Deliverables:**
- [ ] Create worklet.ts AudioWorkletProcessor
- [ ] Create audio-context.ts helper
- [ ] Configure bundler for worklet (separate bundle)
- [ ] Add to @quiver/wasm exports

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
#[cfg(feature = "wasm")]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_engine_creation() {
        let engine = QuiverEngine::new(44100.0);
        assert_eq!(engine.sample_rate(), 44100.0);
    }

    #[wasm_bindgen_test]
    fn test_catalog() {
        let engine = QuiverEngine::new(44100.0);
        let catalog = engine.get_catalog().unwrap();
        // Verify catalog is valid
    }
}
```

### Integration Tests

```typescript
// packages/@quiver/wasm/__tests__/engine.test.ts
import { QuiverEngine } from '../quiver';

describe('QuiverEngine', () => {
  let engine: QuiverEngine;

  beforeEach(() => {
    engine = new QuiverEngine(44100);
  });

  test('creates engine with sample rate', () => {
    expect(engine.sample_rate).toBe(44100);
  });

  test('returns module catalog', () => {
    const catalog = engine.get_catalog();
    expect(catalog.modules.length).toBeGreaterThan(30);
    expect(catalog.categories).toContain('Oscillators');
  });

  test('loads and saves patch', () => {
    const patch = {
      version: 1,
      name: 'Test',
      tags: [],
      modules: [{ name: 'vco1', module_type: 'vco' }],
      cables: [],
      parameters: {},
    };

    engine.load_patch(patch);
    const saved = engine.save_patch('Test');

    expect(saved.name).toBe('Test');
    expect(saved.modules).toHaveLength(1);
  });
});
```

**Deliverables:**
- [ ] Add wasm_bindgen_test to dev-dependencies
- [ ] Write Rust WASM tests
- [ ] Write TypeScript integration tests
- [ ] Add test scripts to package.json

---

## Implementation Order Summary

| Phase | Description | Effort | Priority |
|-------|-------------|--------|----------|
| 1 | Feature flags & dependencies | Low | High |
| 2 | Tsify derives | Low | High |
| 3 | WASM engine module | Medium | High |
| 4 | Missing Patch methods | Medium | High |
| 5 | Observer integration | Low | Medium |
| 6 | Build configuration | Low | High |
| 7 | React hooks | Medium | Medium |
| 8 | AudioWorklet | High | Medium |

**Recommended order:** 1 → 2 → 4 → 3 → 5 → 6 → 7 → 8

---

## Success Criteria

1. `wasm-pack build --features wasm` produces valid WASM package
2. TypeScript types are auto-generated matching Rust types
3. React app can:
   - Display module catalog
   - Load/save patches
   - Connect modules and cables
   - Control parameters with UI
   - Play audio through AudioWorklet
4. Real-time updates at 60fps without audio glitches
5. WASM binary size < 2MB (ideally < 1MB with wasm-opt)

---

## Future Enhancements

- **MIDI support** via Web MIDI API
- **Preset browser** with cloud storage
- **Collaborative editing** with CRDTs
- **Plugin loading** via dynamic WASM imports
- **Offline support** with Service Worker
