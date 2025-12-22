# Quiver Improvements Roadmap

This document outlines identified gaps, missing features, and improvements needed in the Quiver audio synthesis library.

## Priority Legend

- **P0** - Critical: Documentation/implementation mismatch, broken APIs
- **P1** - High: Core functionality users expect
- **P2** - Medium: Nice-to-have features
- **P3** - Low: Future enhancements

---

## P0: Critical - Documentation/Implementation Mismatches ✅ COMPLETED

### Preset API Alignment ✅

**Location**: `src/presets.rs`, `docs/src/appendix/presets.md`

**Status**: IMPLEMENTED - The documented API now works as expected:

| Documented API | Status |
|----------------|--------|
| `PresetLibrary::new()` | ✅ Implemented |
| `library.get("name")` | ✅ Implemented - returns `Option<Preset>` |
| `preset.build(sample_rate)` | ✅ Implemented - returns `Result<Patch, PresetError>` |
| `library.search_tags(&["tag"])` | ✅ Implemented |

Additional features added:
- `Preset` struct with `build()` and `build_with_registry()` methods
- `PresetError` enum for error handling
- `preset.into_def()` to get raw `PatchDef`

### Chorus Module ✅

**Location**: `src/modules.rs`

**Status**: IMPLEMENTED - Full chorus effect with:
- 3-voice modulated delay lines
- Rate and depth CV control
- Stereo spread outputs (mono, left, right)
- Registered in `ModuleRegistry` as "chorus"

### DelayLine Module ✅ (Bonus)

**Location**: `src/modules.rs`

**Status**: IMPLEMENTED - Multi-sample delay with:
- CV-controllable delay time (1ms to 2 seconds)
- Feedback with stability limiting
- Wet/dry mix
- Linear interpolation for smooth modulation
- Registered in `ModuleRegistry` as "delay_line"

### Juno Pad Preset Updated ✅

The "Juno Pad" preset now includes the Chorus module in its signal chain:
```
VCO (PWM) → SVF → VCA → Chorus → Stereo Output
```

---

## P1: High Priority - Missing Core Modules ✅ MOSTLY COMPLETE

### Effects Modules

| Module | Description | Complexity | Status |
|--------|-------------|------------|--------|
| **DelayLine** | Multi-tap delay with feedback, wet/dry mix | Medium | ✅ Done |
| **Chorus** | Modulated delay for thickening | Medium | ✅ Done |
| **Flanger** | Short modulated delay with feedback | Medium | ✅ Done |
| **Phaser** | All-pass filter chain with modulation | Medium | ✅ Done |
| **Bitcrusher** | Lo-fi bit depth and sample rate reduction | Low | ✅ Done |
| **Reverb** | Algorithmic reverb (Freeverb, Schroeder) | High | Pending |

### Dynamics Modules

| Module | Description | Complexity | Status |
|--------|-------------|------------|--------|
| **Limiter** | Hard/soft limiting | Low | ✅ Done |
| **NoiseGate** | Noise gate with threshold and hysteresis | Low | ✅ Done |
| **Compressor** | Dynamic range compression with sidechain | Medium | ✅ Done |

### Utility Modules

| Module | Description | Complexity | Status |
|--------|-------------|------------|--------|
| **EnvelopeFollower** | Extract amplitude envelope from signal | Low | ✅ Done |
| **PitchShifter** | Frequency domain pitch shifting | High | Pending |

---

## P2: Medium Priority - Integration Gaps ✅ MOSTLY COMPLETE

### Plugin Wrapper Completion ✅

**Location**: `src/extended_io.rs` (lines 690-1050+)

**Status**: IMPLEMENTED - Complete infrastructure for plugin development:

| Feature | Status |
|---------|--------|
| `MidiStatus` enum | ✅ Full MIDI status byte parsing |
| `MidiMessage` struct | ✅ Note On/Off, CC, Pitch Bend with sample-accurate timing |
| `MidiBuffer` | ✅ Event buffer with sorting and filtering |
| `PluginProcessor` trait | ✅ Full plugin interface with MIDI, state, parameters |
| `ProcessContext` | ✅ Transport, tempo, sample rate, MIDI I/O |
| Latency reporting | ✅ Added to PluginWrapper |

Additional features:
- MIDI note to frequency/V-Oct conversion
- Sample-accurate MIDI event timing
- CC value normalization (0-127 → 0.0-1.0)
- Pitch bend normalization (-8192/+8191 → -1.0/+1.0)

**Remaining** (external dependencies):
- [ ] VST3 SDK bindings (requires vst3-sys crate integration)
- [ ] AU (Audio Unit) bindings (requires coreaudio-rs)
- [ ] LV2 bindings (requires lv2 crate)
- [ ] Example plugin project template

### Web Audio Integration ✅

**Location**: `src/extended_io.rs` (lines 1050-1340+)

**Status**: IMPLEMENTED - Complete worklet integration:

| Feature | Status |
|---------|--------|
| `WebAudioBlockProcessor` | ✅ 128-sample block processing |
| Process with closure | ✅ `process_with()` for easy sample generation |
| Direct buffer access | ✅ `left_buffer_mut()`, `right_buffer_mut()` |
| Parameter handling | ✅ Thread-safe atomic parameters |
| Interleave/deinterleave | ✅ Stereo channel helpers |
| f32/f64 conversion | ✅ Efficient block conversion |

JavaScript integration example included in documentation.

**Remaining** (project setup):
- [ ] Example browser project with real-time audio
- [ ] npm package publishing workflow

### WASM Improvements ✅

**Location**: `src/wasm/engine.rs`

**Status**: IMPLEMENTED - Enhanced worklet integration:

| Feature | Status |
|---------|--------|
| MIDI Note On/Off | ✅ `midi_note_on()`, `midi_note_off()` |
| MIDI CC handling | ✅ `midi_cc()`, `get_midi_cc()` |
| MIDI Pitch Bend | ✅ `midi_pitch_bend()`, `pitch_bend` getter |
| V/Oct output | ✅ `midi_note` getter returns V/Oct |
| Velocity output | ✅ `midi_velocity` getter (0-1) |
| Gate output | ✅ `midi_gate` getter |
| Block processing | ✅ `process_block()` returns Float32Array |

**Remaining** (testing):
- [ ] Integration tests with headless browser
- [ ] Performance benchmarks in browser

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
| Preset API implementation | ✅ Complete | - | `PresetLibrary::new()`, `get()`, `search_tags()`, `Preset::build()` |
| DelayLine | ✅ Complete | - | Multi-sample delay with feedback, CV time control |
| Chorus | ✅ Complete | - | 3-voice stereo chorus effect |
| Juno Pad preset update | ✅ Complete | - | Now uses Chorus module |
| Flanger | ✅ Complete | - | Short modulated delay with feedback |
| Phaser | ✅ Complete | - | 6-stage all-pass filter with LFO |
| Limiter | ✅ Complete | - | Hard/soft limiting with gain reduction output |
| NoiseGate | ✅ Complete | - | Threshold with hysteresis and range control |
| Compressor | ✅ Complete | - | Full compressor with sidechain support |
| EnvelopeFollower | ✅ Complete | - | Amplitude detection with inverted output |
| Bitcrusher | ✅ Complete | - | Bit depth and sample rate reduction |
| MIDI Support | ✅ Complete | - | `MidiStatus`, `MidiMessage`, `MidiBuffer` for plugin integration |
| PluginProcessor trait | ✅ Complete | - | Full plugin interface with MIDI, state, parameters |
| WebAudioBlockProcessor | ✅ Complete | - | 128-sample block processing for AudioWorklet |
| WASM MIDI | ✅ Complete | - | `midi_note_on/off`, CC, pitch bend in QuiverEngine |
| Reverb | Not started | - | High complexity |
| PitchShifter | Not started | - | High complexity |
| WASM browser examples | Not started | - | Requires project setup |

---

*Last updated: 2025-12*
