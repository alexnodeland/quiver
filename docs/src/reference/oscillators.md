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
| `voct` | V/Oct | ±5V | Pitch (0V = C4) |
| `fm` | Bipolar CV | ±5V | Exponential FM (±5V ≈ ±5 octaves) |
| `pw` | Unipolar CV | 0-10V | Pulse width, default 50% |
| `sync` | Gate | 0/5V | Hard sync reset |
| `fm_lin` | Bipolar CV | ±5V | Linear through-zero FM (±5V ≈ ±100% of base freq) |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `sin` | Audio | Sine wave |
| `tri` | Audio | Triangle wave (bandlimited, PolyBLAMP) |
| `saw` | Audio | Sawtooth wave (bandlimited, PolyBLEP) |
| `sqr` | Audio | Square/pulse wave (bandlimited, PolyBLEP) |

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

White and pink noise sources with a CV-controllable stereo second channel.

```rust,ignore
let noise = patch.add("noise", NoiseGenerator::new());
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `correlation` | Unipolar CV | 0-10V | Stereo correlation (0 = independent, 1 = identical), default 0.3 |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `white` | Audio | White noise |
| `pink` | Audio | Pink noise |
| `white2` | Audio | Second white channel, correlated with `white` |
| `pink2` | Audio | Second pink channel, correlated with `pink` |

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

## Supersaw

JP-8000-style stack of seven detuned PolyBLEP saws with an octave-down sub. `type_id`: `supersaw`.

```rust,ignore
let saw = patch.add("saw", Supersaw::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `voct` | V/Oct | ±5V | Pitch (0V = C4) |
| `detune` | Unipolar CV | 0-10V | Detune spread of the 7 voices, default 50% |
| `mix` | Unipolar CV | 0-10V | Blend between center voice and full supersaw, default 50% |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Mixed 7-oscillator supersaw |
| `sub` | Audio | Octave-down bandlimited saw sub-oscillator |

---

## Wavetable

Mip-mapped, bandlimited wavetable oscillator with 8 tables and smooth crossfade morphing. `type_id`: `wavetable`.

```rust,ignore
let wt = patch.add("wt", Wavetable::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `v_oct` | V/Oct | ±5V | Pitch (0V = C4) |
| `table` | Unipolar CV | 0-10V | Table select across 8 tables |
| `morph` | Unipolar CV | 0-10V | Crossfade morph between adjacent tables |
| `sync` | Gate | 0/5V | Hard sync (resets phase) |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Wavetable output |

The 8 built-in tables are: Sine, Triangle, Saw, Square, Pulse (25%), Pulse (12%), Formant A, Formant O.

---

## FormantOsc

Vocal-synthesis oscillator: a glottal pulse driven through five parallel resonant formant filters. `type_id`: `formant_osc`.

```rust,ignore
let vox = patch.add("vox", FormantOsc::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `v_oct` | V/Oct | ±5V | Pitch (0V = C4) |
| `vowel` | Unipolar CV | 0-10V | Interpolates the vowel A → E → I → O → U |
| `formant_shift` | Bipolar CV | ±5V | Shifts all formant frequencies (0.5×–2×) |
| `vibrato` | Unipolar CV | 0-10V | Vibrato depth (up to ±0.5 semitone) |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Vocal formant output |

---

## KarplusStrong

Karplus-Strong physical-model plucked string with damping, brightness, and inharmonicity. `type_id`: `karplus_strong`.

```rust,ignore
let string = patch.add("string", KarplusStrong::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `voct` | V/Oct | ±5V | Pitch → string period |
| `trigger` | Trigger | 0/5V | Rising edge plucks the string |
| `damping` | Unipolar CV | 0-10V | Loop lowpass amount, default 50% |
| `brightness` | Unipolar CV | 0-10V | Noise-vs-impulse excitation blend, default 50% |
| `stretch` | Bipolar CV | ±5V | All-pass stretch / inharmonicity |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Plucked-string output |

---

## SamplePlayer

Mono sample playback with V/Oct pitch, selectable start position, one-shot / looping modes, and an end-of-sample trigger. Reads are cubic-interpolated; the audio path is allocation-free (only `set_buffer` allocates). `type_id`: `sample_player`.

```rust,ignore
// buffer: Vec<f64>, recorded at buffer_sample_rate; engine runs at engine_sample_rate.
let sp = patch.add("sp", SamplePlayer::new(buffer, 44100.0, 44100.0));
// Or start empty and load later with `set_buffer`:
let sp = patch.add("sp", SamplePlayer::empty(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `trig` | Trigger | 0/5V | Rising edge starts one-shot playback |
| `gate` | Gate | 0/5V | Gated playback (release stops it) |
| `voct` | V/Oct | ±5V | Playback pitch (0V = unity rate) |
| `start` | Unipolar CV | 0-10V | Start position (0–1 of buffer) |
| `loop` | Gate | 0/5V | Enables looping when high |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Sample output |
| `eos` | Trigger | Fires at end of sample |

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
