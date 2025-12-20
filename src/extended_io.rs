//! Extended I/O
//!
//! This module provides extended I/O capabilities:
//! - OSC (Open Sound Control) protocol support
//! - VST/AU plugin wrapper infrastructure
//! - Web Audio backend interface
//!
//! # OSC Support
//!
//! The OSC implementation provides a bridge between network OSC messages
//! and the patch graph, allowing remote control of synthesizer parameters.
//!
//! # Plugin Wrapper
//!
//! The plugin wrapper infrastructure provides the foundation for building
//! VST3/AU/LV2 plugins from Quiver patches.
//!
//! # Web Audio
//!
//! The Web Audio interface provides traits and structures for integrating
//! Quiver with WebAssembly-based audio processing.

use crate::io::AtomicF64;
use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// ============================================================================
// OSC Protocol Support
// ============================================================================

/// OSC message types
#[derive(Debug, Clone)]
pub enum OscValue {
    /// 32-bit integer
    Int(i32),
    /// 32-bit float
    Float(f32),
    /// String
    String(String),
    /// Blob (binary data)
    Blob(Vec<u8>),
    /// True boolean
    True,
    /// False boolean
    False,
    /// Nil/null
    Nil,
    /// Infinitum/bang
    Infinitum,
    /// 64-bit integer
    Long(i64),
    /// 64-bit float
    Double(f64),
}

impl OscValue {
    /// Convert to f64 for CV use
    pub fn to_f64(&self) -> Option<f64> {
        match self {
            OscValue::Int(v) => Some(*v as f64),
            OscValue::Float(v) => Some(*v as f64),
            OscValue::Long(v) => Some(*v as f64),
            OscValue::Double(v) => Some(*v),
            OscValue::True => Some(1.0),
            OscValue::False => Some(0.0),
            _ => None,
        }
    }

    /// Convert to bool
    pub fn to_bool(&self) -> Option<bool> {
        match self {
            OscValue::Int(v) => Some(*v != 0),
            OscValue::Float(v) => Some(*v != 0.0),
            OscValue::True => Some(true),
            OscValue::False => Some(false),
            _ => None,
        }
    }
}

/// An OSC message with address and arguments
#[derive(Debug, Clone)]
pub struct OscMessage {
    /// OSC address pattern (e.g., "/synth/filter/cutoff")
    pub address: String,
    /// Message arguments
    pub args: Vec<OscValue>,
}

impl OscMessage {
    /// Create a new OSC message
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            args: Vec::new(),
        }
    }

    /// Add an argument
    pub fn with_arg(mut self, arg: OscValue) -> Self {
        self.args.push(arg);
        self
    }

    /// Add a float argument
    pub fn with_float(self, value: f32) -> Self {
        self.with_arg(OscValue::Float(value))
    }

    /// Add an int argument
    pub fn with_int(self, value: i32) -> Self {
        self.with_arg(OscValue::Int(value))
    }

    /// Get the first argument as f64
    pub fn first_f64(&self) -> Option<f64> {
        self.args.first().and_then(|v| v.to_f64())
    }
}

/// OSC address pattern matching
pub struct OscPattern {
    /// Pattern segments
    segments: Vec<PatternSegment>,
}

#[derive(Debug, Clone)]
enum PatternSegment {
    Literal(String),
    Wildcard,             // *
    SingleChar,           // ?
    CharClass(Vec<char>), // [abc]
}

impl OscPattern {
    /// Parse an OSC address pattern
    pub fn new(pattern: &str) -> Self {
        let mut segments = Vec::new();
        let mut current = String::new();

        let mut chars = pattern.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '/' => {
                    if !current.is_empty() {
                        segments.push(PatternSegment::Literal(current.clone()));
                        current.clear();
                    }
                }
                '*' => {
                    if !current.is_empty() {
                        segments.push(PatternSegment::Literal(current.clone()));
                        current.clear();
                    }
                    segments.push(PatternSegment::Wildcard);
                }
                '?' => {
                    if !current.is_empty() {
                        segments.push(PatternSegment::Literal(current.clone()));
                        current.clear();
                    }
                    segments.push(PatternSegment::SingleChar);
                }
                '[' => {
                    if !current.is_empty() {
                        segments.push(PatternSegment::Literal(current.clone()));
                        current.clear();
                    }
                    let mut class = Vec::new();
                    while let Some(&next) = chars.peek() {
                        if next == ']' {
                            chars.next();
                            break;
                        }
                        class.push(chars.next().unwrap());
                    }
                    segments.push(PatternSegment::CharClass(class));
                }
                _ => {
                    current.push(c);
                }
            }
        }
        if !current.is_empty() {
            segments.push(PatternSegment::Literal(current));
        }

        Self { segments }
    }

    /// Check if an address matches this pattern
    pub fn matches(&self, address: &str) -> bool {
        // Simplified matching - just check for literal prefix
        let parts: Vec<&str> = address.split('/').filter(|s| !s.is_empty()).collect();
        let mut part_idx = 0;

        for segment in &self.segments {
            if part_idx >= parts.len() {
                return matches!(segment, PatternSegment::Wildcard);
            }

            match segment {
                PatternSegment::Literal(lit) => {
                    if parts[part_idx] != lit {
                        return false;
                    }
                    part_idx += 1;
                }
                PatternSegment::Wildcard => {
                    return true; // Match rest
                }
                PatternSegment::SingleChar => {
                    if parts[part_idx].len() != 1 {
                        return false;
                    }
                    part_idx += 1;
                }
                PatternSegment::CharClass(chars) => {
                    let p = parts[part_idx];
                    if p.len() != 1 || !chars.contains(&p.chars().next().unwrap()) {
                        return false;
                    }
                    part_idx += 1;
                }
            }
        }

        part_idx >= parts.len()
    }
}

/// Binding between an OSC address and a parameter
pub struct OscBinding {
    /// OSC address pattern
    pub pattern: OscPattern,
    /// Target value
    pub value: Arc<AtomicF64>,
    /// Optional scale factor
    pub scale: f64,
    /// Optional offset
    pub offset: f64,
}

impl OscBinding {
    /// Create a new binding
    pub fn new(pattern: &str, value: Arc<AtomicF64>) -> Self {
        Self {
            pattern: OscPattern::new(pattern),
            value,
            scale: 1.0,
            offset: 0.0,
        }
    }

    /// Set scale factor
    pub fn with_scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }

    /// Set offset
    pub fn with_offset(mut self, offset: f64) -> Self {
        self.offset = offset;
        self
    }

    /// Apply a message to this binding
    pub fn apply(&self, msg: &OscMessage) -> bool {
        if !self.pattern.matches(&msg.address) {
            return false;
        }

        if let Some(v) = msg.first_f64() {
            self.value.set(v * self.scale + self.offset);
            return true;
        }

        false
    }
}

/// OSC receiver that routes messages to bindings
pub struct OscReceiver {
    /// Registered bindings
    bindings: Vec<OscBinding>,
    /// Counter for total messages received
    message_count: AtomicU32,
    /// Counter for messages that matched at least one binding
    matched_count: AtomicU32,
}

impl OscReceiver {
    /// Create a new OSC receiver
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            message_count: AtomicU32::new(0),
            matched_count: AtomicU32::new(0),
        }
    }

    /// Add a binding
    pub fn add_binding(&mut self, binding: OscBinding) {
        self.bindings.push(binding);
    }

    /// Create a binding for a parameter
    pub fn bind(&mut self, pattern: &str, value: Arc<AtomicF64>) {
        self.add_binding(OscBinding::new(pattern, value));
    }

    /// Create a scaled binding (e.g., 0-1 to 20-20000 Hz)
    pub fn bind_scaled(&mut self, pattern: &str, value: Arc<AtomicF64>, scale: f64, offset: f64) {
        self.add_binding(
            OscBinding::new(pattern, value)
                .with_scale(scale)
                .with_offset(offset),
        );
    }

    /// Process an OSC message
    /// Returns true if at least one binding matched
    pub fn handle_message(&self, msg: &OscMessage) -> bool {
        self.message_count.fetch_add(1, Ordering::Relaxed);
        let mut handled = false;
        for binding in &self.bindings {
            if binding.apply(msg) {
                handled = true;
            }
        }
        if handled {
            self.matched_count.fetch_add(1, Ordering::Relaxed);
        }
        handled
    }

    /// Get the number of bindings
    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    /// Get the total number of messages received
    pub fn message_count(&self) -> u32 {
        self.message_count.load(Ordering::Relaxed)
    }

    /// Get the number of messages that matched at least one binding
    pub fn matched_count(&self) -> u32 {
        self.matched_count.load(Ordering::Relaxed)
    }

    /// Reset the message counters
    pub fn reset_counters(&self) {
        self.message_count.store(0, Ordering::Relaxed);
        self.matched_count.store(0, Ordering::Relaxed);
    }
}

impl Default for OscReceiver {
    fn default() -> Self {
        Self::new()
    }
}

/// OSC input module for patch graphs
pub struct OscInput {
    /// Target value
    value: Arc<AtomicF64>,
    /// Port specification
    spec: PortSpec,
    /// OSC address (for documentation)
    address: String,
}

impl OscInput {
    /// Create a new OSC input
    pub fn new(address: impl Into<String>, value: Arc<AtomicF64>, kind: SignalKind) -> Self {
        Self {
            value,
            spec: PortSpec {
                inputs: vec![],
                outputs: vec![PortDef::new(0, "out", kind)],
            },
            address: address.into(),
        }
    }

    /// Get the OSC address
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Get reference to the value
    pub fn value_ref(&self) -> &Arc<AtomicF64> {
        &self.value
    }
}

impl GraphModule for OscInput {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, _inputs: &PortValues, outputs: &mut PortValues) {
        outputs.set(0, self.value.get());
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "osc_input"
    }
}

// ============================================================================
// Plugin Wrapper Infrastructure
// ============================================================================

/// Plugin parameter definition
#[derive(Debug, Clone)]
pub struct PluginParameter {
    /// Parameter ID
    pub id: u32,
    /// Display name
    pub name: String,
    /// Short name (for limited displays)
    pub short_name: String,
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// Default value
    pub default: f64,
    /// Unit label (e.g., "Hz", "dB", "%")
    pub unit: String,
    /// Number of steps (0 = continuous)
    pub steps: u32,
}

impl PluginParameter {
    /// Create a new continuous parameter
    pub fn new(id: u32, name: &str, min: f64, max: f64, default: f64) -> Self {
        Self {
            id,
            name: name.to_string(),
            short_name: name.chars().take(8).collect(),
            min,
            max,
            default,
            unit: String::new(),
            steps: 0,
        }
    }

    /// Set the unit label
    pub fn with_unit(mut self, unit: &str) -> Self {
        self.unit = unit.to_string();
        self
    }

    /// Set the number of steps (for discrete parameters)
    pub fn with_steps(mut self, steps: u32) -> Self {
        self.steps = steps;
        self
    }

    /// Set the short name
    pub fn with_short_name(mut self, short_name: &str) -> Self {
        self.short_name = short_name.to_string();
        self
    }

    /// Normalize a value to 0.0-1.0 range
    pub fn normalize(&self, value: f64) -> f64 {
        (value - self.min) / (self.max - self.min)
    }

    /// Denormalize from 0.0-1.0 to parameter range
    pub fn denormalize(&self, normalized: f64) -> f64 {
        self.min + normalized * (self.max - self.min)
    }

    /// Quantize to steps (if discrete)
    pub fn quantize(&self, value: f64) -> f64 {
        if self.steps == 0 {
            return value;
        }
        let step_size = (self.max - self.min) / self.steps as f64;
        let steps = ((value - self.min) / step_size).round();
        self.min + steps * step_size
    }
}

/// Plugin audio bus configuration
#[derive(Debug, Clone)]
pub struct AudioBusConfig {
    /// Number of input channels
    pub inputs: u32,
    /// Number of output channels
    pub outputs: u32,
    /// Bus name
    pub name: String,
}

impl AudioBusConfig {
    /// Create a stereo output configuration
    pub fn stereo_out() -> Self {
        Self {
            inputs: 0,
            outputs: 2,
            name: "Main".to_string(),
        }
    }

    /// Create a stereo I/O configuration
    pub fn stereo_io() -> Self {
        Self {
            inputs: 2,
            outputs: 2,
            name: "Main".to_string(),
        }
    }

    /// Create a mono output configuration
    pub fn mono_out() -> Self {
        Self {
            inputs: 0,
            outputs: 1,
            name: "Main".to_string(),
        }
    }
}

/// Plugin metadata
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// Plugin unique identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Vendor name
    pub vendor: String,
    /// Version string
    pub version: String,
    /// Plugin category
    pub category: PluginCategory,
    /// Whether the plugin is a synth (has no audio inputs)
    pub is_synth: bool,
    /// Supported sample rates (empty = any)
    pub sample_rates: Vec<f64>,
    /// Maximum block size (0 = any)
    pub max_block_size: usize,
    /// Latency in samples
    pub latency: u32,
}

/// Plugin category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginCategory {
    Effect,
    Instrument,
    Analyzer,
    Spatial,
    Generator,
    Other,
}

impl PluginInfo {
    /// Create a new synth plugin info
    pub fn synth(id: &str, name: &str, vendor: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            vendor: vendor.to_string(),
            version: "1.0.0".to_string(),
            category: PluginCategory::Instrument,
            is_synth: true,
            sample_rates: vec![],
            max_block_size: 0,
            latency: 0,
        }
    }

    /// Create a new effect plugin info
    pub fn effect(id: &str, name: &str, vendor: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            vendor: vendor.to_string(),
            version: "1.0.0".to_string(),
            category: PluginCategory::Effect,
            is_synth: false,
            sample_rates: vec![],
            max_block_size: 0,
            latency: 0,
        }
    }
}

/// Plugin wrapper for adapting Quiver patches to plugin formats
pub struct PluginWrapper {
    /// Plugin metadata
    pub info: PluginInfo,
    /// Audio bus configuration
    pub bus_config: AudioBusConfig,
    /// Parameter definitions
    pub parameters: Vec<PluginParameter>,
    /// Parameter values (atomic for thread-safe access)
    pub param_values: Vec<Arc<AtomicF64>>,
    /// Sample rate
    pub sample_rate: f64,
    /// Processing state
    pub is_processing: AtomicBool,
}

impl PluginWrapper {
    /// Create a new plugin wrapper
    pub fn new(info: PluginInfo, bus_config: AudioBusConfig) -> Self {
        Self {
            info,
            bus_config,
            parameters: Vec::new(),
            param_values: Vec::new(),
            sample_rate: 44100.0,
            is_processing: AtomicBool::new(false),
        }
    }

    /// Add a parameter
    pub fn add_parameter(&mut self, param: PluginParameter) -> Arc<AtomicF64> {
        let value = Arc::new(AtomicF64::new(param.default));
        self.param_values.push(value.clone());
        self.parameters.push(param);
        value
    }

    /// Get parameter count
    pub fn parameter_count(&self) -> usize {
        self.parameters.len()
    }

    /// Get parameter value by index
    pub fn get_parameter(&self, index: usize) -> Option<f64> {
        self.param_values.get(index).map(|v| v.get())
    }

    /// Set parameter value by index (normalized 0-1)
    pub fn set_parameter_normalized(&self, index: usize, normalized: f64) {
        if let (Some(param), Some(value)) =
            (self.parameters.get(index), self.param_values.get(index))
        {
            let denormalized = param.denormalize(normalized.clamp(0.0, 1.0));
            let quantized = param.quantize(denormalized);
            value.set(quantized);
        }
    }

    /// Set sample rate
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    /// Start processing
    pub fn start_processing(&self) {
        self.is_processing.store(true, Ordering::SeqCst);
    }

    /// Stop processing
    pub fn stop_processing(&self) {
        self.is_processing.store(false, Ordering::SeqCst);
    }

    /// Check if processing is active
    pub fn is_processing(&self) -> bool {
        self.is_processing.load(Ordering::SeqCst)
    }
}

// ============================================================================
// Web Audio Backend Interface
// ============================================================================

/// Web Audio processor configuration
#[derive(Debug, Clone)]
pub struct WebAudioConfig {
    /// Number of input channels
    pub input_channels: u32,
    /// Number of output channels
    pub output_channels: u32,
    /// Sample rate (typically 44100 or 48000 for web)
    pub sample_rate: f64,
    /// Block size (typically 128 for Web Audio)
    pub block_size: usize,
}

impl Default for WebAudioConfig {
    fn default() -> Self {
        Self {
            input_channels: 0,
            output_channels: 2,
            sample_rate: 44100.0,
            block_size: 128,
        }
    }
}

/// Trait for Web Audio compatible processors
///
/// This trait provides the interface for WebAssembly-based audio processing.
/// Implementations can be compiled to WASM and used with AudioWorkletProcessor.
pub trait WebAudioProcessor: Send {
    /// Initialize the processor with the given configuration
    fn initialize(&mut self, config: &WebAudioConfig);

    /// Process a block of audio
    ///
    /// `inputs`: Interleaved input samples (channels * block_size)
    /// `outputs`: Buffer for interleaved output samples
    /// Returns true to keep processing, false to end
    fn process(&mut self, inputs: &[f32], outputs: &mut [f32]) -> bool;

    /// Handle a parameter change
    fn set_parameter(&mut self, name: &str, value: f64);

    /// Get current parameter value
    fn get_parameter(&self, name: &str) -> Option<f64>;

    /// Get all parameter names
    fn parameter_names(&self) -> Vec<String>;

    /// Handle a message from the main thread
    fn handle_message(&mut self, _data: &[u8]) {}
}

/// Web Audio worklet adapter
///
/// Adapts a Quiver patch for use as a Web Audio worklet.
pub struct WebAudioWorklet {
    /// Configuration
    config: WebAudioConfig,
    /// Parameter map
    parameters: HashMap<String, Arc<AtomicF64>>,
    /// Active state
    active: bool,
}

impl WebAudioWorklet {
    /// Create a new worklet adapter
    pub fn new() -> Self {
        Self {
            config: WebAudioConfig::default(),
            parameters: HashMap::new(),
            active: false,
        }
    }

    /// Add a parameter
    pub fn add_parameter(&mut self, name: &str, initial: f64) -> Arc<AtomicF64> {
        let value = Arc::new(AtomicF64::new(initial));
        self.parameters.insert(name.to_string(), value.clone());
        value
    }

    /// Initialize with configuration
    pub fn initialize(&mut self, config: WebAudioConfig) {
        self.config = config;
        self.active = true;
    }

    /// Get configuration
    pub fn config(&self) -> &WebAudioConfig {
        &self.config
    }

    /// Check if active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get a parameter value
    pub fn get_parameter(&self, name: &str) -> Option<f64> {
        self.parameters.get(name).map(|v| v.get())
    }

    /// Set a parameter value
    pub fn set_parameter(&mut self, name: &str, value: f64) {
        if let Some(param) = self.parameters.get(name) {
            param.set(value);
        }
    }
}

impl Default for WebAudioWorklet {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert f64 audio block to f32 for Web Audio
#[inline]
pub fn f64_to_f32_block(src: &[f64], dst: &mut [f32]) {
    let len = src.len().min(dst.len());
    for i in 0..len {
        dst[i] = src[i] as f32;
    }
}

/// Convert f32 audio block to f64 from Web Audio
#[inline]
pub fn f32_to_f64_block(src: &[f32], dst: &mut [f64]) {
    let len = src.len().min(dst.len());
    for i in 0..len {
        dst[i] = src[i] as f64;
    }
}

/// Interleave stereo channels for Web Audio
#[inline]
pub fn interleave_stereo(left: &[f64], right: &[f64], output: &mut [f32]) {
    let frames = left.len().min(right.len()).min(output.len() / 2);
    for i in 0..frames {
        output[i * 2] = left[i] as f32;
        output[i * 2 + 1] = right[i] as f32;
    }
}

/// Deinterleave stereo channels from Web Audio
#[inline]
pub fn deinterleave_stereo(input: &[f32], left: &mut [f64], right: &mut [f64]) {
    let frames = (input.len() / 2).min(left.len()).min(right.len());
    for i in 0..frames {
        left[i] = input[i * 2] as f64;
        right[i] = input[i * 2 + 1] as f64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // OSC Tests
    #[test]
    fn test_osc_message() {
        let msg = OscMessage::new("/synth/filter/cutoff").with_float(0.75);
        assert_eq!(msg.address, "/synth/filter/cutoff");
        assert!((msg.first_f64().unwrap() - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_osc_pattern_literal() {
        let pattern = OscPattern::new("/synth/osc/pitch");
        assert!(pattern.matches("/synth/osc/pitch"));
        assert!(!pattern.matches("/synth/osc/volume"));
        assert!(!pattern.matches("/synth/osc"));
    }

    #[test]
    fn test_osc_pattern_wildcard() {
        let pattern = OscPattern::new("/synth/*");
        assert!(pattern.matches("/synth/osc"));
        assert!(pattern.matches("/synth/filter/cutoff"));
    }

    #[test]
    fn test_osc_binding() {
        let value = Arc::new(AtomicF64::new(0.0));
        let binding = OscBinding::new("/test/param", value.clone()).with_scale(10.0);

        let msg = OscMessage::new("/test/param").with_float(0.5);
        assert!(binding.apply(&msg));
        assert!((value.get() - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_osc_receiver() {
        let mut receiver = OscReceiver::new();
        let value = Arc::new(AtomicF64::new(0.0));
        receiver.bind("/synth/volume", value.clone());

        let msg = OscMessage::new("/synth/volume").with_float(0.8);
        assert!(receiver.handle_message(&msg));
        assert!((value.get() - 0.8).abs() < 0.001);

        let msg2 = OscMessage::new("/synth/pitch").with_float(0.5);
        assert!(!receiver.handle_message(&msg2));
    }

    // Plugin Wrapper Tests
    #[test]
    fn test_plugin_parameter() {
        let param = PluginParameter::new(0, "Cutoff", 20.0, 20000.0, 1000.0).with_unit("Hz");

        assert!((param.normalize(20.0) - 0.0).abs() < 0.001);
        assert!((param.normalize(20000.0) - 1.0).abs() < 0.001);
        assert!((param.denormalize(0.5) - 10010.0).abs() < 1.0);
    }

    #[test]
    fn test_plugin_parameter_quantize() {
        let param = PluginParameter::new(0, "Steps", 0.0, 10.0, 5.0).with_steps(10);

        // With 10 steps over 0-10 range, step size is 1.0
        // 0.5 rounds to 0.0 or 1.0
        assert!((param.quantize(0.4) - 0.0).abs() < 0.1);
        assert!((param.quantize(0.6) - 1.0).abs() < 0.1);
        assert!((param.quantize(4.7) - 5.0).abs() < 0.1);
        assert!((param.quantize(9.9) - 10.0).abs() < 0.1);
    }

    #[test]
    fn test_plugin_wrapper() {
        let info = PluginInfo::synth("com.quiver.test", "Test Synth", "Quiver");
        let bus = AudioBusConfig::stereo_out();

        let mut wrapper = PluginWrapper::new(info, bus);
        let cutoff =
            wrapper.add_parameter(PluginParameter::new(0, "Cutoff", 20.0, 20000.0, 1000.0));

        assert_eq!(wrapper.parameter_count(), 1);
        assert!((wrapper.get_parameter(0).unwrap() - 1000.0).abs() < 0.001);

        wrapper.set_parameter_normalized(0, 0.5);
        assert!((cutoff.get() - 10010.0).abs() < 1.0);
    }

    // Web Audio Tests
    #[test]
    fn test_web_audio_config() {
        let config = WebAudioConfig::default();
        assert_eq!(config.output_channels, 2);
        assert_eq!(config.block_size, 128);
    }

    #[test]
    fn test_web_audio_worklet() {
        let mut worklet = WebAudioWorklet::new();
        let freq = worklet.add_parameter("frequency", 440.0);

        assert!((worklet.get_parameter("frequency").unwrap() - 440.0).abs() < 0.001);

        worklet.set_parameter("frequency", 880.0);
        assert!((freq.get() - 880.0).abs() < 0.001);
    }

    #[test]
    fn test_interleave_stereo() {
        let left = vec![1.0, 2.0, 3.0];
        let right = vec![4.0, 5.0, 6.0];
        let mut output = vec![0.0f32; 6];

        interleave_stereo(&left, &right, &mut output);

        assert!((output[0] - 1.0).abs() < 0.001);
        assert!((output[1] - 4.0).abs() < 0.001);
        assert!((output[2] - 2.0).abs() < 0.001);
        assert!((output[3] - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_deinterleave_stereo() {
        let input = vec![1.0f32, 4.0, 2.0, 5.0, 3.0, 6.0];
        let mut left = vec![0.0; 3];
        let mut right = vec![0.0; 3];

        deinterleave_stereo(&input, &mut left, &mut right);

        assert!((left[0] - 1.0).abs() < 0.001);
        assert!((left[1] - 2.0).abs() < 0.001);
        assert!((left[2] - 3.0).abs() < 0.001);
        assert!((right[0] - 4.0).abs() < 0.001);
        assert!((right[1] - 5.0).abs() < 0.001);
        assert!((right[2] - 6.0).abs() < 0.001);
    }
}
