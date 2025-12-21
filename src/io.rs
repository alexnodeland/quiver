//! External I/O Integration
//!
//! This module provides components for bridging the patch graph with
//! external systems: MIDI controllers, audio interfaces, etc.

use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// Atomic f64 for lock-free communication between threads
///
/// Uses AtomicU64 internally since there's no native AtomicF64.
/// Suitable for real-time audio thread communication.
#[derive(Debug)]
pub struct AtomicF64(AtomicU64);

impl AtomicF64 {
    /// Create a new atomic f64 with the given initial value
    pub fn new(value: f64) -> Self {
        Self(AtomicU64::new(value.to_bits()))
    }

    /// Get the current value
    pub fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }

    /// Set a new value
    pub fn set(&self, value: f64) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }

    /// Load with specified ordering
    pub fn load(&self, ordering: Ordering) -> f64 {
        f64::from_bits(self.0.load(ordering))
    }

    /// Store with specified ordering
    pub fn store(&self, value: f64, ordering: Ordering) {
        self.0.store(value.to_bits(), ordering);
    }
}

impl Default for AtomicF64 {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl Clone for AtomicF64 {
    fn clone(&self) -> Self {
        Self::new(self.get())
    }
}

/// External input source - reads from an atomic value set by another thread
///
/// This module allows values from external sources (MIDI, OSC, GUI, etc.)
/// to be brought into the patch graph in a lock-free manner.
pub struct ExternalInput {
    value: Arc<AtomicF64>,
    spec: PortSpec,
}

impl ExternalInput {
    /// Create a new external input with the specified signal kind
    pub fn new(value: Arc<AtomicF64>, kind: SignalKind) -> Self {
        Self {
            value,
            spec: PortSpec {
                inputs: vec![],
                outputs: vec![PortDef::new(0, "out", kind)],
            },
        }
    }

    /// Create for pitch CV (V/Oct)
    pub fn voct(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::VoltPerOctave)
    }

    /// Create for gate signals
    pub fn gate(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::Gate)
    }

    /// Create for unipolar CV
    pub fn cv(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::CvUnipolar)
    }

    /// Create for bipolar CV
    pub fn cv_bipolar(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::CvBipolar)
    }

    /// Create for trigger signals
    pub fn trigger(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::Trigger)
    }

    /// Create for audio input
    pub fn audio(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::Audio)
    }

    /// Get a reference to the underlying atomic value
    pub fn value_ref(&self) -> &Arc<AtomicF64> {
        &self.value
    }
}

impl GraphModule for ExternalInput {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, _inputs: &PortValues, outputs: &mut PortValues) {
        outputs.set(0, self.value.get());
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "external_input"
    }
}

/// MIDI state that can be updated from a MIDI thread
///
/// This structure holds atomic values for common MIDI controllers.
/// Update from a MIDI callback thread, read from the audio thread.
#[derive(Debug)]
pub struct MidiState {
    /// Pitch in V/Oct (0V = C4, MIDI note 60)
    pub pitch: Arc<AtomicF64>,

    /// Gate signal (0 or 5V)
    pub gate: Arc<AtomicF64>,

    /// Velocity (0-10V)
    pub velocity: Arc<AtomicF64>,

    /// Mod wheel (0-10V)
    pub mod_wheel: Arc<AtomicF64>,

    /// Pitch bend (±semitones as V/Oct)
    pub pitch_bend: Arc<AtomicF64>,

    /// Channel aftertouch (0-10V)
    pub aftertouch: Arc<AtomicF64>,

    /// Sustain pedal (0 or 5V)
    pub sustain: Arc<AtomicF64>,

    /// Expression pedal (0-10V)
    pub expression: Arc<AtomicF64>,

    // Internal state for note handling
    held_notes: Vec<u8>,
}

impl MidiState {
    /// Create a new MIDI state with all values at zero
    pub fn new() -> Self {
        Self {
            pitch: Arc::new(AtomicF64::new(0.0)),
            gate: Arc::new(AtomicF64::new(0.0)),
            velocity: Arc::new(AtomicF64::new(0.0)),
            mod_wheel: Arc::new(AtomicF64::new(0.0)),
            pitch_bend: Arc::new(AtomicF64::new(0.0)),
            aftertouch: Arc::new(AtomicF64::new(0.0)),
            sustain: Arc::new(AtomicF64::new(0.0)),
            expression: Arc::new(AtomicF64::new(10.0)),
            held_notes: Vec::new(),
        }
    }

    /// Process a MIDI message (3-byte format)
    ///
    /// Call this from your MIDI callback to update the state.
    pub fn handle_message(&mut self, msg: &[u8]) {
        if msg.is_empty() {
            return;
        }

        let status = msg[0] & 0xF0;
        let _channel = msg[0] & 0x0F;

        match (status, msg.len()) {
            // Note On (with velocity > 0)
            (0x90, 3) if msg[2] > 0 => {
                let note = msg[1];
                let vel = msg[2];

                self.held_notes.push(note);
                self.pitch.set(Self::note_to_voct(note));
                self.velocity.set(vel as f64 / 127.0 * 10.0);
                self.gate.set(5.0);
            }

            // Note Off (or Note On with velocity 0)
            (0x80, 3) | (0x90, 3) => {
                let note = msg[1];
                self.held_notes.retain(|&n| n != note);

                if self.held_notes.is_empty() {
                    self.gate.set(0.0);
                } else {
                    // Legato: switch to last held note
                    let last = *self.held_notes.last().unwrap();
                    self.pitch.set(Self::note_to_voct(last));
                }
            }

            // Control Change
            (0xB0, 3) => {
                let cc = msg[1];
                let value = msg[2];
                let v = value as f64 / 127.0 * 10.0;

                match cc {
                    1 => self.mod_wheel.set(v),                                  // Mod wheel
                    11 => self.expression.set(v),                                // Expression
                    64 => self.sustain.set(if value >= 64 { 5.0 } else { 0.0 }), // Sustain
                    _ => {}
                }
            }

            // Pitch Bend
            (0xE0, 3) => {
                let lsb = msg[1] as u16;
                let msb = msg[2] as u16;
                let bend_raw = lsb | (msb << 7);
                // ±2 semitones = ±2/12 V
                let bend = (bend_raw as f64 - 8192.0) / 8192.0 * (2.0 / 12.0);
                self.pitch_bend.set(bend);
            }

            // Channel Aftertouch
            (0xD0, 2) => {
                let pressure = msg[1];
                self.aftertouch.set(pressure as f64 / 127.0 * 10.0);
            }

            // Polyphonic Aftertouch (we'll treat it as channel AT for mono)
            (0xA0, 3) => {
                let pressure = msg[2];
                self.aftertouch.set(pressure as f64 / 127.0 * 10.0);
            }

            _ => {}
        }
    }

    /// Convert MIDI note number to V/Oct
    ///
    /// 0V = C4 = MIDI note 60
    fn note_to_voct(note: u8) -> f64 {
        (note as f64 - 60.0) / 12.0
    }

    /// Get all held notes
    pub fn held_notes(&self) -> &[u8] {
        &self.held_notes
    }

    /// Check if any notes are currently held
    pub fn notes_active(&self) -> bool {
        !self.held_notes.is_empty()
    }

    /// Reset all state
    pub fn reset(&mut self) {
        self.pitch.set(0.0);
        self.gate.set(0.0);
        self.velocity.set(0.0);
        self.mod_wheel.set(0.0);
        self.pitch_bend.set(0.0);
        self.aftertouch.set(0.0);
        self.sustain.set(0.0);
        self.expression.set(10.0);
        self.held_notes.clear();
    }

    /// All notes off
    pub fn all_notes_off(&mut self) {
        self.held_notes.clear();
        self.gate.set(0.0);
    }
}

impl Default for MidiState {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MidiState {
    fn clone(&self) -> Self {
        Self {
            pitch: Arc::new(AtomicF64::new(self.pitch.get())),
            gate: Arc::new(AtomicF64::new(self.gate.get())),
            velocity: Arc::new(AtomicF64::new(self.velocity.get())),
            mod_wheel: Arc::new(AtomicF64::new(self.mod_wheel.get())),
            pitch_bend: Arc::new(AtomicF64::new(self.pitch_bend.get())),
            aftertouch: Arc::new(AtomicF64::new(self.aftertouch.get())),
            sustain: Arc::new(AtomicF64::new(self.sustain.get())),
            expression: Arc::new(AtomicF64::new(self.expression.get())),
            held_notes: self.held_notes.clone(),
        }
    }
}

/// External output - writes to an atomic value for reading by another thread
///
/// Useful for sending CV values out to external systems.
pub struct ExternalOutput {
    value: Arc<AtomicF64>,
    spec: PortSpec,
}

impl ExternalOutput {
    pub fn new(value: Arc<AtomicF64>, kind: SignalKind) -> Self {
        Self {
            value,
            spec: PortSpec {
                inputs: vec![PortDef::new(0, "in", kind)],
                outputs: vec![],
            },
        }
    }

    pub fn value_ref(&self) -> &Arc<AtomicF64> {
        &self.value
    }
}

impl GraphModule for ExternalOutput {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, _outputs: &mut PortValues) {
        let value = inputs.get_or(0, 0.0);
        self.value.set(value);
    }

    fn reset(&mut self) {
        self.value.set(0.0);
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "external_output"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_f64() {
        let a = AtomicF64::new(3.5);
        assert!((a.get() - 3.5).abs() < 0.001);

        a.set(2.5);
        assert!((a.get() - 2.5).abs() < 0.001);
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_atomic_f64_thread_safe() {
        let a = Arc::new(AtomicF64::new(0.0));
        let a2 = Arc::clone(&a);

        std::thread::spawn(move || {
            a2.set(42.0);
        })
        .join()
        .unwrap();

        assert!((a.get() - 42.0).abs() < 0.001);
    }

    #[test]
    fn test_external_input() {
        let value = Arc::new(AtomicF64::new(5.0));
        let mut input = ExternalInput::voct(value.clone());

        let inputs = PortValues::new();
        let mut outputs = PortValues::new();

        input.tick(&inputs, &mut outputs);
        assert!((outputs.get(0).unwrap() - 5.0).abs() < 0.001);

        // Update from "external thread"
        value.set(10.0);
        input.tick(&inputs, &mut outputs);
        assert!((outputs.get(0).unwrap() - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_midi_state_note_on_off() {
        let mut midi = MidiState::new();

        // Note on: C4 (note 60) with velocity 100
        midi.handle_message(&[0x90, 60, 100]);
        assert!((midi.pitch.get() - 0.0).abs() < 0.001); // C4 = 0V
        assert!((midi.gate.get() - 5.0).abs() < 0.001);
        assert!(midi.velocity.get() > 0.0);

        // Note on: C5 (note 72)
        midi.handle_message(&[0x90, 72, 100]);
        assert!((midi.pitch.get() - 1.0).abs() < 0.001); // C5 = 1V

        // Note off: C5
        midi.handle_message(&[0x80, 72, 0]);
        // Should return to C4 (legato)
        assert!((midi.pitch.get() - 0.0).abs() < 0.001);
        assert!((midi.gate.get() - 5.0).abs() < 0.001); // Still held

        // Note off: C4
        midi.handle_message(&[0x80, 60, 0]);
        assert!((midi.gate.get() - 0.0).abs() < 0.001); // Gate off
    }

    #[test]
    fn test_midi_state_pitch_bend() {
        let mut midi = MidiState::new();

        // Center (no bend)
        midi.handle_message(&[0xE0, 0, 64]);
        assert!(midi.pitch_bend.get().abs() < 0.01);

        // Full up (should be ~+2 semitones = +1/6 V)
        midi.handle_message(&[0xE0, 127, 127]);
        assert!(midi.pitch_bend.get() > 0.1);

        // Full down (should be ~-2 semitones = -1/6 V)
        midi.handle_message(&[0xE0, 0, 0]);
        assert!(midi.pitch_bend.get() < -0.1);
    }

    #[test]
    fn test_midi_state_cc() {
        let mut midi = MidiState::new();

        // Mod wheel
        midi.handle_message(&[0xB0, 1, 127]);
        assert!((midi.mod_wheel.get() - 10.0).abs() < 0.01);

        // Sustain pedal on
        midi.handle_message(&[0xB0, 64, 127]);
        assert!((midi.sustain.get() - 5.0).abs() < 0.01);

        // Sustain pedal off
        midi.handle_message(&[0xB0, 64, 0]);
        assert!((midi.sustain.get() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_external_output() {
        let value = Arc::new(AtomicF64::new(0.0));
        let mut output = ExternalOutput::new(value.clone(), SignalKind::CvUnipolar);

        let mut inputs = PortValues::new();
        inputs.set(0, 7.5);

        output.tick(&inputs, &mut PortValues::new());
        assert!((value.get() - 7.5).abs() < 0.001);
    }

    #[test]
    fn test_atomic_f64_load_store() {
        use core::sync::atomic::Ordering;
        let a = AtomicF64::new(1.0);
        assert!((a.load(Ordering::SeqCst) - 1.0).abs() < 0.001);

        a.store(99.0, Ordering::SeqCst);
        assert!((a.load(Ordering::SeqCst) - 99.0).abs() < 0.001);
    }

    #[test]
    fn test_external_input_constructors() {
        let value = Arc::new(AtomicF64::new(0.0));

        let gate = ExternalInput::gate(value.clone());
        assert!(gate.spec.outputs[0].kind == SignalKind::Gate);

        let cv = ExternalInput::cv(value.clone());
        assert!(cv.spec.outputs[0].kind == SignalKind::CvUnipolar);

        let cv_bi = ExternalInput::cv_bipolar(value.clone());
        assert!(cv_bi.spec.outputs[0].kind == SignalKind::CvBipolar);

        let trigger = ExternalInput::trigger(value.clone());
        assert!(trigger.spec.outputs[0].kind == SignalKind::Trigger);

        let audio = ExternalInput::audio(value.clone());
        assert!(audio.spec.outputs[0].kind == SignalKind::Audio);
    }

    #[test]
    fn test_external_input_value_ref() {
        let value = Arc::new(AtomicF64::new(42.0));
        let input = ExternalInput::voct(value.clone());
        assert!((input.value_ref().get() - 42.0).abs() < 0.001);
    }

    #[test]
    fn test_external_input_reset_set_sample_rate() {
        let value = Arc::new(AtomicF64::new(5.0));
        let mut input = ExternalInput::voct(value.clone());

        input.reset();
        input.set_sample_rate(48000.0);
        assert_eq!(input.type_id(), "external_input");
    }

    #[test]
    fn test_external_output_reset_type_id() {
        let value = Arc::new(AtomicF64::new(5.0));
        let mut output = ExternalOutput::new(value.clone(), SignalKind::Audio);

        output.reset();
        assert!((value.get() - 0.0).abs() < 0.001);

        output.set_sample_rate(48000.0);
        assert_eq!(output.type_id(), "external_output");
        assert!(output.value_ref().get().abs() < 0.001);
    }

    #[test]
    fn test_midi_state_default() {
        let midi = MidiState::default();
        assert!(midi.pitch.get().abs() < 0.001);
    }

    #[test]
    fn test_midi_state_clone() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0x90, 60, 100]);

        let cloned = midi.clone();
        assert!((cloned.pitch.get() - midi.pitch.get()).abs() < 0.001);
    }

    #[test]
    fn test_midi_state_reset() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0x90, 60, 100]);
        midi.handle_message(&[0xB0, 1, 127]);

        midi.reset();
        assert!(midi.pitch.get().abs() < 0.001);
        assert!(midi.gate.get().abs() < 0.001);
        assert!(midi.held_notes.is_empty());
    }

    #[test]
    fn test_midi_state_all_notes_off() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0x90, 60, 100]);
        midi.handle_message(&[0x90, 62, 100]);

        assert!(midi.notes_active());

        midi.all_notes_off();
        assert!(!midi.notes_active());
        assert!(midi.gate.get().abs() < 0.001);
    }

    #[test]
    fn test_midi_state_held_notes() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0x90, 60, 100]);
        midi.handle_message(&[0x90, 62, 100]);

        assert_eq!(midi.held_notes(), &[60, 62]);
    }

    #[test]
    fn test_midi_state_channel_aftertouch() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0xD0, 100]);
        assert!(midi.aftertouch.get() > 0.0);
    }

    #[test]
    fn test_midi_state_poly_aftertouch() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0xA0, 60, 100]);
        assert!(midi.aftertouch.get() > 0.0);
    }

    #[test]
    fn test_midi_state_expression() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0xB0, 11, 100]);
        assert!(midi.expression.get() > 0.0);
    }

    #[test]
    fn test_midi_state_note_on_with_zero_velocity() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0x90, 60, 100]);
        assert!(midi.gate.get() > 0.0);

        // Note on with velocity 0 = note off
        midi.handle_message(&[0x90, 60, 0]);
        assert!(midi.gate.get().abs() < 0.001);
    }
}
