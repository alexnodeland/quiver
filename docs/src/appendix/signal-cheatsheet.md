# Signal Types Cheatsheet

Quick reference for Quiver's signal conventions.

## Signal Ranges

| Type | Range | Zero Point | Use |
|------|-------|------------|-----|
| **Audio** | ±5V | 0V | Sound waveforms |
| **CV Unipolar** | 0-10V | 0V | Cutoff, rate, depth |
| **CV Bipolar** | ±5V | 0V | Pan, FM, bend |
| **V/Oct** | ±10V | 0V = C4 | Pitch |
| **Gate** | 0V or 5V | 0V | Sustained on/off |
| **Trigger** | 0V or 5V | 0V | Brief pulse |
| **Clock** | 0V or 5V | 0V | Timing pulses |

## SignalKind Enum

```rust,ignore
pub enum SignalKind {
    Audio,           // ±5V AC-coupled
    CvBipolar,       // ±5V control
    CvUnipolar,      // 0-10V control
    VoltPerOctave,   // 1V/Oct pitch
    Gate,            // 0V/+5V sustained
    Trigger,         // 0V/+5V pulse
    Clock,           // Timing pulses
}
```

## PortDef Factory Methods

| Method | Signal Kind | Default | Attenuverter |
|--------|-------------|---------|--------------|
| `::audio()` | Audio | 0.0 | No |
| `::cv_unipolar()` | CvUnipolar | 0.0 | Yes |
| `::cv_bipolar()` | CvBipolar | 0.0 | Yes |
| `::voct()` | VoltPerOctave | 0.0 | No |
| `::gate()` | Gate | 0.0 | No |
| `::trigger()` | Trigger | 0.0 | No |
| `::clock()` | Clock | 0.0 | No |

## Compatibility Quick Reference

```
Audio ←→ CV:      ⚠ Works but check intent
CV ←→ V/Oct:      ⚠ Usually wrong
Gate ←→ Trigger:  ✓ Compatible
Clock ←→ Trigger: ✓ Compatible
V/Oct ←→ Audio:   ✗ Usually wrong
```

## Common Voltage Conversions

### MIDI Note to V/Oct

```rust,ignore
fn midi_to_voct(note: u8) -> f64 {
    (note as f64 - 60.0) / 12.0
}
```

### V/Oct to Frequency

```rust,ignore
fn voct_to_hz(v: f64) -> f64 {
    261.63 * 2.0_f64.powf(v)
}
```

### MIDI CC to CV

```rust,ignore
// 0-127 → 0-10V
fn cc_to_cv(cc: u8) -> f64 {
    cc as f64 / 127.0 * 10.0
}

// 0-127 → ±5V
fn cc_to_cv_bipolar(cc: u8) -> f64 {
    (cc as f64 / 127.0 - 0.5) * 10.0
}
```

### MIDI Velocity to CV

```rust,ignore
fn velocity_to_cv(vel: u8) -> f64 {
    vel as f64 / 127.0 * 10.0  // 0-10V
}
```

### Pitch Bend to V/Oct

```rust,ignore
// Standard ±2 semitones
fn bend_to_voct(bend: i16) -> f64 {
    (bend as f64 / 8192.0) * (2.0 / 12.0)
}
```

## Attenuverter Reference

| Value | Effect |
|-------|--------|
| -2.0 | Invert and double |
| -1.0 | Invert |
| -0.5 | Invert and halve |
| 0.0 | Silence |
| 0.5 | Half level |
| 1.0 | Unity (unchanged) |
| 2.0 | Double |

## Cable Attenuation

```rust,ignore
Cable::new()
    .with_attenuation(0.5)   // Scale signal
    .with_offset(2.0)        // Add DC offset
```

## Input Summing

Multiple cables to one input are added:

```
LFO1 (+2V) ─┐
            ├── Input receives +5V
LFO2 (+3V) ─┘
```

## Normalled Connections

When input is unpatched, uses normalled source:

```rust,ignore
PortDef::audio().with_normalled_to("other_port")
```
