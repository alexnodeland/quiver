# I/O Modules

Modules for external communication, MIDI, OSC, and audio output.

## StereoOutput

The final audio output stage—every patch needs one.

```rust,ignore
let output = patch.add("output", StereoOutput::new());
patch.set_output(output.id());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `left` | Audio | Left channel |
| `right` | Audio | Right channel |

### Normalled Behavior

If only `left` is connected, `right` automatically mirrors it.

```rust,ignore
// Mono output - left copied to right
patch.connect(mono_source, output.in_("left"))?;

// Stereo output
patch.connect(left_source, output.in_("left"))?;
patch.connect(right_source, output.in_("right"))?;
```

### Getting Output

```rust,ignore
let (left, right) = patch.tick();  // Returns (f64, f64)
```

---

## ExternalInput

Injects values from external sources (MIDI, UI, etc.). The module holds an
`Arc<AtomicF64>`; any thread can `set()` the value and the audio thread reads
the latest value each tick.

```rust,ignore
use std::sync::Arc;

let cv = Arc::new(AtomicF64::new(0.0));
let input = patch.add("cv_in", ExternalInput::new(
    Arc::clone(&cv),
    SignalKind::CvUnipolar,
));
```

### Factory Methods

| Method | Signal Kind | Typical Use |
|--------|-------------|-------------|
| `::voct(arc)` | V/Oct | Pitch from MIDI |
| `::gate(arc)` | Gate | Note on/off |
| `::trigger(arc)` | Trigger | Clock pulses |
| `::cv(arc)` | Unipolar CV | Mod wheel, expression |
| `::cv_bipolar(arc)` | Bipolar CV | Pitch bend |
| `::audio(arc)` | Audio | External audio sample feed |

### Thread-Safe Updates

```rust,ignore
// From MIDI thread
cv.set(midi_cc_value / 127.0 * 10.0);

// Audio thread reads latest value
let input_module = ExternalInput::cv(Arc::clone(&cv));
```

---

## MidiState

Comprehensive MIDI state tracking. Feed it raw 3-byte MIDI messages with
`handle_message`; it maintains a set of atomic values (`Arc<AtomicF64>` fields)
that plug straight into `ExternalInput` modules.

```rust,ignore
let mut midi = MidiState::new();

// In your MIDI callback: pass raw MIDI bytes
midi.handle_message(&[0x90, 60, 100]);  // Note on: note 60, velocity 100
midi.handle_message(&[0x80, 60, 0]);    // Note off
midi.handle_message(&[0xB0, 1, 64]);    // CC1 (mod wheel) = 64
midi.handle_message(&[0xE0, 0x00, 0x40]); // Pitch bend (center)

// Read current state (atomic fields, safe from the audio thread)
let voct = midi.pitch.get();        // V/Oct of current note
let gate = midi.gate.get();         // Gate state (0 or 5V)
let velocity = midi.velocity.get(); // 0-10V
let mod_wheel = midi.mod_wheel.get();

// Coherent, torn-free (pitch, gate) pair from the same note event
let (pitch, gate) = midi.note_snapshot();
```

Bridge the state into a patch by cloning its atomic fields into
`ExternalInput` modules:

```rust,ignore
let pitch_in = patch.add("pitch", ExternalInput::voct(Arc::clone(&midi.pitch)));
let gate_in = patch.add("gate", ExternalInput::gate(Arc::clone(&midi.gate)));
let vel_in = patch.add("vel", ExternalInput::cv(Arc::clone(&midi.velocity)));
```

Other fields: `pitch_bend`, `aftertouch`, `sustain`, `expression`. Held-note
queries: `held_notes()`, `notes_active()`. Housekeeping: `reset()`,
`all_notes_off()`.

---

## OSC Integration

Quiver's OSC support is transport-agnostic: you receive OSC packets with any
network library, parse them into `OscMessage` values, and Quiver routes them to
`Arc<AtomicF64>` values shared with the patch.

### OscInput

A graph module that emits the current value of an `Arc<AtomicF64>` updated by
OSC. Constructed with the OSC address (for documentation), the shared value,
and the output signal kind.

```rust,ignore
let cutoff = Arc::new(AtomicF64::new(5.0));
let osc_in = patch.add(
    "cutoff_osc",
    OscInput::new("/synth/cutoff", Arc::clone(&cutoff), SignalKind::CvUnipolar),
);
patch.connect(osc_in.out("out"), vcf.in_("cutoff"))?;
```

### OscBinding

Maps an OSC address pattern to a shared value, with optional scale and offset
applied to the message's first float argument.

```rust,ignore
let cutoff = Arc::new(AtomicF64::new(0.0));
let binding = OscBinding::new("/synth/cutoff", Arc::clone(&cutoff))
    .with_scale(10.0)   // map incoming 0-1 to 0-10V
    .with_offset(0.0);

// When a message arrives (returns true if the pattern matched)
let msg = OscMessage::new("/synth/cutoff").with_float(0.5);
binding.apply(&msg);    // cutoff is now 5.0
```

### OscReceiver

Routes incoming `OscMessage`s to a set of bindings. It does not open a network
socket—feed it messages from whatever transport you use.

```rust,ignore
let mut receiver = OscReceiver::new();
receiver.bind("/synth/cutoff", Arc::clone(&cutoff));
receiver.bind_scaled("/synth/resonance", Arc::clone(&resonance), 1.0, 0.0);

// In your control thread, after parsing a packet into an OscMessage
if receiver.handle_message(&msg) {
    // At least one binding matched
}

// Diagnostics
let total = receiver.message_count();
let matched = receiver.matched_count();
```

### OscPattern

Pattern matching for OSC addresses. `*` matches within a single path
component, `[a-c]` matches character classes, `{a,b}` matches alternatives.

```rust,ignore
let pattern = OscPattern::new("/synth/voice/*/cutoff");

// Matches:
// /synth/voice/1/cutoff
// /synth/voice/2/cutoff
// etc.

if pattern.matches(&msg.address) {
    // Handle message
}
```

---

## Web Audio

### WebAudioConfig

Configuration shared by the Web Audio types:

```rust,ignore
let config = WebAudioConfig {
    input_channels: 0,
    output_channels: 2,
    sample_rate: 44100.0,
    block_size: 128,   // Web Audio render quantum
};
```

### WebAudioProcessor

A *trait* for Web Audio-compatible processors—implement it on your own type to
adapt it for AudioWorklet use:

```rust,ignore
impl WebAudioProcessor for MySynth {
    fn initialize(&mut self, config: &WebAudioConfig) { /* ... */ }
    fn process(&mut self, inputs: &[f32], outputs: &mut [f32]) -> bool {
        // Fill `outputs` with interleaved samples; return true to keep running
        true
    }
    fn set_parameter(&mut self, name: &str, value: f64) { /* ... */ }
    fn get_parameter(&self, name: &str) -> Option<f64> { None }
    fn parameter_names(&self) -> Vec<String> { vec![] }
}
```

### WebAudioBlockProcessor

Handles the 128-sample render quantum with pre-allocated buffers. Drive it
with a closure that produces one stereo frame per call—typically
`patch.tick()`:

```rust,ignore
let mut processor = WebAudioBlockProcessor::new();  // or ::with_config(config)
processor.activate();

// Each render quantum: returns interleaved f32 samples
let interleaved = processor.process_with(|_i| patch.tick());
```

Parameters registered with `add_parameter(name, initial)` return an
`Arc<AtomicF64>` you can share with `ExternalInput` modules in the patch.

### WebAudioWorklet

A lightweight adapter holding configuration and a parameter map:

```rust,ignore
let mut worklet = WebAudioWorklet::new();
let cutoff = worklet.add_parameter("cutoff", 5.0);
worklet.initialize(WebAudioConfig::default());

worklet.set_parameter("cutoff", 7.5);
```

### Interleaving

Web Audio uses interleaved f32 stereo; Quiver processes f64 channels. The
conversion helpers write into caller-provided buffers (no allocation):

```rust,ignore
// Separate f64 channels -> interleaved f32
let mut interleaved = vec![0.0f32; left.len() * 2];
interleave_stereo(&left, &right, &mut interleaved);

// Interleaved f32 -> separate f64 channels
let mut left = vec![0.0f64; input.len() / 2];
let mut right = vec![0.0f64; input.len() / 2];
deinterleave_stereo(&input, &mut left, &mut right);
```

`f64_to_f32_block` and `f32_to_f64_block` convert single channels in place.

---

## Common Patterns

### MIDI-Controlled Synth

```rust,ignore
let pitch_cv = Arc::new(AtomicF64::new(0.0));
let gate_cv = Arc::new(AtomicF64::new(0.0));
let vel_cv = Arc::new(AtomicF64::new(5.0));

let pitch = patch.add("pitch", ExternalInput::voct(pitch_cv.clone()));
let gate = patch.add("gate", ExternalInput::gate(gate_cv.clone()));
let velocity = patch.add("vel", ExternalInput::cv(vel_cv.clone()));

// In MIDI handler
fn handle_note_on(note: u8, vel: u8) {
    pitch_cv.set((note as f64 - 60.0) / 12.0);
    vel_cv.set(vel as f64 / 127.0 * 10.0);
    gate_cv.set(5.0);
}

fn handle_note_off(note: u8) {
    gate_cv.set(0.0);
}
```

Or let `MidiState` do the message parsing and share its atomic fields with the
patch as shown above.

### OSC-Controlled Parameters

```rust,ignore
let cutoff_cv = Arc::new(AtomicF64::new(5.0));
let reso_cv = Arc::new(AtomicF64::new(0.5));
let attack_cv = Arc::new(AtomicF64::new(0.01));

// Modules in the patch read these values
let cutoff_in = patch.add(
    "cutoff",
    OscInput::new("/filter/cutoff", cutoff_cv.clone(), SignalKind::CvUnipolar),
);

// Control thread routes messages
let mut receiver = OscReceiver::new();
receiver.bind_scaled("/filter/cutoff", cutoff_cv.clone(), 10.0, 0.0);
receiver.bind("/filter/reso", reso_cv.clone());
receiver.bind("/env/attack", attack_cv.clone());

// In OSC handler
receiver.handle_message(&msg);
```
