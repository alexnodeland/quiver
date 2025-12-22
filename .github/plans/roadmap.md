# Quiver Roadmap

Remaining features and improvements for the Quiver audio synthesis library.

---

## High Complexity DSP Modules

These require significant algorithmic work:

### Reverb (P1)

**Complexity**: High

Algorithmic reverb implementation. Options:
- Freeverb (Schroeder-Moorer)
- Dattorro plate reverb
- FDN (Feedback Delay Network)

**Ports needed**:
- `in`: Audio input
- `size`: Room size
- `damping`: High frequency damping
- `mix`: Wet/dry mix
- `pre_delay`: Pre-delay time
- `out_left`, `out_right`: Stereo output

### PitchShifter (P1)

**Complexity**: High

Frequency domain pitch shifting. Options:
- Phase vocoder (FFT-based)
- Granular pitch shifting
- PSOLA (for monophonic sources)

**Ports needed**:
- `in`: Audio input
- `shift`: Pitch shift in semitones (-24 to +24)
- `window`: Analysis window size
- `out`: Audio output

---

## External Dependencies (P2)

These require third-party crate integration:

### Plugin Format Bindings

| Format | Crate | Platform |
|--------|-------|----------|
| VST3 | `vst3-sys` | Cross-platform |
| Audio Unit | `coreaudio-rs` | macOS/iOS |
| LV2 | `lv2` | Linux |

### Project Templates

- [ ] Example VST3 plugin project
- [ ] Example AU plugin project
- [ ] Example LV2 plugin project

---

## Remaining P3 Modules

### Effects

| Module | Description | Complexity |
|--------|-------------|------------|
| EQ | Parametric equalizer (biquad bands) | Medium |
| Vocoder | Spectral processing with carrier/modulator | High |
| Granular | Granular synthesis/processing engine | High |

### Oscillators

| Module | Description | Complexity |
|--------|-------------|------------|
| Wavetable | Wavetable oscillator with morphing | Medium |
| Formant | Formant oscillator for vocal sounds | Medium |

### Utilities

| Module | Description | Complexity |
|--------|-------------|------------|
| Arpeggiator | Pattern-based note sequencer | Medium |
| ChordMemory | Chord voicing generator | Low |

---

## Testing Infrastructure

### WASM Testing

- [ ] Integration tests with headless browser (Playwright/Puppeteer)
- [ ] Performance benchmarks in browser environment
- [ ] Cross-browser compatibility testing

### Browser Project Setup

- [ ] Example browser project with real-time audio
- [ ] npm package publishing workflow
- [ ] TypeScript type definitions verification

---

## Priority Order

Recommended implementation order based on user value:

1. **Reverb** - Most requested effect, essential for usable patches
2. **EQ** - Basic tone shaping, relatively straightforward
3. **Wavetable** - Popular synthesis method
4. **Arpeggiator** - Essential for melodic patches
5. **Vocoder** - Unique sound design capability
6. **PitchShifter** - Complex but useful
7. **Granular** - Advanced sound design
8. **Formant** - Niche but interesting
9. **ChordMemory** - Convenience utility

---

## Design Notes

### Reverb Implementation

Recommended: Freeverb algorithm (public domain)

```
Input → Pre-delay → 8 Parallel Comb Filters → 4 Series All-pass → Output
                         ↓
                    Damping LPF
```

Key parameters:
- Room size: Controls comb filter feedback
- Damping: LPF cutoff in feedback path
- Width: Stereo spread

### EQ Implementation

3-band parametric with:
- Low shelf
- Peaking mid band
- High shelf

Each band: frequency, gain, Q

### Wavetable Implementation

- 256-sample wavetables
- Linear interpolation between samples
- Crossfade morphing between tables
- Anti-aliasing via bandlimited wavetables

---

*Last updated: 2025-12*
