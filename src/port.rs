//! Layer 2: Signal Conventions and Port System
//!
//! This module defines the signal types, port definitions, and type-erased interfaces
//! that bridge the typed combinator layer with the graph-based patching system.

use crate::StdMap;
use alloc::string::String;
#[cfg(feature = "wasm")]
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use libm::Libm;
use serde::{Deserialize, Serialize};

/// Unique identifier for a port within a module
pub type PortId = u32;

/// Unique identifier for a parameter within a module
pub type ParamId = u32;

/// Semantic signal classification following hardware modular conventions
///
/// Serialized in `snake_case` (e.g. `"cv_bipolar"`, `"volt_per_octave"`) to match
/// the JSON schema (`schemas/patch.schema.json`) and all TypeScript consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    /// Audio signal, AC-coupled, typically ±5V peak
    Audio,

    /// Bipolar control voltage, ±5V (LFO, pitch bend, modulation)
    CvBipolar,

    /// Unipolar control voltage, 0–10V (envelope, velocity, expression)
    CvUnipolar,

    /// Pitch CV following 1V/octave standard
    /// Reference: 0V = C4 (middle C, 261.63 Hz)
    VoltPerOctave,

    /// Gate signal, binary state: 0V (low) or +5V (high)
    /// Remains high while note/event is active
    Gate,

    /// Trigger signal, short pulse (~1–10ms) at +5V
    /// Used for instantaneous events
    Trigger,

    /// Clock signal, regular trigger pulses at tempo
    Clock,
}

impl SignalKind {
    /// Returns the typical voltage range (min, max) for this signal type
    pub fn voltage_range(&self) -> (f64, f64) {
        match self {
            SignalKind::Audio => (-5.0, 5.0),
            SignalKind::CvBipolar => (-5.0, 5.0),
            SignalKind::CvUnipolar => (0.0, 10.0),
            SignalKind::VoltPerOctave => (-5.0, 5.0), // ~C-1 to C9
            SignalKind::Gate => (0.0, 5.0),
            SignalKind::Trigger => (0.0, 5.0),
            SignalKind::Clock => (0.0, 5.0),
        }
    }

    /// Whether multiple signals of this kind should be summed when connected
    pub fn is_summable(&self) -> bool {
        matches!(
            self,
            SignalKind::Audio
                | SignalKind::CvBipolar
                | SignalKind::CvUnipolar
                | SignalKind::VoltPerOctave
        )
    }

    /// Threshold voltage for high/low detection
    pub fn gate_threshold(&self) -> Option<f64> {
        match self {
            SignalKind::Gate | SignalKind::Trigger | SignalKind::Clock => Some(2.5),
            _ => None,
        }
    }
}

// =============================================================================
// GUI Signal Semantics (Phase 2)
// =============================================================================

/// CSS hex color values for each signal type (for cable coloring in UI)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct SignalColors {
    /// Audio signal color (default: red #e94560)
    pub audio: String,
    /// Bipolar CV color (default: dark blue #0f3460)
    pub cv_bipolar: String,
    /// Unipolar CV color (default: cyan #00b4d8)
    pub cv_unipolar: String,
    /// V/Oct pitch CV color (default: green #90be6d)
    pub volt_per_octave: String,
    /// Gate signal color (default: yellow #f9c74f)
    pub gate: String,
    /// Trigger signal color (default: orange #f8961e)
    pub trigger: String,
    /// Clock signal color (default: purple #9d4edd)
    pub clock: String,
}

impl Default for SignalColors {
    fn default() -> Self {
        Self {
            audio: "#e94560".into(),
            cv_bipolar: "#0f3460".into(),
            cv_unipolar: "#00b4d8".into(),
            volt_per_octave: "#90be6d".into(),
            gate: "#f9c74f".into(),
            trigger: "#f8961e".into(),
            clock: "#9d4edd".into(),
        }
    }
}

impl SignalColors {
    /// Get the color for a specific signal kind
    pub fn get(&self, kind: SignalKind) -> &str {
        match kind {
            SignalKind::Audio => &self.audio,
            SignalKind::CvBipolar => &self.cv_bipolar,
            SignalKind::CvUnipolar => &self.cv_unipolar,
            SignalKind::VoltPerOctave => &self.volt_per_octave,
            SignalKind::Gate => &self.gate,
            SignalKind::Trigger => &self.trigger,
            SignalKind::Clock => &self.clock,
        }
    }
}

/// Enhanced port information for GUI display
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct PortInfo {
    /// Unique identifier within the module
    pub id: u32,
    /// Human-readable name
    pub name: String,
    /// Signal type
    pub kind: SignalKind,
    /// Port this is normalled to (by name, for UI display)
    pub normalled_to: Option<String>,
    /// Optional description for tooltips
    pub description: Option<String>,
}

impl PortInfo {
    /// Create a new PortInfo
    pub fn new(id: u32, name: impl Into<String>, kind: SignalKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            normalled_to: None,
            description: None,
        }
    }

    /// Set the normalled connection
    pub fn with_normalled_to(mut self, port_name: impl Into<String>) -> Self {
        self.normalled_to = Some(port_name.into());
        self
    }

    /// Set the description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

impl From<&PortDef> for PortInfo {
    fn from(def: &PortDef) -> Self {
        Self {
            id: def.id,
            name: def.name.clone(),
            kind: def.kind,
            normalled_to: None, // PortDef uses PortId, PortInfo uses name string
            description: None,
        }
    }
}

/// Compatibility status for port connections
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum Compatibility {
    /// Exact signal type match
    Exact,
    /// Compatible connection (different but valid)
    Allowed,
    /// Connection works but may have issues
    Warning { message: String },
}

/// Check if two signal kinds are compatible for connection
///
/// Returns the compatibility status indicating whether the connection is:
/// - Exact: Same signal types
/// - Allowed: Different but compatible types
/// - Warning: Works but may cause issues (e.g., clicks, tuning problems)
///
/// # Single source of truth
///
/// This function is a thin adapter over the authoritative
/// [`SignalKind::is_compatible_with`] implementation used by
/// the patch graph's validation. Both APIs therefore always agree: a warning from one
/// is a [`Compatibility::Warning`] from the other, and a clean verdict maps to
/// [`Compatibility::Allowed`] (or [`Compatibility::Exact`] for identical kinds). Keep
/// the compatibility rules in `is_compatible_with` only; do not fork them here.
pub fn ports_compatible(from: SignalKind, to: SignalKind) -> Compatibility {
    if from == to {
        return Compatibility::Exact;
    }

    // Delegate to the authoritative compatibility check (defined in `graph`).
    match from.is_compatible_with(&to).warning {
        None => Compatibility::Allowed,
        Some(message) => Compatibility::Warning { message },
    }
}

/// Definition of a single port (input or output)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct PortDef {
    /// Unique identifier within the module
    pub id: PortId,

    /// Human-readable name (e.g., "cutoff", "voct", "out")
    pub name: String,

    /// Signal type for validation and UI hints
    pub kind: SignalKind,

    /// Default value when no cable connected
    pub default: f64,

    /// For inputs: internal source when unpatched (normalled connection)
    pub normalled_to: Option<PortId>,

    /// Whether this input has an associated attenuverter control
    pub has_attenuverter: bool,
}

impl PortDef {
    pub fn new(id: PortId, name: impl Into<String>, kind: SignalKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            default: 0.0,
            normalled_to: None,
            has_attenuverter: false,
        }
    }

    pub fn with_default(mut self, default: f64) -> Self {
        self.default = default;
        self
    }

    pub fn with_attenuverter(mut self) -> Self {
        self.has_attenuverter = true;
        self
    }

    pub fn normalled_to(mut self, port: PortId) -> Self {
        self.normalled_to = Some(port);
        self
    }
}

/// Specification of all ports for a module
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct PortSpec {
    pub inputs: Vec<PortDef>,
    pub outputs: Vec<PortDef>,
}

impl PortSpec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn input_by_name(&self, name: &str) -> Option<&PortDef> {
        self.inputs.iter().find(|p| p.name == name)
    }

    pub fn output_by_name(&self, name: &str) -> Option<&PortDef> {
        self.outputs.iter().find(|p| p.name == name)
    }

    pub fn input_by_id(&self, id: PortId) -> Option<&PortDef> {
        self.inputs.iter().find(|p| p.id == id)
    }

    pub fn output_by_id(&self, id: PortId) -> Option<&PortDef> {
        self.outputs.iter().find(|p| p.id == id)
    }
}

/// Runtime port values container.
///
/// A small dense map from [`PortId`] to `f64`, laid out as two parallel vectors: `ids` in
/// first-write order, and `values` where `None` means "not written since the last
/// [`clear`](Self::clear)". Lookups are a linear scan of `ids`, which for the ≤ 8 ports a
/// module declares is a handful of `u32` comparisons out of a single cache line — far
/// cheaper than hashing a key, and with no per-key hashing on the audio path at all.
///
/// Two properties the graph engine relies on:
///
/// - **Slot order is stable.** [`clear`](Self::clear) blanks the values but *keeps* the id
///   layout, so once a container has been warmed with a module's [`PortSpec`] order, slot
///   `k` belongs to spec port `k` for the rest of its life. That is what lets
///   `NodeExec::scatter` read outputs by index instead of by lookup.
/// - **Unwritten is absent.** A port that a module never set reads back as `None` from
///   [`get`](Self::get) and `false` from [`has`](Self::has), exactly as when this was a
///   `HashMap` that simply had no such key.
#[derive(Debug, Clone, Default)]
pub struct PortValues {
    /// Port ids in first-write order; slot `k` of `values` belongs to `ids[k]`.
    ids: Vec<PortId>,
    /// Value per slot, `None` when unwritten since the last [`clear`](PortValues::clear).
    values: Vec<Option<f64>>,
}

impl PortValues {
    pub fn new() -> Self {
        Self::default()
    }

    /// Dense slot holding `id`, whether or not it currently holds a value.
    #[inline]
    fn slot_of(&self, id: PortId) -> Option<usize> {
        self.ids.iter().position(|&candidate| candidate == id)
    }

    #[inline]
    pub fn get(&self, id: PortId) -> Option<f64> {
        self.slot_of(id).and_then(|k| self.values[k])
    }

    #[inline]
    pub fn get_or(&self, id: PortId, default: f64) -> f64 {
        self.get(id).unwrap_or(default)
    }

    #[inline]
    pub fn set(&mut self, id: PortId, value: f64) {
        match self.slot_of(id) {
            Some(k) => self.values[k] = Some(value),
            None => {
                self.ids.push(id);
                self.values.push(Some(value));
            }
        }
    }

    /// Accumulate (sum) a value into a port (for input mixing)
    #[inline]
    pub fn accumulate(&mut self, id: PortId, value: f64) {
        // An absent port accumulates onto a fresh `0.0`, not onto `value` itself — which
        // matters for signed zero: `0.0 + -0.0` is `+0.0`.
        match self.slot_of(id) {
            Some(k) => self.values[k] = Some(self.values[k].unwrap_or(0.0) + value),
            None => {
                self.ids.push(id);
                self.values.push(Some(0.0 + value));
            }
        }
    }

    #[inline]
    pub fn has(&self, id: PortId) -> bool {
        self.get(id).is_some()
    }

    /// Forget every value, keeping the id layout (and its allocation) intact.
    #[inline]
    pub fn clear(&mut self) {
        self.values.fill(None);
    }

    /// Value at dense slot `slot`, which is expected to hold `id`.
    ///
    /// The fast path for callers that know the layout — the graph's scatter, whose scratch
    /// buffers were warmed in [`PortSpec`] output order at compile time — turning a lookup
    /// into an indexed read. Falls back to [`get`](Self::get) whenever the slot does not
    /// hold the expected id, so the result is always identical to `get(id)`.
    #[inline]
    pub(crate) fn get_at(&self, slot: usize, id: PortId) -> Option<f64> {
        match self.ids.get(slot) {
            Some(&found) if found == id => self.values[slot],
            _ => self.get(id),
        }
    }

    /// Iterate the ports that currently hold a value, in slot order.
    #[inline]
    pub(crate) fn iter(&self) -> impl Iterator<Item = (PortId, f64)> + '_ {
        self.ids
            .iter()
            .zip(self.values.iter())
            .filter_map(|(&id, value)| value.map(|v| (id, v)))
    }
}

/// Block-oriented port values for efficient processing
pub struct BlockPortValues {
    buffers: StdMap<PortId, Vec<f64>>,
    block_size: usize,
}

impl BlockPortValues {
    pub fn new(block_size: usize) -> Self {
        Self {
            buffers: StdMap::new(),
            block_size,
        }
    }

    pub fn block_size(&self) -> usize {
        self.block_size
    }

    pub fn get_buffer(&self, port: PortId) -> Option<&[f64]> {
        self.buffers.get(&port).map(|v| v.as_slice())
    }

    pub fn get_buffer_mut(&mut self, port: PortId) -> &mut Vec<f64> {
        self.buffers
            .entry(port)
            .or_insert_with(|| vec![0.0; self.block_size])
    }

    pub fn frame(&self, index: usize) -> PortValues {
        let mut values = PortValues::new();
        self.frame_into(index, &mut values);
        values
    }

    /// Read frame `index` into an existing [`PortValues`], reusing its allocation.
    ///
    /// Clears `dst` and refills it from each port buffer at `index`. Unlike [`Self::frame`], this
    /// performs no allocation once `dst` has been warmed with the same key set, which lets
    /// block loops (e.g. the default [`GraphModule::process_block`]) avoid a fresh
    /// [`PortValues`] per frame.
    pub fn frame_into(&self, index: usize, dst: &mut PortValues) {
        dst.clear();
        for (&port, buffer) in &self.buffers {
            if index < buffer.len() {
                dst.set(port, buffer[index]);
            }
        }
    }

    pub fn set_frame(&mut self, index: usize, values: PortValues) {
        self.set_frame_ref(index, &values);
    }

    /// Write a borrowed [`PortValues`] into frame `index`, without taking ownership.
    ///
    /// The by-reference companion to [`Self::set_frame`], so a caller can reuse a single output
    /// [`PortValues`] across every frame of a block instead of moving (and reallocating) one
    /// per frame.
    pub fn set_frame_ref(&mut self, index: usize, values: &PortValues) {
        for (port, value) in values.iter() {
            let buffer = self.get_buffer_mut(port);
            if index < buffer.len() {
                buffer[index] = value;
            }
        }
    }

    pub fn clear(&mut self) {
        for buffer in self.buffers.values_mut() {
            buffer.fill(0.0);
        }
    }
}

/// Parameter range mapping for modulated parameters
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ParamRange {
    /// Linear mapping from normalized (0–1) to (min, max)
    Linear { min: f64, max: f64 },

    /// Exponential mapping, useful for frequency/time controls
    Exponential { min: f64, max: f64 },

    /// V/Oct: input is in volts, output is frequency multiplier
    VoltPerOctave { base_freq: f64 },
}

impl ParamRange {
    pub fn apply(&self, normalized: f64) -> f64 {
        match self {
            ParamRange::Linear { min, max } => min + normalized.clamp(0.0, 1.0) * (max - min),
            ParamRange::Exponential { min, max } => {
                let clamped = normalized.clamp(0.0, 1.0);
                // Exponential interpolation `min * (max/min)^t` is only defined for a
                // strictly positive domain (0 < min, 0 < max). If either bound is
                // non-positive, `max/min` can be negative and `pow(neg, frac)` yields
                // NaN, so fall back to a plain linear interpolation which is always
                // finite. This guards callers that construct e.g. Exponential{min:20,
                // max:-1} from silently poisoning frequency/time controls with NaN.
                if *min > 0.0 && *max > 0.0 {
                    min * Libm::<f64>::pow(max / min, clamped)
                } else {
                    min + clamped * (max - min)
                }
            }
            ParamRange::VoltPerOctave { base_freq } => {
                base_freq * Libm::<f64>::pow(2.0, normalized)
            }
        }
    }
}

/// A parameter that combines a base value (knob) with CV modulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModulatedParam {
    /// Base value from panel knob (typically 0.0–1.0 normalized)
    pub base: f64,

    /// Incoming CV **voltage** (set during tick).
    ///
    /// Interpreted on the bipolar ±5 V scale: [`value`](Self::value) normalizes it by
    /// [`CV_FULL_SCALE_VOLTS`](Self::CV_FULL_SCALE_VOLTS) so that a full +5 V of CV
    /// (with attenuverter at +1.0) contributes +1.0 to the normalized parameter.
    pub cv: f64,

    /// Attenuverter setting (-1.0 to 1.0)
    /// Positive: CV adds to base
    /// Negative: CV subtracts from base (inverted)
    pub attenuverter: f64,

    /// Output range mapping
    pub range: ParamRange,
}

impl ModulatedParam {
    /// Full-scale CV voltage used to normalize [`cv`](Self::cv) into the 0–1 base domain.
    ///
    /// Bipolar CV spans ±5 V, so dividing by 5 V maps a full-swing CV signal onto the
    /// same normalized 0–1 range as `base` before the two are combined.
    pub const CV_FULL_SCALE_VOLTS: f64 = 5.0;

    pub fn new(range: ParamRange) -> Self {
        Self {
            base: 0.5,
            cv: 0.0,
            attenuverter: 1.0,
            range,
        }
    }

    pub fn with_base(mut self, base: f64) -> Self {
        self.base = base;
        self
    }

    /// Compute the effective parameter value.
    ///
    /// `base` is a normalized 0–1 knob position; `cv` is a voltage that is normalized by
    /// [`CV_FULL_SCALE_VOLTS`](Self::CV_FULL_SCALE_VOLTS) before being scaled by the
    /// attenuverter (±1.0) and summed with `base`. This keeps CV modulation proportional:
    /// a full +5 V of CV shifts the normalized value by at most ±1.0 rather than slamming
    /// the parameter to its rail. The combined value is then mapped through `range`.
    pub fn value(&self) -> f64 {
        let modulated = self.base + (self.cv / Self::CV_FULL_SCALE_VOLTS) * self.attenuverter;
        self.range.apply(modulated)
    }

    /// Update CV from port value
    pub fn set_cv(&mut self, cv: f64) {
        self.cv = cv;
    }
}

/// Parameter definition for UI binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDef {
    pub id: ParamId,
    pub name: String,
    pub default: f64,
    pub range: ParamRange,
}

/// Type-erased module interface for graph-based patching
pub trait GraphModule: Send + Sync {
    /// Returns the module's port specification
    fn port_spec(&self) -> &PortSpec;

    /// Process one sample given port values
    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues);

    /// Process a block of samples (optional optimization).
    ///
    /// The default drives [`tick`](Self::tick) frame-by-frame. It reuses a single input and
    /// output [`PortValues`] across the whole block (via
    /// [`BlockPortValues::frame_into`]/[`set_frame_ref`](BlockPortValues::set_frame_ref)), so
    /// it does **not** allocate per frame — only once per call to warm the reused buffers.
    ///
    /// For the graph engine, prefer [`Patch::tick_block`](crate::graph::Patch::tick_block),
    /// which is fully allocation-free after compile.
    fn process_block(
        &mut self,
        inputs: &BlockPortValues,
        outputs: &mut BlockPortValues,
        frames: usize,
    ) {
        let mut in_frame = PortValues::new();
        let mut out_frame = PortValues::new();
        for i in 0..frames {
            inputs.frame_into(i, &mut in_frame);
            out_frame.clear();
            self.tick(&in_frame, &mut out_frame);
            outputs.set_frame_ref(i, &out_frame);
        }
    }

    /// Reset internal state
    fn reset(&mut self);

    /// Set sample rate
    fn set_sample_rate(&mut self, sample_rate: f64);

    /// Whether this module breaks a feedback cycle in the patch graph.
    ///
    /// The graph normally rejects any cable cycle with [`PatchError::CycleDetected`].
    /// A module that returns `true` (delay-style modules such as `UnitDelay` and
    /// `DelayLine`) is treated as a one-sample delay boundary: [`Patch::compile`] excludes
    /// the edges feeding *into* it from the topological sort, so a loop routed through it
    /// compiles. At runtime such a module reads its inputs from the previous tick's output
    /// buffers, giving the classic single-sample feedback delay. Cycles that contain no
    /// cycle-breaker still fail to compile.
    ///
    /// [`PatchError::CycleDetected`]: crate::graph::PatchError::CycleDetected
    /// [`Patch::compile`]: crate::graph::Patch::compile
    fn breaks_feedback_cycle(&self) -> bool {
        false
    }

    /// Get parameter definitions for UI binding.
    ///
    /// **Most built-in modules do not use this API.** Nearly all of them expose their
    /// controllable quantities as **input ports** (see [`port_spec`](Self::port_spec)) —
    /// e.g. a VCO's frequency, an SVF's cutoff, or an ADSR's stage times are all input
    /// ports driven by cables or their `default` values — and leave this method at its
    /// empty default. The authoritative way to discover and drive parameters for GUIs is
    /// the `ModuleIntrospection` API (available with the `alloc` feature), not this
    /// trait-default no-op. It remains here only for the handful of modules whose
    /// parameters are genuinely not ports.
    fn params(&self) -> &[ParamDef] {
        &[]
    }

    /// Get a parameter value.
    ///
    /// Defaults to `None`. See [`params`](Self::params): most modules surface their state
    /// through input ports and `ModuleIntrospection`, not through this method.
    fn get_param(&self, _id: ParamId) -> Option<f64> {
        None
    }

    /// Set a parameter value.
    ///
    /// Defaults to a no-op. See [`params`](Self::params): most modules surface their state
    /// through input ports and `ModuleIntrospection`, not through this method.
    fn set_param(&mut self, _id: ParamId, _value: f64) {}

    /// Get module type identifier for serialization
    fn type_id(&self) -> &'static str {
        "unknown"
    }

    /// Serialize module state (alloc feature only)
    #[cfg(feature = "alloc")]
    fn serialize_state(&self) -> Option<serde_json::Value> {
        None
    }

    /// Deserialize module state (alloc feature only)
    #[cfg(feature = "alloc")]
    fn deserialize_state(
        &mut self,
        _state: &serde_json::Value,
    ) -> Result<(), alloc::string::String> {
        Ok(())
    }

    /// Downcast this module to its [`ModuleIntrospection`](crate::introspection::ModuleIntrospection) view, if it exposes one.
    ///
    /// A `Box<dyn GraphModule>` (as stored inside a [`Patch`](crate::graph::Patch)) cannot
    /// otherwise reach the module's `ModuleIntrospection` impl, so this hook bridges the two
    /// trait objects. It returns `None` by default; modules with genuine internal (non-port)
    /// parameters override it — typically via [`impl_introspect!`](crate::impl_introspect) —
    /// to return `Some(self)`. Parameters that are input ports are discovered and driven
    /// through the port system instead (see [`Patch::param_infos`](crate::graph::Patch::param_infos)),
    /// so most modules leave this at the default.
    ///
    /// Gated on `alloc` because `ModuleIntrospection` (and its `Vec`/`String` payloads) live
    /// in the alloc tier; pure `no_std` builds never see this method.
    #[cfg(feature = "alloc")]
    fn introspect(&self) -> Option<&dyn crate::introspection::ModuleIntrospection> {
        None
    }

    /// Mutable companion to [`introspect`](Self::introspect), used to set internal parameters.
    #[cfg(feature = "alloc")]
    fn introspect_mut(&mut self) -> Option<&mut dyn crate::introspection::ModuleIntrospection> {
        None
    }
}

/// Wire a module's [`ModuleIntrospection`](crate::introspection::ModuleIntrospection) impl into the [`GraphModule`] trait object.
///
/// Invoke once inside a module's `impl GraphModule for T { .. }` block. It expands to the
/// `introspect`/`introspect_mut` overrides (both `alloc`-gated) returning `Some(self)`, so a
/// live [`Patch`](crate::graph::Patch) can reach the module's parameter metadata through its
/// boxed trait object. Requires `T: ModuleIntrospection` (satisfied under `alloc`).
#[macro_export]
macro_rules! impl_introspect {
    () => {
        #[cfg(feature = "alloc")]
        fn introspect(&self) -> Option<&dyn $crate::introspection::ModuleIntrospection> {
            Some(self)
        }
        #[cfg(feature = "alloc")]
        fn introspect_mut(
            &mut self,
        ) -> Option<&mut dyn $crate::introspection::ModuleIntrospection> {
            Some(self)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_kind_ranges() {
        assert_eq!(SignalKind::Audio.voltage_range(), (-5.0, 5.0));
        assert_eq!(SignalKind::Gate.voltage_range(), (0.0, 5.0));
        assert_eq!(SignalKind::CvUnipolar.voltage_range(), (0.0, 10.0));
    }

    #[test]
    fn test_signal_kind_summable() {
        assert!(SignalKind::Audio.is_summable());
        assert!(SignalKind::CvBipolar.is_summable());
        assert!(!SignalKind::Gate.is_summable());
        assert!(!SignalKind::Trigger.is_summable());
    }

    #[test]
    fn test_port_values() {
        let mut pv = PortValues::new();
        pv.set(0, 1.0);
        pv.set(1, 2.0);
        assert_eq!(pv.get(0), Some(1.0));
        assert_eq!(pv.get(1), Some(2.0));
        assert_eq!(pv.get(2), None);
        assert_eq!(pv.get_or(2, 5.0), 5.0);

        pv.accumulate(0, 0.5);
        assert_eq!(pv.get(0), Some(1.5));
    }

    /// `clear` must forget values without forgetting the slot layout, and an unwritten port
    /// must read back as absent — the two properties the graph engine's scratch buffers and
    /// "unwritten output keeps its previous routing value" rule are built on.
    #[test]
    fn test_port_values_clear_keeps_layout_and_absence() {
        let mut pv = PortValues::new();
        pv.set(7, 1.0);
        pv.set(3, 2.0);

        pv.clear();
        assert!(!pv.has(7));
        assert!(!pv.has(3));
        assert_eq!(pv.get(7), None);
        assert_eq!(pv.get_or(3, -1.0), -1.0);
        assert_eq!(pv.iter().count(), 0);

        // Rewriting one port leaves the other absent, and slot order is unchanged.
        pv.set(3, 4.0);
        assert_eq!(pv.get_at(1, 3), Some(4.0));
        assert_eq!(pv.get(7), None);
        assert_eq!(pv.iter().collect::<Vec<_>>(), vec![(3, 4.0)]);
    }

    /// `get_at` is a hint, never a source of truth: a wrong slot still resolves through the
    /// normal lookup, so it can never disagree with `get`.
    #[test]
    fn test_port_values_get_at_falls_back_to_lookup() {
        let mut pv = PortValues::new();
        pv.set(10, 1.0);
        pv.set(11, 2.0);

        assert_eq!(pv.get_at(0, 10), Some(1.0));
        // Mismatched slot, out-of-range slot, and unknown id all agree with `get`.
        assert_eq!(pv.get_at(1, 10), Some(1.0));
        assert_eq!(pv.get_at(99, 11), Some(2.0));
        assert_eq!(pv.get_at(0, 12), None);
    }

    /// Accumulating onto an absent port starts from `+0.0`, so a `-0.0` contribution does
    /// not leave a negative zero behind (matching the previous entry-API implementation).
    #[test]
    fn test_port_values_accumulate_from_absent_normalizes_signed_zero() {
        let mut pv = PortValues::new();
        pv.accumulate(0, -0.0);
        assert_eq!(pv.get(0).map(f64::to_bits), Some(0.0f64.to_bits()));
    }

    #[test]
    fn test_param_range_linear() {
        let range = ParamRange::Linear {
            min: 0.0,
            max: 100.0,
        };
        assert!((range.apply(0.0) - 0.0).abs() < 1e-10);
        assert!((range.apply(0.5) - 50.0).abs() < 1e-10);
        assert!((range.apply(1.0) - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_param_range_exponential() {
        let range = ParamRange::Exponential {
            min: 20.0,
            max: 20000.0,
        };
        assert!((range.apply(0.0) - 20.0).abs() < 1e-10);
        assert!((range.apply(1.0) - 20000.0).abs() < 1e-10);
    }

    #[test]
    fn test_param_range_voct() {
        let range = ParamRange::VoltPerOctave { base_freq: 261.63 };
        // 0V = C4 = 261.63 Hz
        assert!((range.apply(0.0) - 261.63).abs() < 0.01);
        // +1V = C5 = 523.26 Hz
        assert!((range.apply(1.0) - 523.26).abs() < 0.01);
    }

    #[test]
    fn test_modulated_param() {
        let mut param = ModulatedParam::new(ParamRange::Linear {
            min: 0.0,
            max: 100.0,
        })
        .with_base(0.5);

        // No CV: should return base * range
        assert!((param.value() - 50.0).abs() < 1e-10);

        // Realistic CV is a *voltage*, normalized by 5 V full scale.
        // +1 V of CV shifts the normalized value by 1/5 = 0.2 -> 0.7 -> 70.
        param.set_cv(1.0);
        assert!((param.value() - 70.0).abs() < 1e-10);

        // Invert attenuverter: +1 V CV now subtracts -> 0.3 -> 30.
        param.attenuverter = -1.0;
        assert!((param.value() - 30.0).abs() < 1e-10);
    }

    #[test]
    fn test_modulated_param_full_scale_cv_is_proportional() {
        // Q081 regression: a full ±5 V CV must map to a full ±1.0 normalized swing,
        // not slam the parameter to a rail from any modest voltage.
        let mut param = ModulatedParam::new(ParamRange::Linear {
            min: 0.0,
            max: 100.0,
        })
        .with_base(0.5);

        // A modest +1 V of CV should move the param proportionally to
        // 0.5 + (1/5)*1 = 0.7 -> 70, NOT slam to the maximum (the pre-fix behavior added
        // the raw voltage: 0.5 + 1.0 = 1.5 -> clamped to 100).
        param.set_cv(1.0);
        assert!(
            (param.value() - 70.0).abs() < 1e-10,
            "1 V CV should be proportional, got {}",
            param.value()
        );

        // Full +5 V reaches exactly the top of the range.
        param.set_cv(5.0);
        assert!((param.value() - 100.0).abs() < 1e-10);

        // Full -5 V reaches the bottom.
        param.set_cv(-5.0);
        assert!((param.value() - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_signal_kind_gate_threshold() {
        assert!(SignalKind::Gate.gate_threshold().is_some());
        assert!(SignalKind::Trigger.gate_threshold().is_some());
        assert!(SignalKind::Audio.gate_threshold().is_none());
    }

    #[test]
    fn test_port_def_with_default_and_attenuverter() {
        let port = PortDef::new(0, "test", SignalKind::CvUnipolar)
            .with_default(5.0)
            .with_attenuverter();

        assert!((port.default - 5.0).abs() < 0.001);
        assert!(port.has_attenuverter);
    }

    #[test]
    fn test_port_def_normalled_to() {
        let port = PortDef::new(0, "test", SignalKind::CvUnipolar).normalled_to(1);
        assert_eq!(port.normalled_to, Some(1));
    }

    #[test]
    fn test_port_spec_lookup() {
        let spec = PortSpec {
            inputs: vec![
                PortDef::new(0, "in1", SignalKind::Audio),
                PortDef::new(1, "in2", SignalKind::CvBipolar),
            ],
            outputs: vec![
                PortDef::new(10, "out1", SignalKind::Audio),
                PortDef::new(11, "out2", SignalKind::Gate),
            ],
        };

        assert!(spec.input_by_name("in1").is_some());
        assert!(spec.input_by_name("nonexistent").is_none());
        assert!(spec.output_by_name("out1").is_some());
        assert!(spec.output_by_name("nonexistent").is_none());

        assert!(spec.input_by_id(0).is_some());
        assert!(spec.input_by_id(99).is_none());
        assert!(spec.output_by_id(10).is_some());
        assert!(spec.output_by_id(99).is_none());
    }

    #[test]
    fn test_port_values_has() {
        let mut pv = PortValues::new();
        assert!(!pv.has(0));
        pv.set(0, 1.0);
        assert!(pv.has(0));
    }

    #[test]
    fn test_port_values_clear() {
        let mut pv = PortValues::new();
        pv.set(0, 1.0);
        pv.set(1, 2.0);
        pv.clear();
        assert!(!pv.has(0));
        assert!(!pv.has(1));
    }

    #[test]
    fn test_block_port_values() {
        let mut bpv = BlockPortValues::new(64);
        assert_eq!(bpv.block_size(), 64);

        // Get mutable buffer (creates buffer for port 0)
        let buf_mut = bpv.get_buffer_mut(0);
        assert_eq!(buf_mut.len(), 64);
        buf_mut[0] = 1.0;

        // Now we can read it
        assert_eq!(bpv.get_buffer(0).unwrap()[0], 1.0);

        // Frame operations
        let mut frame_vals = PortValues::new();
        frame_vals.set(0, 99.0);
        bpv.set_frame(1, frame_vals);

        // Clear
        bpv.clear();
    }

    #[test]
    fn test_signal_kind_clock() {
        let range = SignalKind::Clock.voltage_range();
        assert_eq!(range, (0.0, 5.0));
        assert!(!SignalKind::Clock.is_summable());
    }

    #[test]
    fn test_param_range_exponential_clamped() {
        let range = ParamRange::Exponential {
            min: 20.0,
            max: 20000.0,
        };
        // Test with values outside 0-1
        let below = range.apply(-0.5);
        assert!((below - 20.0).abs() < 1e-10);

        let above = range.apply(1.5);
        assert!((above - 20000.0).abs() < 1e-10);
    }

    #[test]
    fn test_param_range_exponential_invalid_domain_no_nan() {
        // Q082 regression: min>0 with max<=0 makes max/min negative, and
        // pow(negative, fractional) is NaN. The guard must fall back to linear.
        let range = ParamRange::Exponential {
            min: 20.0,
            max: -1.0,
        };
        for &t in &[0.0, 0.25, 0.5, 0.75, 1.0] {
            let v = range.apply(t);
            assert!(v.is_finite(), "apply({}) produced non-finite {}", t, v);
        }
        // Endpoints match the linear fallback.
        assert!((range.apply(0.0) - 20.0).abs() < 1e-10);
        assert!((range.apply(1.0) - (-1.0)).abs() < 1e-10);

        // max == 0 (also invalid for exponential) must stay finite too.
        let zero_max = ParamRange::Exponential {
            min: 10.0,
            max: 0.0,
        };
        assert!(zero_max.apply(0.5).is_finite());
    }

    // =============================================================================
    // Signal Semantics Tests (Phase 2)
    // =============================================================================

    #[test]
    fn test_signal_colors_default() {
        let colors = SignalColors::default();
        assert_eq!(colors.audio, "#e94560");
        assert_eq!(colors.cv_bipolar, "#0f3460");
        assert_eq!(colors.cv_unipolar, "#00b4d8");
        assert_eq!(colors.volt_per_octave, "#90be6d");
        assert_eq!(colors.gate, "#f9c74f");
        assert_eq!(colors.trigger, "#f8961e");
        assert_eq!(colors.clock, "#9d4edd");
    }

    #[test]
    fn test_signal_colors_get() {
        let colors = SignalColors::default();
        assert_eq!(colors.get(SignalKind::Audio), "#e94560");
        assert_eq!(colors.get(SignalKind::Gate), "#f9c74f");
        assert_eq!(colors.get(SignalKind::VoltPerOctave), "#90be6d");
    }

    #[test]
    fn test_port_info_creation() {
        let info = PortInfo::new(0, "test", SignalKind::Audio)
            .with_description("A test port")
            .with_normalled_to("other");

        assert_eq!(info.id, 0);
        assert_eq!(info.name, "test");
        assert_eq!(info.kind, SignalKind::Audio);
        assert_eq!(info.description, Some("A test port".to_string()));
        assert_eq!(info.normalled_to, Some("other".to_string()));
    }

    #[test]
    fn test_port_info_from_port_def() {
        let def = PortDef::new(5, "cutoff", SignalKind::CvUnipolar);
        let info = PortInfo::from(&def);

        assert_eq!(info.id, 5);
        assert_eq!(info.name, "cutoff");
        assert_eq!(info.kind, SignalKind::CvUnipolar);
        assert!(info.normalled_to.is_none());
        assert!(info.description.is_none());
    }

    #[test]
    fn test_ports_compatible_exact() {
        assert_eq!(
            ports_compatible(SignalKind::Audio, SignalKind::Audio),
            Compatibility::Exact
        );
        assert_eq!(
            ports_compatible(SignalKind::Gate, SignalKind::Gate),
            Compatibility::Exact
        );
        assert_eq!(
            ports_compatible(SignalKind::VoltPerOctave, SignalKind::VoltPerOctave),
            Compatibility::Exact
        );
    }

    #[test]
    fn test_ports_compatible_audio_to_anything() {
        // Unified with graph::SignalKind::is_compatible_with: Audio->CV / Audio->Gate are
        // permitted but flagged with a warning ("ensure this is intentional").
        assert!(matches!(
            ports_compatible(SignalKind::Audio, SignalKind::CvBipolar),
            Compatibility::Warning { .. }
        ));
        assert!(matches!(
            ports_compatible(SignalKind::Audio, SignalKind::Gate),
            Compatibility::Warning { .. }
        ));
    }

    #[test]
    fn test_ports_compatible_cv_interop() {
        // Bipolar<->Unipolar CV crossings warn (possible clip/offset).
        assert!(matches!(
            ports_compatible(SignalKind::CvBipolar, SignalKind::CvUnipolar),
            Compatibility::Warning { .. }
        ));
        assert!(matches!(
            ports_compatible(SignalKind::CvUnipolar, SignalKind::CvBipolar),
            Compatibility::Warning { .. }
        ));
        // V/Oct -> bipolar CV is a clean, warning-free pitch extraction.
        assert_eq!(
            ports_compatible(SignalKind::VoltPerOctave, SignalKind::CvBipolar),
            Compatibility::Allowed
        );
    }

    #[test]
    fn test_ports_compatible_gate_trigger_interop() {
        // Gate<->Trigger warn about timing differences.
        assert!(matches!(
            ports_compatible(SignalKind::Gate, SignalKind::Trigger),
            Compatibility::Warning { .. }
        ));
        assert!(matches!(
            ports_compatible(SignalKind::Trigger, SignalKind::Gate),
            Compatibility::Warning { .. }
        ));
        // Clock->Trigger is clean; Clock->Gate warns about duty cycle.
        assert_eq!(
            ports_compatible(SignalKind::Clock, SignalKind::Trigger),
            Compatibility::Allowed
        );
        assert!(matches!(
            ports_compatible(SignalKind::Clock, SignalKind::Gate),
            Compatibility::Warning { .. }
        ));
    }

    #[test]
    fn test_ports_compatible_warnings() {
        // Gate to Audio: unusual connection -> warning.
        let compat = ports_compatible(SignalKind::Gate, SignalKind::Audio);
        assert!(matches!(compat, Compatibility::Warning { .. }));

        // Bipolar CV -> V/Oct is treated as clean pitch modulation (no warning).
        assert_eq!(
            ports_compatible(SignalKind::CvBipolar, SignalKind::VoltPerOctave),
            Compatibility::Allowed
        );
    }

    #[test]
    fn test_ports_compatible_agrees_with_is_compatible_with() {
        // Q124: the two public compatibility APIs must never disagree. Pin the
        // Audio -> CvBipolar case explicitly, then cross-check every ordered pair.
        // `is_compatible_with` (defined in `graph`) is the single source of truth.
        let audio_cv = SignalKind::Audio.is_compatible_with(&SignalKind::CvBipolar);
        assert!(
            audio_cv.warning.is_some(),
            "is_compatible_with should warn on Audio->CvBipolar"
        );
        assert!(
            matches!(
                ports_compatible(SignalKind::Audio, SignalKind::CvBipolar),
                Compatibility::Warning { .. }
            ),
            "ports_compatible should agree and warn on Audio->CvBipolar"
        );

        let all = [
            SignalKind::Audio,
            SignalKind::CvBipolar,
            SignalKind::CvUnipolar,
            SignalKind::VoltPerOctave,
            SignalKind::Gate,
            SignalKind::Trigger,
            SignalKind::Clock,
        ];
        for &a in &all {
            for &b in &all {
                let low = ports_compatible(a, b);
                let high = a.is_compatible_with(&b);
                // Warning verdicts must match exactly between the two APIs.
                let low_warns = matches!(low, Compatibility::Warning { .. });
                assert_eq!(
                    low_warns,
                    high.warning.is_some(),
                    "compatibility APIs disagree for {:?} -> {:?}",
                    a,
                    b
                );
            }
        }
    }

    #[test]
    fn test_signal_kind_serializes_snake_case() {
        // Q091: SignalKind must serialize snake_case to match the JSON schema and TS.
        assert_eq!(
            serde_json::to_string(&SignalKind::CvBipolar).unwrap(),
            "\"cv_bipolar\""
        );
        assert_eq!(
            serde_json::to_string(&SignalKind::VoltPerOctave).unwrap(),
            "\"volt_per_octave\""
        );
        assert_eq!(
            serde_json::to_string(&SignalKind::Audio).unwrap(),
            "\"audio\""
        );
        // Round-trips from snake_case.
        let k: SignalKind = serde_json::from_str("\"cv_unipolar\"").unwrap();
        assert_eq!(k, SignalKind::CvUnipolar);
    }

    #[test]
    fn test_compatibility_serialization() {
        let exact = Compatibility::Exact;
        let json = serde_json::to_string(&exact).unwrap();
        assert!(json.contains("exact"));

        let warning = Compatibility::Warning {
            message: "test".to_string(),
        };
        let json = serde_json::to_string(&warning).unwrap();
        assert!(json.contains("warning"));
        assert!(json.contains("test"));
    }
}
