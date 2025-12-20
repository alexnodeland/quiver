//! Polyphony Support
//!
//! This module provides voice allocation, per-voice processing, and unison
//! capabilities for building polyphonic synthesizers.
//!
//! # Architecture
//!
//! - `VoiceAllocator` - Manages which voices get assigned to incoming notes
//! - `Voice` - A single voice with its own state and modules
//! - `PolyPatch` - A polyphonic patch containing multiple voice instances
//! - `UnisonVoice` - Stacked voices with detuning for thick unison sounds

use crate::graph::{Patch, PatchError};
use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use std::collections::VecDeque;

/// Voice allocation algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AllocationMode {
    /// Reuse the oldest voice when all voices are active
    #[default]
    RoundRobin,
    /// Steal the quietest voice (based on envelope level)
    QuietestSteal,
    /// Steal the oldest active voice
    OldestSteal,
    /// Never steal - new notes are ignored if no free voices
    NoSteal,
    /// Lowest priority - higher notes steal lower notes
    HighestPriority,
    /// Highest priority - lower notes steal higher notes
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
#[derive(Debug)]
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
    /// Trigger signal (momentary pulse)
    pub trigger: f64,
    /// Age counter (samples since note on)
    pub age: u64,
    /// Current envelope level (for quiet-steal algorithm)
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
        self.trigger = 1.0; // Will be cleared after one sample
        self.age = 0;
    }

    /// Release the voice
    pub fn note_off(&mut self) {
        if self.state == VoiceState::Active {
            self.state = VoiceState::Releasing;
            self.gate = 0.0;
        }
    }

    /// Mark voice as completely free
    pub fn free(&mut self) {
        self.state = VoiceState::Free;
        self.note = None;
        self.velocity = 0.0;
        self.gate = 0.0;
        self.trigger = 0.0;
        self.envelope_level = 0.0;
    }

    /// Update voice state each sample
    pub fn tick(&mut self) {
        self.age = self.age.saturating_add(1);
        self.trigger = 0.0; // Clear trigger after one sample

        // Auto-free releasing voices when envelope is done
        if self.state == VoiceState::Releasing && self.envelope_level < 0.0001 {
            self.free();
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
    (voct * 12.0 + 60.0).round().clamp(0.0, 127.0) as u8
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

    /// Count active voices
    pub fn active_count(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| v.state != VoiceState::Free)
            .count()
    }

    /// Allocate a voice for a note
    /// Returns the voice index if successful
    pub fn note_on(&mut self, note: u8, velocity: f64) -> Option<usize> {
        // First check if this note is already playing (retrigger)
        for voice in &mut self.voices {
            if voice.is_playing_note(note) {
                voice.note_on(note, velocity);
                return Some(voice.index);
            }
        }

        // Try to find a free voice
        if let Some(voice_idx) = self.find_free_voice() {
            self.voices[voice_idx].note_on(note, velocity);
            self.update_lru(voice_idx);
            return Some(voice_idx);
        }

        // No free voices - attempt voice stealing based on mode
        if let Some(voice_idx) = self.find_steal_voice(note) {
            self.voices[voice_idx].note_on(note, velocity);
            self.update_lru(voice_idx);
            return Some(voice_idx);
        }

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

    /// Update all voices (call once per sample)
    pub fn tick(&mut self) {
        for voice in &mut self.voices {
            voice.tick();
        }
    }

    /// Update envelope level for a voice (for quiet-steal algorithm)
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

    fn find_steal_voice(&self, note: u8) -> Option<usize> {
        match self.mode {
            AllocationMode::NoSteal => None,
            AllocationMode::RoundRobin | AllocationMode::OldestSteal => {
                // Find oldest voice
                self.voices.iter().max_by_key(|v| v.age).map(|v| v.index)
            }
            AllocationMode::QuietestSteal => {
                // Find voice with lowest envelope level
                self.voices
                    .iter()
                    .min_by(|a, b| {
                        a.envelope_level
                            .partial_cmp(&b.envelope_level)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|v| v.index)
            }
            AllocationMode::HighestPriority => {
                // Steal lowest note if new note is higher
                self.voices
                    .iter()
                    .filter(|v| v.note.map(|n| n < note).unwrap_or(false))
                    .min_by_key(|v| v.note)
                    .map(|v| v.index)
            }
            AllocationMode::LowestPriority => {
                // Steal highest note if new note is lower
                self.voices
                    .iter()
                    .filter(|v| v.note.map(|n| n > note).unwrap_or(false))
                    .max_by_key(|v| v.note)
                    .map(|v| v.index)
            }
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
    /// Detune spread in cents (total spread across all voices)
    pub detune_cents: f64,
    /// Stereo spread (0.0 = mono, 1.0 = full stereo)
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
    /// Create a unison configuration
    pub fn new(voices: usize, detune_cents: f64) -> Self {
        Self {
            voices: voices.max(1),
            detune_cents,
            stereo_spread: 0.5,
            phase_random: 0.0,
        }
    }

    /// Calculate the detune offset for a specific unison voice
    /// Returns V/Oct offset
    pub fn detune_offset(&self, voice_index: usize) -> f64 {
        if self.voices <= 1 {
            return 0.0;
        }

        // Spread voices evenly across the detune range
        let normalized = voice_index as f64 / (self.voices - 1) as f64;
        let centered = normalized * 2.0 - 1.0; // -1 to +1

        // Convert cents to V/Oct (100 cents = 1 semitone = 1/12 octave)
        centered * self.detune_cents / 1200.0
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

    /// Get the gain multiplier per voice to maintain consistent output level
    pub fn voice_gain(&self) -> f64 {
        1.0 / (self.voices as f64).sqrt()
    }
}

/// A polyphonic voice module that wraps a single voice's processing
pub struct PolyVoice {
    /// Voice index
    pub index: usize,
    spec: PortSpec,
}

impl PolyVoice {
    /// Create a new poly voice input module
    pub fn new(index: usize) -> Self {
        Self {
            index,
            spec: PortSpec {
                inputs: vec![],
                outputs: vec![
                    PortDef::new(0, "voct", SignalKind::VoltPerOctave),
                    PortDef::new(1, "gate", SignalKind::Gate),
                    PortDef::new(2, "trigger", SignalKind::Trigger),
                    PortDef::new(3, "velocity", SignalKind::CvUnipolar),
                ],
            },
        }
    }
}

impl GraphModule for PolyVoice {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, _inputs: &PortValues, _outputs: &mut PortValues) {
        // Values are set externally by PolyPatch
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "poly_voice"
    }
}

/// Polyphonic patch container
///
/// Manages multiple voice instances and handles voice allocation.
pub struct PolyPatch {
    /// Voice allocator
    allocator: VoiceAllocator,
    /// Per-voice patches
    voice_patches: Vec<Patch>,
    /// Per-voice input modules for injecting CV signals
    voice_inputs: Vec<VoiceInput>,
    /// Unison configuration
    unison: UnisonConfig,
    /// Sample rate
    sample_rate: f64,
    /// Output buffers (left, right)
    output_left: f64,
    output_right: f64,
}

impl PolyPatch {
    /// Create a new polyphonic patch
    pub fn new(num_voices: usize, sample_rate: f64) -> Self {
        let allocator = VoiceAllocator::new(num_voices);
        let voice_patches = (0..num_voices).map(|_| Patch::new(sample_rate)).collect();
        let voice_inputs = (0..num_voices).map(|_| VoiceInput::new()).collect();

        Self {
            allocator,
            voice_patches,
            voice_inputs,
            unison: UnisonConfig::default(),
            sample_rate,
            output_left: 0.0,
            output_right: 0.0,
        }
    }

    /// Get the sample rate
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Set the sample rate for all voice patches
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        // Note: Individual patches would need to be recompiled after this
        // This is typically done by recreating the patches
    }

    /// Get a voice input module for configuration
    pub fn voice_input(&self, index: usize) -> Option<&VoiceInput> {
        self.voice_inputs.get(index)
    }

    /// Get a mutable voice input module
    pub fn voice_input_mut(&mut self, index: usize) -> Option<&mut VoiceInput> {
        self.voice_inputs.get_mut(index)
    }

    /// Get the voice allocator
    pub fn allocator(&self) -> &VoiceAllocator {
        &self.allocator
    }

    /// Get mutable access to the voice allocator
    pub fn allocator_mut(&mut self) -> &mut VoiceAllocator {
        &mut self.allocator
    }

    /// Set unison configuration
    pub fn set_unison(&mut self, config: UnisonConfig) {
        self.unison = config;
    }

    /// Get unison configuration
    pub fn unison(&self) -> &UnisonConfig {
        &self.unison
    }

    /// Get a voice patch for configuration
    pub fn voice_patch(&self, index: usize) -> Option<&Patch> {
        self.voice_patches.get(index)
    }

    /// Get a mutable voice patch for configuration
    pub fn voice_patch_mut(&mut self, index: usize) -> Option<&mut Patch> {
        self.voice_patches.get_mut(index)
    }

    /// Get all voice patches
    pub fn voice_patches(&self) -> &[Patch] {
        &self.voice_patches
    }

    /// Get all voice patches mutably
    pub fn voice_patches_mut(&mut self) -> &mut [Patch] {
        &mut self.voice_patches
    }

    /// Handle MIDI note on
    pub fn note_on(&mut self, note: u8, velocity: u8) {
        let velocity_f = velocity as f64 / 127.0;
        self.allocator.note_on(note, velocity_f);
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

    /// Compile all voice patches
    pub fn compile(&mut self) -> Result<(), PatchError> {
        for patch in &mut self.voice_patches {
            patch.compile()?;
        }
        Ok(())
    }

    /// Process one sample and return stereo output
    pub fn tick(&mut self) -> (f64, f64) {
        self.allocator.tick();

        let mut left = 0.0;
        let mut right = 0.0;

        // First, update voice inputs from allocator state
        for (i, voice) in self.allocator.voices().iter().enumerate() {
            if let Some(input) = self.voice_inputs.get_mut(i) {
                input.set_from_voice(voice);
            }
        }

        // Process each active voice
        for (i, voice) in self.allocator.voices().iter().enumerate() {
            if voice.state == VoiceState::Free {
                continue;
            }

            // Process unison voices
            let unison_gain = self.unison.voice_gain();
            for u in 0..self.unison.voices {
                // Calculate detune offset in V/Oct
                let detune = self.unison.detune_offset(u);
                let pan = self.unison.pan_position(u);

                // Apply detune to voice input V/Oct
                if let Some(input) = self.voice_inputs.get_mut(i) {
                    let base_voct = voice.voct;
                    input.set_voct(base_voct + detune);
                }

                // Get the voice patch and process
                if let Some(patch) = self.voice_patches.get_mut(i) {
                    let (l, r) = patch.tick();

                    // Apply pan law (constant power)
                    let pan_angle = (pan + 1.0) * std::f64::consts::PI / 4.0;
                    let left_gain = pan_angle.cos();
                    let right_gain = pan_angle.sin();

                    left += l * left_gain * unison_gain;
                    right += r * right_gain * unison_gain;
                }
            }
        }

        self.output_left = left;
        self.output_right = right;
        (left, right)
    }

    /// Get the last output
    pub fn output(&self) -> (f64, f64) {
        (self.output_left, self.output_right)
    }

    /// Reset all voice patches
    pub fn reset(&mut self) {
        for patch in &mut self.voice_patches {
            patch.reset();
        }
        self.allocator.panic();
        self.output_left = 0.0;
        self.output_right = 0.0;
    }
}

/// Voice input module for injecting per-voice CV into a patch
///
/// This module provides the per-voice signals (V/Oct, gate, trigger, velocity)
/// that drive a voice patch.
pub struct VoiceInput {
    voct: f64,
    gate: f64,
    trigger: f64,
    velocity: f64,
    spec: PortSpec,
}

impl VoiceInput {
    /// Create a new voice input module
    pub fn new() -> Self {
        Self {
            voct: 0.0,
            gate: 0.0,
            trigger: 0.0,
            velocity: 1.0,
            spec: PortSpec {
                inputs: vec![],
                outputs: vec![
                    PortDef::new(0, "voct", SignalKind::VoltPerOctave),
                    PortDef::new(1, "gate", SignalKind::Gate),
                    PortDef::new(2, "trigger", SignalKind::Trigger),
                    PortDef::new(3, "velocity", SignalKind::CvUnipolar),
                ],
            },
        }
    }

    /// Set voice state from allocator voice
    pub fn set_from_voice(&mut self, voice: &Voice) {
        self.voct = voice.voct;
        self.gate = voice.gate;
        self.trigger = voice.trigger;
        self.velocity = voice.velocity;
    }

    /// Set V/Oct directly
    pub fn set_voct(&mut self, voct: f64) {
        self.voct = voct;
    }

    /// Set gate directly
    pub fn set_gate(&mut self, gate: f64) {
        self.gate = gate;
    }

    /// Set trigger directly
    pub fn set_trigger(&mut self, trigger: f64) {
        self.trigger = trigger;
    }

    /// Set velocity directly
    pub fn set_velocity(&mut self, velocity: f64) {
        self.velocity = velocity;
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
        outputs.set(0, self.voct);
        outputs.set(1, if self.gate > 0.5 { 5.0 } else { 0.0 });
        outputs.set(2, if self.trigger > 0.5 { 5.0 } else { 0.0 });
        outputs.set(3, self.velocity * 10.0); // Scale to 0-10V
    }

    fn reset(&mut self) {
        self.voct = 0.0;
        self.gate = 0.0;
        self.trigger = 0.0;
        self.velocity = 1.0;
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "voice_input"
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

    #[test]
    fn test_voice_allocation_basic() {
        let mut allocator = VoiceAllocator::new(4);

        // Allocate first note
        let voice1 = allocator.note_on(60, 0.8);
        assert_eq!(voice1, Some(0));
        assert_eq!(allocator.active_count(), 1);

        // Allocate second note
        let voice2 = allocator.note_on(64, 0.7);
        assert_eq!(voice2, Some(1));
        assert_eq!(allocator.active_count(), 2);

        // Release first note
        allocator.note_off(60);
        assert_eq!(allocator.active_count(), 2); // Still active (releasing)

        // Tick to clear trigger
        allocator.tick();
    }

    #[test]
    fn test_voice_allocation_retrigger() {
        let mut allocator = VoiceAllocator::new(4);

        // Allocate note
        let voice1 = allocator.note_on(60, 0.8);
        assert_eq!(voice1, Some(0));

        // Retrigger same note - should use same voice
        let voice2 = allocator.note_on(60, 0.9);
        assert_eq!(voice2, Some(0));
        assert_eq!(allocator.active_count(), 1);
    }

    #[test]
    fn test_voice_stealing() {
        let mut allocator = VoiceAllocator::new(2);
        allocator.set_mode(AllocationMode::OldestSteal);

        // Fill all voices
        allocator.note_on(60, 0.8);
        allocator.tick();
        allocator.note_on(62, 0.7);
        allocator.tick();

        // Should steal oldest voice (voice 0)
        let stolen = allocator.note_on(64, 0.6);
        assert_eq!(stolen, Some(0));
    }

    #[test]
    fn test_no_steal_mode() {
        let mut allocator = VoiceAllocator::new(2);
        allocator.set_mode(AllocationMode::NoSteal);

        // Fill all voices
        allocator.note_on(60, 0.8);
        allocator.note_on(62, 0.7);

        // Should fail to allocate
        let result = allocator.note_on(64, 0.6);
        assert_eq!(result, None);
    }

    #[test]
    fn test_midi_note_to_voct() {
        // C4 = 0V
        assert!((midi_note_to_voct(60) - 0.0).abs() < 0.001);

        // C5 = +1V
        assert!((midi_note_to_voct(72) - 1.0).abs() < 0.001);

        // C3 = -1V
        assert!((midi_note_to_voct(48) - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_unison_detune() {
        let config = UnisonConfig::new(3, 10.0); // 3 voices, 10 cents spread

        // First voice should be detuned down
        let d0 = config.detune_offset(0);
        assert!(d0 < 0.0);

        // Middle voice should be centered
        let d1 = config.detune_offset(1);
        assert!((d1 - 0.0).abs() < 0.001);

        // Last voice should be detuned up
        let d2 = config.detune_offset(2);
        assert!(d2 > 0.0);

        // Spread should be symmetric
        assert!((d0 + d2).abs() < 0.001);
    }

    #[test]
    fn test_unison_pan() {
        let mut config = UnisonConfig::new(3, 10.0);
        config.stereo_spread = 1.0;

        // First voice should be panned left
        let p0 = config.pan_position(0);
        assert!((p0 - (-1.0)).abs() < 0.001);

        // Middle voice should be centered
        let p1 = config.pan_position(1);
        assert!((p1 - 0.0).abs() < 0.001);

        // Last voice should be panned right
        let p2 = config.pan_position(2);
        assert!((p2 - 1.0).abs() < 0.001);
    }

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
    fn test_poly_patch_basic() {
        let mut poly = PolyPatch::new(4, 44100.0);

        poly.note_on(60, 100);
        assert_eq!(poly.allocator().active_count(), 1);

        poly.note_on(64, 90);
        assert_eq!(poly.allocator().active_count(), 2);

        poly.note_off(60);
        // Voice should be in releasing state
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
    fn test_voct_to_midi_note() {
        assert_eq!(voct_to_midi_note(0.0), 60);
        assert_eq!(voct_to_midi_note(1.0), 72);
        assert_eq!(voct_to_midi_note(-1.0), 48);
    }

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
    fn test_voice_tick() {
        let mut voice = Voice::new(0);
        voice.note_on(60, 1.0);
        voice.tick();
        assert!(voice.trigger == 0.0);
    }

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

        let voice = allocator.voice(0);
        assert!(voice.is_some());

        let voice_mut = allocator.voice_mut(0);
        assert!(voice_mut.is_some());
    }

    #[test]
    fn test_voice_allocator_voices() {
        let allocator = VoiceAllocator::new(4);
        let voices: Vec<_> = allocator.voices().iter().collect();
        assert_eq!(voices.len(), 4);
    }

    #[test]
    fn test_voice_allocator_voices_mut() {
        let mut allocator = VoiceAllocator::new(4);
        let voices: Vec<_> = allocator.voices_mut().iter_mut().collect();
        assert_eq!(voices.len(), 4);
    }

    #[test]
    fn test_voice_allocator_all_notes_off() {
        let mut allocator = VoiceAllocator::new(4);
        allocator.note_on(60, 1.0);
        allocator.note_on(64, 1.0);
        allocator.all_notes_off();
        // All voices should be in releasing state
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
        // After tick, trigger should be cleared
    }

    #[test]
    fn test_voice_allocator_set_envelope_level() {
        let mut allocator = VoiceAllocator::new(4);
        let idx = allocator.note_on(60, 1.0);
        if let Some(i) = idx {
            allocator.set_envelope_level(i, 0.5);
        }
    }

    #[test]
    fn test_unison_config_voice_gain() {
        let config = UnisonConfig::new(4, 10.0);
        let gain = config.voice_gain();
        assert!(gain > 0.0 && gain < 1.0);
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
    fn test_poly_patch_voice_input_access() {
        let mut poly = PolyPatch::new(4, 44100.0);

        let input = poly.voice_input(0);
        assert!(input.is_some());

        let input_mut = poly.voice_input_mut(0);
        assert!(input_mut.is_some());
    }

    #[test]
    fn test_poly_patch_allocator_mut() {
        let mut poly = PolyPatch::new(4, 44100.0);
        let alloc = poly.allocator_mut();
        alloc.set_mode(AllocationMode::OldestSteal);
    }

    #[test]
    fn test_poly_patch_unison() {
        let mut poly = PolyPatch::new(4, 44100.0);
        poly.set_unison(UnisonConfig::new(2, 5.0));

        let unison = poly.unison();
        assert!(unison.voices > 0);
    }

    #[test]
    fn test_poly_patch_voice_patches_access() {
        let mut poly = PolyPatch::new(4, 44100.0);

        let patches: Vec<_> = poly.voice_patches().iter().collect();
        assert_eq!(patches.len(), 4);

        let patches_mut: Vec<_> = poly.voice_patches_mut().iter_mut().collect();
        assert_eq!(patches_mut.len(), 4);

        let patch = poly.voice_patch(0);
        assert!(patch.is_some());

        let patch_mut = poly.voice_patch_mut(0);
        assert!(patch_mut.is_some());
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
    }

    #[test]
    fn test_voice_input_default() {
        let input = VoiceInput::default();
        assert!((input.voct - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_voice_input_set_from_voice() {
        let mut voice = Voice::new(0);
        voice.note_on(72, 0.8);

        let mut input = VoiceInput::new();
        input.set_from_voice(&voice);

        assert!((input.voct - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_voice_input_reset_type_id() {
        let mut input = VoiceInput::new();
        input.set_voct(1.0);
        input.reset();
        assert!((input.voct - 0.0).abs() < 0.001);
        assert_eq!(input.type_id(), "voice_input");
        input.set_sample_rate(48000.0);
    }

    #[test]
    fn test_voice_input_set_trigger() {
        let mut input = VoiceInput::new();
        input.set_trigger(1.0);
        assert!((input.trigger - 1.0).abs() < 0.001);
    }

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
}
