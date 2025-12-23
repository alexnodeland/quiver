# Documentation and Examples

Missing examples and documentation.

---

## Examples Needed

### Browser/WASM

| File | Description |
|------|-------------|
| `wasm_browser.rs` | Browser integration with Web Audio |
| `web_audio.rs` | Web Audio API integration patterns |

### Effects Usage

| File | Description |
|------|-------------|
| `delay_effects.rs` | Delay-based effects chain |
| `dynamics.rs` | Compressor/limiter/gate usage |

---

## Documentation Needed

### API Guides

| Topic | Description |
|-------|-------------|
| Web Audio integration | How to use `WebAudioBlockProcessor` and WASM |
| WASM deployment | Building and deploying WASM packages |

### Tutorials

| Title | Description |
|-------|-------------|
| Building a Delay Effect | Step-by-step custom module creation |
| Browser Audio with WASM | Web synth from scratch |

---

## Example Sketches

### delay_effects.rs

```rust
//! Delay-based effects demonstration
use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    let input = patch.add("input", /* ... */);
    let delay = patch.add("delay", DelayLine::new(sample_rate));
    let chorus = patch.add("chorus", Chorus::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    // input → delay → chorus → output
    patch.connect(input.out("out"), delay.in_("in")).unwrap();
    patch.connect(delay.out("out"), chorus.in_("in")).unwrap();
    patch.connect(chorus.out("left"), output.in_("left")).unwrap();
    patch.connect(chorus.out("right"), output.in_("right")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();
}
```

### dynamics.rs

```rust
//! Dynamics processing demonstration
use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    let input = patch.add("input", /* ... */);
    let gate = patch.add("gate", NoiseGate::new(sample_rate));
    let comp = patch.add("comp", Compressor::new(sample_rate));
    let limiter = patch.add("limiter", Limiter::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    // gate → compressor → limiter
    patch.connect(input.out("out"), gate.in_("in")).unwrap();
    patch.connect(gate.out("out"), comp.in_("in")).unwrap();
    patch.connect(comp.out("out"), limiter.in_("in")).unwrap();
    patch.connect(limiter.out("out"), output.in_("left")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();
}
```

---

## Doc Structure

Suggested location for new docs:

```
docs/src/
├── how-to/
│   ├── wasm-deployment.md      # WASM build & deploy
│   └── web-audio.md            # Browser audio guide
└── tutorials/
    ├── delay-effect.md         # Building effects
    └── browser-synth.md        # Web synth tutorial
```
