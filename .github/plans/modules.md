# Modules to Implement

Remaining DSP modules for the Quiver library.

---

## Effects

### Reverb

Algorithmic reverb (Freeverb/Schroeder style).

**Ports**:
- `in`: Audio input
- `size`: Room size (0-1)
- `damping`: High frequency damping (0-1)
- `mix`: Wet/dry mix (0-1)
- `pre_delay`: Pre-delay time
- `out_left`, `out_right`: Stereo output

**Implementation notes**:
```
Input → Pre-delay → 8 Parallel Comb Filters → 4 Series All-pass → Output
                         ↓
                    Damping LPF
```

### EQ

3-band parametric equalizer.

**Ports**:
- `in`: Audio input
- `low_gain`, `low_freq`: Low shelf
- `mid_gain`, `mid_freq`, `mid_q`: Peaking mid
- `high_gain`, `high_freq`: High shelf
- `out`: Audio output

### Vocoder

Spectral processing with carrier/modulator.

**Ports**:
- `carrier`: Carrier input (oscillator)
- `modulator`: Modulator input (voice)
- `bands`: Number of filter bands
- `out`: Audio output

### Granular

Granular synthesis/processing engine.

**Ports**:
- `in`: Audio input (or buffer source)
- `position`: Playback position
- `grain_size`: Grain duration
- `density`: Grains per second
- `pitch`: Pitch shift
- `spray`: Position randomization
- `out`: Audio output

---

## Oscillators

### Wavetable

Wavetable oscillator with morphing.

**Ports**:
- `v_oct`: Pitch input (V/Oct)
- `table`: Wavetable index
- `morph`: Morphing between tables
- `out`: Audio output

**Implementation notes**:
- 256-sample wavetables
- Linear interpolation between samples
- Crossfade morphing between tables
- Bandlimited wavetables for anti-aliasing

### Formant

Formant oscillator for vocal sounds.

**Ports**:
- `v_oct`: Pitch input
- `formant`: Formant frequency
- `vowel`: Vowel selector (0-1 for a/e/i/o/u)
- `out`: Audio output

---

## Utilities

### Arpeggiator

Pattern-based note sequencer.

**Ports**:
- `v_oct`: Input pitch
- `gate`: Input gate
- `clock`: Clock input
- `pattern`: Pattern select (up/down/up-down/random)
- `octaves`: Octave range
- `v_oct_out`: Output pitch
- `gate_out`: Output gate

### ChordMemory

Chord voicing generator.

**Ports**:
- `v_oct`: Root note input
- `chord`: Chord type (major/minor/7th/etc.)
- `inversion`: Chord inversion
- `voice1`-`voice4`: Individual note outputs (V/Oct)

---

## High Complexity

### PitchShifter

Frequency domain pitch shifting.

**Ports**:
- `in`: Audio input
- `shift`: Pitch shift in semitones (-24 to +24)
- `window`: Analysis window size
- `out`: Audio output

**Options**:
- Phase vocoder (FFT-based)
- Granular pitch shifting
- PSOLA (for monophonic sources)

---

## Priority

Recommended order based on user value:

1. Reverb - Essential for usable patches
2. EQ - Basic tone shaping
3. Wavetable - Popular synthesis method
4. Arpeggiator - Essential for melodic patches
5. Vocoder - Unique sound design
6. PitchShifter - Complex but useful
7. Granular - Advanced sound design
8. Formant - Niche but interesting
9. ChordMemory - Convenience utility
