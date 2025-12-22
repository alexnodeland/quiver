# Quiver Improvements Roadmap

This document outlines identified gaps, missing features, and improvements needed in the Quiver audio synthesis library.

## Priority Legend

- **P0** - Critical: Documentation/implementation mismatch, broken APIs
- **P1** - High: Core functionality users expect
- **P2** - Medium: Nice-to-have features
- **P3** - Low: Future enhancements

---

## P0: Critical - Documentation/Implementation Mismatches

### Preset API Alignment

**Location**: `src/presets.rs`, `docs/src/appendix/presets.md`

The documented API doesn't match the implementation:

| Documented API | Actual API | Action Needed |
|----------------|------------|---------------|
| `PresetLibrary::new()` | Does not exist | Implement or update docs |
| `library.get("name")` | `PresetLibrary::load(name)` | Update docs |
| `preset.build(sample_rate)` | Does not exist | Implement or update docs |
| `library.search_tags(&["tag"])` | `PresetLibrary::by_tag(tag)` | Update docs |

**Options**:
1. Update documentation to match current static API
2. Implement instance-based API as documented

### Chorus Module Missing

**Location**: `docs/src/appendix/presets.md`

The "Juno Pad" preset references a Chorus module that doesn't exist:
```
VCO (saw + sub) → SVF → VCA → Chorus
```

**Action**: Either implement `Chorus` module or remove from preset documentation.

---

## P1: High Priority - Missing Core Modules

### Effects Modules

These are standard synthesizer effects that users expect:

| Module | Description | Complexity |
|--------|-------------|------------|
| **DelayLine** | Multi-tap delay with feedback, wet/dry mix | Medium |
| **Chorus** | Modulated delay for thickening | Medium |
| **Reverb** | Algorithmic reverb (Freeverb, Schroeder) | High |
| **Flanger** | Short modulated delay with feedback | Medium |
| **Phaser** | All-pass filter chain with modulation | Medium |

### Dynamics Modules

| Module | Description | Complexity |
|--------|-------------|------------|
| **Compressor** | Dynamic range compression with attack/release | Medium |
| **Limiter** | Hard/soft limiting | Low |
| **Gate** | Noise gate with threshold | Low |

### Utility Modules

| Module | Description | Complexity |
|--------|-------------|------------|
| **EnvelopeFollower** | Extract amplitude envelope from signal | Low |
| **Bitcrusher** | Sample rate/bit depth reduction | Low |
| **PitchShifter** | Frequency domain pitch shifting | High |

---

## P2: Medium Priority - Integration Gaps

### Plugin Wrapper Completion

**Location**: `src/extended_io.rs` (lines 690-900+)

Current state: Infrastructure defined (`PluginWrapper`, `PluginInfo`, `PluginParameter`) but no actual plugin API bindings.

**Needed**:
- [ ] VST3 SDK bindings or integration guide
- [ ] AU (Audio Unit) bindings for macOS
- [ ] LV2 bindings for Linux
- [ ] Example plugin project template

### Web Audio Integration

**Location**: `src/extended_io.rs`, `src/wasm/`

Current state: `WebAudioConfig` and `WebAudioWorklet` defined but incomplete.

**Needed**:
- [ ] AudioWorkletProcessor integration
- [ ] Example browser project with real-time audio
- [ ] Documentation for WASM deployment
- [ ] npm package publishing workflow

### WASM Improvements

**Location**: `src/wasm/engine.rs`

Current state: Basic `QuiverEngine` works for patch loading.

**Needed**:
- [ ] Real-time audio streaming to Web Audio API
- [ ] Thread/worklet communication patterns
- [ ] Performance optimization for browser
- [ ] Integration tests with headless browser

---

## P3: Low Priority - Enhancements

### Additional Effects

| Module | Description |
|--------|-------------|
| Tremolo | Amplitude modulation effect |
| Vibrato | Pitch modulation effect |
| Distortion | Waveshaping/overdrive |
| EQ | Parametric equalizer |
| Vocoder | Spectral processing |
| Granular | Granular synthesis engine |

### Additional Oscillators

| Module | Description |
|--------|-------------|
| Wavetable | Wavetable oscillator with morphing |
| Supersaw | Detuned saw stack (like JP-8000) |
| Formant | Formant oscillator for vocal sounds |
| Karplus-Strong | Physical modeling string |

### Additional Utilities

| Module | Description |
|--------|-------------|
| Arpeggiator | Pattern-based note sequencer |
| ChordMemory | Chord voicing generator |
| ScaleQuantizer | Musical scale quantization |
| Euclidean | Euclidean rhythm generator |

---

## Missing Examples

**Location**: `examples/`

Current examples cover basics but missing:

- [ ] `wasm_browser.rs` - Browser integration example
- [ ] `plugin_template.rs` - Plugin wrapper usage
- [ ] `web_audio.rs` - Web Audio API integration
- [ ] `delay_effects.rs` - Delay-based effects chain
- [ ] `dynamics.rs` - Compressor/limiter usage

---

## Documentation Gaps

### API Reference

- [ ] `PluginWrapper` needs usage documentation
- [ ] `WebAudioWorklet` needs integration guide
- [ ] WASM deployment guide needed

### Tutorials

- [ ] "Building a Delay Effect" tutorial
- [ ] "Browser Audio with WASM" tutorial
- [ ] "Creating a VST Plugin" tutorial

---

## Implementation Notes

### DelayLine Module Design

```rust
pub struct DelayLine {
    buffer: Vec<f32>,
    write_pos: usize,
    sample_rate: f32,
}

// Ports:
// - in: Audio input
// - time: Delay time in ms (CV controllable)
// - feedback: Feedback amount 0-1
// - mix: Wet/dry mix
// - out: Audio output
```

### Chorus Module Design

```rust
pub struct Chorus {
    delay_lines: [DelayLine; 3],  // Multiple voices
    lfos: [Lfo; 3],               // Modulation sources
    sample_rate: f32,
}

// Ports:
// - in: Audio input
// - rate: LFO rate
// - depth: Modulation depth
// - mix: Wet/dry mix
// - out: Audio output (stereo?)
```

### Compressor Module Design

```rust
pub struct Compressor {
    envelope: f32,
    sample_rate: f32,
}

// Ports:
// - in: Audio input
// - threshold: Compression threshold in dB
// - ratio: Compression ratio
// - attack: Attack time in ms
// - release: Release time in ms
// - makeup: Makeup gain
// - out: Audio output
// - gain_reduction: CV output showing gain reduction
```

---

## Testing Requirements

New modules should include:

1. Unit tests for DSP correctness
2. Port specification tests
3. Serialization round-trip tests
4. Real-time compliance benchmarks
5. Integration tests with patch graph

Coverage must remain above 80% threshold.

---

## Contributing

When implementing features from this list:

1. Create a feature branch
2. Implement module with full port specification
3. Add unit tests (aim for >90% coverage on new code)
4. Add integration example
5. Update documentation
6. Run `make check` before PR
7. Reference this document in PR description

---

## Progress Tracking

| Feature | Status | PR | Notes |
|---------|--------|-----|-------|
| Preset API docs fix | Not started | - | |
| DelayLine | Not started | - | |
| Chorus | Not started | - | Blocked by DelayLine |
| Compressor | Not started | - | |
| Limiter | Not started | - | |
| WASM examples | Not started | - | |

---

*Last updated: 2024-12*
