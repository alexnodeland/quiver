# Quiver

[![CI](https://github.com/alexnodeland/quiver/actions/workflows/ci.yml/badge.svg)](https://github.com/alexnodeland/quiver/actions/workflows/ci.yml)
[![Documentation](https://github.com/alexnodeland/quiver/actions/workflows/docs.yml/badge.svg)](https://github.com/alexnodeland/quiver/actions/workflows/docs.yml)
[![Coverage](https://img.shields.io/badge/coverage-%E2%89%A580%25-brightgreen)](https://github.com/alexnodeland/quiver/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

A modular audio synthesis library using Arrow-style combinators and graph-based patching.

## Features

- **Typed Combinators**: Compose audio modules using category-theory-inspired operators (`>>>`, `***`, `&&&`)
- **Graph-Based Patching**: Build complex synthesizer patches with a flexible node/cable system
- **Analog Modeling**: Realistic VCO drift, filter saturation, and component tolerances
- **Polyphony**: Built-in voice allocation with multiple algorithms
- **SIMD Optimization**: Optional vectorized processing for performance-critical applications
- **Serialization**: Save and load patches as JSON

## Quick Start

Add Quiver to your `Cargo.toml`:

```toml
[dependencies]
quiver = "0.1"
```

Build a simple synthesizer patch:

```rust
use quiver::prelude::*;

fn main() {
    // Create a patch
    let mut patch = Patch::new();

    // Add modules
    let vco = patch.add_module(Vco::new());
    let vcf = patch.add_module(Svf::new());
    let vca = patch.add_module(Vca::new());
    let output = patch.add_module(StereoOutput::new());

    // Connect them
    patch.connect(vco, "out", vcf, "input");
    patch.connect(vcf, "lowpass", vca, "input");
    patch.connect(vca, "out", output, "left");

    // Process audio
    patch.tick();
}
```

## Documentation

- [User Guide](https://alexnodeland.github.io/quiver/) - Comprehensive tutorials and concepts
- [API Reference](https://alexnodeland.github.io/quiver/api/quiver/) - Rustdoc documentation
- [Examples](./examples/) - Runnable example patches

## Examples

Run the examples to hear Quiver in action:

```bash
# Simple patch demo
cargo run --example simple_patch

# FM synthesis tutorial
cargo run --example tutorial_fm

# Polyphonic synth
cargo run --example tutorial_polyphony
```

## Development

See [DEVELOPMENT.md](./DEVELOPMENT.md) for the development roadmap and architecture decisions.

```bash
# Run all checks
make check

# Run tests with coverage
make coverage

# Format and lint
make fmt lint
```

## Contributing

Contributions are welcome! Please read our [Contributing Guidelines](./.github/CONTRIBUTING.md) before submitting a PR.

Areas where help is appreciated:
- DSP algorithms (filter models, oscillator antialiasing)
- Testing (audio comparison tests, benchmarks)
- Documentation (tutorials, examples)
- Module implementations

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
