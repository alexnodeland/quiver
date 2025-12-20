//! Visual Tools
//!
//! This module provides visualization and analysis tools:
//! - Patch visualization (DOT/GraphViz export)
//! - Parameter automation recording
//! - Scope/analyzer modules for signal monitoring

use crate::graph::{NodeId, Patch};
use crate::port::{PortSpec, SignalKind};
use std::collections::VecDeque;

// =============================================================================
// Patch Visualization (DOT/GraphViz Export)
// =============================================================================

/// Style options for DOT graph export
#[derive(Debug, Clone)]
pub struct DotStyle {
    /// Graph direction: "TB" (top-bottom), "LR" (left-right), "BT", "RL"
    pub rankdir: String,
    /// Node shape for modules
    pub node_shape: String,
    /// Font name
    pub font_name: String,
    /// Font size
    pub font_size: u32,
    /// Whether to show port names on edges
    pub show_port_names: bool,
    /// Whether to color-code by signal type
    pub color_by_signal: bool,
    /// Background color
    pub bg_color: String,
    /// Node fill color
    pub node_color: String,
    /// Edge color
    pub edge_color: String,
}

impl Default for DotStyle {
    fn default() -> Self {
        Self {
            rankdir: "LR".to_string(),
            node_shape: "box".to_string(),
            font_name: "Helvetica".to_string(),
            font_size: 12,
            show_port_names: true,
            color_by_signal: true,
            bg_color: "#1a1a2e".to_string(),
            node_color: "#16213e".to_string(),
            edge_color: "#e94560".to_string(),
        }
    }
}

impl DotStyle {
    /// Create a light theme style
    pub fn light() -> Self {
        Self {
            bg_color: "#ffffff".to_string(),
            node_color: "#f0f0f0".to_string(),
            edge_color: "#333333".to_string(),
            ..Default::default()
        }
    }

    /// Create a minimal style
    pub fn minimal() -> Self {
        Self {
            show_port_names: false,
            color_by_signal: false,
            node_shape: "ellipse".to_string(),
            ..Default::default()
        }
    }

    pub fn with_rankdir(mut self, dir: impl Into<String>) -> Self {
        self.rankdir = dir.into();
        self
    }

    pub fn with_node_shape(mut self, shape: impl Into<String>) -> Self {
        self.node_shape = shape.into();
        self
    }
}

/// DOT/GraphViz exporter for patches
pub struct DotExporter;

impl DotExporter {
    /// Export a patch to DOT format
    pub fn export(patch: &Patch, style: &DotStyle) -> String {
        let mut dot = String::new();

        // Graph header
        dot.push_str("digraph patch {\n");
        dot.push_str(&format!("    rankdir={};\n", style.rankdir));
        dot.push_str(&format!("    bgcolor=\"{}\";\n", style.bg_color));
        dot.push_str(&format!(
            "    node [shape={}, style=filled, fillcolor=\"{}\", fontname=\"{}\", fontsize={}];\n",
            style.node_shape, style.node_color, style.font_name, style.font_size
        ));
        dot.push_str(&format!(
            "    edge [color=\"{}\", fontname=\"{}\", fontsize={}];\n",
            style.edge_color,
            style.font_name,
            style.font_size - 2
        ));
        dot.push('\n');

        // Collect node info
        let mut node_map: std::collections::HashMap<NodeId, String> =
            std::collections::HashMap::new();

        for (id, name, module) in patch.nodes() {
            node_map.insert(id, name.to_string());

            let spec = module.port_spec();
            let label = Self::create_node_label(name, module.type_id(), spec);

            dot.push_str(&format!("    \"{}\" [label=<{}>];\n", name, label));
        }

        dot.push('\n');

        // Export cables as edges
        for cable in patch.cables() {
            let from_name = node_map
                .get(&cable.from.node)
                .map(|s| s.as_str())
                .unwrap_or("?");
            let to_name = node_map
                .get(&cable.to.node)
                .map(|s| s.as_str())
                .unwrap_or("?");

            // Get port names
            let from_port = Self::get_port_name(patch, cable.from.node, cable.from.port, false);
            let to_port = Self::get_port_name(patch, cable.to.node, cable.to.port, true);

            let mut edge_attrs = Vec::new();

            if style.show_port_names {
                let label = format!("{}→{}", from_port, to_port);
                edge_attrs.push(format!("label=\"{}\"", label));
            }

            // Color by signal type if enabled
            if style.color_by_signal {
                if let Some(color) = Self::get_signal_color(patch, cable.from.node, cable.from.port)
                {
                    edge_attrs.push(format!("color=\"{}\"", color));
                }
            }

            // Show attenuation if present
            if let Some(att) = cable.attenuation {
                if (att - 1.0).abs() > 0.01 {
                    edge_attrs.push("style=dashed".to_string());
                    if !style.show_port_names {
                        edge_attrs.push(format!("label=\"×{:.2}\"", att));
                    }
                }
            }

            let attrs = if edge_attrs.is_empty() {
                String::new()
            } else {
                format!(" [{}]", edge_attrs.join(", "))
            };

            dot.push_str(&format!(
                "    \"{}\" -> \"{}\"{};\n",
                from_name, to_name, attrs
            ));
        }

        dot.push_str("}\n");
        dot
    }

    /// Export a patch to DOT format with default style
    pub fn export_default(patch: &Patch) -> String {
        Self::export(patch, &DotStyle::default())
    }

    fn create_node_label(name: &str, type_id: &str, spec: &PortSpec) -> String {
        let mut label = String::new();

        // Use HTML-like label for better formatting
        label.push_str("<TABLE BORDER=\"0\" CELLBORDER=\"1\" CELLSPACING=\"0\">");

        // Header row with module name and type
        label.push_str(&format!(
            "<TR><TD COLSPAN=\"2\"><B>{}</B><BR/><FONT POINT-SIZE=\"10\">{}</FONT></TD></TR>",
            name, type_id
        ));

        // Inputs column
        if !spec.inputs.is_empty() || !spec.outputs.is_empty() {
            label.push_str("<TR>");

            // Inputs
            label.push_str("<TD ALIGN=\"LEFT\">");
            for input in &spec.inputs {
                label.push_str(&format!("→ {}<BR/>", input.name));
            }
            if spec.inputs.is_empty() {
                label.push(' ');
            }
            label.push_str("</TD>");

            // Outputs
            label.push_str("<TD ALIGN=\"RIGHT\">");
            for output in &spec.outputs {
                label.push_str(&format!("{} →<BR/>", output.name));
            }
            if spec.outputs.is_empty() {
                label.push(' ');
            }
            label.push_str("</TD>");

            label.push_str("</TR>");
        }

        label.push_str("</TABLE>");
        label
    }

    fn get_port_name(patch: &Patch, node: NodeId, port_id: u32, is_input: bool) -> String {
        for (id, _, module) in patch.nodes() {
            if id == node {
                let spec = module.port_spec();
                let ports = if is_input {
                    &spec.inputs
                } else {
                    &spec.outputs
                };
                for p in ports {
                    if p.id == port_id {
                        return p.name.clone();
                    }
                }
                break;
            }
        }
        format!("port_{}", port_id)
    }

    fn get_signal_color(patch: &Patch, node: NodeId, port_id: u32) -> Option<String> {
        for (id, _, module) in patch.nodes() {
            if id == node {
                let spec = module.port_spec();
                for p in &spec.outputs {
                    if p.id == port_id {
                        return Some(Self::signal_kind_color(&p.kind));
                    }
                }
                break;
            }
        }
        None
    }

    fn signal_kind_color(kind: &SignalKind) -> String {
        match kind {
            SignalKind::Audio => "#e94560".to_string(),         // Red
            SignalKind::CvBipolar => "#0f3460".to_string(),     // Blue
            SignalKind::CvUnipolar => "#00b4d8".to_string(),    // Cyan
            SignalKind::VoltPerOctave => "#90be6d".to_string(), // Green
            SignalKind::Gate => "#f9c74f".to_string(),          // Yellow
            SignalKind::Trigger => "#f8961e".to_string(),       // Orange
            SignalKind::Clock => "#9d4edd".to_string(),         // Purple
        }
    }
}

// =============================================================================
// Parameter Automation Recording
// =============================================================================

/// A single automation point (time, value)
#[derive(Debug, Clone, Copy)]
pub struct AutomationPoint {
    /// Time in samples from start
    pub time: u64,
    /// Parameter value at this time
    pub value: f64,
}

/// Recorded automation data for a single parameter
#[derive(Debug, Clone)]
pub struct AutomationTrack {
    /// Parameter identifier (module_name.param_name)
    pub param_id: String,
    /// Recorded points
    pub points: Vec<AutomationPoint>,
    /// Sample rate used during recording
    pub sample_rate: f64,
}

impl AutomationTrack {
    pub fn new(param_id: impl Into<String>, sample_rate: f64) -> Self {
        Self {
            param_id: param_id.into(),
            points: Vec::new(),
            sample_rate,
        }
    }

    /// Add a point to the track
    pub fn record(&mut self, time: u64, value: f64) {
        self.points.push(AutomationPoint { time, value });
    }

    /// Get the value at a specific time (linear interpolation)
    pub fn value_at(&self, time: u64) -> Option<f64> {
        if self.points.is_empty() {
            return None;
        }

        // Find surrounding points
        let mut before: Option<&AutomationPoint> = None;
        let mut after: Option<&AutomationPoint> = None;

        for point in &self.points {
            if point.time <= time {
                before = Some(point);
            }
            if point.time >= time && after.is_none() {
                after = Some(point);
            }
        }

        match (before, after) {
            (Some(b), Some(a)) if b.time == a.time => Some(b.value),
            (Some(b), Some(a)) => {
                // Linear interpolation
                let t = (time - b.time) as f64 / (a.time - b.time) as f64;
                Some(b.value + t * (a.value - b.value))
            }
            (Some(b), None) => Some(b.value),
            (None, Some(a)) => Some(a.value),
            (None, None) => None,
        }
    }

    /// Get duration in samples
    pub fn duration(&self) -> u64 {
        self.points.last().map(|p| p.time).unwrap_or(0)
    }

    /// Get duration in seconds
    pub fn duration_seconds(&self) -> f64 {
        self.duration() as f64 / self.sample_rate
    }

    /// Simplify the track by removing redundant points
    pub fn simplify(&mut self, tolerance: f64) {
        if self.points.len() < 3 {
            return;
        }

        let mut simplified = vec![self.points[0]];

        for i in 1..self.points.len() - 1 {
            let prev = simplified.last().unwrap();
            let curr = &self.points[i];
            let next = &self.points[i + 1];

            // Check if current point is needed (not on line between prev and next)
            let expected = prev.value
                + (next.value - prev.value) * ((curr.time - prev.time) as f64)
                    / ((next.time - prev.time) as f64);

            if (curr.value - expected).abs() > tolerance {
                simplified.push(*curr);
            }
        }

        simplified.push(*self.points.last().unwrap());
        self.points = simplified;
    }
}

/// Automation recorder for multiple parameters
#[derive(Debug)]
pub struct AutomationRecorder {
    /// Active recording tracks
    tracks: Vec<AutomationTrack>,
    /// Current time in samples
    current_time: u64,
    /// Sample rate
    sample_rate: f64,
    /// Whether currently recording
    recording: bool,
    /// Recording interval (record every N samples)
    record_interval: u64,
    /// Sample counter for interval
    sample_counter: u64,
}

impl AutomationRecorder {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            tracks: Vec::new(),
            current_time: 0,
            sample_rate,
            recording: false,
            record_interval: 1,
            sample_counter: 0,
        }
    }

    /// Set recording interval (record every N samples)
    /// Lower values = more precision but more data
    pub fn set_interval(&mut self, interval: u64) {
        self.record_interval = interval.max(1);
    }

    /// Start recording
    pub fn start(&mut self) {
        self.recording = true;
        self.current_time = 0;
        self.sample_counter = 0;
    }

    /// Stop recording
    pub fn stop(&mut self) {
        self.recording = false;
    }

    /// Check if recording
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Add a parameter to record
    pub fn add_track(&mut self, param_id: impl Into<String>) {
        let id = param_id.into();
        if !self.tracks.iter().any(|t| t.param_id == id) {
            self.tracks.push(AutomationTrack::new(id, self.sample_rate));
        }
    }

    /// Remove a track
    pub fn remove_track(&mut self, param_id: &str) {
        self.tracks.retain(|t| t.param_id != param_id);
    }

    /// Record current parameter values
    /// Call this every sample (or at record_interval)
    pub fn tick(&mut self, mut get_value: impl FnMut(&str) -> Option<f64>) {
        if !self.recording {
            return;
        }

        self.sample_counter += 1;

        if self.sample_counter >= self.record_interval {
            self.sample_counter = 0;

            for track in &mut self.tracks {
                if let Some(value) = get_value(&track.param_id) {
                    track.record(self.current_time, value);
                }
            }
        }

        self.current_time += 1;
    }

    /// Get all recorded tracks
    pub fn tracks(&self) -> &[AutomationTrack] {
        &self.tracks
    }

    /// Get a specific track
    pub fn get_track(&self, param_id: &str) -> Option<&AutomationTrack> {
        self.tracks.iter().find(|t| t.param_id == param_id)
    }

    /// Clear all recorded data
    pub fn clear(&mut self) {
        for track in &mut self.tracks {
            track.points.clear();
        }
        self.current_time = 0;
    }

    /// Simplify all tracks
    pub fn simplify_all(&mut self, tolerance: f64) {
        for track in &mut self.tracks {
            track.simplify(tolerance);
        }
    }

    /// Export to a simple format
    pub fn export(&self) -> AutomationData {
        AutomationData {
            sample_rate: self.sample_rate,
            duration: self.current_time,
            tracks: self.tracks.clone(),
        }
    }
}

/// Exported automation data
#[derive(Debug, Clone)]
pub struct AutomationData {
    pub sample_rate: f64,
    pub duration: u64,
    pub tracks: Vec<AutomationTrack>,
}

// =============================================================================
// Scope/Analyzer Modules
// =============================================================================

/// Oscilloscope for signal visualization
#[derive(Debug)]
pub struct Scope {
    /// Buffer size (samples to display)
    buffer_size: usize,
    /// Signal buffer
    buffer: VecDeque<f64>,
    /// Trigger level
    trigger_level: f64,
    /// Trigger mode
    trigger_mode: TriggerMode,
    /// Whether triggered
    triggered: bool,
    /// Samples since trigger
    samples_since_trigger: usize,
    /// Previous sample (for edge detection)
    prev_sample: f64,
    /// Time division (samples per division)
    time_div: usize,
    /// Voltage division (volts per division)
    volt_div: f64,
    /// Frozen display buffer
    frozen_buffer: Option<Vec<f64>>,
}

/// Scope trigger mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    /// No triggering, free-running
    Free,
    /// Trigger on rising edge
    RisingEdge,
    /// Trigger on falling edge
    FallingEdge,
    /// Trigger on any edge
    AnyEdge,
    /// Single shot (trigger once then freeze)
    Single,
}

impl Scope {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer_size,
            buffer: VecDeque::with_capacity(buffer_size),
            trigger_level: 0.0,
            trigger_mode: TriggerMode::Free,
            triggered: false,
            samples_since_trigger: 0,
            prev_sample: 0.0,
            time_div: buffer_size / 10,
            volt_div: 1.0,
            frozen_buffer: None,
        }
    }

    pub fn set_trigger_level(&mut self, level: f64) {
        self.trigger_level = level;
    }

    pub fn set_trigger_mode(&mut self, mode: TriggerMode) {
        self.trigger_mode = mode;
        self.triggered = false;
        self.frozen_buffer = None;
    }

    pub fn set_time_div(&mut self, samples: usize) {
        self.time_div = samples.max(1);
    }

    pub fn set_volt_div(&mut self, volts: f64) {
        self.volt_div = volts.max(0.001);
    }

    /// Process a sample
    pub fn tick(&mut self, sample: f64) {
        // Check for trigger
        let trigger_detected = match self.trigger_mode {
            TriggerMode::Free => true,
            TriggerMode::RisingEdge => {
                self.prev_sample < self.trigger_level && sample >= self.trigger_level
            }
            TriggerMode::FallingEdge => {
                self.prev_sample > self.trigger_level && sample <= self.trigger_level
            }
            TriggerMode::AnyEdge => {
                (self.prev_sample < self.trigger_level && sample >= self.trigger_level)
                    || (self.prev_sample > self.trigger_level && sample <= self.trigger_level)
            }
            TriggerMode::Single => {
                if self.frozen_buffer.is_some() {
                    false
                } else {
                    self.prev_sample < self.trigger_level && sample >= self.trigger_level
                }
            }
        };

        if trigger_detected && !self.triggered {
            self.triggered = true;
            self.samples_since_trigger = 0;
            self.buffer.clear();
        }

        if self.triggered || self.trigger_mode == TriggerMode::Free {
            self.buffer.push_back(sample);
            if self.buffer.len() > self.buffer_size {
                self.buffer.pop_front();
            }
            self.samples_since_trigger += 1;

            // Check if we've filled the buffer after trigger
            if self.samples_since_trigger >= self.buffer_size {
                if self.trigger_mode == TriggerMode::Single {
                    self.frozen_buffer = Some(self.buffer.iter().copied().collect());
                }
                self.triggered = false;
            }
        }

        self.prev_sample = sample;
    }

    /// Get the display buffer
    pub fn get_buffer(&self) -> &[f64] {
        if let Some(ref frozen) = self.frozen_buffer {
            frozen
        } else {
            // Convert VecDeque to slice via make_contiguous would need &mut
            // For now, return empty if frozen, otherwise caller should use iter
            &[]
        }
    }

    /// Get buffer as Vec
    pub fn buffer_vec(&self) -> Vec<f64> {
        if let Some(ref frozen) = self.frozen_buffer {
            frozen.clone()
        } else {
            self.buffer.iter().copied().collect()
        }
    }

    /// Get display data with normalized coordinates
    /// Returns Vec of (x, y) where x is 0.0-1.0 and y is in voltage
    pub fn get_display_data(&self) -> Vec<(f64, f64)> {
        let samples = self.buffer_vec();
        let len = samples.len();
        if len == 0 {
            return vec![];
        }

        samples
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as f64 / len as f64, v))
            .collect()
    }

    /// Reset the scope
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.triggered = false;
        self.samples_since_trigger = 0;
        self.prev_sample = 0.0;
        self.frozen_buffer = None;
    }
}

/// Spectrum analyzer
#[derive(Debug)]
pub struct SpectrumAnalyzer {
    /// FFT size
    fft_size: usize,
    /// Sample buffer
    buffer: Vec<f64>,
    /// Current write position
    write_pos: usize,
    /// Sample rate
    sample_rate: f64,
    /// Latest magnitude spectrum (dB)
    spectrum: Vec<f64>,
    /// Smoothing factor (0.0 = no smoothing, 0.99 = heavy smoothing)
    smoothing: f64,
}

impl SpectrumAnalyzer {
    pub fn new(fft_size: usize, sample_rate: f64) -> Self {
        // Ensure power of 2
        let fft_size = fft_size.next_power_of_two();
        Self {
            fft_size,
            buffer: vec![0.0; fft_size],
            write_pos: 0,
            sample_rate,
            spectrum: vec![-100.0; fft_size / 2],
            smoothing: 0.8,
        }
    }

    pub fn set_smoothing(&mut self, smoothing: f64) {
        self.smoothing = smoothing.clamp(0.0, 0.99);
    }

    /// Process a sample
    pub fn tick(&mut self, sample: f64) {
        self.buffer[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) % self.fft_size;

        // When buffer is full, compute spectrum
        if self.write_pos == 0 {
            self.compute_spectrum();
        }
    }

    fn compute_spectrum(&mut self) {
        // Simple DFT (not optimized, but works for demonstration)
        // In production, you'd use a proper FFT library
        let n = self.fft_size;
        let half = n / 2;

        for k in 0..half {
            let mut real = 0.0;
            let mut imag = 0.0;

            for (i, &sample) in self.buffer.iter().enumerate() {
                // Apply Hann window
                let window =
                    0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos());
                let windowed = sample * window;

                let angle = -2.0 * std::f64::consts::PI * k as f64 * i as f64 / n as f64;
                real += windowed * angle.cos();
                imag += windowed * angle.sin();
            }

            let magnitude = (real * real + imag * imag).sqrt() / (n as f64);
            let db = 20.0 * (magnitude + 1e-10).log10();

            // Apply smoothing
            self.spectrum[k] = self.smoothing * self.spectrum[k] + (1.0 - self.smoothing) * db;
        }
    }

    /// Get the spectrum as (frequency, magnitude_db) pairs
    pub fn get_spectrum(&self) -> Vec<(f64, f64)> {
        let freq_resolution = self.sample_rate / self.fft_size as f64;

        self.spectrum
            .iter()
            .enumerate()
            .map(|(i, &db)| (i as f64 * freq_resolution, db))
            .collect()
    }

    /// Get magnitude at a specific frequency
    pub fn magnitude_at(&self, freq: f64) -> f64 {
        let bin = (freq * self.fft_size as f64 / self.sample_rate) as usize;
        if bin < self.spectrum.len() {
            self.spectrum[bin]
        } else {
            -100.0
        }
    }

    /// Get peak frequency
    pub fn peak_frequency(&self) -> f64 {
        let freq_resolution = self.sample_rate / self.fft_size as f64;

        let (peak_bin, _) = self
            .spectrum
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap_or((0, &-100.0));

        peak_bin as f64 * freq_resolution
    }

    /// Reset the analyzer
    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.spectrum.fill(-100.0);
        self.write_pos = 0;
    }
}

/// Level meter with peak hold
#[derive(Debug)]
pub struct LevelMeter {
    /// Current RMS level (dB)
    rms_db: f64,
    /// Current peak level (dB)
    peak_db: f64,
    /// Peak hold level (dB)
    peak_hold_db: f64,
    /// Peak hold counter
    peak_hold_counter: u64,
    /// Peak hold time in samples
    peak_hold_samples: u64,
    /// RMS window
    rms_window: VecDeque<f64>,
    /// RMS window size
    window_size: usize,
    /// Attack coefficient
    attack_coeff: f64,
    /// Release coefficient
    release_coeff: f64,
}

impl LevelMeter {
    pub fn new(sample_rate: f64) -> Self {
        let window_size = (sample_rate * 0.05) as usize; // 50ms window
        Self {
            rms_db: -100.0,
            peak_db: -100.0,
            peak_hold_db: -100.0,
            peak_hold_counter: 0,
            peak_hold_samples: (sample_rate * 1.5) as u64, // 1.5 second hold
            rms_window: VecDeque::with_capacity(window_size),
            window_size,
            attack_coeff: (-1.0 / (sample_rate * 0.001)).exp(), // 1ms attack
            release_coeff: (-1.0 / (sample_rate * 0.300)).exp(), // 300ms release
        }
    }

    pub fn set_peak_hold_time(&mut self, seconds: f64, sample_rate: f64) {
        self.peak_hold_samples = (sample_rate * seconds) as u64;
    }

    /// Process a sample
    pub fn tick(&mut self, sample: f64) {
        let abs_sample = sample.abs();

        // Update RMS window
        self.rms_window.push_back(sample * sample);
        if self.rms_window.len() > self.window_size {
            self.rms_window.pop_front();
        }

        // Calculate RMS
        let rms = (self.rms_window.iter().sum::<f64>() / self.rms_window.len() as f64).sqrt();
        let target_rms_db = 20.0 * (rms + 1e-10).log10();

        // Smooth RMS
        let coeff = if target_rms_db > self.rms_db {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.rms_db = coeff * self.rms_db + (1.0 - coeff) * target_rms_db;

        // Update peak
        let sample_db = 20.0 * (abs_sample + 1e-10).log10();
        if sample_db > self.peak_db {
            self.peak_db = sample_db;
        } else {
            self.peak_db =
                self.release_coeff * self.peak_db + (1.0 - self.release_coeff) * sample_db;
        }

        // Update peak hold
        if sample_db >= self.peak_hold_db {
            self.peak_hold_db = sample_db;
            self.peak_hold_counter = 0;
        } else {
            self.peak_hold_counter += 1;
            if self.peak_hold_counter >= self.peak_hold_samples {
                self.peak_hold_db = self.peak_db;
            }
        }
    }

    /// Get current RMS level in dB
    pub fn rms(&self) -> f64 {
        self.rms_db
    }

    /// Get current peak level in dB
    pub fn peak(&self) -> f64 {
        self.peak_db
    }

    /// Get peak hold level in dB
    pub fn peak_hold(&self) -> f64 {
        self.peak_hold_db
    }

    /// Check if clipping (peak > 0dB)
    pub fn is_clipping(&self) -> bool {
        self.peak_db > 0.0
    }

    /// Reset the meter
    pub fn reset(&mut self) {
        self.rms_db = -100.0;
        self.peak_db = -100.0;
        self.peak_hold_db = -100.0;
        self.peak_hold_counter = 0;
        self.rms_window.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // DOT Export tests

    #[test]
    fn test_dot_style_default() {
        let style = DotStyle::default();
        assert_eq!(style.rankdir, "LR");
        assert!(style.show_port_names);
        assert!(style.color_by_signal);
    }

    #[test]
    fn test_dot_style_light() {
        let style = DotStyle::light();
        assert_eq!(style.bg_color, "#ffffff");
    }

    #[test]
    fn test_dot_style_minimal() {
        let style = DotStyle::minimal();
        assert!(!style.show_port_names);
        assert!(!style.color_by_signal);
    }

    // Automation tests

    #[test]
    fn test_automation_track() {
        let mut track = AutomationTrack::new("test.param", 44100.0);

        track.record(0, 0.0);
        track.record(44100, 1.0);

        assert_eq!(track.value_at(0), Some(0.0));
        assert_eq!(track.value_at(44100), Some(1.0));

        // Test interpolation
        let mid = track.value_at(22050).unwrap();
        assert!((mid - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_automation_recorder() {
        let mut recorder = AutomationRecorder::new(44100.0);
        recorder.add_track("osc.freq");
        recorder.start();

        let mut value = 0.0;
        for _ in 0..100 {
            recorder.tick(|_| {
                value += 0.01;
                Some(value)
            });
        }

        recorder.stop();

        let track = recorder.get_track("osc.freq").unwrap();
        assert_eq!(track.points.len(), 100);
    }

    #[test]
    fn test_automation_simplify() {
        let mut track = AutomationTrack::new("test", 44100.0);

        // Create a straight line with points in between
        for i in 0..100 {
            track.record(i * 100, i as f64);
        }

        let original_len = track.points.len();
        track.simplify(0.1);

        // Should be simplified to just start and end
        assert!(track.points.len() < original_len);
    }

    // Scope tests

    #[test]
    fn test_scope_free_running() {
        let mut scope = Scope::new(100);
        scope.set_trigger_mode(TriggerMode::Free);

        for i in 0..200 {
            scope.tick((i as f64 * 0.1).sin());
        }

        let data = scope.get_display_data();
        assert_eq!(data.len(), 100);
    }

    #[test]
    fn test_scope_trigger() {
        let mut scope = Scope::new(100);
        scope.set_trigger_mode(TriggerMode::RisingEdge);
        scope.set_trigger_level(0.0);

        // Feed negative values
        for _ in 0..50 {
            scope.tick(-1.0);
        }

        // Trigger on crossing zero
        for i in 0..150 {
            scope.tick(i as f64 * 0.1);
        }

        let data = scope.get_display_data();
        // Should have captured data after trigger
        assert!(!data.is_empty());
    }

    // Spectrum analyzer tests

    #[test]
    fn test_spectrum_analyzer() {
        let mut analyzer = SpectrumAnalyzer::new(256, 44100.0);

        // Feed a simple sine wave
        for i in 0..512 {
            let sample = (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 44100.0).sin();
            analyzer.tick(sample);
        }

        let peak = analyzer.peak_frequency();
        // Should be close to 440 Hz (within one bin)
        assert!((peak - 440.0).abs() < 200.0);
    }

    // Level meter tests

    #[test]
    fn test_level_meter() {
        let mut meter = LevelMeter::new(44100.0);

        // Feed a 0dB sine wave (amplitude 1.0)
        for i in 0..44100 {
            let sample = (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 44100.0).sin();
            meter.tick(sample);
        }

        // RMS of sine wave should be about -3dB
        let rms = meter.rms();
        assert!(rms > -6.0 && rms < 0.0);
    }

    #[test]
    fn test_level_meter_clipping() {
        let mut meter = LevelMeter::new(44100.0);

        // Feed a signal > 1.0
        for _ in 0..1000 {
            meter.tick(2.0);
        }

        assert!(meter.is_clipping());
    }
}
