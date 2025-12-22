//! WASM bindings for Quiver
//!
//! This module provides the JavaScript-facing API for running Quiver
//! in a browser environment via WebAssembly.

mod engine;
mod error;

pub use engine::QuiverEngine;
pub use error::QuiverError;

// Re-export wasm_bindgen for convenience
pub use wasm_bindgen::prelude::*;
