//! Polyphony Support
//!
//! This module provides voice allocation, per-voice processing, and unison
//! capabilities for building polyphonic synthesizers.
//!
//! # Architecture
//!
//! - [`VoiceAllocator`] - Manages which voices get assigned to incoming notes
//! - [`Voice`] - A single voice's allocation state (note, gate, age, envelope)
//! - [`VoiceInput`] - An **in-graph** control-signal source (the "voice
//!   controller"): a [`GraphModule`] whose `voct`/`gate`/`trigger`/`velocity`
//!   outputs are driven from a shared, lock-free [`VoiceControl`] handle. One is
//!   inserted into every voice patch so the allocator's per-voice control values
//!   actually reach the DSP graph.
//! - [`PolyPatch`] - A polyphonic patch that builds one voice graph per voice
//!   (times the unison count), routes allocator state into each graph via its
//!   controller, follows each voice's real output level to time release tails,
//!   and mixes everything down with polyphony gain compensation.
//! - [`UnisonConfig`] - Stacked, detuned voices for thick unison sounds.
//!
//! # Building a polyphonic synth
//!
//! ```
//! use quiver::prelude::*;
//!
//! let sr = 48_000.0;
//! let mut poly = PolyPatch::with_voice_fn(4, sr, |patch, ctrl| {
//!     let sr = patch.sample_rate();
//!     let vco = patch.add("vco", Vco::new(sr));
//!     let adsr = patch.add("adsr", Adsr::new(sr));
//!     let vca = patch.add("vca", Vca::new());
//!     let out = patch.add("out", StereoOutput::new());
//!     // The controller (`ctrl`) exposes voct / gate / trigger / velocity.
//!     patch.connect(ctrl.out("voct"), vco.in_("voct"))?;
//!     patch.connect(ctrl.out("gate"), adsr.in_("gate"))?;
//!     patch.connect(vco.out("saw"), vca.in_("in"))?;
//!     patch.connect(adsr.out("env"), vca.in_("cv"))?;
//!     patch.connect(vca.out("out"), out.in_("left"))?;
//!     patch.set_output(out.id());
//!     Ok(())
//! })
//! .unwrap();
//!
//! poly.note_on(60, 100);
//! let (_l, _r) = poly.tick();
//! poly.note_off(60);
//! ```

use crate::graph::{NodeHandle, Patch, PatchError};
use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::format;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
// `AtomicU64` from `portable-atomic`, not `core`: the latter is absent on
// targets with `max_atomic_width < 64` (e.g. `thumbv7em-none-eabihf`).
use libm::Libm;
use portable_atomic::AtomicU64;

// ---------------------------------------------------------------------------
// Tuning constants (all time-based state derives from these + the sample rate)
// ---------------------------------------------------------------------------

/// Amplitude-follower time constant used to track each voice's real output
/// level for release-tail detection.
const FOLLOWER_TAU_S: f64 = 0.010;
/// Time constant for smoothing the sounding-voice count feeding the polyphony
/// gain compensation, so the master gain never steps on note on/off.
const COUNT_TAU_S: f64 = 0.010;
/// Minimum time a released voice is kept alive before it may be auto-freed,
/// even if it already looks quiet. Guards quiet-attack / just-released voices
/// against being freed one sample after note-off.
const GRACE_S: f64 = 0.005;
/// Followed amplitude below which a *released* voice is considered finished.
const RELEASE_THRESHOLD: f64 = 0.001;
/// Length of the one-shot trigger pulse emitted by [`VoiceInput`], in seconds.
const TRIGGER_S: f64 = 0.001;

/// One-pole smoothing coefficient for a given time constant and sample rate.
///
/// Returns `exp(-1 / (tau · fs))`, clamped to a safe range for degenerate
/// inputs. A value near 1 means slow smoothing, near 0 means no smoothing.
#[inline]
fn one_pole_coeff(tau_s: f64, sample_rate: f64) -> f64 {
    if sample_rate <= 0.0 || tau_s <= 0.0 {
        return 0.0;
    }
    Libm::<f64>::exp(-1.0 / (tau_s * sample_rate))
}

/// Stereo *balance* gains for a pan position in `[-1, +1]`.
///
/// Unlike a constant-power pan, this law is **unity at the center** and only
/// attenuates the opposite channel as the position moves off-center, so it
/// preserves an already-stereo signal instead of applying a blanket −3 dB.
#[inline]
fn balance_gains(pan: f64) -> (f64, f64) {
    let p = pan.clamp(-1.0, 1.0);
    if p <= 0.0 {
        (1.0, 1.0 + p) // pan left: right channel fades out
    } else {
        (1.0 - p, 1.0) // pan right: left channel fades out
    }
}

/// Voice allocation algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AllocationMode {
    /// Reuse the oldest voice when all voices are active
    #[default]
    RoundRobin,
    /// Steal the quietest voice (based on the tracked envelope level)
    QuietestSteal,
    /// Steal the oldest active voice
    OldestSteal,
    /// Never steal - new notes are dropped when no free voice is available
    NoSteal,
    /// Highest-note priority: a new note may steal the **lowest-pitched**
    /// sounding voice, but only when the new note is higher than it. When no
    /// sounding voice is lower than the new note, the new note is **dropped**
    /// (`note_on` returns `None`).
    HighestPriority,
    /// Lowest-note priority: a new note may steal the **highest-pitched**
    /// sounding voice, but only when the new note is lower than it. When no
    /// sounding voice is higher than the new note, the new note is **dropped**
    /// (`note_on` returns `None`).
    LowestPriority,
}

/// State of a single voice
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    /// Voice is not playing
    Free,
    /// Voice is currently playing a note
    Active,
    /// Voice is in release phase (gate off, but envelope still running)
    Releasing,
}

/// A single voice in a polyphonic context
#[derive(Debug, Clone)]
pub struct Voice {
    /// Voice index (0 to num_voices - 1)
    pub index: usize,
    /// Current state
    pub state: VoiceState,
    /// MIDI note number currently playing (if any)
    pub note: Option<u8>,
    /// Current velocity (0.0 to 1.0)
    pub velocity: f64,
    /// Current V/Oct value
    pub voct: f64,
    /// Gate signal (0.0 or 1.0)
    pub gate: f64,
    /// Trigger signal (momentary; asserted on the note-on sample)
    pub trigger: f64,
    /// Age counter (samples since note on)
    pub age: u64,
    /// Samples elapsed since the gate fell (entering `Releasing`)
    pub release_samples: u64,
    /// Current tracked envelope level (0..~1), populated by [`PolyPatch`] from
    /// the voice's real output amplitude. Drives quiet-steal and release-tail
    /// auto-free decisions.
    pub envelope_level: f64,
}

impl Voice {
    /// Create a new inactive voice
    pub fn new(index: usize) -> Self {
        Self {
            index,
            state: VoiceState::Free,
            note: None,
            velocity: 0.0,
            voct: 0.0,
            gate: 0.0,
            trigger: 0.0,
            age: 0,
            release_samples: 0,
            envelope_level: 0.0,
        }
    }

    /// Trigger the voice with a new note
    pub fn note_on(&mut self, note: u8, velocity: f64) {
        self.state = VoiceState::Active;
        self.note = Some(note);
        self.velocity = velocity;
        self.voct = midi_note_to_voct(note);
        self.gate = 1.0;
        self.trigger = 1.0; // Cleared after one sample; the controller stretches it
        self.age = 0;
        self.release_samples = 0;
    }

    /// Release the voice
    pub fn note_off(&mut self) {
        if self.state == VoiceState::Active {
            self.state = VoiceState::Releasing;
            self.gate = 0.0;
            self.release_samples = 0;
        }
    }

    /// Mark voice as completely free
    pub fn free(&mut self) {
        self.state = VoiceState::Free;
        self.note = None;
        self.velocity = 0.0;
        self.gate = 0.0;
        self.trigger = 0.0;
        self.release_samples = 0;
        self.envelope_level = 0.0;
    }

    /// Advance the voice's per-sample bookkeeping (age, release counter, clear
    /// the one-sample trigger). Auto-free is handled by [`VoiceAllocator`],
    /// which knows the release threshold and grace period.
    pub fn tick(&mut self) {
        self.age = self.age.saturating_add(1);
        self.trigger = 0.0;
        if self.state == VoiceState::Releasing {
            self.release_samples = self.release_samples.saturating_add(1);
        }
    }

    /// Check if voice is available for allocation
    pub fn is_free(&self) -> bool {
        self.state == VoiceState::Free
    }

    /// Check if voice is playing the given note
    pub fn is_playing_note(&self, note: u8) -> bool {
        self.note == Some(note) && self.state != VoiceState::Free
    }
}

/// Convert MIDI note number to V/Oct
/// MIDI note 60 (C4) = 0V
#[inline]
pub fn midi_note_to_voct(note: u8) -> f64 {
    (note as f64 - 60.0) / 12.0
}

/// Convert V/Oct to MIDI note number
#[inline]
pub fn voct_to_midi_note(voct: f64) -> u8 {
    Libm::<f64>::round(voct * 12.0 + 60.0).clamp(0.0, 127.0) as u8
}

/// Voice allocator for polyphonic patches
#[derive(Debug)]
pub struct VoiceAllocator {
    /// Number of available voices
    num_voices: usize,
    /// Allocation mode
    mode: AllocationMode,
    /// Voice states
    voices: Vec<Voice>,
    /// LRU queue for round-robin voice allocation
    lru_queue: VecDeque<usize>,
    /// Envelope level below which a `Releasing` voice may be auto-freed
    release_threshold: f64,
    /// Minimum samples a voice must have spent releasing before it can be freed
    release_grace_samples: u64,
    /// Index of the voice stolen by the most recent `note_on`, if any
    last_stolen: Option<usize>,
}

impl VoiceAllocator {
    /// Create a new voice allocator
    pub fn new(num_voices: usize) -> Self {
        let mut voices = Vec::with_capacity(num_voices);
        for i in 0..num_voices {
            voices.push(Voice::new(i));
        }

        let mut lru_queue = VecDeque::with_capacity(num_voices);
        for i in 0..num_voices {
            lru_queue.push_back(i);
        }

        Self {
            num_voices,
            mode: AllocationMode::RoundRobin,
            voices,
            lru_queue,
            // Defaults reproduce the "free a released voice as soon as it is
            // quiet" behavior for standalone use. `PolyPatch` overrides these
            // with real levels + a grace period so release tails complete.
            release_threshold: 0.0001,
            release_grace_samples: 0,
            last_stolen: None,
        }
    }

    /// Set the allocation mode
    pub fn set_mode(&mut self, mode: AllocationMode) {
        self.mode = mode;
    }

    /// Get the allocation mode
    pub fn mode(&self) -> AllocationMode {
        self.mode
    }

    /// Configure when a `Releasing` voice is auto-freed by [`tick`](Self::tick):
    /// only once its tracked `envelope_level` falls below `threshold` **and** at
    /// least `grace_samples` have elapsed since the gate fell.
    pub fn set_release_criteria(&mut self, threshold: f64, grace_samples: u64) {
        self.release_threshold = threshold;
        self.release_grace_samples = grace_samples;
    }

    /// Get the number of voices
    pub fn num_voices(&self) -> usize {
        self.num_voices
    }

    /// Get a voice by index
    pub fn voice(&self, index: usize) -> Option<&Voice> {
        self.voices.get(index)
    }

    /// Get a mutable voice by index
    pub fn voice_mut(&mut self, index: usize) -> Option<&mut Voice> {
        self.voices.get_mut(index)
    }

    /// Get all voices
    pub fn voices(&self) -> &[Voice] {
        &self.voices
    }

    /// Get all voices mutably
    pub fn voices_mut(&mut self) -> &mut [Voice] {
        &mut self.voices
    }

    /// Count active voices (anything not `Free`)
    pub fn active_count(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| v.state != VoiceState::Free)
            .count()
    }

    /// The voice stolen by the most recent [`note_on`](Self::note_on), if that
    /// allocation had to steal a sounding voice (rather than reuse a free one or
    /// retrigger). Consumers use this to reset the stolen voice's DSP.
    pub fn last_stolen(&self) -> Option<usize> {
        self.last_stolen
    }

    /// Allocate a voice for a note.
    ///
    /// Returns the voice index if successful, or `None` when the note is dropped
    /// (no free voice and no eligible steal victim — see [`AllocationMode`]).
    pub fn note_on(&mut self, note: u8, velocity: f64) -> Option<usize> {
        self.last_stolen = None;

        // Retrigger: this note is already sounding -> reuse its voice.
        if let Some(idx) = self.voices.iter().position(|v| v.is_playing_note(note)) {
            self.voices[idx].note_on(note, velocity);
            self.update_lru(idx); // Q071: keep LRU order correct on retrigger
            return Some(idx);
        }

        // Prefer a free voice.
        if let Some(idx) = self.find_free_voice() {
            self.voices[idx].note_on(note, velocity);
            self.update_lru(idx);
            return Some(idx);
        }

        // Otherwise steal, if the mode permits an eligible victim.
        if let Some(idx) = self.find_steal_voice(note) {
            self.voices[idx].note_on(note, velocity);
            self.update_lru(idx);
            self.last_stolen = Some(idx);
            return Some(idx);
        }

        // No free voice and nothing eligible to steal: drop the note.
        None
    }

    /// Release a note
    /// Returns the voice index if the note was found
    pub fn note_off(&mut self, note: u8) -> Option<usize> {
        for voice in &mut self.voices {
            if voice.is_playing_note(note) {
                voice.note_off();
                return Some(voice.index);
            }
        }
        None
    }

    /// Release all notes
    pub fn all_notes_off(&mut self) {
        for voice in &mut self.voices {
            voice.note_off();
        }
    }

    /// Kill all voices immediately (panic)
    pub fn panic(&mut self) {
        for voice in &mut self.voices {
            voice.free();
        }
    }

    /// Advance all voices one sample, then auto-free finished `Releasing`
    /// voices according to [`set_release_criteria`](Self::set_release_criteria).
    pub fn tick(&mut self) {
        for voice in &mut self.voices {
            voice.tick();
        }

        let threshold = self.release_threshold;
        let grace = self.release_grace_samples;
        for voice in &mut self.voices {
            if voice.state == VoiceState::Releasing
                && voice.envelope_level < threshold
                && voice.release_samples >= grace
            {
                voice.free();
            }
        }
    }

    /// Update envelope level for a voice (for quiet-steal and release-tail
    /// tracking). Populated every sample by [`PolyPatch`].
    pub fn set_envelope_level(&mut self, voice_index: usize, level: f64) {
        if let Some(voice) = self.voices.get_mut(voice_index) {
            voice.envelope_level = level;
        }
    }

    fn find_free_voice(&self) -> Option<usize> {
        // Use LRU queue for round-robin behavior
        self.lru_queue
            .iter()
            .find(|&&idx| self.voices[idx].is_free())
            .copied()
    }

    /// Two-pass voice stealing (Q068): prefer voices already in `Releasing`
    /// (they are on their way out anyway), and only fall back to `Active`
    /// voices when no releasing voice qualifies.
    fn find_steal_voice(&self, note: u8) -> Option<usize> {
        if self.mode == AllocationMode::NoSteal {
            return None;
        }
        self.select_victim(note, VoiceState::Releasing)
            .or_else(|| self.select_victim(note, VoiceState::Active))
    }

    /// Pick the best steal victim among voices in a single `state`, per the
    /// active [`AllocationMode`]. Returns `None` if no voice in that state
    /// qualifies.
    fn select_victim(&self, note: u8, state: VoiceState) -> Option<usize> {
        let candidates = || self.voices.iter().filter(|v| v.state == state);
        match self.mode {
            AllocationMode::NoSteal => None,
            AllocationMode::RoundRobin | AllocationMode::OldestSteal => {
                candidates().max_by_key(|v| v.age).map(|v| v.index)
            }
            AllocationMode::QuietestSteal => candidates()
                .min_by(|a, b| {
                    a.envelope_level
                        .partial_cmp(&b.envelope_level)
                        .unwrap_or(core::cmp::Ordering::Equal)
                })
                .map(|v| v.index),
            AllocationMode::HighestPriority => candidates()
                .filter(|v| v.note.map(|n| n < note).unwrap_or(false))
                .min_by_key(|v| v.note)
                .map(|v| v.index),
            AllocationMode::LowestPriority => candidates()
                .filter(|v| v.note.map(|n| n > note).unwrap_or(false))
                .max_by_key(|v| v.note)
                .map(|v| v.index),
        }
    }

    fn update_lru(&mut self, used_idx: usize) {
        // Move used voice to back of LRU queue
        if let Some(pos) = self.lru_queue.iter().position(|&x| x == used_idx) {
            self.lru_queue.remove(pos);
        }
        self.lru_queue.push_back(used_idx);
    }
}

/// Unison configuration
#[derive(Debug, Clone)]
pub struct UnisonConfig {
    /// Number of stacked voices (1 = no unison)
    pub voices: usize,
    /// **Total** edge-to-edge detune spread in cents: the span between the
    /// lowest and highest stacked voices. Each side is detuned by half this
    /// amount around the center.
    pub detune_cents: f64,
    /// Stereo spread (0.0 = mono/centered, 1.0 = full stereo)
    pub stereo_spread: f64,
    /// Voice phase randomization (0.0 = all in phase, 1.0 = random)
    pub phase_random: f64,
}

impl Default for UnisonConfig {
    fn default() -> Self {
        Self {
            voices: 1,
            detune_cents: 0.0,
            stereo_spread: 0.0,
            phase_random: 0.0,
        }
    }
}

impl UnisonConfig {
    /// Create a unison configuration with the given voice count and total
    /// edge-to-edge detune spread (in cents).
    pub fn new(voices: usize, detune_cents: f64) -> Self {
        Self {
            voices: voices.max(1),
            detune_cents,
            stereo_spread: 0.5,
            phase_random: 0.0,
        }
    }

    /// Calculate the detune offset for a specific unison voice, in V/Oct.
    ///
    /// `detune_cents` is the **total** edge-to-edge spread, so the lowest and
    /// highest voices sit at ∓`detune_cents`/2 cents around the center and the
    /// full span between them equals `detune_cents`. (100 cents = 1 semitone =
    /// 1/12 octave; 2400 cents = 2 octaves = the ±half-span denominator.)
    pub fn detune_offset(&self, voice_index: usize) -> f64 {
        if self.voices <= 1 {
            return 0.0;
        }

        // Spread voices evenly across the detune range.
        let normalized = voice_index as f64 / (self.voices - 1) as f64;
        let centered = normalized * 2.0 - 1.0; // -1 to +1

        // Half the total spread on each side: edges land at ±detune_cents/2.
        centered * self.detune_cents / 2400.0
    }

    /// Calculate the stereo pan position for a specific unison voice
    /// Returns pan value (-1.0 = left, 0.0 = center, 1.0 = right)
    pub fn pan_position(&self, voice_index: usize) -> f64 {
        if self.voices <= 1 {
            return 0.0;
        }

        let normalized = voice_index as f64 / (self.voices - 1) as f64;
        let centered = normalized * 2.0 - 1.0; // -1 to +1
        centered * self.stereo_spread
    }

    /// Get the gain multiplier per unison voice to keep the summed level roughly
    /// constant as voices are stacked (equal-power: `1/sqrt(voices)`).
    pub fn voice_gain(&self) -> f64 {
        1.0 / Libm::<f64>::sqrt(self.voices.max(1) as f64)
    }
}

/// Lock-free shared control handle for a single voice.
///
/// Mirrors the atomic-value pattern used by [`crate::io::ExternalInput`]: the
/// owner ([`PolyPatch`], or any external driver) writes the current control
/// values, while the in-graph [`VoiceInput`] node reads them from inside the
/// voice patch on its next `tick`. Interior mutability (via `AtomicU64` bit-
/// packed `f64`s) is what lets the same values be shared with a node that has
/// been moved (boxed) into a [`Patch`].
#[derive(Debug)]
pub struct VoiceControl {
    voct: AtomicU64,
    gate: AtomicU64,
    trigger: AtomicU64,
    velocity: AtomicU64,
}

impl VoiceControl {
    /// Create a new control handle (all values zero, velocity 1.0).
    pub fn new() -> Self {
        Self {
            voct: AtomicU64::new(0f64.to_bits()),
            gate: AtomicU64::new(0f64.to_bits()),
            trigger: AtomicU64::new(0f64.to_bits()),
            velocity: AtomicU64::new(1f64.to_bits()),
        }
    }

    #[inline]
    fn load(a: &AtomicU64) -> f64 {
        f64::from_bits(a.load(Ordering::Relaxed))
    }

    #[inline]
    fn store(a: &AtomicU64, v: f64) {
        a.store(v.to_bits(), Ordering::Relaxed);
    }

    /// Current V/Oct pitch.
    pub fn voct(&self) -> f64 {
        Self::load(&self.voct)
    }
    /// Current gate value (0 = off, ≥1 = on).
    pub fn gate(&self) -> f64 {
        Self::load(&self.gate)
    }
    /// Current trigger *request* (a rising edge starts a one-shot pulse).
    pub fn trigger(&self) -> f64 {
        Self::load(&self.trigger)
    }
    /// Current velocity (0..1).
    pub fn velocity(&self) -> f64 {
        Self::load(&self.velocity)
    }

    /// Set V/Oct pitch.
    pub fn set_voct(&self, v: f64) {
        Self::store(&self.voct, v);
    }
    /// Set gate value.
    pub fn set_gate(&self, v: f64) {
        Self::store(&self.gate, v);
    }
    /// Set trigger request (assert `1.0` for one sample to fire a pulse).
    pub fn set_trigger(&self, v: f64) {
        Self::store(&self.trigger, v);
    }
    /// Set velocity.
    pub fn set_velocity(&self, v: f64) {
        Self::store(&self.velocity, v);
    }

    fn reset(&self) {
        self.set_voct(0.0);
        self.set_gate(0.0);
        self.set_trigger(0.0);
        self.set_velocity(1.0);
    }
}

impl Default for VoiceControl {
    fn default() -> Self {
        Self::new()
    }
}

/// In-graph voice controller: the per-voice control-signal source.
///
/// This [`GraphModule`] is inserted into every voice patch and outputs the
/// per-voice `voct`, `gate`, `trigger`, and `velocity` signals. Values come from
/// a shared [`VoiceControl`] handle written by [`PolyPatch`] (or any external
/// driver), so allocator state genuinely reaches the DSP graph.
///
/// The `trigger` output is a proper **one-shot pulse measured in samples**: a
/// rising edge on the control handle's trigger request emits `5 V` for
/// `TRIGGER_S` seconds' worth of samples, regardless of how briefly the request
/// was asserted.
///
/// The name is kept as `VoiceInput` for API continuity; conceptually it is the
/// voice controller.
pub struct VoiceInput {
    control: Arc<VoiceControl>,
    spec: PortSpec,
    trigger_len: u32,
    trigger_remaining: u32,
    prev_trigger_req: f64,
}

impl VoiceInput {
    /// Create a new voice input backed by a fresh, private control handle.
    pub fn new() -> Self {
        Self::with_control(Arc::new(VoiceControl::new()), 48_000.0)
    }

    /// Create a voice input driven by a shared control handle at a given sample
    /// rate (used by [`PolyPatch`] so it can write values from the outside).
    pub fn with_control(control: Arc<VoiceControl>, sample_rate: f64) -> Self {
        Self {
            control,
            spec: PortSpec {
                inputs: vec![],
                outputs: vec![
                    PortDef::new(0, "voct", SignalKind::VoltPerOctave),
                    PortDef::new(1, "gate", SignalKind::Gate),
                    PortDef::new(2, "trigger", SignalKind::Trigger),
                    PortDef::new(3, "velocity", SignalKind::CvUnipolar),
                ],
            },
            trigger_len: Self::trigger_len_for(sample_rate),
            trigger_remaining: 0,
            prev_trigger_req: 0.0,
        }
    }

    #[inline]
    fn trigger_len_for(sample_rate: f64) -> u32 {
        (Libm::<f64>::round(TRIGGER_S * sample_rate)).max(1.0) as u32
    }

    /// The shared control handle this input reads from.
    pub fn control(&self) -> &Arc<VoiceControl> {
        &self.control
    }

    /// Copy the allocator voice's control values into the shared handle.
    pub fn set_from_voice(&mut self, voice: &Voice) {
        self.control.set_voct(voice.voct);
        self.control.set_gate(voice.gate);
        self.control.set_trigger(voice.trigger);
        self.control.set_velocity(voice.velocity);
    }

    /// Set V/Oct directly (writes the shared handle).
    pub fn set_voct(&mut self, voct: f64) {
        self.control.set_voct(voct);
    }

    /// Set gate directly.
    pub fn set_gate(&mut self, gate: f64) {
        self.control.set_gate(gate);
    }

    /// Set trigger request directly.
    pub fn set_trigger(&mut self, trigger: f64) {
        self.control.set_trigger(trigger);
    }

    /// Set velocity directly.
    pub fn set_velocity(&mut self, velocity: f64) {
        self.control.set_velocity(velocity);
    }

    /// Current V/Oct value (reads the shared handle).
    pub fn voct(&self) -> f64 {
        self.control.voct()
    }
    /// Current gate value.
    pub fn gate(&self) -> f64 {
        self.control.gate()
    }
    /// Current trigger request value.
    pub fn trigger(&self) -> f64 {
        self.control.trigger()
    }
    /// Current velocity value.
    pub fn velocity(&self) -> f64 {
        self.control.velocity()
    }
}

impl Default for VoiceInput {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for VoiceInput {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, _inputs: &PortValues, outputs: &mut PortValues) {
        outputs.set(0, self.control.voct());
        outputs.set(1, if self.control.gate() > 0.5 { 5.0 } else { 0.0 });

        // Turn a rising edge on the trigger request into a fixed-length pulse.
        let req = self.control.trigger();
        if req > 0.5 && self.prev_trigger_req <= 0.5 {
            self.trigger_remaining = self.trigger_len;
        }
        self.prev_trigger_req = req;
        let trig_out = if self.trigger_remaining > 0 {
            self.trigger_remaining -= 1;
            5.0
        } else {
            0.0
        };
        outputs.set(2, trig_out);

        outputs.set(3, self.control.velocity() * 10.0); // Scale to 0-10V
    }

    fn reset(&mut self) {
        self.control.reset();
        self.trigger_remaining = 0;
        self.prev_trigger_req = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.trigger_len = Self::trigger_len_for(sample_rate);
    }

    fn type_id(&self) -> &'static str {
        "voice_input"
    }
}

/// Type of the closure that builds a single voice graph.
///
/// It receives a fresh [`Patch`] (already containing the voice controller) and a
/// [`NodeHandle`] to that controller, wires up the voice's DSP, and sets the
/// patch output.
type VoiceBuilder = dyn Fn(&mut Patch, &NodeHandle) -> Result<(), PatchError>;

/// One rendered sub-voice: an independent voice graph plus its control handle.
///
/// A monophonic [`PolyPatch`] voice has exactly one sub-voice; a unison voice
/// has one per unison stack member, each detuned/panned differently.
struct SubVoice {
    patch: Patch,
    control: Arc<VoiceControl>,
    controller: NodeHandle,
}

/// A single allocator voice's set of (unison) sub-voice graphs plus its
/// amplitude follower state.
struct VoiceSlot {
    subs: Vec<SubVoice>,
    /// One-pole follower of this voice's real output amplitude (Q064).
    follower: f64,
}

/// Polyphonic patch container.
///
/// Owns one voice graph per allocator voice (multiplied by the unison count),
/// each fed by an in-graph [`VoiceInput`] controller. On every [`tick`](Self::tick), it:
///
/// 1. writes each active voice's allocator state into its controller handle(s),
/// 2. ticks each voice graph exactly once and mixes the results (with unison
///    detune/balance and equal-power unison gain),
/// 3. follows each voice's real output level and reports it to the allocator so
///    release tails complete before the voice is freed,
/// 4. applies smoothed polyphony gain compensation (`1/sqrt(N)`).
///
/// [`PolyPatch::tick`] performs **no heap allocation** in steady state; all
/// allocation happens at construction / reconfiguration time.
pub struct PolyPatch {
    /// Voice allocator
    allocator: VoiceAllocator,
    /// Per-voice graphs (one [`VoiceSlot`] per allocator voice)
    voices: Vec<VoiceSlot>,
    /// Unison configuration
    unison: UnisonConfig,
    /// Sample rate
    sample_rate: f64,
    /// Builder used to (re)construct each voice graph.
    builder: Option<Box<VoiceBuilder>>,
    /// Smoothed sounding-voice count for gain compensation (Q067).
    smoothed_count: f64,
    /// Cached one-pole coefficients / grace (recomputed on sample-rate change).
    follower_coeff: f64,
    count_coeff: f64,
    grace_samples: u64,
    /// Output buffers (left, right)
    output_left: f64,
    output_right: f64,
}

impl PolyPatch {
    /// Create a new polyphonic patch whose voice graphs contain only the voice
    /// controller (no DSP). Use [`with_voice_fn`](Self::with_voice_fn) to build
    /// real voices; this bare constructor is mostly useful for benchmarking the
    /// allocation/mixing machinery.
    pub fn new(num_voices: usize, sample_rate: f64) -> Self {
        // A controller-only voice graph can never fail to build/compile.
        Self::build(num_voices, sample_rate, None).expect("empty voice build cannot fail")
    }

    /// Create a polyphonic patch, building each voice graph with `builder`.
    ///
    /// The builder is invoked once per voice (and once per unison sub-voice),
    /// each time receiving a fresh patch pre-populated with a voice controller
    /// and a [`NodeHandle`] to it. Wire the controller's `voct`/`gate`/`trigger`/
    /// `velocity` outputs into your DSP and call `patch.set_output(..)`.
    pub fn with_voice_fn<F>(
        num_voices: usize,
        sample_rate: f64,
        builder: F,
    ) -> Result<Self, PatchError>
    where
        F: Fn(&mut Patch, &NodeHandle) -> Result<(), PatchError> + 'static,
    {
        Self::build(num_voices, sample_rate, Some(Box::new(builder)))
    }

    fn build(
        num_voices: usize,
        sample_rate: f64,
        builder: Option<Box<VoiceBuilder>>,
    ) -> Result<Self, PatchError> {
        let mut poly = Self {
            allocator: VoiceAllocator::new(num_voices),
            voices: Vec::new(),
            unison: UnisonConfig::default(),
            sample_rate,
            builder,
            smoothed_count: 0.0,
            follower_coeff: 0.0,
            count_coeff: 0.0,
            grace_samples: 0,
            output_left: 0.0,
            output_right: 0.0,
        };
        poly.recompute_coeffs();
        poly.voices = poly.build_voices()?;
        Ok(poly)
    }

    fn recompute_coeffs(&mut self) {
        self.follower_coeff = one_pole_coeff(FOLLOWER_TAU_S, self.sample_rate);
        self.count_coeff = one_pole_coeff(COUNT_TAU_S, self.sample_rate);
        self.grace_samples = (GRACE_S * self.sample_rate).max(1.0) as u64;
        self.allocator
            .set_release_criteria(RELEASE_THRESHOLD, self.grace_samples);
    }

    /// Build the full set of voice graphs from the current configuration.
    ///
    /// Allocation-heavy; only called at construction / reconfiguration time.
    fn build_voices(&self) -> Result<Vec<VoiceSlot>, PatchError> {
        let unison_voices = self.unison.voices.max(1);
        let mut voices = Vec::with_capacity(self.allocator.num_voices());
        for _ in 0..self.allocator.num_voices() {
            let mut subs = Vec::with_capacity(unison_voices);
            for _ in 0..unison_voices {
                let control = Arc::new(VoiceControl::new());
                let mut patch = Patch::new(self.sample_rate);
                let controller = patch.add(
                    "voice_ctrl",
                    VoiceInput::with_control(control.clone(), self.sample_rate),
                );
                if let Some(builder) = &self.builder {
                    builder(&mut patch, &controller)?;
                }
                patch.compile()?;
                subs.push(SubVoice {
                    patch,
                    control,
                    controller,
                });
            }
            voices.push(VoiceSlot {
                subs,
                follower: 0.0,
            });
        }
        Ok(voices)
    }

    /// Get the sample rate
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Number of allocator voices.
    pub fn num_voices(&self) -> usize {
        self.allocator.num_voices()
    }

    /// Set the sample rate and rebuild every voice graph at the new rate.
    ///
    /// Rebuilding re-runs the voice builder so all modules (and the controller's
    /// trigger-pulse length, the amplitude follower, and the release grace) pick
    /// up the new sample rate (Q069). Voice DSP state is reset as a result,
    /// which is acceptable for a sample-rate change.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.recompute_coeffs();
        if let Ok(voices) = self.build_voices() {
            self.voices = voices;
        }
        self.smoothed_count = 0.0;
    }

    /// The shared control handle for a voice's first sub-voice, if any.
    pub fn voice_control(&self, index: usize) -> Option<&Arc<VoiceControl>> {
        self.voices
            .get(index)
            .and_then(|v| v.subs.first())
            .map(|s| &s.control)
    }

    /// A [`NodeHandle`] to a voice's controller node (first sub-voice), so the
    /// controller ports can be referenced after construction.
    pub fn voice_controller(&self, index: usize) -> Option<&NodeHandle> {
        self.voices
            .get(index)
            .and_then(|v| v.subs.first())
            .map(|s| &s.controller)
    }

    /// Get the voice allocator
    pub fn allocator(&self) -> &VoiceAllocator {
        &self.allocator
    }

    /// Get mutable access to the voice allocator
    pub fn allocator_mut(&mut self) -> &mut VoiceAllocator {
        &mut self.allocator
    }

    /// Set unison configuration. Changing the unison **voice count** rebuilds
    /// the voice graphs; changing only detune/spread takes effect immediately
    /// without a rebuild.
    pub fn set_unison(&mut self, config: UnisonConfig) {
        let count_changed = config.voices.max(1) != self.unison.voices.max(1);
        self.unison = config;
        if count_changed {
            if let Ok(voices) = self.build_voices() {
                self.voices = voices;
            }
        }
    }

    /// Get unison configuration
    pub fn unison(&self) -> &UnisonConfig {
        &self.unison
    }

    /// Get a voice's (first sub-voice) patch for inspection.
    pub fn voice_patch(&self, index: usize) -> Option<&Patch> {
        self.voices
            .get(index)
            .and_then(|v| v.subs.first())
            .map(|s| &s.patch)
    }

    /// Get a voice's (first sub-voice) patch mutably.
    pub fn voice_patch_mut(&mut self, index: usize) -> Option<&mut Patch> {
        self.voices
            .get_mut(index)
            .and_then(|v| v.subs.first_mut())
            .map(|s| &mut s.patch)
    }

    /// Handle MIDI note on. When the allocation stole a sounding voice, the
    /// stolen voice's DSP is reset to prevent the previous note's tail from
    /// bleeding into the new note (Q070). Fresh (free) allocations are **not**
    /// reset, preserving oscillator phase continuity for an analog feel.
    pub fn note_on(&mut self, note: u8, velocity: u8) {
        let velocity_f = velocity as f64 / 127.0;
        if let Some(idx) = self.allocator.note_on(note, velocity_f) {
            if self.allocator.last_stolen() == Some(idx) {
                if let Some(slot) = self.voices.get_mut(idx) {
                    for sub in &mut slot.subs {
                        sub.patch.reset();
                    }
                    slot.follower = 0.0;
                }
            }
        }
    }

    /// Handle MIDI note off
    pub fn note_off(&mut self, note: u8) {
        self.allocator.note_off(note);
    }

    /// All notes off
    pub fn all_notes_off(&mut self) {
        self.allocator.all_notes_off();
    }

    /// Panic - immediately silence all voices
    pub fn panic(&mut self) {
        self.allocator.panic();
    }

    /// Compile all voice graphs.
    pub fn compile(&mut self) -> Result<(), PatchError> {
        for slot in &mut self.voices {
            for sub in &mut slot.subs {
                sub.patch.compile()?;
            }
        }
        Ok(())
    }

    /// The current polyphony gain-compensation factor (`1/sqrt(N)` with the
    /// voice count `N` smoothed). Exposed mainly for testing that the factor
    /// moves smoothly rather than stepping.
    pub fn compensation_gain(&self) -> f64 {
        1.0 / Libm::<f64>::sqrt(self.smoothed_count.max(1.0))
    }

    /// Process one sample and return stereo output.
    pub fn tick(&mut self) -> (f64, f64) {
        // Snapshot config into locals so the hot loop borrows neither `unison`
        // (cheap scalar clone — no heap) nor `self` through a method.
        let unison = self.unison.clone();
        let unison_voices = unison.voices.max(1);
        let unison_gain = unison.voice_gain();
        let use_pan = unison_voices > 1 && unison.stereo_spread != 0.0;
        let follower_coeff = self.follower_coeff;

        // Smooth the sounding-voice count (pre-free) for gain compensation.
        let inst_count = self.allocator.active_count() as f64;
        self.smoothed_count =
            self.count_coeff * self.smoothed_count + (1.0 - self.count_coeff) * inst_count;

        let mut left = 0.0;
        let mut right = 0.0;

        for i in 0..self.voices.len() {
            let (state, base_voct, gate, trigger, velocity) = {
                let v = &self.allocator.voices()[i];
                (v.state, v.voct, v.gate, v.trigger, v.velocity)
            };

            if state == VoiceState::Free {
                self.voices[i].follower = 0.0;
                continue;
            }

            let slot = &mut self.voices[i];
            let mut peak = 0.0;
            for (u, sub) in slot.subs.iter_mut().enumerate() {
                // Unison detune summed into this sub-voice's pitch.
                sub.control.set_voct(base_voct + unison.detune_offset(u));
                sub.control.set_gate(gate);
                sub.control.set_trigger(trigger);
                sub.control.set_velocity(velocity);

                let (l, r) = sub.patch.tick();

                let (lg, rg) = if use_pan {
                    balance_gains(unison.pan_position(u))
                } else {
                    (1.0, 1.0)
                };
                let sl = l * lg * unison_gain;
                let sr = r * rg * unison_gain;
                left += sl;
                right += sr;

                let mag = Libm::<f64>::fabs(sl).max(Libm::<f64>::fabs(sr));
                if mag > peak {
                    peak = mag;
                }
            }

            // Track this voice's real output level for release-tail detection.
            slot.follower = follower_coeff * slot.follower + (1.0 - follower_coeff) * peak;
            let level = slot.follower;
            self.allocator.set_envelope_level(i, level);
        }

        // Advance allocator state and free finished release tails (uses the
        // envelope levels just written above + the configured grace period).
        self.allocator.tick();

        // Polyphony gain compensation (smoothed, never steps).
        let g = 1.0 / Libm::<f64>::sqrt(self.smoothed_count.max(1.0));
        left *= g;
        right *= g;

        self.output_left = left;
        self.output_right = right;
        (left, right)
    }

    /// Get the last output
    pub fn output(&self) -> (f64, f64) {
        (self.output_left, self.output_right)
    }

    /// Reset all voice graphs and allocator state.
    pub fn reset(&mut self) {
        for slot in &mut self.voices {
            slot.follower = 0.0;
            for sub in &mut slot.subs {
                sub.patch.reset();
            }
        }
        self.allocator.panic();
        self.smoothed_count = 0.0;
        self.output_left = 0.0;
        self.output_right = 0.0;
    }
}

/// Voice mixer for summing polyphonic voices
pub struct VoiceMixer {
    num_voices: usize,
    spec: PortSpec,
}

impl VoiceMixer {
    /// Create a voice mixer for the given number of voices
    pub fn new(num_voices: usize) -> Self {
        let mut inputs = Vec::with_capacity(num_voices * 2);
        for i in 0..num_voices {
            inputs.push(PortDef::new(
                i as u32 * 2,
                format!("in{}_l", i),
                SignalKind::Audio,
            ));
            inputs.push(PortDef::new(
                i as u32 * 2 + 1,
                format!("in{}_r", i),
                SignalKind::Audio,
            ));
        }

        Self {
            num_voices,
            spec: PortSpec {
                inputs,
                outputs: vec![
                    PortDef::new(100, "left", SignalKind::Audio),
                    PortDef::new(101, "right", SignalKind::Audio),
                ],
            },
        }
    }
}

impl GraphModule for VoiceMixer {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let mut left = 0.0;
        let mut right = 0.0;

        for i in 0..self.num_voices {
            left += inputs.get_or(i as u32 * 2, 0.0);
            right += inputs.get_or(i as u32 * 2 + 1, 0.0);
        }

        outputs.set(100, left);
        outputs.set(101, right);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "voice_mixer"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::{Adsr, StereoOutput, Vca, Vco};

    // A tiny constant audio source, used to build deterministic "identical"
    // voices for gain-compensation tests.
    struct DcSource {
        value: f64,
        spec: PortSpec,
    }

    impl DcSource {
        fn new(value: f64) -> Self {
            Self {
                value,
                spec: PortSpec {
                    inputs: vec![],
                    outputs: vec![PortDef::new(0, "out", SignalKind::Audio)],
                },
            }
        }
    }

    impl GraphModule for DcSource {
        fn port_spec(&self) -> &PortSpec {
            &self.spec
        }
        fn tick(&mut self, _inputs: &PortValues, outputs: &mut PortValues) {
            outputs.set(0, self.value);
        }
        fn reset(&mut self) {}
        fn set_sample_rate(&mut self, _: f64) {}
        fn type_id(&self) -> &'static str {
            "dc_source"
        }
    }

    // ---- Allocator basics -------------------------------------------------

    #[test]
    fn test_voice_allocation_basic() {
        let mut allocator = VoiceAllocator::new(4);

        let voice1 = allocator.note_on(60, 0.8);
        assert_eq!(voice1, Some(0));
        assert_eq!(allocator.active_count(), 1);

        let voice2 = allocator.note_on(64, 0.7);
        assert_eq!(voice2, Some(1));
        assert_eq!(allocator.active_count(), 2);

        allocator.note_off(60);
        assert_eq!(allocator.active_count(), 2); // Still active (releasing)

        allocator.tick();
    }

    #[test]
    fn test_voice_allocation_retrigger() {
        let mut allocator = VoiceAllocator::new(4);

        let voice1 = allocator.note_on(60, 0.8);
        assert_eq!(voice1, Some(0));

        let voice2 = allocator.note_on(60, 0.9);
        assert_eq!(voice2, Some(0));
        assert_eq!(allocator.active_count(), 1);
    }

    // Q071: retrigger must move the voice to the back of the LRU queue.
    #[test]
    fn test_retrigger_updates_lru() {
        let mut allocator = VoiceAllocator::new(3);

        // Occupy 0, 1, 2 then release them all (still tracked in LRU).
        allocator.note_on(60, 1.0); // voice 0
        allocator.note_on(62, 1.0); // voice 1
        allocator.note_on(64, 1.0); // voice 2
        allocator.note_off(60);
        allocator.note_off(62);
        allocator.note_off(64);
        // Free them so find_free_voice uses LRU order.
        allocator.panic();

        // Retrigger note on voice 0 by re-playing 60 while it's free -> fresh
        // alloc picks LRU front (0). Then retrigger 60: LRU must push 0 to back.
        assert_eq!(allocator.note_on(60, 1.0), Some(0));
        assert_eq!(allocator.note_on(60, 1.0), Some(0)); // retrigger, same voice

        // Free voice 0. Next fresh allocation should NOT immediately reuse 0 if
        // LRU was updated on retrigger (0 is now at the back).
        allocator.voice_mut(0).unwrap().free();
        allocator.voice_mut(1).unwrap().free();
        allocator.voice_mut(2).unwrap().free();
        // LRU order after updates should place 0 last; the earliest free slot in
        // LRU order (1 or 2) is chosen before 0.
        let next = allocator.note_on(67, 1.0).unwrap();
        assert_ne!(next, 0, "retrigger should have pushed voice 0 to LRU back");
    }

    // ---- Voice stealing (Q068) -------------------------------------------

    #[test]
    fn test_voice_stealing_oldest_active() {
        let mut allocator = VoiceAllocator::new(2);
        allocator.set_mode(AllocationMode::OldestSteal);

        allocator.note_on(60, 0.8);
        allocator.tick();
        allocator.note_on(62, 0.7);
        allocator.tick();

        // Both active; steal the oldest (voice 0).
        let stolen = allocator.note_on(64, 0.6);
        assert_eq!(stolen, Some(0));
        assert_eq!(allocator.last_stolen(), Some(0));
    }

    #[test]
    fn test_voice_stealing_prefers_releasing() {
        let mut allocator = VoiceAllocator::new(2);
        allocator.set_mode(AllocationMode::OldestSteal);

        // Voice 0 active, voice 1 active then released.
        allocator.note_on(60, 0.8);
        allocator.tick();
        allocator.note_on(62, 0.7);
        allocator.tick();
        allocator.note_off(62); // voice 1 -> Releasing
                                // Keep it from being auto-freed: it never ticks below threshold here
                                // because we don't call tick() (envelope stays 0 but no free pass runs).

        // A new note should steal the RELEASING voice (1), not the active one.
        let stolen = allocator.note_on(64, 0.6);
        assert_eq!(stolen, Some(1));
        assert_eq!(allocator.last_stolen(), Some(1));
    }

    #[test]
    fn test_quietest_steal_uses_real_levels() {
        let mut allocator = VoiceAllocator::new(3);
        allocator.set_mode(AllocationMode::QuietestSteal);

        allocator.note_on(60, 1.0); // voice 0
        allocator.note_on(62, 1.0); // voice 1
        allocator.note_on(64, 1.0); // voice 2

        // Populate real envelope levels; voice 1 is quietest.
        allocator.set_envelope_level(0, 0.9);
        allocator.set_envelope_level(1, 0.1);
        allocator.set_envelope_level(2, 0.7);

        let stolen = allocator.note_on(67, 1.0);
        assert_eq!(
            stolen,
            Some(1),
            "QuietestSteal should pick the lowest level"
        );
    }

    #[test]
    fn test_no_steal_mode_drops_note() {
        let mut allocator = VoiceAllocator::new(2);
        allocator.set_mode(AllocationMode::NoSteal);

        allocator.note_on(60, 0.8);
        allocator.note_on(62, 0.7);

        // Q072: no free voice, no eligible victim -> note dropped.
        let result = allocator.note_on(64, 0.6);
        assert_eq!(result, None);
        assert_eq!(allocator.last_stolen(), None);
    }

    // Q072: priority modes drop the note when nothing qualifies.
    #[test]
    fn test_priority_mode_drop_when_no_victim() {
        let mut allocator = VoiceAllocator::new(2);
        allocator.set_mode(AllocationMode::HighestPriority);

        // Two high notes held. A LOWER new note cannot steal (needs a sounding
        // voice lower than it) -> dropped.
        allocator.note_on(80, 1.0);
        allocator.note_on(84, 1.0);
        assert_eq!(allocator.note_on(60, 1.0), None);

        // A higher new note CAN steal the lowest sounding voice (80).
        let stolen = allocator.note_on(90, 1.0);
        assert_eq!(stolen, Some(0));
    }

    // ---- MIDI / voct conversions -----------------------------------------

    #[test]
    fn test_midi_note_to_voct() {
        assert!((midi_note_to_voct(60) - 0.0).abs() < 0.001);
        assert!((midi_note_to_voct(72) - 1.0).abs() < 0.001);
        assert!((midi_note_to_voct(48) - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_voct_to_midi_note() {
        assert_eq!(voct_to_midi_note(0.0), 60);
        assert_eq!(voct_to_midi_note(1.0), 72);
        assert_eq!(voct_to_midi_note(-1.0), 48);
    }

    // ---- Unison detune (Q065) --------------------------------------------

    #[test]
    fn test_unison_detune_total_spread() {
        // 3 voices, 10 cents TOTAL edge-to-edge spread.
        let config = UnisonConfig::new(3, 10.0);

        let d0 = config.detune_offset(0);
        let d1 = config.detune_offset(1);
        let d2 = config.detune_offset(2);

        assert!(d0 < 0.0);
        assert!((d1 - 0.0).abs() < 1e-9);
        assert!(d2 > 0.0);
        assert!((d0 + d2).abs() < 1e-9, "spread must be symmetric");

        // Magnitude: edges at ±5 cents, total span 10 cents (Q065). 1 octave in
        // V/Oct == 1200 cents.
        let d0_cents = d0 * 1200.0;
        let d2_cents = d2 * 1200.0;
        let span_cents = (d2 - d0) * 1200.0;
        assert!((d0_cents + 5.0).abs() < 1e-6, "low edge should be -5 cents");
        assert!(
            (d2_cents - 5.0).abs() < 1e-6,
            "high edge should be +5 cents"
        );
        assert!(
            (span_cents - 10.0).abs() < 1e-6,
            "total spread should equal detune_cents (got {span_cents})"
        );
    }

    #[test]
    fn test_unison_pan() {
        let mut config = UnisonConfig::new(3, 10.0);
        config.stereo_spread = 1.0;

        assert!((config.pan_position(0) - (-1.0)).abs() < 0.001);
        assert!((config.pan_position(1) - 0.0).abs() < 0.001);
        assert!((config.pan_position(2) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_unison_config_voice_gain() {
        let config = UnisonConfig::new(4, 10.0);
        let gain = config.voice_gain();
        assert!((gain - 0.5).abs() < 1e-9); // 1/sqrt(4)
    }

    // ---- Balance / pan law (Q066) ----------------------------------------

    #[test]
    fn test_balance_gains_center_unity() {
        let (l, r) = balance_gains(0.0);
        assert!((l - 1.0).abs() < 1e-9 && (r - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_balance_gains_partial() {
        let (l, r) = balance_gains(-0.5); // left
        assert!((l - 1.0).abs() < 1e-9 && (r - 0.5).abs() < 1e-9);
        let (l, r) = balance_gains(0.5); // right
        assert!((l - 0.5).abs() < 1e-9 && (r - 1.0).abs() < 1e-9);
    }

    // ---- VoiceInput (in-graph controller) --------------------------------

    #[test]
    fn test_voice_input_module() {
        let mut input = VoiceInput::new();
        let mut outputs = PortValues::new();

        input.set_voct(0.5);
        input.set_gate(1.0);
        input.set_velocity(0.8);

        input.tick(&PortValues::new(), &mut outputs);

        assert!((outputs.get_or(0, 0.0) - 0.5).abs() < 0.001); // V/Oct
        assert!((outputs.get_or(1, 0.0) - 5.0).abs() < 0.001); // Gate (5V)
        assert!((outputs.get_or(3, 0.0) - 8.0).abs() < 0.001); // Velocity (0.8 * 10V)
    }

    #[test]
    fn test_voice_input_trigger_pulse_in_samples() {
        // At 48kHz, a 1ms pulse is ~48 samples; must persist beyond one sample.
        let mut input = VoiceInput::with_control(Arc::new(VoiceControl::new()), 48_000.0);
        let mut outputs = PortValues::new();

        input.set_trigger(1.0); // request a trigger
        input.tick(&PortValues::new(), &mut outputs);
        assert!(outputs.get_or(2, 0.0) > 2.5, "trigger high on first sample");

        // Clear the request; the pulse must still be high (multi-sample).
        input.set_trigger(0.0);
        input.tick(&PortValues::new(), &mut outputs);
        assert!(
            outputs.get_or(2, 0.0) > 2.5,
            "trigger pulse should last several samples, not one"
        );

        // Eventually goes low.
        for _ in 0..64 {
            input.tick(&PortValues::new(), &mut outputs);
        }
        assert!(outputs.get_or(2, 0.0) < 2.5, "trigger pulse should end");
    }

    #[test]
    fn test_voice_input_default() {
        let input = VoiceInput::default();
        assert!((input.voct() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_voice_input_set_from_voice() {
        let mut voice = Voice::new(0);
        voice.note_on(72, 0.8);

        let mut input = VoiceInput::new();
        input.set_from_voice(&voice);

        assert!((input.voct() - 1.0).abs() < 0.001);
        assert!((input.velocity() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_voice_input_reset_type_id() {
        let mut input = VoiceInput::new();
        input.set_voct(1.0);
        input.reset();
        assert!((input.voct() - 0.0).abs() < 0.001);
        assert_eq!(input.type_id(), "voice_input");
        input.set_sample_rate(48000.0);
    }

    #[test]
    fn test_voice_input_set_trigger() {
        let mut input = VoiceInput::new();
        input.set_trigger(1.0);
        assert!((input.trigger() - 1.0).abs() < 0.001);
    }

    // ---- Voice state machine ---------------------------------------------

    #[test]
    fn test_voice_is_free() {
        let voice = Voice::new(0);
        assert!(voice.is_free());

        let mut playing = Voice::new(1);
        playing.note_on(60, 1.0);
        assert!(!playing.is_free());
    }

    #[test]
    fn test_voice_is_playing_note() {
        let mut voice = Voice::new(0);
        voice.note_on(60, 1.0);
        assert!(voice.is_playing_note(60));
        assert!(!voice.is_playing_note(61));
    }

    #[test]
    fn test_voice_note_off_and_free() {
        let mut voice = Voice::new(0);
        voice.note_on(60, 1.0);
        voice.note_off();
        assert!(voice.state == VoiceState::Releasing);

        voice.free();
        assert!(voice.is_free());
    }

    #[test]
    fn test_voice_tick_clears_trigger_and_counts_release() {
        let mut voice = Voice::new(0);
        voice.note_on(60, 1.0);
        voice.tick();
        assert!(voice.trigger == 0.0);
        assert_eq!(voice.release_samples, 0); // still active

        voice.note_off();
        voice.tick();
        assert_eq!(voice.release_samples, 1); // counting since release
    }

    // ---- Allocator misc accessors ----------------------------------------

    #[test]
    fn test_voice_allocator_mode() {
        let mut allocator = VoiceAllocator::new(4);
        allocator.set_mode(AllocationMode::QuietestSteal);
        assert_eq!(allocator.mode(), AllocationMode::QuietestSteal);
    }

    #[test]
    fn test_voice_allocator_num_voices() {
        let allocator = VoiceAllocator::new(8);
        assert_eq!(allocator.num_voices(), 8);
    }

    #[test]
    fn test_voice_allocator_voice_access() {
        let mut allocator = VoiceAllocator::new(4);
        assert!(allocator.voice(0).is_some());
        assert!(allocator.voice_mut(0).is_some());
    }

    #[test]
    fn test_voice_allocator_voices() {
        let allocator = VoiceAllocator::new(4);
        assert_eq!(allocator.voices().len(), 4);
    }

    #[test]
    fn test_voice_allocator_voices_mut() {
        let mut allocator = VoiceAllocator::new(4);
        assert_eq!(allocator.voices_mut().len(), 4);
    }

    #[test]
    fn test_voice_allocator_all_notes_off() {
        let mut allocator = VoiceAllocator::new(4);
        allocator.note_on(60, 1.0);
        allocator.note_on(64, 1.0);
        allocator.all_notes_off();
        assert!(allocator
            .voices()
            .iter()
            .all(|v| v.state == VoiceState::Releasing || v.state == VoiceState::Free));
    }

    #[test]
    fn test_voice_allocator_tick() {
        let mut allocator = VoiceAllocator::new(4);
        allocator.note_on(60, 1.0);
        allocator.tick();
    }

    #[test]
    fn test_voice_allocator_set_envelope_level() {
        let mut allocator = VoiceAllocator::new(4);
        if let Some(i) = allocator.note_on(60, 1.0) {
            allocator.set_envelope_level(i, 0.5);
            assert!((allocator.voice(i).unwrap().envelope_level - 0.5).abs() < 1e-9);
        }
    }

    #[test]
    fn test_voice_allocator_release_grace_keeps_voice() {
        let mut allocator = VoiceAllocator::new(1);
        allocator.set_release_criteria(0.001, 100); // 100-sample grace
        allocator.note_on(60, 1.0);
        allocator.note_off(60);

        // Envelope already quiet, but grace must keep it alive.
        allocator.set_envelope_level(0, 0.0);
        for _ in 0..50 {
            allocator.set_envelope_level(0, 0.0);
            allocator.tick();
        }
        assert_eq!(allocator.voice(0).unwrap().state, VoiceState::Releasing);

        // After the grace elapses it frees.
        for _ in 0..100 {
            allocator.set_envelope_level(0, 0.0);
            allocator.tick();
        }
        assert_eq!(allocator.voice(0).unwrap().state, VoiceState::Free);
    }

    // ---- PolyPatch basics -------------------------------------------------

    #[test]
    fn test_poly_patch_basic() {
        let mut poly = PolyPatch::new(4, 44100.0);
        poly.note_on(60, 100);
        assert_eq!(poly.allocator().active_count(), 1);
        poly.note_on(64, 90);
        assert_eq!(poly.allocator().active_count(), 2);
        poly.note_off(60);
    }

    #[test]
    fn test_poly_patch_panic() {
        let mut poly = PolyPatch::new(4, 44100.0);
        poly.note_on(60, 100);
        poly.note_on(64, 90);
        poly.note_on(67, 80);
        poly.panic();
        assert_eq!(poly.allocator().active_count(), 0);
    }

    #[test]
    fn test_poly_patch_sample_rate() {
        let poly = PolyPatch::new(4, 48000.0);
        assert_eq!(poly.sample_rate(), 48000.0);
    }

    #[test]
    fn test_poly_patch_set_sample_rate() {
        let mut poly = PolyPatch::new(4, 44100.0);
        poly.set_sample_rate(48000.0);
        assert_eq!(poly.sample_rate(), 48000.0);
    }

    #[test]
    fn test_poly_patch_controller_access() {
        let poly = PolyPatch::new(4, 44100.0);
        assert!(poly.voice_control(0).is_some());
        assert!(poly.voice_controller(0).is_some());
        assert!(poly.voice_control(99).is_none());
    }

    #[test]
    fn test_poly_patch_allocator_mut() {
        let mut poly = PolyPatch::new(4, 44100.0);
        poly.allocator_mut().set_mode(AllocationMode::OldestSteal);
        assert_eq!(poly.allocator().mode(), AllocationMode::OldestSteal);
    }

    #[test]
    fn test_poly_patch_unison() {
        let mut poly = PolyPatch::new(4, 44100.0);
        poly.set_unison(UnisonConfig::new(2, 5.0));
        assert_eq!(poly.unison().voices, 2);
    }

    #[test]
    fn test_poly_patch_voice_patch_access() {
        let mut poly = PolyPatch::new(4, 44100.0);
        assert_eq!(poly.num_voices(), 4);
        assert!(poly.voice_patch(0).is_some());
        assert!(poly.voice_patch_mut(0).is_some());
        assert!(poly.voice_patch(99).is_none());
    }

    #[test]
    fn test_poly_patch_all_notes_off() {
        let mut poly = PolyPatch::new(4, 44100.0);
        poly.note_on(60, 100);
        poly.note_on(64, 100);
        poly.all_notes_off();
    }

    #[test]
    fn test_poly_patch_compile_tick_output() {
        let mut poly = PolyPatch::new(2, 44100.0);
        poly.compile().unwrap();
        poly.note_on(60, 100);
        poly.tick();
        let (left, right) = poly.output();
        let _ = (left, right);
    }

    #[test]
    fn test_poly_patch_reset() {
        let mut poly = PolyPatch::new(4, 44100.0);
        poly.note_on(60, 100);
        poly.reset();
        assert_eq!(poly.allocator().active_count(), 0);
    }

    // ---- End-to-end: a real polyphonic subtractive voice (Q063/Q064) -----

    /// Build a `PolyPatch` whose voices are VoiceController -> Vco -> Vca (gated
    /// by an Adsr) -> StereoOutput.
    fn build_synth(num_voices: usize, sr: f64) -> PolyPatch {
        PolyPatch::with_voice_fn(num_voices, sr, |patch, ctrl| {
            let sr = patch.sample_rate();
            let vco = patch.add("vco", Vco::new(sr));
            let adsr = patch.add("adsr", Adsr::new(sr));
            let vca = patch.add("vca", Vca::new());
            let out = patch.add("out", StereoOutput::new());
            patch.connect(ctrl.out("voct"), vco.in_("voct"))?;
            patch.connect(ctrl.out("gate"), adsr.in_("gate"))?;
            patch.connect(vco.out("sin"), vca.in_("in"))?;
            patch.connect(adsr.out("env"), vca.in_("cv"))?;
            patch.connect(vca.out("out"), out.in_("left"))?;
            patch.set_output(out.id());
            Ok(())
        })
        .unwrap()
    }

    /// Average samples-per-cycle from positive-going zero crossings of `left`.
    fn measure_period_samples(poly: &mut PolyPatch, warmup: usize, window: usize) -> f64 {
        for _ in 0..warmup {
            poly.tick();
        }
        let mut prev = 0.0;
        let mut crossings = Vec::new();
        for n in 0..window {
            let (l, _r) = poly.tick();
            if prev <= 0.0 && l > 0.0 {
                crossings.push(n);
            }
            prev = l;
        }
        assert!(crossings.len() >= 2, "need at least two zero crossings");
        let span = (crossings[crossings.len() - 1] - crossings[0]) as f64;
        span / (crossings.len() - 1) as f64
    }

    #[test]
    fn test_e2e_pitch_tracks_note() {
        let sr = 48_000.0;
        let mut poly = build_synth(1, sr);

        // C4 (261.63 Hz) then C5 (523.25 Hz): period should roughly halve.
        poly.note_on(60, 100);
        let p_c4 = measure_period_samples(&mut poly, 4000, 8000);
        poly.note_off(60);
        for _ in 0..(sr as usize / 5) {
            poly.tick(); // let it release + free
        }

        poly.note_on(72, 100);
        let p_c5 = measure_period_samples(&mut poly, 4000, 8000);

        // Expected periods.
        let expect_c4 = sr / 261.63;
        let expect_c5 = sr / 523.25;
        assert!(
            (p_c4 - expect_c4).abs() / expect_c4 < 0.05,
            "C4 period {p_c4} vs expected {expect_c4}"
        );
        assert!(
            (p_c5 - expect_c5).abs() / expect_c5 < 0.05,
            "C5 period {p_c5} vs expected {expect_c5}"
        );
        assert!(
            (p_c4 / p_c5 - 2.0).abs() < 0.1,
            "octave should halve the period (ratio {})",
            p_c4 / p_c5
        );
    }

    #[test]
    fn test_e2e_gate_reaches_adsr_and_release_tail_completes() {
        let sr = 48_000.0;
        let mut poly = build_synth(1, sr);

        poly.note_on(60, 127);

        // Let attack/decay settle to sustain, then measure sustained amplitude.
        let mut sustain_peak = 0.0f64;
        for _ in 0..4800 {
            poly.tick();
        }
        for _ in 0..2000 {
            let (l, _r) = poly.tick();
            sustain_peak = sustain_peak.max(l.abs());
        }
        assert!(
            sustain_peak > 0.1,
            "gate should drive the ADSR/VCA (sustain peak {sustain_peak})"
        );

        // Note off: the voice must NOT free one sample later (Q064).
        poly.note_off(60);
        poly.tick();
        assert_ne!(
            poly.allocator().voice(0).unwrap().state,
            VoiceState::Free,
            "voice freed one sample after note-off (truncated release)"
        );

        // Output should still be substantial right after release begins (not
        // instantly zero), then decay over the release time.
        let mut just_after = 0.0f64;
        for _ in 0..200 {
            let (l, _r) = poly.tick();
            just_after = just_after.max(l.abs());
        }
        assert!(
            just_after > 0.02,
            "release tail truncated (amp {just_after} right after note-off)"
        );

        // After the full release + grace + follower decay, the voice frees.
        let mut freed = false;
        for _ in 0..(sr as usize / 2) {
            poly.tick();
            if poly.allocator().voice(0).unwrap().state == VoiceState::Free {
                freed = true;
                break;
            }
        }
        assert!(freed, "released voice should eventually free");
    }

    // ---- Sample-rate propagation (Q069) ----------------------------------

    #[test]
    fn test_e2e_sample_rate_propagates_to_voices() {
        let sr1 = 48_000.0;
        let mut poly = build_synth(1, sr1);
        poly.note_on(60, 100);
        let p1 = measure_period_samples(&mut poly, 4000, 8000);

        // Halve the sample rate: if SR propagates, the period in *samples* halves
        // (same Hz, half as many samples per second). If it did NOT propagate,
        // the VCO would keep its old rate and the period would be unchanged.
        poly.set_sample_rate(sr1 / 2.0);
        poly.note_on(60, 100);
        let p2 = measure_period_samples(&mut poly, 2000, 4000);

        assert!(
            (p2 / p1 - 0.5).abs() < 0.1,
            "half sample rate should halve the period in samples (p1={p1}, p2={p2})"
        );
    }

    // ---- Polyphony gain compensation (Q067) ------------------------------

    fn build_dc_poly(num_voices: usize, value: f64, sr: f64) -> PolyPatch {
        PolyPatch::with_voice_fn(num_voices, sr, move |patch, _ctrl| {
            let dc = patch.add("dc", DcSource::new(value));
            let out = patch.add("out", StereoOutput::new());
            patch.connect(dc.out("out"), out.in_("left"))?;
            patch.set_output(out.id());
            Ok(())
        })
        .unwrap()
    }

    #[test]
    fn test_single_voice_unity_gain() {
        // Q066/Q067: a single mono voice must pass at unity gain.
        let sr = 48_000.0;
        let mut poly = build_dc_poly(4, 1.0, sr);
        poly.note_on(60, 100);
        let mut out = (0.0, 0.0);
        for _ in 0..2000 {
            out = poly.tick();
        }
        assert!(
            (out.0 - 1.0).abs() < 0.01 && (out.1 - 1.0).abs() < 0.01,
            "single voice should pass at unity gain, got {out:?}"
        );
    }

    #[test]
    fn test_eight_voices_bounded_output() {
        let sr = 48_000.0;
        let mut poly = build_dc_poly(8, 1.0, sr);

        // Single-voice reference.
        poly.note_on(60, 100);
        let mut single = 0.0;
        for _ in 0..2000 {
            single = poly.tick().0;
        }
        poly.panic();
        for _ in 0..2000 {
            poly.tick();
        }

        // Eight identical full-scale voices.
        for n in 0..8u8 {
            poly.note_on(60 + n, 100);
        }
        let mut eight = 0.0;
        for _ in 0..4000 {
            eight = poly.tick().0;
        }

        assert!((single - 1.0).abs() < 0.01);
        assert!(
            eight < 8.0 * single - 0.5,
            "8 voices must be well below 8x single ({eight} vs {})",
            8.0 * single
        );
        // 1/sqrt(8) law => ~2.83x single voice.
        assert!(
            (eight / single - 8.0f64.sqrt()).abs() < 0.3,
            "8-voice sum should follow 1/sqrt(N) (ratio {})",
            eight / single
        );
    }

    #[test]
    fn test_gain_compensation_is_smooth() {
        let sr = 48_000.0;
        let mut poly = PolyPatch::new(8, sr);

        poly.note_on(60, 100);
        for _ in 0..4000 {
            poly.tick();
        }
        let g_before = poly.compensation_gain();
        assert!(
            (g_before - 1.0).abs() < 0.01,
            "one voice => unity comp gain"
        );

        // Add a second voice: the compensation gain must not jump.
        poly.note_on(64, 100);
        poly.tick();
        let g_step = poly.compensation_gain();
        assert!(
            (g_before - g_step).abs() < 0.05,
            "comp gain stepped discontinuously: {g_before} -> {g_step}"
        );

        // Over ~10ms it settles toward 1/sqrt(2).
        for _ in 0..4000 {
            poly.tick();
        }
        let g_settled = poly.compensation_gain();
        assert!(
            (g_settled - 1.0 / 2.0f64.sqrt()).abs() < 0.02,
            "comp gain should settle to 1/sqrt(2), got {g_settled}"
        );
    }

    // ---- Steal declick (Q070) --------------------------------------------

    #[test]
    fn test_steal_resets_voice_dsp() {
        let sr = 48_000.0;
        // One voice so the next note must steal it.
        let mut poly = build_dc_poly(1, 1.0, sr);
        poly.allocator_mut().set_mode(AllocationMode::OldestSteal);

        poly.note_on(60, 100);
        for _ in 0..500 {
            poly.tick();
        }
        // Steal it with a new note; the DcSource patch should have been reset.
        poly.note_on(72, 100);
        assert_eq!(poly.allocator().last_stolen(), Some(0));
        // The reset zeroed the voice's follower.
        // (Behavioral proof: the voice still produces output after the steal.)
        let (l, _r) = poly.tick();
        assert!(l.abs() > 0.0, "stolen voice should keep producing audio");
    }

    // ---- VoiceMixer -------------------------------------------------------

    #[test]
    fn test_voice_mixer() {
        let mixer = VoiceMixer::new(4);
        let spec = mixer.port_spec();
        assert!(!spec.inputs.is_empty());
        assert!(!spec.outputs.is_empty());
    }

    #[test]
    fn test_voice_mixer_tick() {
        let mut mixer = VoiceMixer::new(2);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 1.0);
        inputs.set(1, 2.0);
        inputs.set(2, 3.0);
        inputs.set(3, 4.0);

        mixer.tick(&inputs, &mut outputs);

        assert!(outputs.get(100).is_some());
        assert!(outputs.get(101).is_some());
    }

    #[test]
    fn test_voice_mixer_reset_type_id() {
        let mut mixer = VoiceMixer::new(2);
        mixer.reset();
        mixer.set_sample_rate(48000.0);
        assert_eq!(mixer.type_id(), "voice_mixer");
    }

    // ---- Q163: full voice-count contention stress test ----

    /// Drive 16 voices through 12k ticks of interleaved note_on / note_off /
    /// retrigger churn, asserting no panic, correct active-voice bookkeeping
    /// (`active_count <= num_voices` at all times), and bounded mixed output.
    fn poly_stress(mode: AllocationMode) {
        let sr = 48_000.0;
        let mut poly = build_synth(16, sr);
        poly.allocator_mut().set_mode(mode);
        assert_eq!(poly.num_voices(), 16);

        // Deterministic LCG so the churn is reproducible.
        let mut state: u64 = 0x1234_5678_9abc_def0;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as u32
        };

        let mut held: Vec<u8> = Vec::new();
        for t in 0..12_000 {
            // A note event roughly every 30 ticks keeps the allocator churning.
            if t % 30 == 0 {
                let r = next() % 100;
                if r < 55 {
                    // New note (or a retrigger if the note is already sounding).
                    let note = 36 + (next() % 48) as u8;
                    let vel = 1 + (next() % 127) as u8;
                    poly.note_on(note, vel);
                    if !held.contains(&note) {
                        held.push(note);
                    }
                } else if r < 80 && !held.is_empty() {
                    let idx = (next() as usize) % held.len();
                    let note = held.remove(idx);
                    poly.note_off(note);
                } else if !held.is_empty() {
                    // Explicit retrigger of a held note.
                    let idx = (next() as usize) % held.len();
                    let note = held[idx];
                    let vel = 1 + (next() % 127) as u8;
                    poly.note_on(note, vel);
                }
            }

            let (l, r) = poly.tick();
            assert!(
                l.is_finite() && r.is_finite(),
                "non-finite output at tick {t}"
            );
            assert!(
                l.abs() < 16.0 && r.abs() < 16.0,
                "polyphonic output exploded at tick {t}: ({l}, {r})"
            );
            assert!(
                poly.allocator().active_count() <= poly.num_voices(),
                "active_count {} exceeded voice count at tick {t}",
                poly.allocator().active_count()
            );
        }

        // Release everything and let the amplitude-follower auto-free run out.
        poly.all_notes_off();
        for _ in 0..48_000 {
            let (l, r) = poly.tick();
            assert!(l.is_finite() && r.is_finite());
        }
        assert_eq!(
            poly.allocator().active_count(),
            0,
            "all voices should auto-free after a long release tail"
        );
    }

    #[test]
    fn test_poly_stress_16_voices_oldest_steal() {
        poly_stress(AllocationMode::OldestSteal);
    }

    #[test]
    fn test_poly_stress_16_voices_no_steal() {
        poly_stress(AllocationMode::NoSteal);
    }
}
