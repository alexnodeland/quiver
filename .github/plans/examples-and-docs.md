# Missing Examples and Documentation

Gaps in examples and documentation for the Quiver library.

---

## Missing Examples

**Location**: `examples/`

### WASM/Browser

| Example | Description | Priority |
|---------|-------------|----------|
| `wasm_browser.rs` | Browser integration with Web Audio | High |
| `web_audio.rs` | Web Audio API integration patterns | High |

### Plugin Development

| Example | Description | Priority |
|---------|-------------|----------|
| `plugin_template.rs` | Plugin wrapper usage template | Medium |

### Effects Usage

| Example | Description | Priority |
|---------|-------------|----------|
| `delay_effects.rs` | Delay-based effects chain (delay, chorus, flanger) | Low |
| `dynamics.rs` | Compressor/limiter/gate usage | Low |

---

## Documentation Gaps

### API Reference

| Topic | Location | Status |
|-------|----------|--------|
| `PluginWrapper` usage | `src/extended_io.rs` | Needs docs |
| `WebAudioWorklet` integration | `src/extended_io.rs` | Needs guide |
| WASM deployment | `src/wasm/` | Needs guide |

### Tutorials Needed

| Tutorial | Description | Target Audience |
|----------|-------------|-----------------|
| Building a Delay Effect | Step-by-step effect creation | Intermediate |
| Browser Audio with WASM | Web Audio + Quiver WASM | Intermediate |
| Creating a VST Plugin | Full plugin development | Advanced |

---

## Example Templates

### wasm_browser.rs

```rust
//! Browser audio integration example
//!
//! Demonstrates:
//! - QuiverEngine in AudioWorklet
//! - MIDI input handling
//! - Parameter automation

use quiver::prelude::*;

fn main() {
    // This example is meant to be compiled to WASM
    // See packages/@quiver/web for the full setup
    println!("See packages/@quiver/web for browser integration");
}
```

### delay_effects.rs

```rust
//! Delay-based effects demonstration
//!
//! Shows usage of:
//! - DelayLine
//! - Chorus
//! - Flanger
//! - Phaser

use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    // Build effects chain
    let input = patch.add("input", ExternalInput::audio(/* ... */));
    let delay = patch.add("delay", DelayLine::new(sample_rate));
    let chorus = patch.add("chorus", Chorus::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    // Connect: input → delay → chorus → output
    patch.connect(input.out("out"), delay.in_("in")).unwrap();
    patch.connect(delay.out("out"), chorus.in_("in")).unwrap();
    patch.connect(chorus.out("left"), output.in_("left")).unwrap();
    patch.connect(chorus.out("right"), output.in_("right")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    // Process audio...
}
```

### dynamics.rs

```rust
//! Dynamics processing demonstration
//!
//! Shows usage of:
//! - Compressor
//! - Limiter
//! - NoiseGate

use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    // Build dynamics chain: gate → compressor → limiter
    let input = patch.add("input", ExternalInput::audio(/* ... */));
    let gate = patch.add("gate", NoiseGate::new(sample_rate));
    let comp = patch.add("comp", Compressor::new(sample_rate));
    let limiter = patch.add("limiter", Limiter::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    // Connect chain
    patch.connect(input.out("out"), gate.in_("in")).unwrap();
    patch.connect(gate.out("out"), comp.in_("in")).unwrap();
    patch.connect(comp.out("out"), limiter.in_("in")).unwrap();
    patch.connect(limiter.out("out"), output.in_("left")).unwrap();
    patch.connect(limiter.out("out"), output.in_("right")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    // Process audio...
}
```

---

## Documentation Structure

Suggested organization for new docs:

```
docs/src/
├── how-to/
│   ├── plugin-development.md    # NEW: Plugin wrapper guide
│   ├── wasm-deployment.md       # NEW: WASM build & deploy
│   └── web-audio-integration.md # NEW: Browser audio guide
├── tutorials/
│   ├── delay-effect.md          # NEW: Building effects
│   └── browser-synth.md         # NEW: Web synth tutorial
```

---

## Priority Order

1. **wasm_browser.rs** + **web-audio-integration.md** - Most requested
2. **plugin_template.rs** + **plugin-development.md** - Plugin authors need this
3. **delay_effects.rs** - Shows off new effects modules
4. **dynamics.rs** - Completes effects coverage

---

*Last updated: 2025-12*
