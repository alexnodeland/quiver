//! Error types for WASM bindings

use alloc::format;
use alloc::string::String;
use wasm_bindgen::prelude::*;

/// Error type for WASM bindings
#[wasm_bindgen]
pub struct QuiverError {
    message: String,
}

#[wasm_bindgen]
impl QuiverError {
    /// Get the error message
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }
}

impl From<crate::graph::PatchError> for QuiverError {
    fn from(e: crate::graph::PatchError) -> Self {
        Self {
            message: format!("{:?}", e),
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

impl QuiverError {
    /// Convert to JsValue for use as error return
    pub fn into_js(self) -> JsValue {
        JsValue::from_str(&self.message)
    }
}
