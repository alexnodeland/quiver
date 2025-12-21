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
| `std` | Yes | Full functionality including OSC, plugins, visualization (implies `alloc`) |
| `alloc` | No | Serialization, presets, and I/O for `no_std` + heap environments |
| `simd` | No | SIMD vectorization for block processing (works with any tier) |

### Feature Tiers

Quiver supports three tiers for different environments:

#### Tier 1: Core Only (`default-features = false`)

For bare-metal embedded systems without heap allocation:

```toml
[dependencies]
quiver = { git = "https://github.com/alexnodeland/quiver", default-features = false }
```

Includes all core DSP modules: oscillators, filters, envelopes, amplifiers, mixers, utilities, logic modules, analog modeling, polyphony, and the patch graph.

#### Tier 2: With Alloc (`features = ["alloc"]`)

For WASM web apps and embedded systems with heap:

```toml
[dependencies]
quiver = { git = "https://github.com/alexnodeland/quiver", default-features = false, features = ["alloc"] }
```

Adds:
- **Serialization** - JSON save/load for patches (`PatchDef`, `ModuleDef`, `CableDef`)
- **Presets** - Ready-to-use patch presets (`ClassicPresets`, `PresetLibrary`)
- **I/O Modules** - External inputs/outputs, MIDI state (`AtomicF64`, `MidiState`)

#### Tier 3: Full Std (default)

For desktop applications and DAW plugins:

```toml
[dependencies]
quiver = { git = "https://github.com/alexnodeland/quiver" }
```

Adds:
- **Extended I/O** - OSC protocol, plugin wrappers, Web Audio interfaces
- **Visual Tools** - Scope, Spectrum Analyzer, Level Meter, Automation Recorder
- **MDK** - Module Development Kit for creating custom modules

#### Feature Matrix

| Tier | DSP | Serialize | Presets | I/O | OSC/Plugins | Visual | MDK |
|------|-----|-----------|---------|-----|-------------|--------|-----|
| Core | ✓ | | | | | | |
| `alloc` | ✓ | ✓ | ✓ | ✓ | | | |
| `std` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

#### Implementation Notes

- Uses `BTreeMap` instead of `HashMap` in non-std modes (no hashing required)
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
