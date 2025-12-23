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

Injects values from external sources (MIDI, UI, etc.).

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

### Thread-Safe Updates

```rust,ignore
// From MIDI thread
cv.set(midi_cc_value / 127.0 * 10.0);

// Audio thread reads latest value
let input_module = ExternalInput::cv(Arc::clone(&cv));
```

---

## MidiState

Comprehensive MIDI state tracking.

```rust,ignore
let midi = MidiState::new();

// In MIDI callback
midi.note_on(60, 100);   // Note 60, velocity 100
midi.note_off(60);
midi.control_change(1, 64);  // CC1 = 64
midi.pitch_bend(8192);       // Center

// Read current state
let voct = midi.voct();      // V/Oct of current note
let gate = midi.gate();      // Gate state (0 or 5V)
let velocity = midi.velocity();  // 0.0 - 1.0
let mod_wheel = midi.cc(1);      // CC value
```

---

## OSC Integration

### OscInput

Receives OSC messages as CV.

```rust,ignore
let osc_in = patch.add("cutoff_osc", OscInput::new("/synth/cutoff"));
patch.connect(osc_in.out("out"), vcf.in_("cutoff"))?;
```

### OscReceiver

Network OSC receiver.

```rust,ignore
let receiver = OscReceiver::new("127.0.0.1:9000")?;

// In your control thread
while let Some(msg) = receiver.recv()? {
    match msg.address.as_str() {
        "/synth/cutoff" => {
            if let Some(OscValue::Float(v)) = msg.args.first() {
                cutoff_cv.set(*v as f64 * 10.0);
            }
        }
        _ => {}
    }
}
```

### OscPattern

Pattern matching for OSC addresses.

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

### OscBinding

Maps OSC to patch parameters.

```rust,ignore
let bindings = vec![
    OscBinding::new("/synth/cutoff", "vcf.cutoff", 0.0..10.0),
    OscBinding::new("/synth/resonance", "vcf.resonance", 0.0..1.0),
];

for binding in &bindings {
    if let Some(value) = binding.process(&msg) {
        patch.set_parameter(&binding.target, value);
    }
}
```

---

## Web Audio

### WebAudioProcessor

Process audio for Web Audio API.

```rust,ignore
let config = WebAudioConfig {
    sample_rate: 44100.0,
    channels: 2,
    buffer_size: 128,
};

let processor = WebAudioProcessor::new(patch);
```

### WebAudioWorklet

For AudioWorklet integration.

```rust,ignore
let worklet = WebAudioWorklet::new(patch);

// In worklet process()
worklet.process(&input, &mut output);
```

### Interleaving

Web Audio uses interleaved stereo:

```rust,ignore
// Convert from separate channels
let interleaved = interleave_stereo(&left, &right);

// Convert to separate channels
let (left, right) = deinterleave_stereo(&interleaved);
```

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

### OSC-Controlled Parameters

```rust,ignore
let bindings = HashMap::from([
    ("/filter/cutoff", vcf_cutoff_cv.clone()),
    ("/filter/reso", vcf_reso_cv.clone()),
    ("/env/attack", env_attack_cv.clone()),
]);

// In OSC handler
if let Some(cv) = bindings.get(&msg.address.as_str()) {
    if let Some(OscValue::Float(v)) = msg.args.first() {
        cv.set(*v as f64);
    }
}
```
