# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This changelog is auto-generated from git history. Run `make changelog` to update.
Sections above the auto-generated marker are hand-written and are preserved.

## [0.2.0] - unreleased

An interpreter performance pass over the patch graph (`perf/interpreter-r-series`).
Every change is bit-exact for audio: the same `(patch, parameters)` renders the same
samples as 0.1.1, pinned in CI by `tests/golden_vectors.rs`. The two breaking items
below are both **API** breaks, not numeric ones.

### Breaking

- **`PortValues` is no longer a public `HashMap` wrapper.** The `pub values:
  HashMap<PortId, f64>` field is gone; `PortValues` is now two private parallel vectors
  (ids in first-write order, `Option<f64>` per slot) scanned linearly, which removes
  per-key hashing from the audio path. All accessors (`get`, `get_or`, `set`, `has`,
  `clear`, `accumulate`) keep their exact signatures and semantics.
  - *Migration*: reading `pv.values` becomes `pv.iter()`, which yields `(PortId, f64)`
    for every port that currently holds a value — and does so in a **deterministic**
    order rather than the `HashMap`'s arbitrary one. Constructing
    `PortValues { values }` by struct literal becomes `PortValues::new()` plus one
    `set()` per port.

### Performance

- Normalled inputs resolve from a precompiled list instead of a blanket per-input probe.
- Dense `PortValues` (above) plus scatter-by-slot: no hashing in `gather`/`scatter`.
- Modules are stored and walked in execution order, so the per-sample loop zips the
  routing plan against the module list instead of resolving a slotmap key per node per
  sample; cable attenuation/offset are baked to plain `f64` at compile time.

### Added

- `tests/golden_vectors.rs`: five fixed patches hashed sample-for-sample, the
  bit-exactness gate for this and every later numeric change.
- `tests/output_masking.rs`: per-module proof that masking an output cannot perturb the
  outputs that survive, on this sample or any later one.

## [Unreleased]

A large correctness-and-capability pass across the whole library. Quiver remains
pre-1.0 with no published release; the public API is still evolving.

### Added
- **New modules**: `SamplePlayer` (mono sample playback with V/Oct, start position,
  looping), `Ducker` (dedicated sidechain ducking), `MidSideEncode` / `MidSideDecode`
  (mid/side with width control).
- **Offline rendering**: `render()` and `render_to_wav()` for non-real-time bounce to
  buffers or WAV files.
- **Microtuning**: `ScaleQuantizer::set_custom_scale` and `load_scala` (Scala `.scl`).
- **Anti-aliasing**: opt-in 2x/4x oversampling on `Distortion` and `Wavefolder`
  (`set_oversample`).
- **Sidechaining**: key/sidechain inputs on `Compressor`, `Limiter`, and `NoiseGate`.
- **Serialization**: `PatchDef.output` field, `PatchMeta` with `Patch::meta` /
  `set_meta`, and parameter round-tripping via module introspection
  (`Patch::param_infos` / `get_param_by_id` / `set_param_by_id`).
- **Block processing**: `Patch::tick_block` for buffer-at-a-time rendering.

### Changed
- **Polyphony redesign**: `PolyPatch::with_voice_fn` builds one graph per voice from a
  closure; automatic voice freeing by amplitude follower, releasing-first voice
  stealing, and smoothed `1/sqrt(N)` level compensation.
- **WASM/TypeScript overhaul**: worklet-based `createQuiverAudioNode` is now the audio
  path (the old `createAudioContext` helper was removed); MIDI CV source inputs,
  stable `CableId`-returning `connect` / `disconnect_cable`, and observer decimation
  via `set_observer_interval`.
- **DSP reparameterization**: SVF rebuilt as a TPT/ZDF filter with stable
  self-oscillation (`k = 2 − 2·res`); ADSR now uses true segment durations plus a
  linear/exponential `shape` port; VCA gains `response` and `gain` ports; VCO adds
  linear through-zero FM (`fm_lin`) and PolyBLEP/PolyBLAMP antialiasing; `Limiter` is
  now a true brick-wall limiter; `Flanger` and `Phaser` are stereo with a `spread`
  control.
- Module `type_id`s are snake_case (`"vco"`, `"svf"`, `"adsr"`, …).

### Fixed
- **Zero-allocation audio path**: the compiled patch graph no longer allocates while
  ticking (verified by `tests/zero_alloc.rs`).
- **Serialization round-trip**: presets and patches load without panics; `from_def`
  is panic-free and rejects patches newer than `CURRENT_PATCH_VERSION`.
- Broad DSP correctness fixes across oscillators, filters, envelopes, and effects.
- Substantially expanded test coverage (edge cases, error handling, subscriptions).

---

<!-- Auto-generated content below this line -->
<!-- Generated on: Run `make changelog` to update -->

## Recent Changes

### Documentation & CI
- Add stress test and extended benchmarks
- Format benchmark code
- Add benchmarks to CI pipeline
- Add comprehensive audio performance benchmarks
- Add docs/book/ to .gitignore (generated output)
- Fix examples to match actual library APIs
- Fix formatting and mdbook build configuration
- Add doc tests and enhanced CI/CD pipelines
- Add comprehensive documentation wiki and examples

### Testing & Coverage
- Remove ModuleRegistry tests that cause UB under tarpaulin
- Fix formatting issues
- Add tarpaulin-report.html to gitignore
- Add 80% coverage threshold check to CI workflow
- Add comprehensive test coverage across all modules

### Features
- Phase 5 ecosystem features (MDK, presets, visual tools)
- Phase 4 advanced features (polyphony, SIMD, extended I/O)
- Phase 3 analog modeling refinement
- Phase 2 hardware fidelity features
- Core modular synthesis library
