# Logic & CV Processing

Modules for gate logic, CV comparison, and signal routing.

## Logic Gates

### LogicAnd

Outputs HIGH only when both inputs are HIGH.

```rust,ignore
let and_gate = patch.add("and", LogicAnd::new());
```

| Inputs | Output |
|--------|--------|
| 0V, 0V | 0V |
| 0V, 5V | 0V |
| 5V, 0V | 0V |
| 5V, 5V | 5V |

---

### LogicOr

Outputs HIGH when either input is HIGH.

```rust,ignore
let or_gate = patch.add("or", LogicOr::new());
```

| Inputs | Output |
|--------|--------|
| 0V, 0V | 0V |
| 0V, 5V | 5V |
| 5V, 0V | 5V |
| 5V, 5V | 5V |

---

### LogicXor

Outputs HIGH when exactly one input is HIGH.

```rust,ignore
let xor_gate = patch.add("xor", LogicXor::new());
```

| Inputs | Output |
|--------|--------|
| 0V, 0V | 0V |
| 0V, 5V | 5V |
| 5V, 0V | 5V |
| 5V, 5V | 0V |

---

### LogicNot

Inverts the input.

```rust,ignore
let not_gate = patch.add("not", LogicNot::new());
```

| Input | Output |
|-------|--------|
| 0V | 5V |
| 5V | 0V |

---

## Comparators

### Comparator

Compares two voltages.

```rust,ignore
let cmp = patch.add("cmp", Comparator::new());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `a` | CV | First signal |
| `b` | CV | Second signal |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `gt` | Gate | HIGH if A > B |
| `lt` | Gate | HIGH if A < B |
| `eq` | Gate | HIGH if A ≈ B (within threshold) |

### Use Cases

```rust,ignore
// Trigger envelope when LFO rises above threshold
patch.connect(lfo.out("sin"), cmp.in_("a"))?;
patch.connect(threshold.out("out"), cmp.in_("b"))?;
patch.connect(cmp.out("gt"), env.in_("gate"))?;
```

---

## Min/Max

### Min

Outputs the lower of two signals.

```rust,ignore
let min = patch.add("min", Min::new());
```

$$\text{out} = \min(a, b)$$

### Max

Outputs the higher of two signals.

```rust,ignore
let max = patch.add("max", Max::new());
```

$$\text{out} = \max(a, b)$$

### Use Case: Limiting

```rust,ignore
// Limit modulation depth
patch.connect(lfo.out("sin"), min.in_("a"))?;
patch.connect(limit.out("out"), min.in_("b"))?;  // Maximum value
```

---

## Rectifiers

### Rectifier

Converts bipolar signals to various forms.

```rust,ignore
let rect = patch.add("rect", Rectifier::new());
```

### Outputs

| Port | Description | Formula |
|------|-------------|---------|
| `full` | Full-wave rectified | $|x|$ |
| `half_pos` | Positive half only | $\max(x, 0)$ |
| `half_neg` | Negative half only | $\min(x, 0)$ |
| `abs` | Absolute value | $|x|$ |

```
Input:      ╱╲  ╱╲
           ╱  ╲╱  ╲
Full:      ╱╲╱╲╱╲╱╲

Half+:     ╱╲  ╱╲
           ──╲╱──╲╱

Half-:       ╲╱  ╲╱
           ──  ──
```

### Audio Applications

- Octave doubling (full-wave rectify audio)
- Envelope following (rectify + lowpass)
- Distortion effects

---

## Signal Routing

### VcSwitch

Voltage-controlled signal router.

```rust,ignore
let switch = patch.add("switch", VcSwitch::new());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `a` | Any | First signal |
| `b` | Any | Second signal |
| `select` | Gate | Which to output |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Any | Selected signal |

When `select` < 2.5V: output A
When `select` >= 2.5V: output B

---

### BernoulliGate

Probabilistic gate router.

```rust,ignore
let bernoulli = patch.add("bernoulli", BernoulliGate::new());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `trigger` | Trigger | Input trigger |
| `probability` | Unipolar CV | Chance of A (0-100%) |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `a` | Trigger | Probabilistic output A |
| `b` | Trigger | Probabilistic output B |

When trigger arrives:
- With probability P: fires A
- With probability 1-P: fires B

### Use Case: Random Variations

```rust,ignore
// 70% chance of normal note, 30% chance of accent
patch.connect(clock.out("div_8"), bernoulli.in_("trigger"))?;
patch.connect(prob_cv.out("out"), bernoulli.in_("probability"))?;
patch.connect(bernoulli.out("a"), normal_env.in_("gate"))?;
patch.connect(bernoulli.out("b"), accent_env.in_("gate"))?;
```

---

## Ring Modulator

Four-quadrant multiplier for metallic sounds.

```rust,ignore
let ring = patch.add("ring", RingModulator::new());
```

### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `carrier` | Audio | Carrier signal |
| `modulator` | Audio | Modulator signal |

### Output

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Product (ring mod) |

### Mathematics

$$\text{out} = \text{carrier} \times \text{modulator}$$

Creates sum and difference frequencies:
$$\cos(f_1 t) \cdot \cos(f_2 t) = \frac{1}{2}[\cos((f_1-f_2)t) + \cos((f_1+f_2)t)]$$

### Sound Character

- Bell-like tones with related frequencies
- Metallic, robotic sounds with unrelated frequencies
- Classic AM radio sound
