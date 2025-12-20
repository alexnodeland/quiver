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

| Feature | Description |
|---------|-------------|
| `default` | Core functionality |
| `simd` | SIMD vectorization for block processing |

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
