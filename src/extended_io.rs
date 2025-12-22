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

    /// Get the latency in samples
    pub fn latency(&self) -> u32 {
        self.info.latency
    }

    /// Set the latency in samples
    pub fn set_latency(&mut self, samples: u32) {
        self.info.latency = samples;
    }
}

// ============================================================================
// MIDI Support for Plugin Integration
// ============================================================================

/// MIDI message status bytes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiStatus {
    /// Note Off (channel 0-15)
    NoteOff(u8),
    /// Note On (channel 0-15)
    NoteOn(u8),
    /// Polyphonic Aftertouch (channel 0-15)
    PolyPressure(u8),
    /// Control Change (channel 0-15)
    ControlChange(u8),
    /// Program Change (channel 0-15)
    ProgramChange(u8),
    /// Channel Aftertouch (channel 0-15)
    ChannelPressure(u8),
    /// Pitch Bend (channel 0-15)
    PitchBend(u8),
    /// System message
    System(u8),
}

impl MidiStatus {
    /// Parse status byte
    pub fn from_byte(byte: u8) -> Option<Self> {
        let status = byte & 0xF0;
        let channel = byte & 0x0F;
        match status {
            0x80 => Some(MidiStatus::NoteOff(channel)),
            0x90 => Some(MidiStatus::NoteOn(channel)),
            0xA0 => Some(MidiStatus::PolyPressure(channel)),
            0xB0 => Some(MidiStatus::ControlChange(channel)),
            0xC0 => Some(MidiStatus::ProgramChange(channel)),
            0xD0 => Some(MidiStatus::ChannelPressure(channel)),
            0xE0 => Some(MidiStatus::PitchBend(channel)),
            0xF0..=0xFF => Some(MidiStatus::System(byte)),
            _ => None,
        }
    }

    /// Get the channel (0-15) or None for system messages
    pub fn channel(&self) -> Option<u8> {
        match self {
            MidiStatus::NoteOff(ch)
            | MidiStatus::NoteOn(ch)
            | MidiStatus::PolyPressure(ch)
            | MidiStatus::ControlChange(ch)
            | MidiStatus::ProgramChange(ch)
            | MidiStatus::ChannelPressure(ch)
            | MidiStatus::PitchBend(ch) => Some(*ch),
            MidiStatus::System(_) => None,
        }
    }
}

/// A MIDI message with timing information
#[derive(Debug, Clone)]
pub struct MidiMessage {
    /// Sample offset within the current buffer
    pub sample_offset: u32,
    /// Status byte
    pub status: MidiStatus,
    /// Data byte 1 (note number, CC number, etc.)
    pub data1: u8,
    /// Data byte 2 (velocity, CC value, etc.)
    pub data2: u8,
}

impl MidiMessage {
    /// Create a Note On message
    pub fn note_on(channel: u8, note: u8, velocity: u8) -> Self {
        Self {
            sample_offset: 0,
            status: MidiStatus::NoteOn(channel & 0x0F),
            data1: note & 0x7F,
            data2: velocity & 0x7F,
        }
    }

    /// Create a Note Off message
    pub fn note_off(channel: u8, note: u8, velocity: u8) -> Self {
        Self {
            sample_offset: 0,
            status: MidiStatus::NoteOff(channel & 0x0F),
            data1: note & 0x7F,
            data2: velocity & 0x7F,
        }
    }

    /// Create a Control Change message
    pub fn control_change(channel: u8, cc: u8, value: u8) -> Self {
        Self {
            sample_offset: 0,
            status: MidiStatus::ControlChange(channel & 0x0F),
            data1: cc & 0x7F,
            data2: value & 0x7F,
        }
    }

    /// Create a Pitch Bend message (value: -8192 to 8191)
    pub fn pitch_bend(channel: u8, value: i16) -> Self {
        let unsigned = (value + 8192).clamp(0, 16383) as u16;
        Self {
            sample_offset: 0,
            status: MidiStatus::PitchBend(channel & 0x0F),
            data1: (unsigned & 0x7F) as u8,
            data2: ((unsigned >> 7) & 0x7F) as u8,
        }
    }

    /// Set sample offset for sample-accurate timing
    pub fn at_sample(mut self, offset: u32) -> Self {
        self.sample_offset = offset;
        self
    }

    /// Check if this is a Note On with non-zero velocity
    pub fn is_note_on(&self) -> bool {
        matches!(self.status, MidiStatus::NoteOn(_)) && self.data2 > 0
    }

    /// Check if this is a Note Off (or Note On with velocity 0)
    pub fn is_note_off(&self) -> bool {
        matches!(self.status, MidiStatus::NoteOff(_))
            || (matches!(self.status, MidiStatus::NoteOn(_)) && self.data2 == 0)
    }

    /// Get the note number (0-127)
    pub fn note(&self) -> u8 {
        self.data1
    }

    /// Get the velocity (0-127)
    pub fn velocity(&self) -> u8 {
        self.data2
    }

    /// Convert note to frequency (A4 = 440Hz)
    pub fn note_to_frequency(&self) -> f64 {
        440.0 * 2.0_f64.powf((self.data1 as f64 - 69.0) / 12.0)
    }

    /// Convert note to V/Oct (0V = C4, 1V = C5, etc.)
    pub fn note_to_volt_per_octave(&self) -> f64 {
        (self.data1 as f64 - 60.0) / 12.0
    }

    /// Get pitch bend as -1.0 to 1.0 (for +/- 2 semitones typically)
    pub fn pitch_bend_normalized(&self) -> f64 {
        if !matches!(self.status, MidiStatus::PitchBend(_)) {
            return 0.0;
        }
        let value = (self.data1 as i32) | ((self.data2 as i32) << 7);
        (value - 8192) as f64 / 8192.0
    }
}

/// MIDI event buffer for a processing block
pub struct MidiBuffer {
    /// Events sorted by sample offset
    events: Vec<MidiMessage>,
}

impl MidiBuffer {
    /// Create an empty MIDI buffer
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Create a buffer with capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
        }
    }

    /// Add an event
    pub fn push(&mut self, event: MidiMessage) {
        self.events.push(event);
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Sort events by sample offset
    pub fn sort(&mut self) {
        self.events.sort_by_key(|e| e.sample_offset);
    }

    /// Get iterator over events
    pub fn iter(&self) -> impl Iterator<Item = &MidiMessage> {
        self.events.iter()
    }

    /// Get events at a specific sample offset
    pub fn events_at(&self, sample: u32) -> impl Iterator<Item = &MidiMessage> {
        self.events
            .iter()
            .filter(move |e| e.sample_offset == sample)
    }

    /// Get number of events
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl Default for MidiBuffer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Plugin Processor Trait
// ============================================================================

/// Processing context passed to plugin processor
pub struct ProcessContext<'a> {
    /// Sample rate
    pub sample_rate: f64,
    /// Number of samples in this block
    pub num_samples: usize,
    /// Current transport position in samples (if available)
    pub transport_position: Option<u64>,
    /// Current tempo in BPM (if available)
    pub tempo: Option<f64>,
    /// Whether transport is playing
    pub is_playing: bool,
    /// MIDI input events
    pub midi_in: &'a MidiBuffer,
    /// MIDI output events
    pub midi_out: &'a mut MidiBuffer,
}

/// Trait for plugin audio processors
///
/// This trait provides the complete interface for implementing audio plugins.
/// Implementations can be used with VST3, AU, or LV2 wrappers.
///
/// # Example
///
/// ```rust,ignore
/// use quiver::extended_io::*;
///
/// struct MySynth {
///     patch: Patch,
///     sample_rate: f64,
/// }
///
/// impl PluginProcessor for MySynth {
///     fn initialize(&mut self, sample_rate: f64, max_block_size: usize) {
///         self.sample_rate = sample_rate;
///         self.patch.set_sample_rate(sample_rate);
///     }
///
///     fn process(
///         &mut self,
///         inputs: &[&[f32]],
///         outputs: &mut [&mut [f32]],
///         context: &mut ProcessContext,
///     ) {
///         // Handle MIDI
///         for event in context.midi_in.iter() {
///             if event.is_note_on() {
///                 // Trigger note...
///             }
///         }
///
///         // Process audio
///         for i in 0..context.num_samples {
///             let (left, right) = self.patch.tick();
///             outputs[0][i] = left as f32;
///             outputs[1][i] = right as f32;
///         }
///     }
///
///     fn reset(&mut self) {
///         self.patch.reset();
///     }
/// }
/// ```
pub trait PluginProcessor: Send {
    /// Initialize the processor
    ///
    /// Called once when the plugin is instantiated or when sample rate changes.
    fn initialize(&mut self, sample_rate: f64, max_block_size: usize);

    /// Process a block of audio
    ///
    /// `inputs`: Slice of input channel buffers (may be empty for synths)
    /// `outputs`: Slice of output channel buffers
    /// `context`: Processing context with timing and MIDI
    fn process(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        context: &mut ProcessContext,
    );

    /// Reset processor state
    ///
    /// Called when playback stops or when bypassed for a while.
    fn reset(&mut self);

    /// Set a parameter value
    fn set_parameter(&mut self, id: u32, value: f64);

    /// Get a parameter value
    fn get_parameter(&self, id: u32) -> f64;

    /// Get the number of parameters
    fn parameter_count(&self) -> usize {
        0
    }

    /// Get parameter info
    fn parameter_info(&self, _id: u32) -> Option<PluginParameter> {
        None
    }

    /// Get tail length in samples (for reverbs, delays, etc.)
    fn tail_samples(&self) -> u32 {
        0
    }

    /// Get latency in samples
    fn latency_samples(&self) -> u32 {
        0
    }

    /// Handle state save (return serialized state)
    fn save_state(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Handle state load
    fn load_state(&mut self, _data: &[u8]) -> bool {
        false
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

/// Web Audio block processor for AudioWorklet integration
///
/// This struct provides the sample-accurate processing required for
/// Web Audio's AudioWorkletProcessor. It handles the 128-sample render
/// quantum and provides efficient block processing.
///
/// # JavaScript Integration Example
///
/// ```javascript
/// // In your AudioWorkletProcessor
/// class QuiverProcessor extends AudioWorkletProcessor {
///   constructor() {
///     super();
///     this.engine = new QuiverEngine(sampleRate);
///     this.engine.load_patch(patchJson);
///     this.engine.compile();
///   }
///
///   process(inputs, outputs, parameters) {
///     const output = outputs[0];
///     const samples = this.engine.process_block(128);
///
///     // Deinterleave stereo output
///     for (let i = 0; i < 128; i++) {
///       output[0][i] = samples[i * 2];
///       output[1][i] = samples[i * 2 + 1];
///     }
///     return true;
///   }
/// }
/// ```
pub struct WebAudioBlockProcessor {
    /// Configuration
    config: WebAudioConfig,
    /// Left channel buffer
    left_buffer: Vec<f64>,
    /// Right channel buffer
    right_buffer: Vec<f64>,
    /// Interleaved output buffer (for f32)
    interleaved_buffer: Vec<f32>,
    /// Parameter map
    parameters: HashMap<String, Arc<AtomicF64>>,
    /// Active state
    active: bool,
}

impl WebAudioBlockProcessor {
    /// Create a new block processor with default Web Audio config (128 samples)
    pub fn new() -> Self {
        Self::with_config(WebAudioConfig::default())
    }

    /// Create a new block processor with custom configuration
    pub fn with_config(config: WebAudioConfig) -> Self {
        let block_size = config.block_size;
        Self {
            config,
            left_buffer: vec![0.0; block_size],
            right_buffer: vec![0.0; block_size],
            interleaved_buffer: vec![0.0; block_size * 2],
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

    /// Initialize/activate the processor
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// Deactivate the processor
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Check if active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get configuration
    pub fn config(&self) -> &WebAudioConfig {
        &self.config
    }

    /// Get the block size
    pub fn block_size(&self) -> usize {
        self.config.block_size
    }

    /// Get the sample rate
    pub fn sample_rate(&self) -> f64 {
        self.config.sample_rate
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

    /// Get all parameter names
    pub fn parameter_names(&self) -> Vec<String> {
        self.parameters.keys().cloned().collect()
    }

    /// Process a block using a closure that generates samples
    ///
    /// The closure receives the sample index and should return (left, right).
    /// Returns a reference to the interleaved output buffer.
    pub fn process_with<F>(&mut self, mut generator: F) -> &[f32]
    where
        F: FnMut(usize) -> (f64, f64),
    {
        for i in 0..self.config.block_size {
            let (left, right) = generator(i);
            self.left_buffer[i] = left;
            self.right_buffer[i] = right;
        }

        interleave_stereo(
            &self.left_buffer,
            &self.right_buffer,
            &mut self.interleaved_buffer,
        );

        &self.interleaved_buffer
    }

    /// Get the left channel buffer (for direct writing)
    pub fn left_buffer_mut(&mut self) -> &mut [f64] {
        &mut self.left_buffer
    }

    /// Get the right channel buffer (for direct writing)
    pub fn right_buffer_mut(&mut self) -> &mut [f64] {
        &mut self.right_buffer
    }

    /// Finalize and get interleaved output after writing to channel buffers
    pub fn finalize(&mut self) -> &[f32] {
        interleave_stereo(
            &self.left_buffer,
            &self.right_buffer,
            &mut self.interleaved_buffer,
        );
        &self.interleaved_buffer
    }

    /// Clear all buffers
    pub fn clear(&mut self) {
        self.left_buffer.fill(0.0);
        self.right_buffer.fill(0.0);
        self.interleaved_buffer.fill(0.0);
    }
}

impl Default for WebAudioBlockProcessor {
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

    #[test]
    fn test_osc_value_to_f64() {
        assert!((OscValue::Int(42).to_f64().unwrap() - 42.0).abs() < 0.001);
        assert!((OscValue::Float(2.5).to_f64().unwrap() - 2.5).abs() < 0.01);
        assert!((OscValue::Long(100).to_f64().unwrap() - 100.0).abs() < 0.001);
        assert!((OscValue::Double(2.71).to_f64().unwrap() - 2.71).abs() < 0.001);
        assert!((OscValue::True.to_f64().unwrap() - 1.0).abs() < 0.001);
        assert!((OscValue::False.to_f64().unwrap() - 0.0).abs() < 0.001);
        assert!(OscValue::Nil.to_f64().is_none());
    }

    #[test]
    fn test_osc_value_to_bool() {
        assert_eq!(OscValue::Int(1).to_bool(), Some(true));
        assert_eq!(OscValue::Int(0).to_bool(), Some(false));
        assert_eq!(OscValue::Float(1.0).to_bool(), Some(true));
        assert_eq!(OscValue::Float(0.0).to_bool(), Some(false));
        assert_eq!(OscValue::True.to_bool(), Some(true));
        assert_eq!(OscValue::False.to_bool(), Some(false));
        assert_eq!(OscValue::Nil.to_bool(), None);
    }

    #[test]
    fn test_osc_message_with_int() {
        let msg = OscMessage::new("/test").with_int(42);
        assert_eq!(msg.args.len(), 1);
    }

    #[test]
    fn test_osc_pattern_single_char() {
        let pattern = OscPattern::new("/a/?");
        assert!(pattern.matches("/a/b"));
        assert!(!pattern.matches("/a/bb"));
    }

    #[test]
    fn test_osc_pattern_char_class() {
        let pattern = OscPattern::new("/[abc]");
        assert!(pattern.matches("/a"));
        assert!(pattern.matches("/b"));
        assert!(!pattern.matches("/d"));
    }

    #[test]
    fn test_osc_binding_with_offset() {
        let value = Arc::new(AtomicF64::new(0.0));
        let binding = OscBinding::new("/test", value.clone())
            .with_scale(2.0)
            .with_offset(10.0);

        let msg = OscMessage::new("/test").with_float(5.0);
        binding.apply(&msg);
        assert!((value.get() - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_osc_binding_non_matching() {
        let value = Arc::new(AtomicF64::new(0.0));
        let binding = OscBinding::new("/test", value.clone());

        let msg = OscMessage::new("/other").with_float(5.0);
        assert!(!binding.apply(&msg));
    }

    #[test]
    fn test_osc_receiver_bind_scaled() {
        let mut receiver = OscReceiver::new();
        let value = Arc::new(AtomicF64::new(0.0));
        receiver.bind_scaled("/test", value.clone(), 10.0, 5.0);

        let msg = OscMessage::new("/test").with_float(1.0);
        receiver.handle_message(&msg);
        assert!((value.get() - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_osc_receiver_counters() {
        let mut receiver = OscReceiver::new();
        let value = Arc::new(AtomicF64::new(0.0));
        receiver.bind("/test", value.clone());

        let msg = OscMessage::new("/test").with_float(1.0);
        receiver.handle_message(&msg);

        assert_eq!(receiver.message_count(), 1);
        assert_eq!(receiver.matched_count(), 1);
        assert_eq!(receiver.binding_count(), 1);

        receiver.reset_counters();
        assert_eq!(receiver.message_count(), 0);
    }

    #[test]
    fn test_osc_receiver_default() {
        let receiver = OscReceiver::default();
        assert_eq!(receiver.binding_count(), 0);
    }

    #[test]
    fn test_osc_input_module() {
        let value = Arc::new(AtomicF64::new(5.0));
        let mut input = OscInput::new("/test/param", value.clone(), SignalKind::CvUnipolar);

        assert_eq!(input.address(), "/test/param");
        assert!((input.value_ref().get() - 5.0).abs() < 0.001);

        let inputs = PortValues::new();
        let mut outputs = PortValues::new();
        input.tick(&inputs, &mut outputs);

        assert!((outputs.get(0).unwrap() - 5.0).abs() < 0.001);

        input.reset();
        input.set_sample_rate(48000.0);
        assert_eq!(input.type_id(), "osc_input");
    }

    // MIDI Tests
    #[test]
    fn test_midi_status_parsing() {
        assert_eq!(MidiStatus::from_byte(0x90), Some(MidiStatus::NoteOn(0)));
        assert_eq!(MidiStatus::from_byte(0x95), Some(MidiStatus::NoteOn(5)));
        assert_eq!(MidiStatus::from_byte(0x80), Some(MidiStatus::NoteOff(0)));
        assert_eq!(
            MidiStatus::from_byte(0xB0),
            Some(MidiStatus::ControlChange(0))
        );
        assert_eq!(MidiStatus::from_byte(0xE0), Some(MidiStatus::PitchBend(0)));
        assert_eq!(MidiStatus::from_byte(0xF0), Some(MidiStatus::System(0xF0)));
    }

    #[test]
    fn test_midi_status_channel() {
        let note_on = MidiStatus::NoteOn(5);
        assert_eq!(note_on.channel(), Some(5));

        let system = MidiStatus::System(0xF0);
        assert_eq!(system.channel(), None);
    }

    #[test]
    fn test_midi_note_on() {
        let msg = MidiMessage::note_on(0, 60, 100);
        assert!(msg.is_note_on());
        assert!(!msg.is_note_off());
        assert_eq!(msg.note(), 60);
        assert_eq!(msg.velocity(), 100);
    }

    #[test]
    fn test_midi_note_off() {
        let msg = MidiMessage::note_off(0, 60, 0);
        assert!(!msg.is_note_on());
        assert!(msg.is_note_off());
    }

    #[test]
    fn test_midi_note_on_zero_velocity() {
        // Note On with velocity 0 is treated as Note Off
        let msg = MidiMessage::note_on(0, 60, 0);
        assert!(!msg.is_note_on());
        assert!(msg.is_note_off());
    }

    #[test]
    fn test_midi_control_change() {
        let msg = MidiMessage::control_change(0, 1, 64);
        assert_eq!(msg.data1, 1); // CC number
        assert_eq!(msg.data2, 64); // Value
    }

    #[test]
    fn test_midi_pitch_bend() {
        // Center position (0)
        let msg = MidiMessage::pitch_bend(0, 0);
        let normalized = msg.pitch_bend_normalized();
        assert!(normalized.abs() < 0.001);

        // Max positive
        let msg = MidiMessage::pitch_bend(0, 8191);
        let normalized = msg.pitch_bend_normalized();
        assert!((normalized - 1.0).abs() < 0.01);

        // Max negative
        let msg = MidiMessage::pitch_bend(0, -8192);
        let normalized = msg.pitch_bend_normalized();
        assert!((normalized + 1.0).abs() < 0.01);
    }

    #[test]
    fn test_midi_note_to_frequency() {
        let msg = MidiMessage::note_on(0, 69, 100); // A4
        assert!((msg.note_to_frequency() - 440.0).abs() < 0.01);

        let msg = MidiMessage::note_on(0, 60, 100); // C4
        assert!((msg.note_to_frequency() - 261.63).abs() < 0.1);
    }

    #[test]
    fn test_midi_note_to_volt_per_octave() {
        let msg = MidiMessage::note_on(0, 60, 100); // C4 = 0V
        assert!(msg.note_to_volt_per_octave().abs() < 0.001);

        let msg = MidiMessage::note_on(0, 72, 100); // C5 = 1V
        assert!((msg.note_to_volt_per_octave() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_midi_at_sample() {
        let msg = MidiMessage::note_on(0, 60, 100).at_sample(64);
        assert_eq!(msg.sample_offset, 64);
    }

    #[test]
    fn test_midi_buffer() {
        let mut buffer = MidiBuffer::new();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);

        buffer.push(MidiMessage::note_on(0, 60, 100).at_sample(0));
        buffer.push(MidiMessage::note_on(0, 64, 100).at_sample(32));
        buffer.push(MidiMessage::note_on(0, 67, 100).at_sample(64));

        assert_eq!(buffer.len(), 3);
        assert!(!buffer.is_empty());

        // Test events_at
        let at_0: Vec<_> = buffer.events_at(0).collect();
        assert_eq!(at_0.len(), 1);
        assert_eq!(at_0[0].note(), 60);

        // Test clear
        buffer.clear();
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_midi_buffer_sort() {
        let mut buffer = MidiBuffer::with_capacity(10);
        buffer.push(MidiMessage::note_on(0, 60, 100).at_sample(64));
        buffer.push(MidiMessage::note_on(0, 64, 100).at_sample(0));
        buffer.push(MidiMessage::note_on(0, 67, 100).at_sample(32));

        buffer.sort();

        let events: Vec<_> = buffer.iter().collect();
        assert_eq!(events[0].sample_offset, 0);
        assert_eq!(events[1].sample_offset, 32);
        assert_eq!(events[2].sample_offset, 64);
    }

    #[test]
    fn test_midi_buffer_default() {
        let buffer = MidiBuffer::default();
        assert!(buffer.is_empty());
    }

    // Plugin Wrapper Extended Tests
    #[test]
    fn test_plugin_wrapper_latency() {
        let info = PluginInfo::effect("com.quiver.test", "Test Effect", "Quiver");
        let bus = AudioBusConfig::stereo_io();

        let mut wrapper = PluginWrapper::new(info, bus);
        assert_eq!(wrapper.latency(), 0);

        wrapper.set_latency(256);
        assert_eq!(wrapper.latency(), 256);
    }

    #[test]
    fn test_plugin_wrapper_processing_state() {
        let info = PluginInfo::synth("com.quiver.test", "Test Synth", "Quiver");
        let bus = AudioBusConfig::stereo_out();
        let wrapper = PluginWrapper::new(info, bus);

        assert!(!wrapper.is_processing());
        wrapper.start_processing();
        assert!(wrapper.is_processing());
        wrapper.stop_processing();
        assert!(!wrapper.is_processing());
    }

    #[test]
    fn test_audio_bus_config() {
        let stereo_out = AudioBusConfig::stereo_out();
        assert_eq!(stereo_out.inputs, 0);
        assert_eq!(stereo_out.outputs, 2);

        let stereo_io = AudioBusConfig::stereo_io();
        assert_eq!(stereo_io.inputs, 2);
        assert_eq!(stereo_io.outputs, 2);

        let mono_out = AudioBusConfig::mono_out();
        assert_eq!(mono_out.inputs, 0);
        assert_eq!(mono_out.outputs, 1);
    }

    // Web Audio Block Processor Tests
    #[test]
    fn test_web_audio_block_processor_new() {
        let processor = WebAudioBlockProcessor::new();
        assert_eq!(processor.block_size(), 128);
        assert!((processor.sample_rate() - 44100.0).abs() < 0.001);
        assert!(!processor.is_active());
    }

    #[test]
    fn test_web_audio_block_processor_with_config() {
        let config = WebAudioConfig {
            input_channels: 0,
            output_channels: 2,
            sample_rate: 48000.0,
            block_size: 256,
        };
        let processor = WebAudioBlockProcessor::with_config(config);
        assert_eq!(processor.block_size(), 256);
        assert!((processor.sample_rate() - 48000.0).abs() < 0.001);
    }

    #[test]
    fn test_web_audio_block_processor_activate() {
        let mut processor = WebAudioBlockProcessor::new();
        assert!(!processor.is_active());

        processor.activate();
        assert!(processor.is_active());

        processor.deactivate();
        assert!(!processor.is_active());
    }

    #[test]
    fn test_web_audio_block_processor_parameters() {
        let mut processor = WebAudioBlockProcessor::new();
        let freq = processor.add_parameter("frequency", 440.0);

        assert!((processor.get_parameter("frequency").unwrap() - 440.0).abs() < 0.001);

        processor.set_parameter("frequency", 880.0);
        assert!((freq.get() - 880.0).abs() < 0.001);

        let names = processor.parameter_names();
        assert!(names.contains(&"frequency".to_string()));
    }

    #[test]
    fn test_web_audio_block_processor_process_with() {
        let mut processor = WebAudioBlockProcessor::new();
        let mut phase = 0.0;

        let output = processor.process_with(|_i| {
            let sample = (phase * std::f64::consts::TAU).sin();
            phase += 440.0 / 44100.0;
            (sample, sample)
        });

        // Output is interleaved stereo, 128 * 2 = 256 samples
        assert_eq!(output.len(), 256);

        // First sample should be close to 0 (sin(0))
        assert!(output[0].abs() < 0.1);
    }

    #[test]
    fn test_web_audio_block_processor_direct_buffer() {
        let mut processor = WebAudioBlockProcessor::new();

        // Write directly to left buffer
        {
            let left = processor.left_buffer_mut();
            for i in 0..128 {
                left[i] = (i as f64) / 128.0;
            }
        }

        // Write directly to right buffer
        {
            let right = processor.right_buffer_mut();
            for i in 0..128 {
                right[i] = 1.0 - (i as f64) / 128.0;
            }
        }

        let output = processor.finalize();

        // Check first sample pair
        assert!(output[0].abs() < 0.01); // left[0] = 0
        assert!((output[1] - 1.0).abs() < 0.01); // right[0] = 1

        // Check last sample pair
        assert!((output[254] - 127.0 / 128.0).abs() < 0.01);
        assert!((output[255] - 1.0 / 128.0).abs() < 0.01);
    }

    #[test]
    fn test_web_audio_block_processor_clear() {
        let mut processor = WebAudioBlockProcessor::new();

        // Fill with non-zero values
        processor.process_with(|_| (1.0, 1.0));

        // Clear
        processor.clear();

        let output = processor.finalize();
        for sample in output {
            assert!(*sample < 0.001);
        }
    }

    #[test]
    fn test_web_audio_block_processor_default() {
        let processor = WebAudioBlockProcessor::default();
        assert_eq!(processor.block_size(), 128);
    }

    #[test]
    fn test_f64_to_f32_block() {
        let src = vec![0.5_f64, -0.5, 1.0, -1.0];
        let mut dst = vec![0.0_f32; 4];

        f64_to_f32_block(&src, &mut dst);

        assert!((dst[0] - 0.5).abs() < 0.001);
        assert!((dst[1] + 0.5).abs() < 0.001);
        assert!((dst[2] - 1.0).abs() < 0.001);
        assert!((dst[3] + 1.0).abs() < 0.001);
    }

    #[test]
    fn test_f32_to_f64_block() {
        let src = vec![0.5_f32, -0.5, 1.0, -1.0];
        let mut dst = vec![0.0_f64; 4];

        f32_to_f64_block(&src, &mut dst);

        assert!((dst[0] - 0.5).abs() < 0.001);
        assert!((dst[1] + 0.5).abs() < 0.001);
        assert!((dst[2] - 1.0).abs() < 0.001);
        assert!((dst[3] + 1.0).abs() < 0.001);
    }
}
