//! Real-Time State Bridge (Phase 4: GUI Framework)
//!
//! This module provides types and infrastructure for streaming live values
//! from the audio processing to the UI, supporting both WASM polling and
//! HTTP WebSocket push architectures.
//!
//! ## Observable Types
//!
//! - **Param**: Parameter value changes (immediate)
//! - **Level**: Audio level metering with RMS and peak in dB
//! - **Gate**: Binary gate/trigger state detection with hysteresis
//! - **Scope**: Oscilloscope waveform capture for visualization
//! - **Spectrum**: Frequency spectrum via DFT for analyzer display

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::f64::consts::PI;
use serde::{Deserialize, Serialize};

use crate::StdMap;

// =============================================================================
// Observable Value Types
// =============================================================================

/// Values that can be observed and streamed to the UI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ObservableValue {
    /// Parameter value change
    Param {
        node_id: String,
        param_id: String,
        value: f64,
    },

    /// Audio level metering
    Level {
        node_id: String,
        port_id: u32,
        rms_db: f64,
        peak_db: f64,
    },

    /// Gate/trigger state
    Gate {
        node_id: String,
        port_id: u32,
        active: bool,
    },

    /// Oscilloscope waveform data
    Scope {
        node_id: String,
        port_id: u32,
        samples: Vec<f32>,
    },

    /// Spectrum analyzer data
    Spectrum {
        node_id: String,
        port_id: u32,
        bins: Vec<f32>,
        freq_range: (f32, f32),
    },
}

impl ObservableValue {
    /// Get a unique key for this value (for deduplication in UI state)
    pub fn key(&self) -> String {
        match self {
            ObservableValue::Param {
                node_id, param_id, ..
            } => {
                alloc::format!("param:{}:{}", node_id, param_id)
            }
            ObservableValue::Level {
                node_id, port_id, ..
            } => {
                alloc::format!("level:{}:{}", node_id, port_id)
            }
            ObservableValue::Gate {
                node_id, port_id, ..
            } => {
                alloc::format!("gate:{}:{}", node_id, port_id)
            }
            ObservableValue::Scope {
                node_id, port_id, ..
            } => {
                alloc::format!("scope:{}:{}", node_id, port_id)
            }
            ObservableValue::Spectrum {
                node_id, port_id, ..
            } => {
                alloc::format!("spectrum:{}:{}", node_id, port_id)
            }
        }
    }
}

/// Subscription target specifying what to observe
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum SubscriptionTarget {
    /// Subscribe to a parameter value
    Param { node_id: String, param_id: String },

    /// Subscribe to audio level metering
    Level { node_id: String, port_id: u32 },

    /// Subscribe to gate/trigger state
    Gate { node_id: String, port_id: u32 },

    /// Subscribe to oscilloscope waveform
    Scope {
        node_id: String,
        port_id: u32,
        buffer_size: usize,
    },

    /// Subscribe to spectrum analyzer
    Spectrum {
        node_id: String,
        port_id: u32,
        fft_size: usize,
    },
}

impl SubscriptionTarget {
    /// Get a unique ID for this subscription target
    pub fn id(&self) -> String {
        match self {
            SubscriptionTarget::Param { node_id, param_id } => {
                alloc::format!("param:{}:{}", node_id, param_id)
            }
            SubscriptionTarget::Level { node_id, port_id } => {
                alloc::format!("level:{}:{}", node_id, port_id)
            }
            SubscriptionTarget::Gate { node_id, port_id } => {
                alloc::format!("gate:{}:{}", node_id, port_id)
            }
            SubscriptionTarget::Scope {
                node_id, port_id, ..
            } => {
                alloc::format!("scope:{}:{}", node_id, port_id)
            }
            SubscriptionTarget::Spectrum {
                node_id, port_id, ..
            } => {
                alloc::format!("spectrum:{}:{}", node_id, port_id)
            }
        }
    }
}

// =============================================================================
// Port Buffer for Sample Accumulation
// =============================================================================

/// Unique key for a port buffer
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct PortKey {
    node_id: String,
    port_id: u32,
}

/// Buffer for accumulating samples from a single port
#[derive(Debug)]
struct PortBuffer {
    /// Accumulated samples
    samples: Vec<f32>,
    /// Target buffer size
    target_size: usize,
    /// Current gate state (for Gate subscriptions)
    gate_active: bool,
}

impl PortBuffer {
    fn new(size: usize) -> Self {
        Self {
            samples: Vec::with_capacity(size),
            target_size: size,
            gate_active: false,
        }
    }

    fn push(&mut self, sample: f32) {
        if self.samples.len() < self.target_size {
            self.samples.push(sample);
        }
    }

    fn is_full(&self) -> bool {
        self.samples.len() >= self.target_size
    }

    fn clear(&mut self) {
        self.samples.clear();
    }
}

// =============================================================================
// State Observer
// =============================================================================

/// Configuration for the state observer
#[derive(Debug, Clone)]
pub struct ObserverConfig {
    /// Maximum updates per second (default: 60)
    pub max_update_rate: u32,
    /// Maximum pending updates before oldest are dropped (default: 1000)
    pub max_pending_updates: usize,
    /// Default scope buffer size (default: 512)
    pub default_scope_buffer_size: usize,
    /// Default FFT size for spectrum analysis (default: 256)
    pub default_fft_size: usize,
    /// Buffer size for level metering (default: 128)
    pub level_buffer_size: usize,
    /// Sample rate for frequency calculations (default: 44100)
    pub sample_rate: f64,
}

impl Default for ObserverConfig {
    fn default() -> Self {
        Self {
            max_update_rate: 60,
            max_pending_updates: 1000,
            default_scope_buffer_size: 512,
            default_fft_size: 256,
            level_buffer_size: 128,
            sample_rate: 44100.0,
        }
    }
}

/// Manages subscriptions and collects updates for the UI
#[derive(Debug)]
pub struct StateObserver {
    /// Active subscriptions
    subscriptions: Vec<SubscriptionTarget>,
    /// Pending updates to send to UI
    pending_updates: Vec<ObservableValue>,
    /// Configuration
    config: ObserverConfig,
    /// Sample buffers for ports requiring accumulation
    port_buffers: StdMap<PortKey, PortBuffer>,
}

impl StateObserver {
    /// Create a new state observer with default configuration
    pub fn new() -> Self {
        Self::with_config(ObserverConfig::default())
    }

    /// Create a new state observer with custom configuration
    pub fn with_config(config: ObserverConfig) -> Self {
        Self {
            subscriptions: Vec::new(),
            pending_updates: Vec::new(),
            config,
            port_buffers: StdMap::new(),
        }
    }

    /// Set the sample rate (call this when engine sample rate changes)
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.config.sample_rate = sample_rate;
    }

    /// Add subscriptions
    pub fn add_subscriptions(&mut self, targets: Vec<SubscriptionTarget>) {
        for target in targets {
            if !self.subscriptions.iter().any(|s| s.id() == target.id()) {
                // Create port buffer for Level/Scope/Spectrum subscriptions
                self.ensure_port_buffer(&target);
                self.subscriptions.push(target);
            }
        }
    }

    /// Ensure a port buffer exists for subscriptions that need sample accumulation
    fn ensure_port_buffer(&mut self, target: &SubscriptionTarget) {
        let (key, size) = match target {
            SubscriptionTarget::Level { node_id, port_id } => (
                PortKey {
                    node_id: node_id.clone(),
                    port_id: *port_id,
                },
                self.config.level_buffer_size,
            ),
            SubscriptionTarget::Gate { node_id, port_id } => (
                PortKey {
                    node_id: node_id.clone(),
                    port_id: *port_id,
                },
                1, // Gate doesn't need accumulation, but we track state
            ),
            SubscriptionTarget::Scope {
                node_id,
                port_id,
                buffer_size,
            } => (
                PortKey {
                    node_id: node_id.clone(),
                    port_id: *port_id,
                },
                *buffer_size,
            ),
            SubscriptionTarget::Spectrum {
                node_id,
                port_id,
                fft_size,
            } => (
                PortKey {
                    node_id: node_id.clone(),
                    port_id: *port_id,
                },
                *fft_size,
            ),
            SubscriptionTarget::Param { .. } => return, // Params don't need buffers
        };

        self.port_buffers
            .entry(key)
            .or_insert_with(|| PortBuffer::new(size));
    }

    /// Remove subscriptions by ID
    pub fn remove_subscriptions(&mut self, ids: &[String]) {
        self.subscriptions.retain(|s| !ids.contains(&s.id()));
        // Clean up orphaned port buffers
        self.cleanup_port_buffers();
    }

    /// Remove port buffers that no longer have subscriptions
    fn cleanup_port_buffers(&mut self) {
        let active_keys: Vec<PortKey> = self
            .subscriptions
            .iter()
            .filter_map(|s| match s {
                SubscriptionTarget::Level { node_id, port_id }
                | SubscriptionTarget::Gate { node_id, port_id }
                | SubscriptionTarget::Scope {
                    node_id, port_id, ..
                }
                | SubscriptionTarget::Spectrum {
                    node_id, port_id, ..
                } => Some(PortKey {
                    node_id: node_id.clone(),
                    port_id: *port_id,
                }),
                SubscriptionTarget::Param { .. } => None,
            })
            .collect();

        self.port_buffers.retain(|k, _| active_keys.contains(k));
    }

    /// Clear all subscriptions
    pub fn clear_subscriptions(&mut self) {
        self.subscriptions.clear();
        self.port_buffers.clear();
    }

    /// Get all active subscriptions
    pub fn subscriptions(&self) -> &[SubscriptionTarget] {
        &self.subscriptions
    }

    /// Check if a target is subscribed
    pub fn is_subscribed(&self, target: &SubscriptionTarget) -> bool {
        self.subscriptions.iter().any(|s| s.id() == target.id())
    }

    /// Push an update (should be called from audio processing)
    pub fn push_update(&mut self, value: ObservableValue) {
        // Only push if subscribed
        let is_subscribed = self.subscriptions.iter().any(|s| s.id() == value.key());

        if is_subscribed {
            // Remove old update for same key (keep latest)
            self.pending_updates.retain(|v| v.key() != value.key());

            // Add new update
            self.pending_updates.push(value);

            // Trim if over limit (drop oldest)
            while self.pending_updates.len() > self.config.max_pending_updates {
                self.pending_updates.remove(0);
            }
        }
    }

    /// Drain all pending updates (for WASM polling)
    pub fn drain_updates(&mut self) -> Vec<ObservableValue> {
        core::mem::take(&mut self.pending_updates)
    }

    /// Peek at pending updates without draining
    pub fn pending_updates(&self) -> &[ObservableValue] {
        &self.pending_updates
    }

    /// Get number of pending updates
    pub fn pending_count(&self) -> usize {
        self.pending_updates.len()
    }

    /// Get the configuration
    pub fn config(&self) -> &ObserverConfig {
        &self.config
    }

    /// Collect observable values from the patch after processing
    ///
    /// This method should be called after each audio processing cycle
    /// to update subscribed values. It accumulates samples for Level/Scope/Spectrum
    /// and emits updates when buffers are full.
    pub fn collect_from_patch(&mut self, patch: &crate::graph::Patch) {
        // Clone subscriptions to avoid borrow issues
        let subscriptions = self.subscriptions.clone();

        for target in &subscriptions {
            match target {
                SubscriptionTarget::Param { node_id, param_id } => {
                    self.collect_param(patch, node_id, param_id);
                }
                SubscriptionTarget::Level { node_id, port_id } => {
                    self.collect_level(patch, node_id, *port_id);
                }
                SubscriptionTarget::Gate { node_id, port_id } => {
                    self.collect_gate(patch, node_id, *port_id);
                }
                SubscriptionTarget::Scope {
                    node_id, port_id, ..
                } => {
                    self.collect_scope(patch, node_id, *port_id);
                }
                SubscriptionTarget::Spectrum {
                    node_id, port_id, ..
                } => {
                    self.collect_spectrum(patch, node_id, *port_id);
                }
            }
        }
    }

    /// Collect parameter value
    fn collect_param(&mut self, patch: &crate::graph::Patch, node_id: &str, param_id: &str) {
        if let Some(nid) = patch.get_node_id_by_name(node_id) {
            if let Ok(idx) = param_id.parse::<u32>() {
                if let Some(value) = patch.get_param(nid, idx) {
                    self.push_update(ObservableValue::Param {
                        node_id: node_id.into(),
                        param_id: param_id.into(),
                        value,
                    });
                }
            }
        }
    }

    /// Collect level metering (accumulates samples, emits when buffer full)
    fn collect_level(&mut self, patch: &crate::graph::Patch, node_id: &str, port_id: u32) {
        let key = PortKey {
            node_id: node_id.into(),
            port_id,
        };

        // Get current sample value from patch
        let value = patch
            .get_node_id_by_name(node_id)
            .and_then(|nid| patch.get_output_value(nid, port_id));

        let Some(value) = value else { return };

        // Check if buffer is ready and compute update
        let update = if let Some(buffer) = self.port_buffers.get_mut(&key) {
            buffer.push(value as f32);

            if buffer.is_full() {
                let rms_db = calculate_rms_db(&buffer.samples);
                let peak_db = calculate_peak_db(&buffer.samples);
                buffer.clear();
                Some(ObservableValue::Level {
                    node_id: node_id.into(),
                    port_id,
                    rms_db,
                    peak_db,
                })
            } else {
                None
            }
        } else {
            None
        };

        if let Some(update) = update {
            self.push_update(update);
        }
    }

    /// Collect gate state (immediate, with hysteresis)
    fn collect_gate(&mut self, patch: &crate::graph::Patch, node_id: &str, port_id: u32) {
        let key = PortKey {
            node_id: node_id.into(),
            port_id,
        };

        let value = patch
            .get_node_id_by_name(node_id)
            .and_then(|nid| patch.get_output_value(nid, port_id));

        let Some(value) = value else { return };

        // Hysteresis thresholds
        const THRESHOLD_ON: f32 = 2.5;
        const THRESHOLD_OFF: f32 = 0.5;

        let update = if let Some(buffer) = self.port_buffers.get_mut(&key) {
            let sample = value as f32;
            let was_active = buffer.gate_active;

            if buffer.gate_active {
                if sample < THRESHOLD_OFF {
                    buffer.gate_active = false;
                }
            } else if sample > THRESHOLD_ON {
                buffer.gate_active = true;
            }

            // Only emit update on state change
            if buffer.gate_active != was_active {
                Some(ObservableValue::Gate {
                    node_id: node_id.into(),
                    port_id,
                    active: buffer.gate_active,
                })
            } else {
                None
            }
        } else {
            None
        };

        if let Some(update) = update {
            self.push_update(update);
        }
    }

    /// Collect scope waveform (accumulates samples, emits when buffer full)
    fn collect_scope(&mut self, patch: &crate::graph::Patch, node_id: &str, port_id: u32) {
        let key = PortKey {
            node_id: node_id.into(),
            port_id,
        };

        let value = patch
            .get_node_id_by_name(node_id)
            .and_then(|nid| patch.get_output_value(nid, port_id));

        let Some(value) = value else { return };

        let update = if let Some(buffer) = self.port_buffers.get_mut(&key) {
            buffer.push(value as f32);

            if buffer.is_full() {
                let samples = buffer.samples.clone();
                buffer.clear();
                Some(ObservableValue::Scope {
                    node_id: node_id.into(),
                    port_id,
                    samples,
                })
            } else {
                None
            }
        } else {
            None
        };

        if let Some(update) = update {
            self.push_update(update);
        }
    }

    /// Collect spectrum data (accumulates samples, computes DFT when buffer full)
    fn collect_spectrum(&mut self, patch: &crate::graph::Patch, node_id: &str, port_id: u32) {
        let key = PortKey {
            node_id: node_id.into(),
            port_id,
        };

        let value = patch
            .get_node_id_by_name(node_id)
            .and_then(|nid| patch.get_output_value(nid, port_id));

        let Some(value) = value else { return };

        let sample_rate = self.config.sample_rate as f32;

        let update = if let Some(buffer) = self.port_buffers.get_mut(&key) {
            buffer.push(value as f32);

            if buffer.is_full() {
                let bins = compute_magnitude_spectrum(&buffer.samples);
                let freq_range = (0.0, sample_rate / 2.0);
                buffer.clear();
                Some(ObservableValue::Spectrum {
                    node_id: node_id.into(),
                    port_id,
                    bins,
                    freq_range,
                })
            } else {
                None
            }
        } else {
            None
        };

        if let Some(update) = update {
            self.push_update(update);
        }
    }
}

impl Default for StateObserver {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Level Meter Utilities
// =============================================================================

/// Calculate RMS level in decibels from samples
pub fn calculate_rms_db(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return -f64::INFINITY;
    }

    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt();

    if rms > 0.0 {
        20.0 * libm::log10(rms)
    } else {
        -f64::INFINITY
    }
}

/// Calculate peak level in decibels from samples
pub fn calculate_peak_db(samples: &[f32]) -> f64 {
    let peak = samples
        .iter()
        .map(|&s| s.abs())
        .fold(0.0_f32, |a, b| a.max(b)) as f64;

    if peak > 0.0 {
        20.0 * libm::log10(peak)
    } else {
        -f64::INFINITY
    }
}

/// Level meter state with peak hold
#[derive(Debug, Clone)]
pub struct LevelMeterState {
    /// Current RMS level in dB
    pub rms_db: f64,
    /// Current peak level in dB
    pub peak_db: f64,
    /// Peak hold value in dB
    pub peak_hold_db: f64,
    /// Samples since last peak hold update
    samples_since_peak: usize,
}

impl Default for LevelMeterState {
    fn default() -> Self {
        Self {
            rms_db: -f64::INFINITY,
            peak_db: -f64::INFINITY,
            peak_hold_db: -f64::INFINITY,
            samples_since_peak: 0,
        }
    }
}

impl LevelMeterState {
    /// Update the meter with new samples
    pub fn update(&mut self, samples: &[f32], peak_hold_samples: usize) {
        self.rms_db = calculate_rms_db(samples);
        self.peak_db = calculate_peak_db(samples);

        // Update peak hold
        if self.peak_db > self.peak_hold_db {
            self.peak_hold_db = self.peak_db;
            self.samples_since_peak = 0;
        } else {
            self.samples_since_peak += samples.len();
            if self.samples_since_peak > peak_hold_samples {
                // Decay peak hold
                self.peak_hold_db = self.peak_db;
            }
        }
    }

    /// Reset the meter
    pub fn reset(&mut self) {
        self.rms_db = -f64::INFINITY;
        self.peak_db = -f64::INFINITY;
        self.peak_hold_db = -f64::INFINITY;
        self.samples_since_peak = 0;
    }
}

// =============================================================================
// Gate Detector
// =============================================================================

/// Gate state detector with hysteresis
#[derive(Debug, Clone)]
pub struct GateDetector {
    /// Threshold for turning gate on
    pub threshold_on: f32,
    /// Threshold for turning gate off (hysteresis)
    pub threshold_off: f32,
    /// Current gate state
    pub active: bool,
}

impl GateDetector {
    /// Create a new gate detector with default thresholds
    pub fn new() -> Self {
        Self {
            threshold_on: 2.5,  // Standard +5V gate threshold
            threshold_off: 0.5, // Hysteresis
            active: false,
        }
    }

    /// Create with custom thresholds
    pub fn with_thresholds(threshold_on: f32, threshold_off: f32) -> Self {
        Self {
            threshold_on,
            threshold_off,
            active: false,
        }
    }

    /// Process a sample and return the gate state
    pub fn process(&mut self, sample: f32) -> bool {
        if self.active {
            if sample < self.threshold_off {
                self.active = false;
            }
        } else if sample > self.threshold_on {
            self.active = true;
        }
        self.active
    }

    /// Reset to initial state
    pub fn reset(&mut self) {
        self.active = false;
    }
}

impl Default for GateDetector {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Spectrum Analysis (Simple DFT for no_std compatibility)
// =============================================================================

/// Compute magnitude spectrum using a simple DFT
///
/// Returns N/2 magnitude bins (positive frequencies only).
/// For production use, consider using a proper FFT library.
fn compute_magnitude_spectrum(samples: &[f32]) -> Vec<f32> {
    let n = samples.len();
    if n == 0 {
        return vec![];
    }

    // Apply Hann window to reduce spectral leakage
    let windowed: Vec<f64> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let window = 0.5 * (1.0 - libm::cos(2.0 * PI * i as f64 / (n - 1) as f64));
            s as f64 * window
        })
        .collect();

    // Compute DFT for positive frequencies only (N/2 bins)
    let num_bins = n / 2;
    let mut magnitudes = Vec::with_capacity(num_bins);

    for k in 0..num_bins {
        let mut real = 0.0;
        let mut imag = 0.0;

        for (i, &sample) in windowed.iter().enumerate() {
            let angle = -2.0 * PI * k as f64 * i as f64 / n as f64;
            real += sample * libm::cos(angle);
            imag += sample * libm::sin(angle);
        }

        // Magnitude in dB (normalized)
        let magnitude = libm::sqrt(real * real + imag * imag) / n as f64;
        let magnitude_db = if magnitude > 1e-10 {
            20.0 * libm::log10(magnitude)
        } else {
            -100.0
        };

        // Clamp to reasonable range and convert to f32
        magnitudes.push(magnitude_db.clamp(-100.0, 0.0) as f32);
    }

    magnitudes
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observable_value_key() {
        let param = ObservableValue::Param {
            node_id: "vco1".into(),
            param_id: "frequency".into(),
            value: 440.0,
        };
        assert_eq!(param.key(), "param:vco1:frequency");

        let level = ObservableValue::Level {
            node_id: "output".into(),
            port_id: 0,
            rms_db: -12.0,
            peak_db: -6.0,
        };
        assert_eq!(level.key(), "level:output:0");
    }

    #[test]
    fn test_subscription_target_id() {
        let param = SubscriptionTarget::Param {
            node_id: "vco1".into(),
            param_id: "frequency".into(),
        };
        assert_eq!(param.id(), "param:vco1:frequency");

        let scope = SubscriptionTarget::Scope {
            node_id: "vco1".into(),
            port_id: 0,
            buffer_size: 512,
        };
        assert_eq!(scope.id(), "scope:vco1:0");
    }

    #[test]
    fn test_state_observer_subscriptions() {
        let mut observer = StateObserver::new();

        let target = SubscriptionTarget::Param {
            node_id: "vco1".into(),
            param_id: "frequency".into(),
        };

        observer.add_subscriptions(vec![target.clone()]);
        assert!(observer.is_subscribed(&target));
        assert_eq!(observer.subscriptions().len(), 1);

        // Adding duplicate should not increase count
        observer.add_subscriptions(vec![target.clone()]);
        assert_eq!(observer.subscriptions().len(), 1);

        observer.remove_subscriptions(&[target.id()]);
        assert!(!observer.is_subscribed(&target));
        assert_eq!(observer.subscriptions().len(), 0);
    }

    #[test]
    fn test_state_observer_push_update() {
        let mut observer = StateObserver::new();

        // Subscribe first
        observer.add_subscriptions(vec![SubscriptionTarget::Param {
            node_id: "vco1".into(),
            param_id: "frequency".into(),
        }]);

        // Push update
        observer.push_update(ObservableValue::Param {
            node_id: "vco1".into(),
            param_id: "frequency".into(),
            value: 440.0,
        });

        assert_eq!(observer.pending_count(), 1);

        // Push another update for same target - should replace
        observer.push_update(ObservableValue::Param {
            node_id: "vco1".into(),
            param_id: "frequency".into(),
            value: 880.0,
        });

        assert_eq!(observer.pending_count(), 1);

        // Drain updates
        let updates = observer.drain_updates();
        assert_eq!(updates.len(), 1);
        if let ObservableValue::Param { value, .. } = &updates[0] {
            assert_eq!(*value, 880.0);
        } else {
            panic!("Expected Param update");
        }

        assert_eq!(observer.pending_count(), 0);
    }

    #[test]
    fn test_state_observer_ignores_unsubscribed() {
        let mut observer = StateObserver::new();

        // Don't subscribe, just push
        observer.push_update(ObservableValue::Param {
            node_id: "vco1".into(),
            param_id: "frequency".into(),
            value: 440.0,
        });

        // Should be ignored
        assert_eq!(observer.pending_count(), 0);
    }

    #[test]
    fn test_state_observer_creates_port_buffers() {
        let mut observer = StateObserver::new();

        // Level subscription should create a port buffer
        observer.add_subscriptions(vec![SubscriptionTarget::Level {
            node_id: "vco1".into(),
            port_id: 0,
        }]);

        assert_eq!(observer.port_buffers.len(), 1);

        // Param subscription should NOT create a port buffer
        observer.add_subscriptions(vec![SubscriptionTarget::Param {
            node_id: "vco1".into(),
            param_id: "freq".into(),
        }]);

        assert_eq!(observer.port_buffers.len(), 1);
    }

    #[test]
    fn test_state_observer_cleans_up_buffers() {
        let mut observer = StateObserver::new();

        observer.add_subscriptions(vec![SubscriptionTarget::Level {
            node_id: "vco1".into(),
            port_id: 0,
        }]);

        assert_eq!(observer.port_buffers.len(), 1);

        observer.remove_subscriptions(&["level:vco1:0".into()]);

        assert_eq!(observer.port_buffers.len(), 0);
    }

    #[test]
    fn test_calculate_rms_db() {
        // Silence
        assert!(calculate_rms_db(&[]).is_infinite());
        assert!(calculate_rms_db(&[0.0, 0.0, 0.0]).is_infinite());

        // Unity sine wave peak -> RMS = 1/sqrt(2) ≈ 0.707 -> -3 dB
        let rms_unity = calculate_rms_db(&[1.0, -1.0]);
        assert!((rms_unity - 0.0).abs() < 0.1); // ~0 dB for unity peak

        // Half amplitude
        let rms_half = calculate_rms_db(&[0.5, -0.5]);
        assert!((rms_half - (-6.0)).abs() < 0.1); // ~-6 dB
    }

    #[test]
    fn test_calculate_peak_db() {
        assert!(calculate_peak_db(&[]).is_infinite());
        assert!(calculate_peak_db(&[0.0, 0.0]).is_infinite());

        let peak_unity = calculate_peak_db(&[1.0, -0.5]);
        assert!((peak_unity - 0.0).abs() < 0.01); // 0 dB

        let peak_half = calculate_peak_db(&[0.5, -0.25]);
        assert!((peak_half - (-6.02)).abs() < 0.1); // ~-6 dB
    }

    #[test]
    fn test_level_meter_state() {
        let mut meter = LevelMeterState::default();

        // Update with samples
        meter.update(&[0.5, -0.5, 0.5, -0.5], 44100); // ~1 second hold at 44.1kHz

        assert!(!meter.rms_db.is_infinite());
        assert!(!meter.peak_db.is_infinite());
        assert_eq!(meter.peak_hold_db, meter.peak_db);

        // Update with lower level - peak hold should remain
        let prev_peak_hold = meter.peak_hold_db;
        meter.update(&[0.1, -0.1], 44100);
        assert_eq!(meter.peak_hold_db, prev_peak_hold);
    }

    #[test]
    fn test_gate_detector() {
        let mut gate = GateDetector::new();

        assert!(!gate.active);

        // Below threshold
        assert!(!gate.process(1.0));

        // Cross threshold
        assert!(gate.process(3.0));
        assert!(gate.active);

        // Still above off threshold (hysteresis)
        assert!(gate.process(1.0));

        // Below off threshold
        assert!(!gate.process(0.1));
        assert!(!gate.active);
    }

    #[test]
    fn test_compute_magnitude_spectrum() {
        // Empty input
        assert!(compute_magnitude_spectrum(&[]).is_empty());

        // Simple test - DC signal should have energy at bin 0
        let dc_signal: Vec<f32> = vec![1.0; 64];
        let spectrum = compute_magnitude_spectrum(&dc_signal);
        assert_eq!(spectrum.len(), 32); // N/2 bins

        // First bin (DC) should have the most energy
        assert!(spectrum[0] > spectrum[1]);
    }

    #[test]
    fn test_observable_value_serialization() {
        let value = ObservableValue::Param {
            node_id: "vco1".into(),
            param_id: "freq".into(),
            value: 440.0,
        };

        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("\"type\":\"param\""));
        assert!(json.contains("\"node_id\":\"vco1\""));

        let deserialized: ObservableValue = serde_json::from_str(&json).unwrap();
        assert_eq!(value.key(), deserialized.key());
    }

    #[test]
    fn test_subscription_target_serialization() {
        let target = SubscriptionTarget::Scope {
            node_id: "vco1".into(),
            port_id: 0,
            buffer_size: 512,
        };

        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains("\"type\":\"scope\""));
        assert!(json.contains("\"buffer_size\":512"));

        let deserialized: SubscriptionTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(target.id(), deserialized.id());
    }

    #[test]
    fn test_level_observable() {
        let level = ObservableValue::Level {
            node_id: "output".into(),
            port_id: 0,
            rms_db: -12.5,
            peak_db: -3.2,
        };

        let json = serde_json::to_string(&level).unwrap();
        assert!(json.contains("\"type\":\"level\""));
        assert!(json.contains("\"rms_db\":-12.5"));
    }

    #[test]
    fn test_gate_observable() {
        let gate = ObservableValue::Gate {
            node_id: "lfo".into(),
            port_id: 1,
            active: true,
        };

        let json = serde_json::to_string(&gate).unwrap();
        assert!(json.contains("\"type\":\"gate\""));
        assert!(json.contains("\"active\":true"));
    }

    #[test]
    fn test_scope_observable() {
        let scope = ObservableValue::Scope {
            node_id: "osc".into(),
            port_id: 0,
            samples: vec![0.0, 0.5, 1.0, 0.5, 0.0, -0.5, -1.0, -0.5],
        };

        let json = serde_json::to_string(&scope).unwrap();
        assert!(json.contains("\"type\":\"scope\""));
        assert!(json.contains("\"samples\""));
    }

    #[test]
    fn test_spectrum_observable() {
        let spectrum = ObservableValue::Spectrum {
            node_id: "analyzer".into(),
            port_id: 0,
            bins: vec![-20.0, -30.0, -40.0, -50.0],
            freq_range: (0.0, 22050.0),
        };

        let json = serde_json::to_string(&spectrum).unwrap();
        assert!(json.contains("\"type\":\"spectrum\""));
        assert!(json.contains("\"freq_range\""));
    }
}
