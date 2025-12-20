# Utilities

Utility modules for signal routing, mixing, and manipulation.

## Mixer

4-channel audio mixer.

```rust,ignore
let mixer = patch.add("mixer", Mixer::new());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `in_1` - `in_4` | Audio | Audio inputs |
| `gain_1` - `gain_4` | Unipolar CV | Channel gains |
| `master` | Unipolar CV | Master gain |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Mixed output |

---

## VCA (Voltage-Controlled Amplifier)

Controls signal amplitude with CV.

```rust,ignore
let vca = patch.add("vca", Vca::new());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `in` | Audio | Audio input |
| `cv` | Unipolar CV | Gain control (0-10V = 0-100%) |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Amplitude-controlled output |

### Response

Linear response:
$$\text{out} = \text{in} \times \frac{\text{cv}}{10}$$

---

## Attenuverter

Attenuates, inverts, or amplifies signals.

```rust,ignore
let atten = patch.add("atten", Attenuverter::new());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `in` | Any | Input signal |
| `amount` | Bipolar CV | Scale factor (-2 to +2) |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Any | Scaled output |

### Amount Values

| Amount | Effect |
|--------|--------|
| -2.0 | Inverted and doubled |
| -1.0 | Inverted |
| 0.0 | Silent |
| 0.5 | Half level |
| 1.0 | Unity (unchanged) |
| 2.0 | Doubled |

---

## Offset

Adds DC offset (constant voltage source).

```rust,ignore
let offset = patch.add("offset", Offset::new(5.0));  // 5V
```

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | CV | Constant voltage |

### Common Uses

```rust,ignore
// Center LFO modulation
patch.connect(offset.out("out"), vcf.in_("cutoff"))?;  // Base cutoff
patch.connect(lfo.out("sin"), vcf.in_("fm"))?;         // Modulation
```

---

## Multiple

Signal splitter (1 input to 4 outputs).

```rust,ignore
let mult = patch.add("mult", Multiple::new());
```

### Input

| Port | Signal | Description |
|------|--------|-------------|
| `in` | Any | Input signal |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out_1` - `out_4` | Any | Identical copies |

---

## UnitDelay

Single-sample delay (z⁻¹).

```rust,ignore
let delay = patch.add("delay", UnitDelay::new());
```

### Input/Output

| Port | Signal | Description |
|------|--------|-------------|
| `in` | Any | Input |
| `out` | Any | Delayed by 1 sample |

Essential for feedback loops.

---

## Crossfader

Crossfade between two signals with equal-power curve.

```rust,ignore
let xfade = patch.add("xfade", Crossfader::new());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `a` | Audio | First signal |
| `b` | Audio | Second signal |
| `mix` | Unipolar CV | Crossfade position |
| `pan` | Bipolar CV | Stereo position |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `left` | Audio | Left output |
| `right` | Audio | Right output |

### Equal Power Curve

$$\text{gain}_A = \cos\left(\frac{\pi}{2} \cdot \text{mix}\right)$$
$$\text{gain}_B = \sin\left(\frac{\pi}{2} \cdot \text{mix}\right)$$

---

## Precision Adder

High-precision CV addition for V/Oct signals.

```rust,ignore
let adder = patch.add("adder", PrecisionAdder::new());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `a` | V/Oct | First pitch |
| `b` | V/Oct | Second pitch (offset) |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | V/Oct | Sum of pitches |

Use for transpose, octave shifts, and pitch offsets.

---

## StereoOutput

Final stereo output stage.

```rust,ignore
let output = patch.add("output", StereoOutput::new());
patch.set_output(output.id());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `left` | Audio | Left channel |
| `right` | Audio | Right channel (normalled to left) |

### Behavior

If only `left` is patched, `right` mirrors it (mono).

---

## ExternalInput

Injects external CV/audio into the patch.

```rust,ignore
use std::sync::Arc;
let cv = Arc::new(AtomicF64::new(0.0));
let input = patch.add("pitch", ExternalInput::voct(Arc::clone(&cv)));
```

### Factory Methods

| Method | Signal Type |
|--------|-------------|
| `::voct()` | V/Oct pitch |
| `::gate()` | Gate signal |
| `::trigger()` | Trigger |
| `::cv()` | Unipolar CV |
| `::cv_bipolar()` | Bipolar CV |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Varies | External value |

---

## Common Patterns

### Voltage Processing Chain

```rust,ignore
// LFO → Attenuverter → Offset → Target
// Allows precise control of modulation depth and center
patch.connect(lfo.out("sin"), atten.in_("in"))?;
patch.connect(atten.out("out"), adder.in_("a"))?;
patch.connect(offset.out("out"), adder.in_("b"))?;
patch.connect(adder.out("out"), vcf.in_("cutoff"))?;
```

### Parallel Signal Path

```rust,ignore
// Split signal to dry and wet paths
patch.connect(input, mult.in_("in"))?;
patch.connect(mult.out("out_1"), dry_path)?;
patch.connect(mult.out("out_2"), wet_path)?;
patch.connect(dry_path, xfade.in_("a"))?;
patch.connect(wet_path, xfade.in_("b"))?;
```
