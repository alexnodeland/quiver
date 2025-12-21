//! Real-Time State Bridge (Phase 4: GUI Framework)
//!
//! This module provides types and infrastructure for streaming live values
//! from the audio processing to the UI, supporting both WASM polling and
//! HTTP WebSocket push architectures.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

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
    /// Default FFT size for spectrum analysis (default: 1024)
    pub default_fft_size: usize,
}

impl Default for ObserverConfig {
    fn default() -> Self {
        Self {
            max_update_rate: 60,
            max_pending_updates: 1000,
            default_scope_buffer_size: 512,
            default_fft_size: 1024,
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
        }
    }

    /// Add subscriptions
    pub fn add_subscriptions(&mut self, targets: Vec<SubscriptionTarget>) {
        for target in targets {
            if !self.subscriptions.iter().any(|s| s.id() == target.id()) {
                self.subscriptions.push(target);
            }
        }
    }

    /// Remove subscriptions by ID
    pub fn remove_subscriptions(&mut self, ids: &[String]) {
        self.subscriptions.retain(|s| !ids.contains(&s.id()));
    }

    /// Clear all subscriptions
    pub fn clear_subscriptions(&mut self) {
        self.subscriptions.clear();
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
    /// to update subscribed values. It uses the graph module's param
    /// values to populate parameter subscriptions.
    pub fn collect_from_patch(&mut self, patch: &crate::graph::Patch) {
        for target in &self.subscriptions.clone() {
            match target {
                SubscriptionTarget::Param { node_id, param_id } => {
                    // Find the node and get its parameter value
                    if let Some(nid) = patch.get_node_id_by_name(node_id) {
                        // Try to get param value by parsing param_id as index
                        if let Ok(idx) = param_id.parse::<u32>() {
                            if let Some(value) = patch.get_param(nid, idx) {
                                self.push_update(ObservableValue::Param {
                                    node_id: node_id.clone(),
                                    param_id: param_id.clone(),
                                    value,
                                });
                            }
                        }
                    }
                }
                SubscriptionTarget::Level {
                    node_id, port_id, ..
                } => {
                    // Level metering would require access to output buffers
                    // For now, we'll skip this - full implementation requires
                    // access to the internal buffer values from Patch
                    let _ = (node_id, port_id);
                }
                SubscriptionTarget::Gate {
                    node_id, port_id, ..
                } => {
                    // Gate detection would require access to output buffers
                    let _ = (node_id, port_id);
                }
                SubscriptionTarget::Scope { .. } | SubscriptionTarget::Spectrum { .. } => {
                    // Scope and spectrum require buffer access - skip for now
                }
            }
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
}
