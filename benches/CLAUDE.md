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

## Running Benchmarks

```bash
# Run all benchmarks
make bench

# Run benchmarks without timing (just compile test)
make bench-test

# Run with Cargo directly
cargo bench

# Run specific benchmark group
cargo bench -- simple_patch
cargo bench -- polyphony
```

## Benchmark Groups

### Module Benchmarks
- Individual module processing (`tick()`)
- Various sample rates

### Patch Benchmarks
- Simple patch (VCO → VCF → VCA → Output)
- Modulated patch (with LFO modulation)
- Complex patch (multiple signal paths)

### Polyphony Benchmarks
- Voice counts: 1, 4, 8, 16, 32
- Extended: 48, 64, 128 voices

### Buffer Size Benchmarks
- Standard: 64, 128, 256, 512 samples
- Ultra-low latency: 16, 32, 48 samples

### Sample Rate Benchmarks
- Standard: 44.1kHz, 48kHz
- High resolution: 96kHz, 192kHz

## Files

```
benches/
├── CLAUDE.md               # This file
└── audio_performance.rs    # Main benchmark suite
```

## Writing Benchmarks

Use Criterion for consistent, reliable benchmarks:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use quiver::prelude::*;

fn bench_module(c: &mut Criterion) {
    let mut vco = Vco::new(44100.0);
    let mut inputs = PortValues::new();
    let mut outputs = PortValues::new();

    c.bench_function("vco_tick", |b| {
        b.iter(|| {
            vco.tick(black_box(&inputs), black_box(&mut outputs));
        })
    });
}

criterion_group!(benches, bench_module);
criterion_main!(benches);
```

## CI Integration

Benchmarks run on the main branch only (too expensive for every PR):
- Compile and test benchmarks: `cargo bench -- --test`
- Full benchmark run stored for comparison

## Interpreting Results

- **Mean**: Average time per iteration
- **Median**: Middle value (less affected by outliers)
- **Throughput**: Samples/second or buffers/second
- **Regression**: Comparison against previous runs

Target: Processing time should be well under the time budget for the target sample rate and buffer size. Aim for <50% of budget to leave headroom.

## Performance Optimization

If benchmarks show regressions:

1. **Profile**: Use `cargo flamegraph` or `perf`
2. **Check allocations**: Audio path should be allocation-free
3. **Review SIMD**: Enable `simd` feature for block processing
4. **Check branching**: Avoid unpredictable branches in hot paths
