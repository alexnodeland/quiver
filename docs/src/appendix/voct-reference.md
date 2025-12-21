# V/Oct Reference

Complete reference for the Volt-per-Octave pitch standard.

## The Standard

**1 Volt = 1 Octave**

$$f = f_0 \cdot 2^V$$

Where:
- $f$ = frequency in Hz
- $f_0$ = 261.63 Hz (C4 at 0V)
- $V$ = voltage

## Complete Note Table

| Note | MIDI | V/Oct | Frequency |
|------|------|-------|-----------|
| C0 | 12 | -4.000V | 16.35 Hz |
| C1 | 24 | -3.000V | 32.70 Hz |
| C2 | 36 | -2.000V | 65.41 Hz |
| C3 | 48 | -1.000V | 130.81 Hz |
| **C4** | **60** | **0.000V** | **261.63 Hz** |
| C5 | 72 | +1.000V | 523.25 Hz |
| C6 | 84 | +2.000V | 1046.50 Hz |
| C7 | 96 | +3.000V | 2093.00 Hz |
| C8 | 108 | +4.000V | 4186.01 Hz |

## Chromatic Scale (Octave 4)

| Note | MIDI | V/Oct | Frequency |
|------|------|-------|-----------|
| C4 | 60 | +0.000V | 261.63 Hz |
| C#4 | 61 | +0.083V | 277.18 Hz |
| D4 | 62 | +0.167V | 293.66 Hz |
| D#4 | 63 | +0.250V | 311.13 Hz |
| E4 | 64 | +0.333V | 329.63 Hz |
| F4 | 65 | +0.417V | 349.23 Hz |
| F#4 | 66 | +0.500V | 369.99 Hz |
| G4 | 67 | +0.583V | 392.00 Hz |
| G#4 | 68 | +0.667V | 415.30 Hz |
| A4 | 69 | +0.750V | 440.00 Hz |
| A#4 | 70 | +0.833V | 466.16 Hz |
| B4 | 71 | +0.917V | 493.88 Hz |

## Intervals

| Interval | Semitones | Voltage |
|----------|-----------|---------|
| Unison | 0 | 0.000V |
| Minor 2nd | 1 | 0.083V |
| Major 2nd | 2 | 0.167V |
| Minor 3rd | 3 | 0.250V |
| Major 3rd | 4 | 0.333V |
| Perfect 4th | 5 | 0.417V |
| Tritone | 6 | 0.500V |
| Perfect 5th | 7 | 0.583V |
| Minor 6th | 8 | 0.667V |
| Major 6th | 9 | 0.750V |
| Minor 7th | 10 | 0.833V |
| Major 7th | 11 | 0.917V |
| Octave | 12 | 1.000V |

## Precise Values

### Semitone

$$1 \text{ semitone} = \frac{1}{12} \text{V} = 83.33\overline{3} \text{mV}$$

### Cent

$$1 \text{ cent} = \frac{1}{1200} \text{V} = 0.833\overline{3} \text{mV}$$

## Conversion Functions

### MIDI to V/Oct

```rust,ignore
fn midi_to_voct(note: u8) -> f64 {
    (note as f64 - 60.0) / 12.0
}
```

### V/Oct to MIDI

```rust,ignore
fn voct_to_midi(v: f64) -> u8 {
    (v * 12.0 + 60.0).round() as u8
}
```

### V/Oct to Frequency

```rust,ignore
fn voct_to_hz(v: f64) -> f64 {
    261.63 * 2.0_f64.powf(v)
}
```

### Frequency to V/Oct

```rust,ignore
fn hz_to_voct(f: f64) -> f64 {
    (f / 261.63).log2()
}
```

## Common Tuning Offsets

| Offset | Effect |
|--------|--------|
| +1V | Up one octave |
| -1V | Down one octave |
| +0.583V | Up a fifth |
| +0.333V | Up a major third |
| +0.01V | ~12 cents (detune) |

## Tracking Errors

Real analog oscillators have tracking errors:

| Error Type | Typical Amount |
|------------|---------------|
| Scale error | ±1-5% |
| Offset error | ±10-50mV |
| Temperature drift | 1-5mV/°C |

At high frequencies, these compound and cause tuning issues.

## A440 Reference

A4 (440 Hz) = MIDI 69 = +0.750V

To tune to A=440:
- C4 must be at 261.63 Hz (0V)
- Ratio: 440/261.63 = 1.682

## Microtonal

For non-12TET tunings:

```rust,ignore
// Pythagorean major third (81/64 instead of 5/4)
let pythagorean_third = (81.0_f64 / 64.0).log2();
// = 0.339 V instead of 0.333 V

// Just intonation fifth (3/2)
let just_fifth = 1.5_f64.log2();
// = 0.585 V instead of 0.583 V
```
