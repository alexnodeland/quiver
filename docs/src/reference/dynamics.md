# Dynamics

Dynamics processors shape a signal's level over time: compressing peaks, limiting
brick-wall ceilings, gating noise, and ducking one signal under another.

The `EnvelopeFollower` (an amplitude detector) is documented under
[Envelopes & Modulators](./modulators.md#envelope-follower).

## Compressor

Feed-forward compressor with dB-domain gain computation, makeup gain, and an internally
normalled external sidechain. `type_id`: `compressor`.

```rust,ignore
let comp = patch.add("comp", Compressor::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `threshold` | Unipolar CV | 0-10V | Threshold (×5 V), default 0.5 |
| `ratio` | Unipolar CV | 0-10V | Ratio (1:1 – 20:1), default 0.5 |
| `attack` | Unipolar CV | 0-10V | Attack time, default 0.2 |
| `release` | Unipolar CV | 0-10V | Release time, default 0.3 |
| `makeup` | Unipolar CV | 0-10V | Makeup gain (1×–4×), default 0.0 |
| `sidechain` | Audio | ±5V | External key; normalled to `in` when unpatched |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Compressed output |
| `gr` | Unipolar CV | Gain-reduction CV |

---

## Limiter

True brick-wall limiter with soft (renormalized tanh) or hard knee, plus an internally
normalled sidechain key. `type_id`: `limiter`.

```rust,ignore
let lim = patch.add("lim", Limiter::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `threshold` | Unipolar CV | 0-10V | Ceiling (×5 V), default 0.8 |
| `release` | Unipolar CV | 0-10V | Release time, default 0.3 |
| `soft` | Gate | 0/5V | Soft (tanh) knee vs hard limiting; default on |
| `sidechain` | Audio | ±5V | External key; normalled to `in` when unpatched |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Limited output (hard-clamped to ±threshold) |
| `gr` | Unipolar CV | Gain reduction |

The output is always hard-clamped to `±threshold`, so peaks never exceed the ceiling
regardless of knee shape.

---

## Noise Gate

Downward noise gate with hysteresis, a hold time, and an anti-click fade, plus an
internally normalled sidechain key. `type_id`: `noise_gate`.

```rust,ignore
let gate = patch.add("gate", NoiseGate::new(44100.0));
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Audio input |
| `threshold` | Unipolar CV | 0-10V | Open threshold (×5 V; close = 0.7×), default 0.1 |
| `attack` | Unipolar CV | 0-10V | Detector attack, default 0.1 |
| `release` | Unipolar CV | 0-10V | Detector release, default 0.3 |
| `range` | Unipolar CV | 0-10V | Maximum attenuation depth, default 1.0 |
| `sidechain` | Audio | ±5V | External key; normalled to `in` when unpatched |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Gated output |
| `gate` | Gate | Gate state (high when open) |

---

## Ducker

Dedicated sidechain ducking: the `key` input attenuates the main signal by up to
`amount`. Knob values combine with CV through `ModulatedParam`. `type_id`: `ducker`.

```rust,ignore
let duck = patch.add("duck", Ducker::new(44100.0));
patch.connect(kick.out("out"), duck.in_("key"))?; // kick ducks the pad
patch.connect(pad.out("out"), duck.in_("in"))?;
```

### Inputs

| Port | Signal | Range | Description |
|------|--------|-------|-------------|
| `in` | Audio | ±5V | Main signal |
| `key` | Audio | ±5V | Sidechain key that drives the ducking |
| `amount` | Bipolar CV | ±5V | Duck depth CV (summed with the knob) |
| `threshold` | Bipolar CV | ±5V | Threshold CV (summed with the knob) |
| `attack` | Unipolar CV | 0-10V | Envelope attack, default 0.1 |
| `release` | Unipolar CV | 0-10V | Envelope release, default 0.3 |

### Outputs

| Port | Signal | Description |
|------|--------|-------------|
| `out` | Audio | Ducked output |
| `gr` | Unipolar CV | Gain reduction |

Unlike the compressor/limiter/gate sidechains (which are normalled to the main input),
the Ducker's `key` is a dedicated, always-separate input. Knob values are also settable
in code with `set_amount`, `amount`, `set_threshold`, `threshold`.
