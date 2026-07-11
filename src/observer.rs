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

use alloc::collections::VecDeque;
use alloc::string::String;
#[cfg(feature = "wasm")]
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::f64::consts::PI;
use serde::{Deserialize, Serialize};

use crate::graph::NodeId;

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

/// Discriminates how a port buffer is consumed when its window fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BufferKind {
    /// Level metering: RMS/peak over the accumulated window.
    Level,
    /// Gate state with hysteresis (no accumulation).
    Gate,
    /// Oscilloscope waveform capture.
    Scope,
    /// Spectrum analysis via FFT.
    Spectrum,
    /// Parameter subscription (no per-sample capture).
    Param,
}

/// A result produced when a buffer's window fills, awaiting formatting into an
/// [`ObservableValue`] on the consumer/poll side.
///
/// `Level`/`Gate` results carry the (allocation-free) derived scalars computed
/// on the capture path. `ScopeFull`/`SpectrumFull` keep their raw samples in the
/// owning [`PortBuffer`] so the expensive cloning / FFT / dB conversion happens
/// off the audio thread (see [`StateObserver::flush_ready`]).
#[derive(Debug, Clone)]
enum ReadyResult {
    Level { rms_db: f64, peak_db: f64 },
    Gate { active: bool },
    ScopeFull,
    SpectrumFull,
}

/// Buffer for accumulating per-sample data from a single subscribed port.
///
/// The sample `Vec` is preallocated to `target_size` and reused; the capture
/// path ([`StateObserver::collect_sample`]) never grows it, so it performs no
/// heap allocation.
#[derive(Debug)]
struct PortBuffer {
    /// Accumulated samples (capacity == `target_size`, never reallocated on the
    /// capture path).
    samples: Vec<f32>,
    /// Target buffer size (window length).
    target_size: usize,
    /// Current gate state (for Gate subscriptions).
    gate_active: bool,
    /// How this buffer is consumed.
    kind: BufferKind,
    /// Resolved node id, cached lazily to avoid a per-sample name lookup (and
    /// the per-sample `String` allocation that a keyed map would require).
    node_id: Option<NodeId>,
    /// Deferred result awaiting formatting on the poll side.
    ready: Option<ReadyResult>,
}

impl PortBuffer {
    fn new(kind: BufferKind, size: usize) -> Self {
        Self {
            samples: Vec::with_capacity(size),
            target_size: size,
            gate_active: false,
            kind,
            node_id: None,
            ready: None,
        }
    }

    #[inline]
    fn push(&mut self, sample: f32) {
        if self.samples.len() < self.target_size {
            self.samples.push(sample);
        }
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.samples.len() >= self.target_size
    }

    #[inline]
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
    /// Per-subscription capture buffers, index-parallel to `subscriptions`.
    buffers: Vec<PortBuffer>,
    /// Pending updates to send to the UI (bounded ring buffer).
    pending_updates: VecDeque<ObservableValue>,
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
            buffers: Vec::new(),
            pending_updates: VecDeque::new(),
            config,
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
                // Each subscription owns one capture buffer, kept index-parallel.
                let buffer = Self::make_buffer(&target, &self.config);
                self.subscriptions.push(target);
                self.buffers.push(buffer);
            }
        }
    }

    /// Build the capture buffer for a subscription target.
    fn make_buffer(target: &SubscriptionTarget, config: &ObserverConfig) -> PortBuffer {
        match target {
            SubscriptionTarget::Level { .. } => {
                PortBuffer::new(BufferKind::Level, config.level_buffer_size)
            }
            // Gate does not accumulate, but tracks state with a 1-sample window.
            SubscriptionTarget::Gate { .. } => PortBuffer::new(BufferKind::Gate, 1),
            SubscriptionTarget::Scope { buffer_size, .. } => {
                PortBuffer::new(BufferKind::Scope, *buffer_size)
            }
            SubscriptionTarget::Spectrum { fft_size, .. } => {
                PortBuffer::new(BufferKind::Spectrum, *fft_size)
            }
            // Params don't capture per-sample; the buffer is inert.
            SubscriptionTarget::Param { .. } => PortBuffer::new(BufferKind::Param, 0),
        }
    }

    /// Remove subscriptions by ID
    pub fn remove_subscriptions(&mut self, ids: &[String]) {
        // Remove in lockstep so `subscriptions` and `buffers` stay index-parallel.
        let mut i = 0;
        while i < self.subscriptions.len() {
            if ids.contains(&self.subscriptions[i].id()) {
                self.subscriptions.remove(i);
                self.buffers.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Clear all subscriptions
    pub fn clear_subscriptions(&mut self) {
        self.subscriptions.clear();
        self.buffers.clear();
    }

    /// Get all active subscriptions
    pub fn subscriptions(&self) -> &[SubscriptionTarget] {
        &self.subscriptions
    }

    /// Check if a target is subscribed
    pub fn is_subscribed(&self, target: &SubscriptionTarget) -> bool {
        self.subscriptions.iter().any(|s| s.id() == target.id())
    }

    /// Push an update directly into the pending queue (if subscribed).
    ///
    /// Retained for compatibility with callers that format their own updates;
    /// the per-sample capture path uses [`Self::collect_sample`] instead.
    pub fn push_update(&mut self, value: ObservableValue) {
        if self.subscriptions.iter().any(|s| s.id() == value.key()) {
            self.enqueue(value);
        }
    }

    /// Enqueue a formatted update, deduplicating by key and enforcing the bound.
    ///
    /// Runs on the consumer/poll side, never on the per-sample capture path.
    /// Uses `VecDeque::pop_front` (O(1)) to drop the oldest when over the limit,
    /// replacing the old `Vec::remove(0)` O(n) shift.
    fn enqueue(&mut self, value: ObservableValue) {
        if let Some(pos) = self
            .pending_updates
            .iter()
            .position(|v| v.key() == value.key())
        {
            self.pending_updates.remove(pos);
        }
        self.pending_updates.push_back(value);
        while self.pending_updates.len() > self.config.max_pending_updates {
            self.pending_updates.pop_front();
        }
    }

    /// Drain all pending updates (for WASM polling).
    ///
    /// Flushes any capture buffers that have filled (formatting them off the
    /// audio thread) before returning the queued updates.
    pub fn drain_updates(&mut self) -> Vec<ObservableValue> {
        self.flush_ready();
        self.pending_updates.drain(..).collect()
    }

    /// Peek at pending updates without draining.
    pub fn pending_updates(&self) -> impl Iterator<Item = &ObservableValue> {
        self.pending_updates.iter()
    }

    /// Get number of pending updates
    pub fn pending_count(&self) -> usize {
        self.pending_updates.len()
    }

    /// Get the configuration
    pub fn config(&self) -> &ObserverConfig {
        &self.config
    }

    /// Capture one sample per subscribed port from the patch.
    ///
    /// This is the real-time capture entry point: call it **once per audio
    /// sample** inside the engine's tick loop so that Scope/Spectrum/Level see
    /// every sample rather than one per block (which aliases everything above
    /// `sample_rate / (2 * block_size)`). It is allocation-free: subscriptions
    /// are iterated by index (no clone), node ids are resolved once and cached,
    /// samples land in preallocated buffers, and formatting/serialization of any
    /// filled buffer is deferred to [`Self::flush_ready`] / [`Self::drain_updates`]
    /// on the consumer side.
    pub fn collect_sample(&mut self, patch: &crate::graph::Patch) {
        const THRESHOLD_ON: f32 = 2.5;
        const THRESHOLD_OFF: f32 = 0.5;

        for i in 0..self.subscriptions.len() {
            // Disjoint field borrows: read the subscription, write its buffer.
            let (node_name, port_id) = match &self.subscriptions[i] {
                SubscriptionTarget::Level { node_id, port_id }
                | SubscriptionTarget::Gate { node_id, port_id }
                | SubscriptionTarget::Scope {
                    node_id, port_id, ..
                }
                | SubscriptionTarget::Spectrum {
                    node_id, port_id, ..
                } => (node_id.as_str(), *port_id),
                SubscriptionTarget::Param { .. } => continue,
            };

            let buffer = &mut self.buffers[i];

            // Resolve the node id once (avoids a per-sample name scan/allocation).
            if buffer.node_id.is_none() {
                buffer.node_id = patch.get_node_id_by_name(node_name);
            }
            let Some(nid) = buffer.node_id else { continue };
            let Some(value) = patch.get_output_value(nid, port_id) else {
                continue;
            };
            let sample = value as f32;

            match buffer.kind {
                BufferKind::Level => {
                    buffer.push(sample);
                    if buffer.is_full() {
                        let rms_db = calculate_rms_db(&buffer.samples);
                        let peak_db = calculate_peak_db(&buffer.samples);
                        buffer.clear();
                        buffer.ready = Some(ReadyResult::Level { rms_db, peak_db });
                    }
                }
                BufferKind::Gate => {
                    let was_active = buffer.gate_active;
                    if buffer.gate_active {
                        if sample < THRESHOLD_OFF {
                            buffer.gate_active = false;
                        }
                    } else if sample > THRESHOLD_ON {
                        buffer.gate_active = true;
                    }
                    if buffer.gate_active != was_active {
                        buffer.ready = Some(ReadyResult::Gate {
                            active: buffer.gate_active,
                        });
                    }
                }
                BufferKind::Scope => {
                    buffer.push(sample);
                    if buffer.is_full() {
                        // Keep samples; clone/format happens off the audio thread.
                        buffer.ready = Some(ReadyResult::ScopeFull);
                    }
                }
                BufferKind::Spectrum => {
                    buffer.push(sample);
                    if buffer.is_full() {
                        // Keep samples; FFT/dB conversion happens off the audio thread.
                        buffer.ready = Some(ReadyResult::SpectrumFull);
                    }
                }
                BufferKind::Param => {}
            }
        }
    }

    /// Format any filled capture buffers into pending updates.
    ///
    /// Runs on the consumer/poll side: this is where String node ids are
    /// allocated, scope samples are cloned, and the spectrum FFT + dB conversion
    /// run — all kept off the per-sample audio path.
    fn flush_ready(&mut self) {
        for i in 0..self.buffers.len() {
            let Some(ready) = self.buffers[i].ready.take() else {
                continue;
            };

            let (node_id, port_id) = match &self.subscriptions[i] {
                SubscriptionTarget::Level { node_id, port_id }
                | SubscriptionTarget::Gate { node_id, port_id }
                | SubscriptionTarget::Scope {
                    node_id, port_id, ..
                }
                | SubscriptionTarget::Spectrum {
                    node_id, port_id, ..
                } => (node_id.clone(), *port_id),
                SubscriptionTarget::Param { .. } => continue,
            };

            let value = match ready {
                ReadyResult::Level { rms_db, peak_db } => ObservableValue::Level {
                    node_id,
                    port_id,
                    rms_db,
                    peak_db,
                },
                ReadyResult::Gate { active } => ObservableValue::Gate {
                    node_id,
                    port_id,
                    active,
                },
                ReadyResult::ScopeFull => {
                    let samples = self.buffers[i].samples.clone();
                    self.buffers[i].clear();
                    ObservableValue::Scope {
                        node_id,
                        port_id,
                        samples,
                    }
                }
                ReadyResult::SpectrumFull => {
                    let bins = compute_magnitude_spectrum(&self.buffers[i].samples);
                    // Bins were captured one-per-sample, so the true capture rate
                    // is the full sample rate and Nyquist is sample_rate / 2.
                    let freq_range = (0.0, self.config.sample_rate as f32 / 2.0);
                    self.buffers[i].clear();
                    ObservableValue::Spectrum {
                        node_id,
                        port_id,
                        bins,
                        freq_range,
                    }
                }
            };

            self.enqueue(value);
        }
    }

    /// Collect observable values from the patch after processing.
    ///
    /// Legacy per-block entry point kept so existing callers (the WASM engine)
    /// compile unchanged. It captures one sample per call via [`Self::collect_sample`]
    /// and formats immediately. **New code should call [`Self::collect_sample`]
    /// once per audio sample** for correct (non-aliased) Scope/Spectrum capture,
    /// then [`Self::drain_updates`] on the UI side.
    pub fn collect_from_patch(&mut self, patch: &crate::graph::Patch) {
        self.collect_params(patch);
        self.collect_sample(patch);
        self.flush_ready();
    }

    /// Collect parameter values (control-rate; off the audio path).
    fn collect_params(&mut self, patch: &crate::graph::Patch) {
        // Iterate immutably, then enqueue, to avoid borrowing conflicts.
        let mut updates: Vec<ObservableValue> = Vec::new();
        for sub in &self.subscriptions {
            if let SubscriptionTarget::Param { node_id, param_id } = sub {
                if let Some(nid) = patch.get_node_id_by_name(node_id) {
                    if let Ok(idx) = param_id.parse::<u32>() {
                        if let Some(value) = patch.get_param(nid, idx) {
                            updates.push(ObservableValue::Param {
                                node_id: node_id.clone(),
                                param_id: param_id.clone(),
                                value,
                            });
                        }
                    }
                }
            }
        }
        for update in updates {
            self.enqueue(update);
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
    let rms = libm::Libm::<f64>::sqrt(sum_sq / samples.len() as f64);

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

/// Peak-hold decay rate once the hold window has elapsed, in dB per sample.
///
/// ≈20 dB/s at a 44.1 kHz sample rate — a typical VU/PPM meter fall-back rate.
const PEAK_HOLD_DECAY_DB_PER_SAMPLE: f64 = 20.0 / 44_100.0;

impl LevelMeterState {
    /// Update the meter with new samples.
    ///
    /// The held peak latches on a new maximum, is held for `peak_hold_samples`,
    /// and then **decays gradually** toward the current peak at
    /// [`PEAK_HOLD_DECAY_DB_PER_SAMPLE`] rather than snapping to it. The hold
    /// window re-arms only when a new higher peak arrives; the counter is bounded
    /// so decay continues every update after the window (it never collapses into
    /// a plain follower).
    pub fn update(&mut self, samples: &[f32], peak_hold_samples: usize) {
        self.rms_db = calculate_rms_db(samples);
        self.peak_db = calculate_peak_db(samples);

        if self.peak_db >= self.peak_hold_db {
            // New (or equal) peak: latch and re-arm the hold window.
            self.peak_hold_db = self.peak_db;
            self.samples_since_peak = 0;
        } else {
            self.samples_since_peak = self.samples_since_peak.saturating_add(samples.len());
            if self.samples_since_peak > peak_hold_samples {
                // Hold elapsed: decay smoothly toward the current peak. Saturate
                // the counter (rather than resetting to zero, which would re-hold
                // and stair-step) so decay keeps progressing each update.
                let decay = PEAK_HOLD_DECAY_DB_PER_SAMPLE * samples.len() as f64;
                self.peak_hold_db = (self.peak_hold_db - decay).max(self.peak_db);
                self.samples_since_peak = peak_hold_samples + 1;
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
// Spectrum Analysis (radix-2 FFT, no_std-safe via libm)
// =============================================================================

/// In-place iterative radix-2 Cooley–Tukey FFT (forward transform).
///
/// `re` and `im` must have equal length that is a power of two. Runs in
/// O(n log n) with only O(log n) transcendental (`libm::cos`/`sin`) calls — the
/// per-butterfly twiddle is advanced by complex recurrence — making it suitable
/// for the real-time path where the old O(n²) DFT called sin/cos O(n²) times.
///
/// Shared by [`compute_magnitude_spectrum`] here and by
/// `visual::SpectrumAnalyzer` (which is `std`-only but reuses this alloc-tier
/// routine).
pub(crate) fn fft_radix2(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    debug_assert_eq!(n, im.len());
    debug_assert!(n == 0 || n.is_power_of_two());
    if n <= 1 {
        return;
    }

    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Danielson–Lanczos stages.
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f64; // negative sign => forward transform
        let (wlen_re, wlen_im) = (libm::cos(ang), libm::sin(ang));
        let half = len / 2;
        let mut base = 0;
        while base < n {
            let (mut w_re, mut w_im) = (1.0_f64, 0.0_f64);
            for k in 0..half {
                let a = base + k;
                let b = base + k + half;
                let t_re = re[b] * w_re - im[b] * w_im;
                let t_im = re[b] * w_im + im[b] * w_re;
                re[b] = re[a] - t_re;
                im[b] = im[a] - t_im;
                re[a] += t_re;
                im[a] += t_im;
                // Advance twiddle: w *= wlen.
                let nw_re = w_re * wlen_re - w_im * wlen_im;
                let nw_im = w_re * wlen_im + w_im * wlen_re;
                w_re = nw_re;
                w_im = nw_im;
            }
            base += len;
        }
        len <<= 1;
    }
}

/// Compute a Hann-windowed magnitude spectrum (dB), N/2 positive-frequency bins.
///
/// Uses the O(n log n) [`fft_radix2`] for power-of-two lengths and falls back to
/// a direct DFT only for the rare non-power-of-two window.
fn compute_magnitude_spectrum(samples: &[f32]) -> Vec<f32> {
    let n = samples.len();
    if n < 2 {
        return vec![];
    }

    if !n.is_power_of_two() {
        return compute_magnitude_spectrum_dft(samples);
    }

    // Hann-window into the real part; imaginary part starts at zero.
    let mut re: Vec<f64> = Vec::with_capacity(n);
    let mut im: Vec<f64> = vec![0.0; n];
    for (i, &s) in samples.iter().enumerate() {
        let window = 0.5 * (1.0 - libm::cos(2.0 * PI * i as f64 / (n - 1) as f64));
        re.push(s as f64 * window);
    }

    fft_radix2(&mut re, &mut im);

    let num_bins = n / 2;
    let mut magnitudes = Vec::with_capacity(num_bins);
    for k in 0..num_bins {
        let magnitude = libm::sqrt(re[k] * re[k] + im[k] * im[k]) / n as f64;
        let magnitude_db = if magnitude > 1e-10 {
            20.0 * libm::log10(magnitude)
        } else {
            -100.0
        };
        magnitudes.push(magnitude_db.clamp(-100.0, 0.0) as f32);
    }

    magnitudes
}

/// Direct O(n²) DFT magnitude spectrum. Used only as a fallback for
/// non-power-of-two windows and as a numeric reference in tests.
fn compute_magnitude_spectrum_dft(samples: &[f32]) -> Vec<f32> {
    let n = samples.len();
    if n < 2 {
        return vec![];
    }

    let windowed: Vec<f64> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let window = 0.5 * (1.0 - libm::cos(2.0 * PI * i as f64 / (n - 1) as f64));
            s as f64 * window
        })
        .collect();

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
        let magnitude = libm::sqrt(real * real + imag * imag) / n as f64;
        let magnitude_db = if magnitude > 1e-10 {
            20.0 * libm::log10(magnitude)
        } else {
            -100.0
        };
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
    fn test_state_observer_creates_buffers() {
        let mut observer = StateObserver::new();

        // Level subscription should create a Level capture buffer.
        observer.add_subscriptions(vec![SubscriptionTarget::Level {
            node_id: "vco1".into(),
            port_id: 0,
        }]);

        assert_eq!(observer.buffers.len(), 1);
        assert_eq!(observer.buffers[0].kind, BufferKind::Level);

        // Buffers stay index-parallel to subscriptions; a Param subscription
        // gets an inert buffer.
        observer.add_subscriptions(vec![SubscriptionTarget::Param {
            node_id: "vco1".into(),
            param_id: "freq".into(),
        }]);

        assert_eq!(observer.buffers.len(), 2);
        assert_eq!(observer.buffers[1].kind, BufferKind::Param);
    }

    #[test]
    fn test_state_observer_cleans_up_buffers() {
        let mut observer = StateObserver::new();

        observer.add_subscriptions(vec![SubscriptionTarget::Level {
            node_id: "vco1".into(),
            port_id: 0,
        }]);

        assert_eq!(observer.buffers.len(), 1);

        observer.remove_subscriptions(&["level:vco1:0".into()]);

        assert_eq!(observer.buffers.len(), 0);
        assert_eq!(observer.subscriptions().len(), 0);
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

    // ---- Q104: peak-hold holds, then decays gradually (no snap / no follower) ----
    #[test]
    fn test_level_meter_peak_hold_and_decay() {
        let mut meter = LevelMeterState::default();
        let hold = 100usize;

        // Establish a peak at ~0 dB (amplitude 1.0).
        meter.update(&[1.0, -1.0], hold);
        let held = meter.peak_hold_db;
        assert!((held - 0.0).abs() < 0.1);

        // Within the hold window: quieter signal, peak hold must NOT drop.
        meter.update(&[0.1; 50], hold); // 50 < 100 samples
        assert_eq!(
            meter.peak_hold_db, held,
            "peak hold must hold within the window"
        );

        // Past the hold window: it must begin decaying.
        meter.update(&[0.1; 60], hold); // total 110 > 100
        assert!(
            meter.peak_hold_db < held,
            "peak hold must decay after the window"
        );
        // ...but must NOT snap straight to the current (much lower) peak.
        let current_peak = meter.peak_db;
        assert!(
            meter.peak_hold_db > current_peak + 1.0,
            "decay must be gradual, not an instant snap"
        );

        // Decay continues each subsequent update (does not collapse to a follower).
        let after_first = meter.peak_hold_db;
        meter.update(&[0.1; 60], hold);
        assert!(
            meter.peak_hold_db < after_first,
            "decay must continue every update after the hold window"
        );
        assert!(
            meter.peak_hold_db >= current_peak,
            "decay never overshoots the current peak"
        );
    }

    // ---- Q101: radix-2 FFT agrees with the direct DFT on a known sine ----
    #[test]
    fn test_fft_matches_dft_on_sine() {
        let n = 64usize;
        let bin = 5usize;

        // Real cosine at exactly `bin` cycles across the window.
        let input: Vec<f64> = (0..n)
            .map(|i| libm::cos(2.0 * PI * bin as f64 * i as f64 / n as f64))
            .collect();

        // FFT magnitudes.
        let mut re = input.clone();
        let mut im = vec![0.0f64; n];
        fft_radix2(&mut re, &mut im);

        // Reference direct DFT magnitude at bin k.
        let dft_mag = |k: usize| -> f64 {
            let mut r = 0.0;
            let mut i = 0.0;
            for (t, &x) in input.iter().enumerate() {
                let ang = -2.0 * PI * k as f64 * t as f64 / n as f64;
                r += x * libm::cos(ang);
                i += x * libm::sin(ang);
            }
            libm::sqrt(r * r + i * i)
        };

        let mut peak_bin = 0usize;
        let mut peak_mag = -1.0;
        for k in 0..n / 2 {
            let fft_mag = libm::sqrt(re[k] * re[k] + im[k] * im[k]);
            assert!(
                (fft_mag - dft_mag(k)).abs() < 1e-9,
                "bin {k}: fft {fft_mag} vs dft {}",
                dft_mag(k)
            );
            if fft_mag > peak_mag {
                peak_mag = fft_mag;
                peak_bin = k;
            }
        }
        assert_eq!(peak_bin, bin, "FFT peak bin must match the input frequency");
    }

    // ---- Q099: collect_sample captures one sample per call (per-sample rate) ----
    fn build_constant_patch(value: f64) -> crate::graph::Patch {
        let mut patch = crate::graph::Patch::new(44_100.0);
        let level = alloc::sync::Arc::new(crate::io::AtomicF64::new(value));
        let src = patch.add("src", crate::io::ExternalInput::audio(level));
        let out = patch.add("out", crate::modules::StereoOutput::new());
        patch.connect(src.out("out"), out.in_("left")).unwrap();
        patch.set_output(out.id());
        patch.compile().unwrap();
        patch
    }

    #[test]
    fn test_collect_sample_per_sample_capture() {
        let mut patch = build_constant_patch(0.5);
        let mut obs = StateObserver::new();
        obs.add_subscriptions(vec![SubscriptionTarget::Scope {
            node_id: "src".into(),
            port_id: 0,
            buffer_size: 8,
        }]);

        // Eight per-sample captures fill an 8-sample scope buffer exactly once
        // (the old path needed eight *blocks*).
        for _ in 0..8 {
            patch.tick();
            obs.collect_sample(&patch);
        }

        let updates = obs.drain_updates();
        let samples = updates
            .iter()
            .find_map(|u| match u {
                ObservableValue::Scope { samples, .. } => Some(samples.clone()),
                _ => None,
            })
            .expect("expected a scope update after 8 per-sample captures");
        assert_eq!(samples.len(), 8);
        for s in &samples {
            assert!((s - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn test_spectrum_freq_range_uses_true_capture_rate() {
        let mut patch = build_constant_patch(0.25);
        let mut obs = StateObserver::new();
        let fft_size = 16usize;
        obs.add_subscriptions(vec![SubscriptionTarget::Spectrum {
            node_id: "src".into(),
            port_id: 0,
            fft_size,
        }]);

        for _ in 0..fft_size {
            patch.tick();
            obs.collect_sample(&patch);
        }

        let updates = obs.drain_updates();
        let (bins, freq_range) = updates
            .iter()
            .find_map(|u| match u {
                ObservableValue::Spectrum {
                    bins, freq_range, ..
                } => Some((bins.clone(), *freq_range)),
                _ => None,
            })
            .expect("expected a spectrum update");
        // Per-sample capture => true rate is the full sample rate; Nyquist = sr/2.
        assert_eq!(freq_range, (0.0, 22_050.0));
        assert_eq!(bins.len(), fft_size / 2);
    }

    // ---- Q100: the per-sample capture path performs no heap allocation ----
    #[test]
    fn test_collect_sample_is_allocation_free() {
        let mut patch = build_constant_patch(0.5);
        let mut obs = StateObserver::new();
        obs.add_subscriptions(vec![
            SubscriptionTarget::Level {
                node_id: "src".into(),
                port_id: 0,
            },
            SubscriptionTarget::Scope {
                node_id: "src".into(),
                port_id: 0,
                buffer_size: 64,
            },
            SubscriptionTarget::Spectrum {
                node_id: "src".into(),
                port_id: 0,
                fft_size: 64,
            },
            SubscriptionTarget::Gate {
                node_id: "src".into(),
                port_id: 0,
            },
        ]);

        // Populate output buffers once (patch.tick itself allocates elsewhere, so
        // it is deliberately outside the measured region), and warm up the
        // node-id cache and the thread-local allocation counter.
        patch.tick();
        obs.collect_sample(&patch);
        let _ = alloc_guard::count_allocations(|| {});

        // Many per-sample captures must not allocate.
        let allocs = alloc_guard::count_allocations(|| {
            for _ in 0..2048 {
                obs.collect_sample(&patch);
            }
        });
        assert_eq!(
            allocs, 0,
            "collect_sample must be allocation-free on the audio path"
        );
    }
}

/// Thread-scoped allocation counter used to prove the per-sample capture path
/// (`StateObserver::collect_sample`) does not allocate. Thread-local so it is
/// robust under the parallel test harness.
#[cfg(all(test, feature = "std"))]
mod alloc_guard {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static COUNTING: Cell<bool> = const { Cell::new(false) };
        static LOCAL_COUNT: Cell<usize> = const { Cell::new(0) };
    }

    struct CountingAllocator;

    #[inline]
    fn note_alloc() {
        // Only count on threads that armed the guard; `try_with` avoids panics
        // (and re-entrancy) during TLS init/teardown. Const-initialized `Cell`s
        // do not themselves allocate on access.
        let _ = COUNTING.try_with(|c| {
            if c.get() {
                let _ = LOCAL_COUNT.try_with(|n| n.set(n.get() + 1));
            }
        });
    }

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            note_alloc();
            System.alloc(layout)
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout);
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            note_alloc();
            System.realloc(ptr, layout, new_size)
        }
    }

    #[global_allocator]
    static GLOBAL: CountingAllocator = CountingAllocator;

    /// Run `f` with allocation counting armed on the current thread and return
    /// the number of allocations observed.
    pub(super) fn count_allocations<F: FnOnce()>(f: F) -> usize {
        LOCAL_COUNT.with(|n| n.set(0));
        COUNTING.with(|c| c.set(true));
        f();
        COUNTING.with(|c| c.set(false));
        LOCAL_COUNT.with(|n| n.get())
    }
}
