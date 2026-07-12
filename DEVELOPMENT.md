# Development Guide

This document is for people working **on** Quiver (rather than building synths with it).
It covers the architecture, the day-to-day workflow, and the quality bar every change is
held to.

For user-facing documentation see the [User Guide](https://alexnodeland.github.io/quiver/)
and the runnable [`examples/`](./examples/).

## Architecture

Quiver is built in three composable layers. Higher layers are written in terms of lower
ones, and you can drop down a layer at any time.

### Layer 1 — Combinators (`src/combinator.rs`)

Category-theory-inspired, Arrow-style composition over the `Module` trait. Modules are
composed like functions with type-safe operators:

- `>>>` — **chain** (sequential composition): `a >>> b` feeds `a`'s output into `b`.
- `***` — **parallel**: process two independent signals side by side.
- `&&&` — **fanout**: send one signal to two processors.
- `Feedback` — feedback loops with a one-sample delay to break the cycle.

This layer is pure, allocation-free, and `no_std`.

### Layer 2 — Port System (`src/port.rs`)

Rich metadata for module inputs and outputs. Each port carries a semantic
[`SignalKind`](./src/port.rs) (`Audio`, `CvBipolar`, `CvUnipolar`, `VoltPerOctave`,
`Gate`, `Trigger`, `Clock`), a stable numeric id, a name, and optional modulation
attributes (defaults, attenuverters, normalled fallbacks). The `GraphModule` trait —
implemented by every DSP module — is defined here.

### Layer 3 — Patch Graph (`src/graph.rs`)

Visual, hardware-style patching: `Patch` holds nodes (modules) and cables (`PortRef` →
`PortRef`) with mixing, attenuation/offset, and normalled connections. A `Patch` is
compiled (topological sort + cycle detection) once, then ticked sample-by-sample
(`tick`) or in blocks (`tick_block`) on the audio thread with **zero allocation**.

### Source layout

```
src/
├── lib.rs              # Entry, prelude, feature gates
├── combinator.rs       # Layer 1: Arrow combinators
├── port.rs             # Layer 2: ports, SignalKind, GraphModule
├── graph.rs            # Layer 3: Patch graph
├── modules/            # All DSP modules (oscillators, filters, dynamics, ...)
├── analog.rs           # Analog modeling (drift, saturation)
├── polyphony.rs        # Voice allocation, PolyPatch, unison
├── simd.rs             # SIMD block processing, AudioBlock
├── rng.rs              # no_std RNG
├── io.rs               # External I/O               [alloc]
├── observer.rs         # Real-time state bridge      [alloc]
├── introspection*.rs   # GUI parameter discovery     [alloc]
├── serialize.rs        # JSON serialization          [alloc]
├── presets.rs          # Preset library              [alloc]
├── scala.rs            # Scala .scl microtuning       [alloc]
├── render.rs           # Offline render / WAV export [std]
├── extended_io.rs      # OSC, Web Audio, plugin trait [std]
├── mdk.rs              # Module Development Kit       [std]
├── visual.rs           # Scope, spectrum, meters      [std]
└── wasm/               # WebAssembly bindings         [wasm]
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std`   | Yes     | Full functionality including OSC, plugins, visualization (implies `alloc`) |
| `alloc` | No      | Serialization, presets, I/O for `no_std` + heap environments |
| `simd`  | No      | SIMD vectorization for block processing |
| `wasm`  | No      | WebAssembly bindings (`wasm-bindgen`) + TypeScript types (`tsify`); implies `alloc` |

Build and test with `--all-features` so every code path is exercised. The crate also
compiles in three tiers: core `no_std` (no default features), `alloc`, and full `std`.

## Development Workflow

The project uses a `Makefile`. The common targets:

```bash
make setup          # Install tooling + git hooks
make check          # Format, lint, and test — run this before committing
make build          # Build with all features
make test           # Run all tests (unit, integration, doc)
make test-doc       # Doc tests only
make coverage       # Coverage with the 80% line threshold enforced
make bench          # Criterion benchmarks (real-time validation)
make fmt            # Format (rustfmt)
make lint           # Clippy with -D warnings
make doc            # Build + open rustdoc
make doc-book       # Build the mdbook user guide
make wasm           # Build the WASM package (release)
make wasm-check     # Verify WASM compilation
make examples       # Build all examples
make help           # List every target
```

Run `make check` before every commit — it mirrors the fast half of CI.

### Git hooks

`make install-hooks` installs a pre-commit hook that runs `cargo fmt --check` and
`cargo clippy` on staged `.rs` files.

## Code Quality Bar

- **Tests**: new code must ship with tests. `cargo test --all-features` must be green,
  **including doc tests**. Coverage is enforced at **80% line coverage**
  (`cargo-llvm-cov`; WASM code excluded).
- **Clippy**: `cargo clippy --all-features -- -D warnings` — warnings are errors.
- **Formatting**: `cargo fmt --all` (see `rustfmt.toml`: 2021 edition, 100-column width,
  4-space tabs, Unix newlines).
- **Docs**: `cargo doc --all-features --no-deps` and `mdbook build docs` must build
  cleanly (no broken intra-doc links).
- **Real-time discipline**: the audio path (`tick`/`tick_block`) must not allocate,
  lock, or block. `tests/zero_alloc.rs` asserts zero allocations during ticking.
- **MSRV**: Rust **1.78** (checked in CI on `main`).

### Commit messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`. Example:
`fix(svf): stabilize self-oscillation at high resonance`.

## CI/CD

On every PR: format check, clippy, `cargo test --all-features`, examples build+run,
`cargo doc` (with `-D warnings`), and the Playwright browser E2E suite (Chromium).

On `main` only (the expensive jobs): MSRV check (Rust 1.78), benchmarks, and coverage
(80% threshold).

## Roadmap

Quiver is **pre-1.0**; the near-term focus is API stabilization ahead of a first
release, broadening DSP coverage and correctness, and hardening the WASM/TypeScript
integration. There is no fixed milestone schedule — the live roadmap is the GitHub
[issue tracker](https://github.com/alexnodeland/quiver/issues) and
[good-first-issue](https://github.com/alexnodeland/quiver/labels/good%20first%20issue)
labels.

## Contributing

1. Fork and branch (`git checkout -b feature/my-change`).
2. Make your change with tests and docs.
3. Run `make check` until it is green.
4. Commit using Conventional Commits.
5. Open a PR.

See [`.github/CONTRIBUTING.md`](./.github/CONTRIBUTING.md) for the full contribution
guidelines. Areas where help is especially appreciated: DSP algorithms (filters,
antialiasing, effects), audio-comparison and performance tests, documentation, and new
hardware-inspired modules.
