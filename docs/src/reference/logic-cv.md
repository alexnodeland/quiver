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

\\[ \text{out} = \min(a, b) \\]

### Max

Outputs the higher of two signals.

```rust,ignore
let max = patch.add("max", Max::new());
```

\\[ \text{out} = \max(a, b) \\]

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
| `full` | Full-wave rectified | \\( \vert x \vert \\) |
| `half_pos` | Positive half only | \\( \max(x, 0) \\) |
| `half_neg` | Negative half only | \\( \min(x, 0) \\) |
| `abs` | Absolute value | \\( \vert x \vert \\) |

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

\\[ \text{out} = \text{carrier} \times \text{modulator} \\]

Creates sum and difference frequencies:
\\[ \cos(f_1 t) \cdot \cos(f_2 t) = \frac{1}{2}[\cos((f_1-f_2)t) + \cos((f_1+f_2)t)] \\]

### Sound Character

- Bell-like tones with related frequencies
- Metallic, robotic sounds with unrelated frequencies
- Classic AM radio sound

---

## Sequencing

### Arpeggiator

Captures held notes on gate edges and replays them across selectable octaves and patterns
on each clock pulse. `type_id`: `arpeggiator`.

```rust,ignore
let arp = patch.add("arp", Arpeggiator::new(44100.0));
```

#### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `v_oct` | V/Oct | Input note to capture |
| `gate` | Gate | Captures/releases the note on rising/falling edge |
| `clock` | Clock | Advances the sequence |
| `pattern` | Unipolar CV | Pattern select (Up / Down / UpDown / Random) |
| `octaves` | Unipolar CV | Octave range (1–4) |
| `reset` | Gate | Resets the sequence and clears held notes |

#### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `v_oct_out` | V/Oct | Arpeggiated pitch |
| `gate_out` | Gate | Gate output (follows the clock) |
| `trigger` | Trigger | Pulse on each step |

---

### Chord Memory

Generates four V/Oct voices from a root note across nine chord types, with inversion and
octave spread. `type_id`: `chord_memory`.

```rust,ignore
let chord = patch.add("chord", ChordMemory::new());
```

#### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `root` | V/Oct | Root note of the chord |
| `chord` | Unipolar CV | Chord-type select (9 types) |
| `inversion` | Unipolar CV | Inversion (rotates the bass note) |
| `spread` | Unipolar CV | Spreads voices across octaves |

#### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `voice1` | V/Oct | Chord voice 1 |
| `voice2` | V/Oct | Chord voice 2 |
| `voice3` | V/Oct | Chord voice 3 |
| `voice4` | V/Oct | Chord voice 4 |

Chord types: Major, Minor, Seventh, MajorSeventh, MinorSeventh, Diminished, Augmented,
Sus2, Sus4.

---

### Euclidean

Euclidean rhythm generator: evenly distributes a pulse count across a step count, with
rotation and a per-cycle accent. `type_id`: `euclidean`.

```rust,ignore
let euclid = patch.add("euclid", Euclidean::new(44100.0));
```

#### Inputs

| Port | Signal | Description |
|------|--------|-------------|
| `clock` | Trigger | Advances the pattern on rising edge |
| `steps` | Unipolar CV | Step count (2–16), default 0.5 |
| `pulses` | Unipolar CV | Pulse (fill) count, default 0.25 |
| `rotation` | Unipolar CV | Rotates the pattern |
| `reset` | Trigger | Resets the step counter |

#### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Trigger | Pulse output for active steps |
| `accent` | Trigger | Accent on the first pulse of each cycle |
