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

Creates complex harmonics through wavefolding.

```rust,ignore
let folder = patch.add("folder", Wavefolder::new());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `in` | Audio | Input signal |
| `folds` | Unipolar CV | Number of folds |

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

$$y = \sin(f \cdot \pi \cdot x)$$

Where $f$ is the fold amount.

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

$$L_{out} = L_{in} + \text{amount} \cdot R_{in}$$
$$R_{out} = R_{in} + \text{amount} \cdot L_{in}$$

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

## Scope

Real-time waveform visualization.

```rust,ignore
let scope = patch.add("scope", Scope::new(44100.0));
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `in` | Audio | Signal to display |
| `trigger` | Gate | Trigger sync |

### Trigger Modes

| Mode | Description |
|------|-------------|
| `Free` | Continuous display |
| `RisingEdge` | Sync on positive zero-cross |
| `FallingEdge` | Sync on negative zero-cross |
| `Single` | One-shot capture |

### Reading the Buffer

```rust,ignore
let waveform = scope.buffer();
// Vec<f64> of recent samples
```

---

## Spectrum Analyzer

FFT-based frequency analysis.

```rust,ignore
let analyzer = patch.add("spectrum", SpectrumAnalyzer::new(44100.0));
```

### Input

| Port | Signal | Description |
|------|--------|-------------|
| `in` | Audio | Signal to analyze |

### Reading Data

```rust,ignore
let bins = analyzer.bins();           // Frequency bins
let mags = analyzer.magnitudes();     // dB values
let peak = analyzer.peak_frequency(); // Dominant frequency
```

---

## Level Meter

RMS and peak level monitoring.

```rust,ignore
let meter = patch.add("meter", LevelMeter::new(44100.0));
```

### Input

| Port | Signal | Description |
|------|--------|-------------|
| `in` | Audio | Signal to meter |

### Reading Levels

```rust,ignore
let rms = meter.rms();       // RMS level in volts
let peak = meter.peak();     // Peak level
let rms_db = meter.rms_db(); // RMS in dB
```

### Peak Hold

```rust,ignore
let meter = LevelMeter::new(44100.0)
    .with_peak_hold(500.0);  // 500ms hold time
```

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
