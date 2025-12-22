# CLAUDE.md

This file provides guidance for AI assistants working with the Quiver codebase.

## Project Overview

Quiver is a modular audio synthesis library written in Rust that combines category-theory-inspired Arrow-style combinators with graph-based patching similar to hardware modular synthesizers. It's designed for real-time audio processing with zero-allocation guarantees.

## Architecture

The library is built in three composable layers:

1. **Layer 1 - Combinators** (`src/combinator.rs`): Functional composition with type-safe signal flow using operators like `>>>` (chain), `***` (parallel), `&&&` (fanout), and feedback loops.

2. **Layer 2 - Port System** (`src/port.rs`): Rich metadata for inputs/outputs with semantic signal types (`Audio`, `CV`, `Gate`, `Trigger`, `V/Oct`) and modulation support.

3. **Layer 3 - Patch Graph** (`src/graph.rs`): Visual patching with cables, mixing, and normalled connections like hardware modular synths.

## Source Structure

```
src/
├── lib.rs              # Main entry, prelude exports, feature gates
├── combinator.rs       # Arrow-style module combinators (Chain, Parallel, Fanout)
├── graph.rs            # Patch graph for visual patching
├── port.rs             # Port system with SignalKind, PortDef, PortSpec
├── modules.rs          # All DSP modules (VCO, VCF, VCA, ADSR, LFO, etc.)
├── analog.rs           # Analog modeling (drift, saturation, thermal)
├── polyphony.rs        # Voice allocation, PolyPatch, unison
├── simd.rs             # SIMD block processing, AudioBlock, RingBuffer
├── rng.rs              # no_std compatible RNG
├── io.rs               # External I/O (AtomicF64, ExternalInput) [alloc]
├── observer.rs         # Real-time state bridge for GUIs [alloc]
├── introspection.rs    # GUI parameter discovery [alloc]
├── serialize.rs        # JSON serialization, ModuleRegistry [alloc]
├── presets.rs          # Preset library [alloc]
├── extended_io.rs      # OSC, plugins, WebAudio [std]
├── mdk.rs              # Module Development Kit [std]
├── visual.rs           # Scope, spectrum analyzer, automation [std]
└── wasm/               # WebAssembly bindings [wasm feature]
    ├── mod.rs
    ├── engine.rs
    └── error.rs

examples/               # Runnable example patches
benches/                # Criterion benchmarks for real-time validation
schemas/                # JSON schemas for patch format
docs/                   # mdbook documentation source
packages/@quiver/       # TypeScript/React packages for WASM
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std`   | Yes     | Full functionality including OSC, plugins, visualization (implies `alloc`) |
| `alloc` | No      | Serialization, presets, I/O for `no_std` + heap environments |
| `simd`  | No      | SIMD vectorization for block processing |
| `wasm`  | No      | WebAssembly bindings with wasm-bindgen and TypeScript types |

Testing and building should use `--all-features` to ensure all code paths are covered.

## Development Commands

The project uses a Makefile for common tasks:

```bash
# Setup development environment (install tools, git hooks)
make setup

# Run all checks (format, lint, test) - USE BEFORE COMMITTING
make check

# Individual commands
make build          # Build with all features
make test           # Run all tests
make fmt            # Format code
make lint           # Run clippy
make coverage       # Run tests with coverage (80% line threshold required)
make bench          # Run benchmarks

# Documentation
make doc            # Build and open rustdoc
make doc-book       # Build mdbook documentation

# WASM
make wasm           # Build WASM package (release)
make wasm-check     # Check WASM compilation
```

## Code Quality Requirements

### Testing
- **80% line coverage threshold** is enforced - new code must include tests
- Run tests with: `cargo test --all-features`
- Coverage uses `cargo-llvm-cov` (WASM code excluded from coverage)
- Doc tests are part of the test suite

### Linting
- **Clippy**: `cargo clippy --all-features -- -D warnings` (warnings are errors)
- **Formatting**: `cargo fmt --all` (uses `rustfmt.toml` config)
  - Max line width: 100 characters
  - Tab spaces: 4
  - Unix newlines

### Pre-commit Hook
A pre-commit hook runs `cargo fmt --check` and `cargo clippy` on staged `.rs` files. Install with `make install-hooks`.

## Commit Message Format

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`

Examples:
```
feat(modules): add delay line with feedback
fix(svf): correct resonance at high frequencies
docs(readme): update installation instructions
```

## Key Types and Patterns

### Module Trait
All DSP modules implement the `Module` trait:
```rust
pub trait Module {
    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues);
    fn port_spec(&self) -> PortSpec;
}
```

### Creating a Patch
```rust
use quiver::prelude::*;

let mut patch = Patch::new(sample_rate);

// Add modules
let vco = patch.add("vco", Vco::new(sample_rate));
let vcf = patch.add("vcf", Svf::new(sample_rate));
let output = patch.add("output", StereoOutput::new());

// Connect them
patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
patch.connect(vcf.out("lp"), output.in_("left")).unwrap();

// Set output and compile (required before tick)
patch.set_output(output.id());
patch.compile().unwrap();

// Process audio
let (left, right) = patch.tick();
```

### Arrow Combinators
```rust
// Chain modules: A >>> B means output of A feeds input of B
let synth = oscillator >>> filter >>> amplifier;

// Parallel: A *** B processes two signals independently
// Fanout: A &&& B sends one signal to two processors
```

### Signal Types
```rust
pub enum SignalKind {
    Audio,        // -1.0 to 1.0 audio signal
    CvBipolar,    // -5V to +5V CV
    CvUnipolar,   // 0V to +10V CV
    VoltPerOctave,// 1V/octave pitch (0V = C4)
    Gate,         // 0V or +5V gate
    Trigger,      // Short pulse
    Clock,        // Clock pulses
}
```

## Real-Time Constraints

This is an audio library with real-time requirements. Key considerations:
- **Zero allocation in audio path**: Use pre-allocated buffers
- **No blocking operations**: No locks, no I/O in tick()
- **Predictable performance**: Avoid variable-time algorithms
- Benchmarks validate real-time compliance at various sample rates and buffer sizes

## CI/CD Pipeline

On every PR:
- Format check (`cargo fmt --check`)
- Clippy lint (`cargo clippy --all-features -- -D warnings`)
- Tests (`cargo test --all-features`)
- Examples build and run
- Documentation build (with `-D warnings`)

On main branch only (expensive checks):
- MSRV check (Rust 1.78)
- Benchmarks
- Coverage (80% threshold)

## Common Module Types

**Oscillators**: `Vco`, `AnalogVco`, `Lfo`, `NoiseGenerator`
**Filters**: `Svf` (state-variable), `DiodeLadderFilter`
**Envelopes**: `Adsr`
**Amplifiers**: `Vca`, `Mixer`
**Utilities**: `Attenuverter`, `Offset`, `Multiple`, `SlewLimiter`, `Quantizer`
**Logic/CV**: `Comparator`, `LogicAnd/Or/Xor/Not`, `Min`, `Max`
**Sequencing**: `Clock`, `StepSequencer`, `SampleAndHold`
**Effects**: `RingModulator`, `Crossfader`, `Rectifier`
**Analog artifacts**: `Crosstalk`, `GroundLoop` (for analog modeling)

## Patch Serialization

Patches can be saved/loaded as JSON using the `PatchDef` type:
```rust
let def = patch.to_def("My Patch");
let json = def.to_json().unwrap();

// Reload
let registry = ModuleRegistry::new();
let patch = Patch::from_def(&def, &registry, sample_rate)?;
```

Schema available at `schemas/patch.schema.json`.

## WASM Support

For WebAssembly targets:
```bash
make wasm-check     # Verify WASM compilation
make wasm           # Build WASM package
```

The `wasm` feature enables JavaScript bindings via `wasm-bindgen` and TypeScript types via `tsify`.

## Documentation

- **User Guide**: `docs/` directory (mdbook format)
- **API Reference**: Generated with `cargo doc`
- **Examples**: `examples/` directory with runnable patches

Run `make doc-serve` to serve documentation locally.
