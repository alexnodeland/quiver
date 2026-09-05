# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This changelog is auto-generated from git history. Run `make changelog` to update.
Sections above the auto-generated marker are hand-written and are preserved.

## [0.4.0] - unreleased

A correctness pass driven by the September 2026 stack audit (findings Q-N1 … Q-N8).
Unlike 0.2.0, this release is **not bit-exact** for every patch: two DSP fixes change
rendered samples, and `tests/golden_vectors.rs` was rebaselined for exactly one of its
five existing patches, deliberately and with the reason recorded in the file. Everything
else that touches audio is either bit-identical (verified by the unchanged golden hashes)
or opt-in.

### Numerics — what changed and what did not

- **Changed: patches with a `DelayLine`/`UnitDelay` fed by an acyclic path.** The
  scheduler used to drop *every* cable into a cycle-breaker from the topological sort, so
  an acyclic `vco → delay → out` read the VCO's *previous* sample — one sample of latency
  on the delay path — unless the VCO happened to be `add`ed first. Only cables that
  actually close a cycle are deferred now, so such a delay reads the current sample and
  the render no longer depends on node insertion order. Golden `delay_chorus` moved
  (`0x0bd3_4a0e_47ac_ee37 → 0x8b20_c2b4_31f7_8812`); the other four golden patches, which
  contain no cycle-breaker, are unchanged. A feedback loop through one breaker renders
  identically to 0.3.x *except* where the deferred edge landed on an acyclic input before.
  When several breakers sit on one cycle the first-inserted one takes the deferred edge —
  the one remaining insertion-order dependence, and one with no topological tie-break.
- **Changed: `KarplusStrong`, at every `stretch` including `0`.** The stretch stage was
  a first-order *difference* whose gain at Nyquist reached 2.9 at full stretch; whether the
  loop stayed stable depended on the fractional part of the period (i.e. on pitch), and an
  overflow left the module silent until `reset()`. The loop is now
  `tap → one-pole → first-order allpass → leak`, every stage of magnitude ≤ 1, so it is
  stable for any pitch, damping and stretch; a pluck restarts the loop-filter state, a
  finite guard clears the loop instead of latching, and non-finite CVs fall back to their
  defaults. At `stretch = 0` the one-pole now keeps its own state (pole at `1 − c` rather
  than `L(1 − c)`) and the allpass is a compensated unit delay, so renders differ slightly
  from 0.3.x even there; tuning at C4 and its octaves stays within the existing ±20 cent
  test. Positive `stretch` now sharpens the upper partials (stiff-string inharmonicity),
  negative flattens them.
- **Unchanged:** every other module, the routing arithmetic, `PitchShifter`/`Granular`/
  `Wavefolder` for finite input (they now `sanitize_audio` non-finite input to silence),
  and the noise sources for callers who do not opt into `Patch::seed` (the golden `noise`
  vector is unchanged).

### Added

- **Per-module random streams.** `GraphModule::seed(&mut self, seed: u64)` (default
  no-op) and `Patch::seed(u64)`, which derives a distinct seed per node
  (`rng::derive_seed`) and re-applies it on `Patch::reset()` and to nodes added later;
  `Patch::seed_value()` reads it back. `PolyPatch::seed` gives each voice (and unison
  member) its own patch seed and keeps it across sample-rate/unison rebuilds.
  `NoiseGenerator`, `KarplusStrong`, `BernoulliGate`, `AnalogVco` (component tolerances
  included), `Granular` and `Arpeggiator` own a `rng::ModuleRng` that draws from the
  thread-global stream until seeded — so `quiver::rng::seed` keeps working exactly as
  before for anyone who never calls `Patch::seed` — and rewinds on `reset()`. Two seeded
  renders are identical regardless of what any other module or patch on the thread
  consumed (`tests/seeded_rng.rs`).
- `Patch::output_slot` / `output_value_at` / `routing_generation` for hash-free
  per-sample metering reads; `StateObserver::collect_params` is public.
- Golden gate: `multi_cable` (several cables summed into one input, plain / attenuated /
  offset), `feedback_loop` (a genuine `mixer ↔ delay` cycle), `poly` (a four-voice
  `PolyPatch`), `set_param_mid_render`, and a `tick_block == tick` bitwise check over every
  golden patch with ragged block sizes. The header no longer claims "multi-edge inputs" for
  the subtractive patch (it never had any).
- `DelayLine::MAX_DELAY_CAP_SECS` (60 s): `with_max_delay` clamps its argument to
  `0.001..=60` and treats a non-finite value as the 2 s default.

### Changed

- **`set_param_by_id` is real-time safe.** A control-input port override is written
  straight into the compiled routing plan — no recompile, no allocation, no routing-buffer
  reset (a cycle-breaker's feedback path is not blanked for a sample) — and takes effect on
  the next tick (`tests/zero_alloc.rs`). It returns `false` for a **cabled** input, whose
  base value is shadowed; the knob position is still recorded and applies when the cable is
  removed. The WASM `set_param_by_name` treats that case as `Ok`.
- **WASM observer captures every sample.** `process_block` renders per sample while any
  port is subscribed and calls `collect_sample` after each; Scope/Spectrum/Level no longer
  see one sample in 1024. Formatting (`Scope` buffer clones, the spectrum FFT, `Param`
  reads) moved out of `process()` into `poll_updates`. `set_observer_interval` is a retained
  no-op.
- `PatchDef.parameters` is a `BTreeMap<String, f64>` (was a `HashMap` under `std`), so
  `to_json()` is canonical: the same patch always serialises to byte-identical JSON.
  *Migration:* code that named the type or relied on `HashMap`-specific methods changes
  type; iteration order is now sorted.
- `Wavetable` shares one process-wide table bank under `std` (`OnceLock`) instead of
  recomputing 131 KB of tables per instance; `no_std` keeps a per-instance bank. Values are
  identical.
- `MDK`: `ModuleTestHarness::new` seeds the module under test (`ModuleTestHarness::SEED`),
  and the stability, output-range and NaN-recovery checks drive Gate/Trigger/Clock inputs
  with a pulse train, so trigger-driven modules are actually excited. The
  `noise / reset_clears_state` allowlist entry is gone.
- `Arpeggiator::reset` and `Granular::reset` rewind their random streams (they used to
  keep, respectively, the stream and a hard-wired seed-42 restart; the default stream is
  still seed 42 until `seed()` is called).
- The unused optional `rand` dependency is gone (it also pulled `getrandom` into
  `std` + wasm32 builds). The README no longer describes the `simd` feature as vectorising
  processing: it provides `AudioBlock`/`RingBuffer` helpers; modules and the patch engine
  are scalar.
- `KarplusStrong` pre-sizes its delay line (`max_len + 2`) so a pluck never reallocates
  on the audio thread; `DelayLine`, `Chorus` and `Flanger` no longer reallocate their
  buffers in `set_sample_rate` when the size is unchanged (the `Patch::add` → same-rate
  case, ~4.6 MB twice for `tape_delay`).
- `flush_denorm`'s doc now says what it is: a `1e-20` tiny-value silence floor, not a
  strict denormal flush.

### Fixed

- `KarplusStrong` stretch instability and permanent-silence-after-overflow (above).
- `PitchShifter`, `Granular` (including the `freeze` case) and `Wavefolder` sanitise their
  audio input; `tests/nan_recovery.rs` covers them and `KarplusStrong` with a strict
  every-sample-finite criterion.
- Cycle-breaker scheduling depended on insertion order (above).
- The MDK harness's stability checks were vacuous for trigger-driven modules (above).

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

- **Mask-aware modules may skip outputs nothing reads.** `GraphModule` gains a defaulted
  `tick_masked(inputs, outputs, wanted: u32)`; the compiled patch tells each module which
  of its outputs anything consumes, and `Vco`, `Lfo`, and `NoiseGenerator` — the only
  three modules that opt in; every other module is unaffected — skip producing the rest.
  - *Consequence*: an output of one of those three modules that has **no cable leaving
    it** is never written into the routing buffer, so `Patch::get_output_value` — and
    with it `StateObserver` / the WASM `Engine.subscribe` metering targets (Level, Gate,
    Scope, Spectrum) — reads a flat `0.0` for it instead of a live sample. Concretely: a
    scope or meter on an unpatched `vco.sin` / `vco.tri` / `vco.sqr`, `lfo.*`, or
    `noise.pink` goes silent. Patched ports, and every port of every other module, are
    unchanged. **Rendered audio is bit-identical either way** — see
    `tests/golden_vectors.rs`.
  - *Opt-out*: `Patch::keep_output_live(node, port)` pins a port so it is produced whether
    or not a cable reads it (`release_output_live` / `clear_kept_live_outputs` /
    `kept_live_outputs` round it out). `StateObserver::sync_output_keepalive(&mut patch)`
    applies the pin to every port a bus meters, and the WASM `Engine` calls it
    automatically from `subscribe`, `unsubscribe`, `clear_subscriptions`, and `compile` —
    so **JS metering of an unpatched port keeps working with no code change**. Native
    callers that meter through `StateObserver` should add one
    `observer.sync_output_keepalive(&mut patch)` after changing subscriptions; callers
    that call `get_output_value` directly should pin the ports they read.

### Performance

- Normalled inputs resolve from a precompiled list instead of a blanket per-input probe.
- Dense `PortValues` (above) plus scatter-by-slot: no hashing in `gather`/`scatter`.
- Modules are stored and walked in execution order, so the per-sample loop zips the
  routing plan against the module list instead of resolving a slotmap key per node per
  sample; cable attenuation/offset are baked to plain `f64` at compile time.
- Outputs nothing consumes are not computed (above).

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
