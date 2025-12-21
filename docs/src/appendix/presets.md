# Preset Library

Quiver includes a library of preset patches for learning and quick starts.

## Using Presets

```rust,ignore
use quiver::prelude::*;

let library = PresetLibrary::new();

// List all presets
for preset in library.list() {
    println!("{}: {}", preset.name, preset.description);
}

// Get by category
let basses = library.by_category(PresetCategory::Bass);

// Search by tag
let acid = library.search_tags(&["acid"]);

// Build a preset
let patch = library.get("Moog Bass")?.build(44100.0)?;
```

## Categories

| Category | Description |
|----------|-------------|
| `Classic` | Iconic synth sounds |
| `Bass` | Bass patches |
| `Lead` | Lead/solo sounds |
| `Pad` | Sustained pad sounds |
| `Percussion` | Drums and percussion |
| `Effect` | Effects and textures |
| `SoundDesign` | Experimental sounds |
| `Tutorial` | Learning examples |

## Classic Presets

### Moog Bass

```
Category: Bass
Tags: moog, classic, warm

Architecture:
  VCO (saw) → Ladder Filter → VCA
  ADSR → Filter + VCA

Character:
  Deep, warm, punchy bass with filter sweep
```

### Juno Pad

```
Category: Pad
Tags: juno, warm, lush

Architecture:
  VCO (saw + sub) → SVF → VCA → Chorus
  Slow ADSR → Filter + VCA

Character:
  Wide, warm pad with subtle movement
```

### 303 Acid

```
Category: Bass
Tags: 303, acid, squelchy

Architecture:
  VCO (saw) → Diode Ladder → VCA
  Fast ADSR → Filter (high resonance)

Character:
  Classic acid squelch with resonant filter
```

### Sync Lead

```
Category: Lead
Tags: sync, aggressive, lead

Architecture:
  Master VCO → sync → Slave VCO
  LFO → Slave pitch
  SVF → VCA

Character:
  Cutting, aggressive lead with sync sweep
```

### PWM Strings

```
Category: Pad
Tags: strings, pwm, ensemble

Architecture:
  VCO (pulse) → SVF → VCA
  LFO → Pulse width
  Detuned voice layering

Character:
  Lush string ensemble with movement
```

## Tutorial Presets

### Basic Subtractive (Difficulty: 1)

```
Purpose: Learn VCO → VCF → VCA chain

Modules:
  - VCO: Basic oscillator
  - SVF: Lowpass filter
  - VCA: Volume control

Try:
  - Change waveform (saw/sqr/tri)
  - Adjust filter cutoff
  - Add resonance
```

### Envelope Basics (Difficulty: 1)

```
Purpose: Learn ADSR envelope shaping

Modules:
  - VCO → VCF → VCA
  - ADSR envelope

Try:
  - Adjust attack for slow fade-in
  - Short decay for plucky sounds
  - Sustain level for held notes
  - Release for pad tails
```

### Filter Modulation (Difficulty: 2)

```
Purpose: Learn LFO → filter modulation

Modules:
  - VCO → VCF → VCA
  - LFO → filter cutoff

Try:
  - Adjust LFO rate
  - Try different LFO waveforms
  - Change modulation depth
```

### FM Basics (Difficulty: 3)

```
Purpose: Intro to FM synthesis

Modules:
  - Carrier VCO
  - Modulator VCO → Carrier FM

Try:
  - Adjust C:M ratio
  - Change modulation depth
  - Envelope the FM amount
```

### Polyphony Intro (Difficulty: 3)

```
Purpose: Learn voice allocation

Modules:
  - 4-voice polyphonic patch
  - VoiceAllocator

Try:
  - Play chords
  - Change allocation mode
  - Add unison/detune
```

## Sound Design Presets

### Metallic Ring

```
Category: SoundDesign
Tags: ring, metallic, experimental

Architecture:
  VCO1 × VCO2 (ring mod)
  Inharmonic ratio

Character:
  Bell-like metallic tones
```

### Noise Sweep

```
Category: SoundDesign
Tags: noise, sweep, texture

Architecture:
  Noise → Resonant filter
  LFO → filter sweep

Character:
  Evolving filtered noise
```

### Wavefold Growl

```
Category: SoundDesign
Tags: wavefold, aggressive, bass

Architecture:
  VCO → Wavefolder → Filter

Character:
  Aggressive, harmonically rich growl
```

## Building Custom Presets

```rust,ignore
// Create preset info
let info = PresetInfo {
    name: "My Preset".to_string(),
    category: PresetCategory::Lead,
    description: "A custom lead sound".to_string(),
    tags: vec!["custom".into(), "lead".into()],
    difficulty: 2,
};

// Build the patch
fn build_preset(sample_rate: f64) -> Patch {
    let mut patch = Patch::new(sample_rate);
    // ... add modules and connections ...
    patch
}
```

## Preset File Format

Presets can be saved as JSON:

```json
{
  "name": "My Preset",
  "category": "Lead",
  "description": "Description here",
  "tags": ["custom", "lead"],
  "patch": {
    "modules": [...],
    "cables": [...],
    "parameters": {...}
  }
}
```

See [Serialization](../how-to/serialization.md) for details.
