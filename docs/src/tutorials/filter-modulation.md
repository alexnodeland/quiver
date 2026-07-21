# Filter Modulation

Modulation brings patches to life. When we connect an LFO (Low Frequency Oscillator) to the filter cutoff, static becomes dynamic—a still photograph becomes a movie.

<div class="quiver-explorable" data-viz="patchgraph">
<script type="application/json">
{
  "modules": [
    {"id": "lfo", "label": "LFO", "x": 0, "y": 2.4,
     "outputs": [{"name": "sin", "kind": "mod"}]},
    {"id": "lfo_depth_cv", "label": "DEPTH CV", "x": 0, "y": 4.6,
     "outputs": [{"name": "out", "kind": "cv"}]},
    {"id": "vco", "label": "VCO", "x": 1, "y": 0,
     "outputs": [{"name": "saw", "kind": "audio"}]},
    {"id": "lfo_depth", "label": "ATTENUVERTER", "x": 1, "y": 2.4,
     "inputs": [{"name": "in", "kind": "mod"}, {"name": "level", "kind": "cv"}],
     "outputs": [{"name": "out", "kind": "mod"}]},
    {"id": "cutoff_base", "label": "CUTOFF CV", "x": 1, "y": 5.2,
     "outputs": [{"name": "out", "kind": "cv"}]},
    {"id": "vcf", "label": "VCF (SVF)", "x": 2, "y": 0,
     "inputs": [{"name": "in", "kind": "audio"}, {"name": "cutoff", "kind": "cv"}, {"name": "fm", "kind": "mod"}],
     "outputs": [{"name": "lp", "kind": "audio"}]},
    {"id": "output", "label": "OUTPUT", "x": 3, "y": 0,
     "inputs": [{"name": "left", "kind": "audio"}]}
  ],
  "cables": [
    {"from": "vco.saw", "to": "vcf.in", "kind": "audio"},
    {"from": "lfo.sin", "to": "lfo_depth.in", "kind": "mod"},
    {"from": "lfo_depth_cv.out", "to": "lfo_depth.level", "kind": "cv"},
    {"from": "lfo_depth.out", "to": "vcf.fm", "kind": "mod"},
    {"from": "cutoff_base.out", "to": "vcf.cutoff", "kind": "cv"},
    {"from": "vcf.lp", "to": "output.left", "kind": "audio"}
  ],
  "caption": "tutorial_filter_mod: the LFO is scaled by an attenuverter before sweeping the SVF's fm input; an Offset parks the base cutoff."
}
</script>
</div>

*Watch a moving cutoff reshape the spectrum in real time in [Sculpting the Spectrum](../explorables/filters.md).*

## LFO: The Modulation Source

An LFO is simply an oscillator running at sub-audio rates:

| Audio Oscillator | LFO |
|------------------|-----|
| 20Hz - 20kHz | 0.01Hz - 30Hz |
| Creates pitch | Creates movement |
| You hear it | You feel its effect |

```mermaid
graph LR
    subgraph "LFO Waveforms"
        SIN[Sine<br/>Smooth sweep]
        TRI[Triangle<br/>Linear sweep]
        SAW[Saw<br/>Ramp + drop]
        SQR[Square<br/>Two states]
    end
```

## The Mathematics of Modulation

Filter cutoff with LFO modulation:

\\[ f_c(t) = f_{center} + f_{depth} \cdot \text{LFO}(t) \\]

Where:
- \\( f_{center} \\) is the base cutoff frequency
- \\( f_{depth} \\) is the modulation depth (how far it sweeps)
- \\( \text{LFO}(t) \\) oscillates between -1 and +1

## Building the Patch

```rust,ignore
{{#include ../../../examples/tutorial_filter_mod.rs}}
```

Run it with `cargo run --example tutorial_filter_mod`.

## Modulation Depth and Attenuverters

The amount of modulation matters:

| Depth | Effect |
|-------|--------|
| 10% | Subtle shimmer |
| 25% | Noticeable movement |
| 50% | Dramatic sweep |
| 100% | Extreme wah-wah |

Quiver cables support attenuation:

```rust,ignore
// Connect with 50% modulation depth
patch.connect_with(
    lfo.out("sin"),
    vcf.in_("cutoff"),
    Cable::new().with_attenuation(0.5),
)?;
```

## Waveform Shapes

Each LFO waveform creates a different movement:

### Sine Wave
Smooth, natural sweeping—good for gentle effects.

```
    ╱╲    ╱╲    ╱╲
   ╱  ╲  ╱  ╲  ╱  ╲
──╱────╲╱────╲╱────╲──
```

### Triangle Wave
Linear sweeping—predictable, good for trills.

```
   ╱╲    ╱╲    ╱╲
  ╱  ╲  ╱  ╲  ╱  ╲
─╱────╲╱────╲╱────╲─
```

### Sawtooth Wave
Rises slowly, drops instantly—creates rhythmic "pumping."

```
   ╱│   ╱│   ╱│
  ╱ │  ╱ │  ╱ │
─╱──│─╱──│─╱──│──
```

### Square Wave
Instant alternation between two states—tremolo/vibrato effect.

```
 ┌──┐  ┌──┐  ┌──┐
 │  │  │  │  │  │
─┘  └──┘  └──┘  └─
```

## Rate and Depth Interaction

```mermaid
quadrantChart
    title LFO Character
    x-axis Slow Rate --> Fast Rate
    y-axis Subtle Depth --> Deep Depth
    quadrant-1 Vibrato/Tremolo
    quadrant-2 Slow Sweep
    quadrant-3 Subtle Texture
    quadrant-4 Frantic Motion
```

| Rate | Depth | Classic Use |
|------|-------|-------------|
| 0.5Hz | 30% | Slow filter sweep |
| 2Hz | 10% | Subtle shimmer |
| 6Hz | 50% | Dubstep wobble |
| 8Hz | 5% | Guitar vibrato |

## Multiple Modulation Sources

Combine LFO with envelope for evolving sounds:

```mermaid
flowchart TD
    LFO[LFO<br/>Ongoing movement]
    ENV[Envelope<br/>Per-note shape]
    SUM((Σ))
    VCF[Filter Cutoff]

    LFO --> SUM
    ENV --> SUM
    SUM --> VCF
```

The envelope provides the initial "brightness burst," while the LFO adds continuous movement during sustain.

---

Next: [Building a Sequenced Bass](./sequenced-bass.md)
