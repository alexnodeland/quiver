# Effects

Signal processing effects for shaping sound character.

## Saturator

Soft clipping distortion based on analog saturation curves.

```rust,ignore
use quiver::analog::{Saturator, saturation};

let sat = patch.add("saturator", Saturator::new(saturation::tanh_sat));
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `in` | Audio | Input signal |
| `drive` | Unipolar CV | Saturation amount |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Saturated output |

### Saturation Types

| Function | Character |
|----------|-----------|
| `tanh_sat` | Smooth, tube-like |
| `soft_clip` | Adjustable knee |
| `asym_sat` | Even harmonics |
| `diode_clip` | Hard, aggressive |

---

## Wavefolder

Creates complex harmonics by reflecting the signal about a threshold. Supports opt-in
oversampling via `set_oversample`. `type_id`: `wavefolder`.

```rust,ignore
let folder = patch.add("folder", Wavefolder::new(1.0)); // threshold
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `in` | Audio | Input signal |
| `threshold` | Unipolar CV | Fold threshold (default = constructor value) |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Folded output |

### The Folding Process

```
Input:   ╱╲
        ╱  ╲
       ╱    ╲

1 Fold: ╱╲╱╲
       ╱    ╲

2 Folds: ╱╲╱╲╱╲╱╲
        ╱      ╲
```

\\[ y = \sin(f \cdot \pi \cdot x) \\]

Where \\( f \\) is the fold amount.

---

## Crosstalk

Simulates channel bleed between left and right.

```rust,ignore
let crosstalk = patch.add("xtalk", Crosstalk::new());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `left` | Audio | Left channel |
| `right` | Audio | Right channel |
| `amount` | Unipolar CV | Bleed amount (0-10%) |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `left` | Audio | Left with right bleed |
| `right` | Audio | Right with left bleed |

### The Effect

\\[ L_{out} = L_{in} + \text{amount} \cdot R_{in} \\]
\\[ R_{out} = R_{in} + \text{amount} \cdot L_{in} \\]

Adds subtle width and analog character.

---

## Ground Loop

Simulates 50/60Hz power supply hum.

```rust,ignore
let hum = patch.add("hum", GroundLoop::new(44100.0));
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `amount` | Unipolar CV | Hum level |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Hum signal |

### Configuration

```rust,ignore
let hum = GroundLoop::new(44100.0)
    .with_frequency(60.0)   // 60Hz (US) or 50Hz (EU)
    .with_harmonics(3);     // Include 2nd and 3rd harmonics
```

Mix very subtly for vintage authenticity.

---

## Signal Monitoring (Scope, Spectrum Analyzer, Level Meter)

`Scope`, `SpectrumAnalyzer`, and `LevelMeter` are standalone visual tools, not
graph modules—they cannot be added to a patch with `patch.add(...)`. Instead,
feed them samples from `patch.tick()`:

```rust,ignore
let mut scope = Scope::new(1024);
let mut meter = LevelMeter::new(44100.0);

let (left, _right) = patch.tick();
scope.tick(left);
meter.tick(left);
```

See [Visualize Your Patch](../how-to/visualization.md) for the full API.

---

## Distortion

Waveshaping distortion with four selectable algorithms (soft clip, hard clip, foldback,
asymmetric), a one-pole tone control, dry/wet mix, and opt-in oversampling
(`set_oversample`). `type_id`: `distortion`.

```rust,ignore
let dist = patch.add("dist", Distortion::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `drive` | Unipolar CV | 0-10V | Drive into the shaper, default 0.5 |
| `tone` | Unipolar CV | 0-10V | Tone (one-pole lowpass), default 0.5 |
| `mode` | Unipolar CV | 0-10V | Algorithm select (4 modes) |
| `mix` | Unipolar CV | 0-10V | Dry/wet, default fully wet |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Distorted output |

---

## Bitcrusher

Lo-fi bit-depth and sample-rate reduction. `type_id`: `bitcrusher`.

```rust,ignore
let crush = patch.add("crush", Bitcrusher::new());
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `bits` | Unipolar CV | 0-10V | Bit-depth reduction (~1–16 bits), default 0.5 |
| `downsample` | Unipolar CV | 0-10V | Sample-rate reduction |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Crushed output |

---

## Delay Line

Delay of up to 2 seconds with feedback and wet/dry mix; slew-smoothed delay time for
CV-modulated effects. Breaks feedback cycles. `type_id`: `delay_line`.

```rust,ignore
let delay = patch.add("delay", DelayLine::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `time` | Unipolar CV | 0-10V | Delay time (1 ms–2 s, exponential), default 0.5 |
| `feedback` | Unipolar CV | 0-10V | Feedback (0–0.99) |
| `mix` | Unipolar CV | 0-10V | Dry/wet, default 0.5 |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Mixed dry + delayed output |

---

## Chorus

Three-voice modulated-delay chorus with a mono and a stereo-spread output. `type_id`: `chorus`.

```rust,ignore
let chorus = patch.add("chorus", Chorus::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `rate` | Unipolar CV | 0-10V | LFO rate (0.1–5 Hz), default 0.3 |
| `depth` | Unipolar CV | 0-10V | Modulation depth (0–25 ms), default 0.5 |
| `mix` | Unipolar CV | 0-10V | Dry/wet, default 0.5 |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Mono mixed output |
| `left` | Audio | Left stereo-spread output |
| `right` | Audio | Right stereo-spread output |

---

## Flanger

Short-modulated-delay flanger with feedback; mono in, stereo out via a `spread` control.
`out` mirrors `left` for backward compatibility. `type_id`: `flanger`.

```rust,ignore
let flanger = patch.add("flanger", Flanger::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `rate` | Unipolar CV | 0-10V | LFO rate, default 0.3 |
| `depth` | Unipolar CV | 0-10V | Sweep depth, default 0.5 |
| `feedback` | Bipolar CV | ±5V | Feedback (−0.95–0.95) |
| `mix` | Unipolar CV | 0-10V | Dry/wet, default 0.5 |
| `spread` | Unipolar CV | 0-10V | Stereo L/R decorrelation (0 = mono, 1 = 180°), default 0.5 |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Legacy mono output (mirrors `left`) |
| `left` | Audio | Left channel |
| `right` | Audio | Right channel (phase-offset sweep) |

---

## Phaser

Cascaded-allpass phaser (2/4/6 selectable stages) with feedback; mono in, stereo out with
a `spread` control. `out` mirrors `left`. `type_id`: `phaser`.

```rust,ignore
let phaser = patch.add("phaser", Phaser::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `rate` | Unipolar CV | 0-10V | LFO rate, default 0.3 |
| `depth` | Unipolar CV | 0-10V | Notch sweep depth, default 0.7 |
| `feedback` | Bipolar CV | ±5V | Feedback (−0.95–0.95) |
| `mix` | Unipolar CV | 0-10V | Dry/wet, default 0.5 |
| `stages` | Unipolar CV | 0-10V | Allpass stage count (<0.33 → 2, <0.66 → 4, else 6) |
| `spread` | Unipolar CV | 0-10V | Stereo L/R decorrelation, default 0.5 |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Legacy mono output (mirrors `left`) |
| `left` | Audio | Left channel |
| `right` | Audio | Right channel (phase-offset notch sweep) |

---

## Tremolo

Amplitude-modulation tremolo with a sine-to-triangle shape blend. `type_id`: `tremolo`.

```rust,ignore
let trem = patch.add("trem", Tremolo::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `rate` | Unipolar CV | 0-10V | LFO rate (0.1–20 Hz), default 0.3 |
| `depth` | Unipolar CV | 0-10V | Modulation depth, default 0.5 |
| `shape` | Unipolar CV | 0-10V | LFO shape blend (sine ↔ triangle) |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Amplitude-modulated output |

---

## Vibrato

Pitch-modulation vibrato via a modulated delay line; defaults fully wet. `type_id`: `vibrato`.

```rust,ignore
let vib = patch.add("vib", Vibrato::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `rate` | Unipolar CV | 0-10V | LFO rate (0.1–15 Hz), default 0.3 |
| `depth` | Unipolar CV | 0-10V | Pitch-modulation depth, default 0.5 |
| `mix` | Unipolar CV | 0-10V | Dry/wet, default fully wet |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Pitch-modulated output |

---

## Reverb

Freeverb-style algorithmic reverb (8 comb + 4 allpass) with size, damping, mix, and
pre-delay. Stereo output. `type_id`: `reverb`.

```rust,ignore
let reverb = patch.add("reverb", Reverb::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `size` | Unipolar CV | 0-10V | Room size / decay, default 0.5 |
| `damping` | Unipolar CV | 0-10V | High-frequency damping, default 0.5 |
| `mix` | Unipolar CV | 0-10V | Dry/wet, default 0.5 |
| `predelay` | Unipolar CV | 0-10V | Pre-delay (0–100 ms) |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `left` | Audio | Left reverb channel |
| `right` | Audio | Right reverb channel |

---

## Pitch Shifter

Granular (two-grain, crossfaded) real-time pitch shifter. `type_id`: `pitch_shifter`.

```rust,ignore
let shift = patch.add("shift", PitchShifter::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `shift` | Bipolar CV | ±5V | Pitch shift (±24 semitones) |
| `window` | Unipolar CV | 0-10V | Grain window (10–100 ms), default 0.5 |
| `mix` | Unipolar CV | 0-10V | Dry/wet, default fully wet |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Pitch-shifted output |

---

## Granular

Granular processor: records the input into a circular buffer and plays overlapping
Hann-windowed grains. `type_id`: `granular`.

```rust,ignore
let gran = patch.add("gran", Granular::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio recorded into the buffer |
| `position` | Unipolar CV | 0-10V | Playback position, default 0.5 |
| `size` | Unipolar CV | 0-10V | Grain size (10–500 ms), default 0.3 |
| `density` | Unipolar CV | 0-10V | Grains per second (1–20), default 0.5 |
| `pitch` | Bipolar CV | ±5V | Pitch shift (±24 semitones) |
| `spray` | Unipolar CV | 0-10V | Position randomization, default 0.1 |
| `freeze` | Gate | 0/5V | Stops recording while high |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Granular output |

---

## Vocoder

Channel vocoder: per-band envelope followers on the modulator impose its spectral
envelope onto the carrier. `type_id`: `vocoder`.

```rust,ignore
let voc = patch.add("voc", Vocoder::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `carrier` | Audio | ±5V | Carrier (typically an oscillator) |
| `modulator` | Audio | ±5V | Modulator (typically voice) |
| `bands` | Unipolar CV | 0-10V | Band count (4–16), default 1.0 |
| `attack` | Unipolar CV | 0-10V | Envelope-follower attack, default 0.3 |
| `release` | Unipolar CV | 0-10V | Envelope-follower release, default 0.3 |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Vocoded output |

---

## Building Effect Chains

### Serial Processing

```rust,ignore
// Input → Saturator → Filter → Output
patch.connect(input, sat.in_("in"))?;
patch.connect(sat.out("out"), vcf.in_("in"))?;
patch.connect(vcf.out("lp"), output)?;
```

### Parallel Processing

```rust,ignore
// Dry/wet mix
patch.connect(input, mult.in_("in"))?;
patch.connect(mult.out("out_1"), effect.in_("in"))?;  // Wet
patch.connect(mult.out("out_2"), xfade.in_("a"))?;    // Dry
patch.connect(effect.out("out"), xfade.in_("b"))?;    // Wet
```

### Feedback Loop

```rust,ignore
// With unit delay to prevent infinite loop
patch.connect(effect.out("out"), delay.in_("in"))?;
patch.connect(delay.out("out"), atten.in_("in"))?;  // Feedback amount
patch.connect(atten.out("out"), effect.in_("in"))?;
```
