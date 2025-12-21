# Installation

Getting Quiver into your project is straightforward. The library is pure Rust with minimal dependencies.

## Prerequisites

- **Rust 1.70+** (2021 edition)
- **Cargo** (comes with Rust)

Verify your installation:

```bash
rustc --version
cargo --version
```

## Adding Quiver to Your Project

### As a Dependency

Add to your `Cargo.toml`:

```toml
[dependencies]
quiver = { git = "https://github.com/alexnodeland/quiver" }
```

Or with specific features:

```toml
[dependencies]
quiver = { git = "https://github.com/alexnodeland/quiver", features = ["simd"] }
```

### Available Features

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | Yes | Full functionality with I/O, serialization, and visualization tools |
| `simd` | No | SIMD vectorization for block processing |

### `no_std` Support

Quiver supports `no_std` environments for embedded systems and WebAssembly targets. To use Quiver without the standard library:

```toml
[dependencies]
quiver = { git = "https://github.com/alexnodeland/quiver", default-features = false }
```

#### What's Available in `no_std` Mode

Core DSP functionality works in `no_std` using the `alloc` crate and `libm` for math operations:

- All oscillators (VCO, LFO, AnalogVco)
- All filters (SVF, DiodeLadderFilter)
- Envelope generators (ADSR)
- Amplifiers and mixers (VCA, Mixer)
- Utility modules (Clock, Quantizer, SlewLimiter, etc.)
- Logic modules (AND, OR, XOR, NOT, Comparator)
- Analog modeling (Saturator, Wavefolder, component drift)
- Polyphony (VoiceAllocator, PolyPatch)
- Patch graph with all connection types

#### What Requires `std`

The following modules are only available with the `std` feature:

| Module | Reason |
|--------|--------|
| `io` (AtomicF64, MidiState, ExternalInput/Output) | Thread-safe atomics and system I/O |
| `extended_io` (OSC, Plugin wrapper, Web Audio) | Network and plugin host interfaces |
| `serialize` (PatchDef, JSON save/load) | serde_json requires std |
| `visual` (Scope, SpectrumAnalyzer, LevelMeter) | FFT and visualization tools |
| `mdk` (ModuleTemplate, DocGenerator) | Module development kit |
| `presets` (ClassicPresets, PresetLibrary) | Preset management |

#### Implementation Notes

- Uses `BTreeMap` instead of `HashMap` (no hashing in `no_std`)
- Includes a seedable Xorshift128+ RNG for deterministic random generation
- Math functions provided by `libm` (sin, cos, pow, sqrt, exp, log, etc.)
- Heap allocations via `alloc` crate (Vec, Box, String)

## Verifying Installation

Create a simple test program:

```rust,ignore
use quiver::prelude::*;

fn main() {
    let patch = Patch::new(44100.0);
    println!("Quiver is working! Patch created at {}Hz", 44100.0);
}
```

Run it:

```bash
cargo run
```

## Building the Examples

Clone the repository and run an example:

```bash
git clone https://github.com/alexnodeland/quiver
cd quiver
cargo run --example simple_patch
```

## Building Documentation

Generate the API documentation locally:

```bash
cargo doc --open
```

This opens the rustdoc documentation in your browser with all type information and examples.

## Editor Setup

For the best experience, use an editor with Rust support:

- **VS Code** with rust-analyzer extension
- **IntelliJ IDEA** with Rust plugin
- **Neovim** with rust-tools.nvim

Type hints are particularly helpful given Quiver's strong typing—your editor will show you exactly what signals flow where.

---

Next: [Your First Patch](./first-patch.md)
