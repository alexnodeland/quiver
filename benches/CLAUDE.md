# Benchmarks

This directory contains Criterion benchmarks for validating Quiver's real-time performance.

## Overview

Audio processing has strict real-time requirements. For a buffer to be processed in time, we must complete processing before the next buffer arrives:

```
time_budget = buffer_size / sample_rate
```

| Sample Rate | Buffer 64  | Buffer 128 | Buffer 256 | Buffer 512 |
|-------------|------------|------------|------------|------------|
| 44.1 kHz    | 1.45 ms    | 2.90 ms    | 5.80 ms    | 11.61 ms   |
| 48 kHz      | 1.33 ms    | 2.67 ms    | 5.33 ms    | 10.67 ms   |
| 96 kHz      | 0.67 ms    | 1.33 ms    | 2.67 ms    | 5.33 ms    |
| 192 kHz     | 0.33 ms    | 0.67 ms    | 1.33 ms    | 2.67 ms    |

### What platform/profile do the numbers represent?

**Every measured number in this document was produced with the `release`
profile (`opt-level = 3`, `lto = true`) on native hardware
(Apple-Silicon `arm64`, macOS).** They are single-threaded and reflect one CPU
core. Treat them as *representative*, not portable guarantees:

- **Profile matters.** The `release` profile is speed-optimized
  (`opt-level = 3`). Benchmarks and the real-time compliance test both measure
  this profile, so the numbers match what a real host ships. (Previously
  `release` was size-optimized `opt-level = "z"` — the benches measured a
  faster config than production shipped. Fixed: see `Cargo.toml`.)
- **Architecture matters.** SIMD width, cache sizes, and autovectorization
  differ across `arm64` (NEON, 128-bit) and `x86-64` (SSE/AVX, up to 512-bit).
  Re-measure on your target before quoting a headline.
- **WASM numbers are NOT measured here.** The browser build is a different
  target (`wasm32`, `+simd128` via `.cargo/config.toml`) and runs inside a
  Web Audio worklet. Native criterion numbers do **not** transfer to wasm; the
  authoritative wasm figures come from the browser-demo path, not this suite.

## Running Benchmarks

```bash
# Run the full suite twice: scalar build, then SIMD build (see SIMD A/B below)
make bench

# Scalar-vs-SIMD A/B for the block ops (saves + compares criterion baselines)
make bench-simd

# Real-time compliance gate (MUST be release)
make bench-rt        # == cargo test --release --test realtime_compliance -- --nocapture

# Compile-only (no timing)
make bench-test      # == cargo bench -- --test

# Direct cargo
cargo bench
cargo bench -- heavy_fx          # one group
cargo bench -- polyphony
```

## Benchmark Groups

### Module Benchmarks
- Individual module `tick()` cost across sample rates.
- `modules/expensive` — per-tick cost of the individually heavy modules
  (Supersaw, Wavetable, KarplusStrong, Reverb, Granular, PitchShifter, Vocoder)
  at 48 kHz and 96 kHz.

### Patch Benchmarks
- Simple patch (VCO → VCF → VCA → Output)
- Modulated patch (with LFO modulation)
- Complex patch (multiple signal paths)

### Heavy-FX Worst Case (`heavy_fx/chain`)
- `Supersaw → DiodeLadderFilter → Chorus → DelayLine → Reverb` at 96 kHz,
  block-processed with `tick_block` into 32- and 64-sample buffers. This is the
  most expensive single-voice path the library can build and is also the
  reference worst case for the real-time compliance test.

### Polyphony Benchmarks
- **Voices are fully populated** (`PolyPatch::with_voice_fn`): every voice is a
  real `ctrl → Vco → Svf → Vca` graph with an `Adsr` driving the VCA. Each
  polyphony bench asserts the synth is **non-silent** during setup, so an
  empty/mis-wired voice graph fails loudly instead of silently measuring
  nothing.
- Voice counts: 1, 4, 8, 16, 32; extended stress: 48, 64, 128.

  > Historical note: earlier benches used `PolyPatch::new(n, sr)`, which builds
  > **controller-only voices with no DSP** — so every "polyphony" number was
  > measuring zero synthesis work. `PolyPatch::new` must not be used for load
  > measurement; use `with_voice_fn`.

### Buffer Size Benchmarks
- Standard: 64, 128, 256, 512 samples
- Ultra-low latency: 16, 32, 48 samples

### Sample Rate Benchmarks
- Standard: 44.1 kHz, 48 kHz; high resolution: 96 kHz, 192 kHz

## SIMD A/B (scalar vs. `wide` f64x4)

The four element-wise `AudioBlock` ops (`add_scalar`, `mul_scalar`,
`add_block`, `mul_block`) are feature-gated in `src/simd.rs`: built with
`--features simd` they use the `wide::f64x4` path, otherwise a scalar loop. The
benches use **identical names** either way, so a criterion baseline saved from
the scalar build compares directly against the SIMD build:

```bash
# what `make bench-simd` runs:
cargo bench -- simd --save-baseline scalar
cargo bench --features simd -- simd --baseline scalar
```

### Honest measured delta (native arm64, opt-level 3)

Measured on Apple-Silicon `arm64`. **The `wide` f64x4 path shows no consistent
win here — and at small block sizes it is *slower*:**

| op          | block 64        | block 512      |
|-------------|-----------------|----------------|
| `add_scalar`| **+50% slower** | −5% (faster)   |
| `mul_scalar`| **+55% slower** | +8% (slower)   |
| `add_block` | −2% (neutral)   | +2% (neutral)  |
| `mul_block` | −5% (faster)    | +1% (neutral)  |

Why: `f64x4` is a 256-bit lane group that arm64 NEON implements as two 128-bit
operations, while the scalar loops already autovectorize under `opt-level = 3`,
so the explicit chunk + `copy_from_slice` mostly adds overhead. **On `x86-64`
with native 256-bit AVX, and on `wasm32` with `+simd128`, the balance can be
very different** — that is exactly why the A/B flow exists: re-run it on your
target and read criterion's `change:` line rather than trusting a single
platform's numbers. (The SIMD path is still validated for *bit-exact*
correctness against scalar in `src/simd.rs` tests, regardless of speed.)

## Real-Time Compliance Test

Criterion only *prints* timings. `tests/realtime_compliance.rs` is a real
`#[test]` that **asserts** the worst cases stay inside the real-time budget:

- `heavy_fx_chain_meets_realtime_deadline` — the heavy-FX chain, 1 s @ 48 kHz.
- `polyphonic_8_voices_meets_realtime_deadline` — 8 populated voices, 1 s @ 48 kHz.

The wall-clock assertion (elapsed < 80% of real time) only fires in **optimized
builds**; debug builds run the workload as a non-silence smoke test and skip the
timing check (unoptimized DSP is 10x+ slower and would be meaningless). CI runs
`cargo test --release --test realtime_compliance`, so the deadline is genuinely
gated. Latest headroom on the reference machine:

- heavy-FX chain: **6.4%** of the 48 kHz budget used → **93.6% headroom**.
- 8 populated voices: **42.4%** of budget used → **57.6% headroom**.

## Measured Reference Numbers (native arm64, release)

Single core, 48 kHz unless noted. Per-sample budget @ 48 kHz = **20.83 µs**.

### Populated polyphony — the honest max-voice headline

| voices | tick (per sample) | µs / voice |
|--------|-------------------|------------|
| 1      | 0.97 µs           | —          |
| 4      | 4.46 µs           | 1.12       |
| 8      | 8.98 µs (43% RT)  | 1.12       |
| 16     | 18.5 µs           | 1.16       |
| 32     | 37.2 µs           | 1.16       |

Marginal cost ≈ **1.17 µs / voice / sample**. Against the 20.83 µs per-sample
budget that is **≈ 18 voices at 100% of budget, and ≈ 14 voices at an 80%
safety margin**, on a single core at 48 kHz, for this VCO→VCF→VCA+ADSR voice.
(Contrast the old empty-voice benches, which measured ~zero DSP and would have
implied "hundreds of voices".)

### Heavy-FX chain
≈ **1.34 µs / sample** (stable across 32- and 64-sample blocks): **6.4%** of the
48 kHz budget, **12.9%** of the 96 kHz budget.

### Expensive modules (per `tick()`, 48 kHz)
Supersaw ≈ 46 ns · Reverb ≈ 81 ns · Granular ≈ 72 ns · PitchShifter ≈ 119 ns ·
Vocoder ≈ 153 ns — each well under 1% of a single-sample budget on its own; the
heavy chain's cost comes from stacking six such stages in series.

## Files

```
benches/
├── CLAUDE.md               # This file
└── audio_performance.rs    # Main benchmark suite

tests/
└── realtime_compliance.rs  # Release-gated real-time deadline assertions
```

## CI Integration

The `bench` job runs on **main only** (too expensive per-PR). It:

1. Compiles the benches both ways (`cargo bench --no-run` and
   `... --features simd`) so both code paths are guaranteed to build.
2. Gates the real-time deadline: `cargo test --release --test realtime_compliance`.
3. Runs the suite with short warm-up/measurement times and uploads
   `target/criterion` as the `criterion-baselines` artifact for cross-run
   comparison. No external benchmark service is involved.

## Interpreting Results

- **Mean / Median**: time per iteration (median is less outlier-sensitive).
- **Throughput**: samples or buffers per second.
- **change:**: criterion's comparison against the saved baseline — this is the
  line to read for A/B and regression work.

Target: processing time well under budget for the target sample rate and buffer
size. Aim for **< 50%** of budget to leave headroom (the RT test uses a more
lenient 80% ceiling to tolerate noisy CI runners).

## Performance Optimization

If benchmarks show regressions:

1. **Profile**: `cargo flamegraph` or `perf`.
2. **Check allocations**: the audio path must be allocation-free (`tick` /
   `tick_block` / `PolyPatch::tick` allocate nothing in steady state; see
   `tests/zero_alloc.rs`).
3. **Review SIMD**: `--features simd` — but *measure*, don't assume (see the
   SIMD A/B section; it does not always win).
4. **Check branching**: avoid unpredictable branches in hot paths.
```
