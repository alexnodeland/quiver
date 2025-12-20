# Oscillators

Oscillators are the sound sources in any synthesizer—they generate the raw waveforms that filters and effects shape.

## VCO (Voltage-Controlled Oscillator)

The primary sound source for subtractive synthesis.

```rust,ignore
let vco = patch.add("vco", Vco::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `voct` | V/Oct | ±10V | Pitch (0V = C4) |
| `fm` | Bipolar CV | ±5V | Frequency modulation |
| `pw` | Unipolar CV | 0-10V | Pulse width (5V = 50%) |
| `sync` | Gate | 0/5V | Hard sync reset |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `sin` | Audio | Sine wave |
| `tri` | Audio | Triangle wave |
| `saw` | Audio | Sawtooth wave |
| `sqr` | Audio | Square/pulse wave |

### Waveform Mathematics

**Sine:**
$$y(t) = A \sin(2\pi f t)$$

**Sawtooth (BLIT):**
$$y(t) = 2 \left( \frac{t}{T} - \lfloor \frac{t}{T} + 0.5 \rfloor \right)$$

**Triangle:**
$$y(t) = 2 \left| 2 \left( \frac{t}{T} - \lfloor \frac{t}{T} + 0.5 \rfloor \right) \right| - 1$$

**Square/Pulse:**
$$y(t) = \text{sign}(\sin(2\pi f t) - \cos(\pi \cdot \text{PW}))$$

### Usage Example

```rust,ignore
// Basic VCO with external pitch
patch.connect(pitch_cv.out("out"), vco.in_("voct"))?;

// FM synthesis
patch.connect(modulator.out("sin"), vco.in_("fm"))?;

// PWM (pulse width modulation)
patch.connect(lfo.out("tri"), vco.in_("pw"))?;
```

---

## LFO (Low-Frequency Oscillator)

Sub-audio oscillator for modulation.

```rust,ignore
let lfo = patch.add("lfo", Lfo::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `rate` | Unipolar CV | 0-10V | Frequency (0.01-30 Hz) |
| `depth` | Unipolar CV | 0-10V | Output amplitude |
| `reset` | Trigger | 0/5V | Phase reset |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `sin` | Bipolar CV | Sine wave (±5V) |
| `tri` | Bipolar CV | Triangle wave |
| `saw` | Bipolar CV | Sawtooth wave |
| `sqr` | Bipolar CV | Square wave |
| `sin_uni` | Unipolar CV | Unipolar sine (0-10V) |

### Rate Mapping

Default rate curve:
$$f = 0.01 \cdot e^{(\text{CV}/10) \cdot \ln(3000)}$$

| CV | Frequency |
|----|-----------|
| 0V | 0.01 Hz |
| 5V | ~1 Hz |
| 10V | 30 Hz |

---

## Noise Generator

White and pink noise sources.

```rust,ignore
let noise = patch.add("noise", NoiseGenerator::new());
```

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `white_left` | Audio | White noise (left) |
| `white_right` | Audio | White noise (right) |
| `pink_left` | Audio | Pink noise (left) |
| `pink_right` | Audio | Pink noise (right) |

### Noise Spectra

**White noise**: Equal energy per frequency (flat spectrum)

$$S(f) = \text{constant}$$

**Pink noise**: Equal energy per octave (-3dB/octave)

$$S(f) \propto \frac{1}{f}$$

Pink noise is generated using the Voss-McCartney algorithm.

---

## AnalogVco

VCO with analog imperfections for authentic vintage sound.

```rust,ignore
use quiver::analog::AnalogVco;

let vco = patch.add("vco", AnalogVco::new(44100.0));
```

### Additional Features

- V/Oct tracking errors
- Component tolerance variation
- High-frequency rolloff
- Soft saturation

See [Analog Modeling](../concepts/analog-modeling.md) for details.

---

## Common Patterns

### Detuned Oscillators

```rust,ignore
let vco1 = patch.add("vco1", Vco::new(sr));
let vco2 = patch.add("vco2", Vco::new(sr));

// Slight detune for thickness
let detune = patch.add("detune", Offset::new(0.01));  // ~12 cents

patch.connect(pitch.out("out"), vco1.in_("voct"))?;
patch.connect(pitch.out("out"), vco2.in_("voct"))?;
patch.connect(detune.out("out"), vco2.in_("voct"))?;  // Adds to pitch
```

### Hard Sync

```rust,ignore
// Slave syncs to master
patch.connect(master.out("sqr"), slave.in_("sync"))?;

// Modulate slave pitch for classic sync sweep
patch.connect(lfo.out("sin"), slave.in_("voct"))?;
```

### FM Synthesis

```rust,ignore
// Carrier:Modulator = 1:1 for harmonic FM
patch.connect(modulator.out("sin"), carrier.in_("fm"))?;

// Control FM depth with envelope
patch.connect(env.out("env"), fm_vca.in_("cv"))?;
patch.connect(fm_vca.out("out"), carrier.in_("fm"))?;
```
