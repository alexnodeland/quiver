# WASM Bindings

This directory contains WebAssembly bindings for running Quiver in browser environments.

## Overview

The WASM module provides a JavaScript-facing API via `wasm-bindgen`, enabling:
- Full Quiver functionality in web browsers
- Audio worklet integration for low-latency processing
- TypeScript type generation via `tsify`
- MIDI and real-time parameter control

## Files

```
wasm/
├── mod.rs              # Module entry point, re-exports
├── engine.rs           # QuiverEngine - main WASM interface
└── error.rs            # QuiverError - error handling for JS
```

## QuiverEngine API

The `QuiverEngine` struct is the main interface exposed to JavaScript:

### Lifecycle
- `new(sample_rate)` - Create a new engine
- `tick()` - Process one sample, returns stereo output

### Patch Management
- `load_patch(json)` - Load a patch from JSON
- `get_patch()` - Get current patch as JSON
- `validate_patch(json)` - Validate a patch definition
- `clear()` - Clear the current patch

### Module Operations
- `add_module(id, type_id)` - Add a module to the patch
- `remove_module(id)` - Remove a module
- `connect(from, to)` - Connect two ports
- `disconnect(cable_id)` - Remove a connection
- `set_param(module_id, param, value)` - Set a parameter

### Catalog API
- `get_catalog()` - Get full module catalog
- `search_modules(query)` - Search modules by name/description
- `get_modules_by_category(category)` - Filter by category
- `get_categories()` - List all categories

### Signal Semantics
- `get_signal_colors()` - Get default signal type colors
- `check_compatibility(from, to)` - Check port compatibility

### MIDI Integration
- `midi_note_on(note, velocity)` - Process MIDI note on
- `midi_note_off(note)` - Process MIDI note off
- `midi_cc(cc, value)` - Process MIDI CC
- `midi_pitch_bend(value)` - Process pitch bend

### State Observation
- `subscribe(target)` - Subscribe to state updates
- `get_observed_state()` - Get current observed values
- `unsubscribe(id)` - Remove subscription

## Building

```bash
# Check WASM compilation
make wasm-check

# Build WASM package (release)
make wasm

# Build WASM package (development, faster)
make wasm-dev
```

Built artifacts go to `packages/@quiver/wasm/`.

## Feature Flag

The WASM module is gated behind the `wasm` feature flag:

```rust
#[cfg(feature = "wasm")]
pub mod wasm;
```

This feature enables:
- `wasm-bindgen` - JavaScript bindings
- `tsify` - TypeScript type generation
- `serde-wasm-bindgen` - Serde integration
- `js-sys` - JavaScript interop
- `console_error_panic_hook` - Better panic messages

## Integration Notes

### Thread Safety
The WASM module runs in a single-threaded environment (no `Send`/`Sync` required). The audio worklet runs on its own thread but communicates via message passing.

### Memory Management
Memory is managed by the WASM linear memory. Rust's ownership system handles cleanup automatically.

### Error Handling
Errors are converted to `JsValue` via `QuiverError` for JavaScript consumption. The `console_error_panic_hook` provides better stack traces for panics.

### Performance
- Optimize for size with `opt-level = "z"` and LTO in release builds
- Avoid allocations in the audio path (`tick()`)
- Use pre-allocated buffers where possible
