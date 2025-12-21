# Understanding Signal Flow

In Quiver, signals flow through modules following the conventions of hardware modular synthesizers. Understanding these conventions is key to creating patches that behave predictably.

## Voltage Ranges

Quiver models its signals on the Eurorack standard:

```mermaid
graph LR
    subgraph "Audio Signals"
        A[±5V Peak<br/>AC-coupled]
    end
    subgraph "Control Voltage"
        B[0-10V Unipolar]
        C[±5V Bipolar]
    end
    subgraph "Pitch"
        D[1V/Octave<br/>0V = C4]
    end
    subgraph "Triggers/Gates"
        E[0V Low<br/>+5V High]
    end

    style A fill:#4a9eff,color:#fff
    style B fill:#f9a826,color:#000
    style C fill:#f9a826,color:#000
    style D fill:#e74c3c,color:#fff
    style E fill:#50c878,color:#fff
```

### Audio Signals

Audio oscillates between **-5V and +5V**:

$$\text{audio}(t) \in [-5, +5]$$

This matches Eurorack levels and allows headroom for mixing.

### Control Voltage (CV)

Two types of control voltage:

| Type | Range | Use Case |
|------|-------|----------|
| **Unipolar** | 0V to +10V | Filter cutoff, LFO rate, envelope times |
| **Bipolar** | -5V to +5V | Vibrato, pan position, FM |

### Volt-per-Octave (V/Oct)

Pitch follows the **1 Volt per Octave** standard:

$$f = f_0 \cdot 2^{V}$$

Where $f_0 = 261.63$ Hz (C4) at 0V.

| Voltage | Note | Frequency |
|---------|------|-----------|
| -1V | C3 | 130.81 Hz |
| 0V | C4 | 261.63 Hz |
| +1V | C5 | 523.25 Hz |
| +2V | C6 | 1046.50 Hz |

### Gates and Triggers

```mermaid
sequenceDiagram
    participant G as Gate
    participant T as Trigger

    Note over G: Gate (sustained)
    G->>G: 0V (off)
    G->>G: +5V (on, held)
    G->>G: +5V (still on)
    G->>G: 0V (off)

    Note over T: Trigger (impulse)
    T->>T: 0V
    T->>T: +5V (1-10ms pulse)
    T->>T: 0V
```

- **Gate**: Sustained high signal (key held down)
- **Trigger**: Brief pulse (≈1-10ms) to start events

## Signal Types in Code

Quiver tracks signal types through `SignalKind`:

```rust,ignore
pub enum SignalKind {
    Audio,           // ±5V AC-coupled
    CvBipolar,       // ±5V control
    CvUnipolar,      // 0-10V control
    VoltPerOctave,   // 1V/Oct pitch
    Gate,            // 0V or +5V sustained
    Trigger,         // 0V or +5V brief pulse
    Clock,           // Regular timing pulses
}
```

The type system helps catch mismatches:

```rust,ignore
// This will warn: connecting audio to a V/Oct input
patch.connect(vco.out("saw"), another_vco.in_("voct"))
```

## Module Input Behavior

### Input Summing

Multiple cables to one input are **summed**:

```mermaid
flowchart LR
    LFO1[LFO 1] -->|+2V| SUM((Σ))
    LFO2[LFO 2] -->|+3V| SUM
    SUM -->|+5V| VCF[VCF cutoff]
```

This models analog behavior where multiple CVs combine.

### Attenuverters

Many inputs support attenuation and inversion:

```rust,ignore
// Half strength, inverted
patch.connect_with(
    lfo.out("sin"),
    vcf.in_("cutoff"),
    Cable::new().with_attenuation(-0.5),
)?;
```

The attenuverter range is typically **-2 to +2**, allowing inversion and some gain.

### Normalled Connections

Some inputs have default sources when unpatched:

```mermaid
flowchart LR
    LEFT[Left Input] --> NORM{Unpatched?}
    NORM -->|Yes| RIGHT[Uses Left<br/>signal]
    NORM -->|No| EXT[External<br/>source]
```

The `StereoOutput` module, for example, normalizes right to left if right is unpatched.

## Processing Order

Quiver automatically determines processing order through topological sort:

```mermaid
flowchart TD
    VCO[1. VCO] --> VCF[2. VCF]
    LFO[1. LFO] --> VCF
    VCF --> VCA[3. VCA]
    ENV[1. ENV] --> VCA
    VCA --> OUT[4. Output]
```

Modules with no dependencies process first. The algorithm (Kahn's) ensures every module has its inputs ready before processing.

## Common Patching Patterns

### Modulation

```mermaid
flowchart LR
    LFO[LFO] -->|mod| TARGET[Target Parameter]
    OFFSET[Offset] -->|base| TARGET
```

Combine a static offset with an LFO for "center + modulation" control.

### Envelope Following

```mermaid
flowchart LR
    AUDIO[Audio In] --> VCA[VCA]
    AUDIO --> ENV[Envelope<br/>Follower]
    ENV -->|level| VCA
```

Use audio amplitude to control other parameters.

### FM (Frequency Modulation)

```mermaid
flowchart LR
    MOD[Modulator<br/>VCO] -->|fm| CARRIER[Carrier<br/>VCO]
    CARRIER --> OUT[Output]
```

Audio-rate modulation of oscillator frequency creates complex timbres.

---

Next: [The Quiver Philosophy](./philosophy.md)
