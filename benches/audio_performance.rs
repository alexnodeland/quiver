//! Audio Performance Benchmarks
//!
//! This module provides comprehensive benchmarks for validating that the library
//! meets real-time audio processing requirements at various sample rates, buffer
//! sizes, and polyphony levels.
//!
//! ## Real-Time Audio Constraints
//!
//! For real-time audio, we must process a buffer of samples before the next
//! buffer arrives. The time budget is:
//!
//! ```text
//! time_budget = buffer_size / sample_rate
//! ```
//!
//! | Sample Rate | Buffer 64  | Buffer 128 | Buffer 256 | Buffer 512 |
//! |-------------|------------|------------|------------|------------|
//! | 44.1 kHz    | 1.45 ms    | 2.90 ms    | 5.80 ms    | 11.61 ms   |
//! | 48 kHz      | 1.33 ms    | 2.67 ms    | 5.33 ms    | 10.67 ms   |
//! | 96 kHz      | 0.67 ms    | 1.33 ms    | 2.67 ms    | 5.33 ms    |
//! | 192 kHz     | 0.33 ms    | 0.67 ms    | 1.33 ms    | 2.67 ms    |
//!
//! These benchmarks help validate that we can meet these constraints.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use quiver::prelude::*;

// ============================================================================
// Sample Rate Constants
// ============================================================================

const SAMPLE_RATES: [f64; 4] = [44100.0, 48000.0, 96000.0, 192000.0];
const BUFFER_SIZES: [usize; 4] = [64, 128, 256, 512];
const VOICE_COUNTS: [usize; 5] = [1, 4, 8, 16, 32];

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a simple VCO → VCF → VCA → Output patch
fn create_simple_patch(sample_rate: f64) -> Patch {
    let mut patch = Patch::new(sample_rate);

    let vco = patch.add("vco", Vco::new(sample_rate));
    let vcf = patch.add("vcf", Svf::new(sample_rate));
    let vca = patch.add("vca", Vca::new());
    let output = patch.add("output", StereoOutput::new());

    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
    patch.connect(vca.out("out"), output.in_("left")).unwrap();
    patch.connect(vca.out("out"), output.in_("right")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();
    patch
}

/// Create a patch with LFO modulation (VCO + LFO → VCF → VCA → Output)
fn create_modulated_patch(sample_rate: f64) -> Patch {
    let mut patch = Patch::new(sample_rate);

    let vco = patch.add("vco", Vco::new(sample_rate));
    let lfo = patch.add("lfo", Lfo::new(sample_rate));
    let vcf = patch.add("vcf", Svf::new(sample_rate));
    let vca = patch.add("vca", Vca::new());
    let adsr = patch.add("adsr", Adsr::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    // Main signal path
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
    patch.connect(vca.out("out"), output.in_("left")).unwrap();
    patch.connect(vca.out("out"), output.in_("right")).unwrap();

    // LFO → filter cutoff modulation
    patch.connect(lfo.out("sin"), vcf.in_("fm")).unwrap();

    // ADSR → VCA
    patch.connect(adsr.out("env"), vca.in_("cv")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();
    patch
}

/// Create a complex patch with multiple oscillators, filters, and modulation
fn create_complex_patch(sample_rate: f64) -> Patch {
    let mut patch = Patch::new(sample_rate);

    // 2 oscillators
    let vco1 = patch.add("vco1", Vco::new(sample_rate));
    let vco2 = patch.add("vco2", Vco::new(sample_rate));

    // 2 LFOs for modulation
    let lfo1 = patch.add("lfo1", Lfo::new(sample_rate));
    let lfo2 = patch.add("lfo2", Lfo::new(sample_rate));

    // Diode ladder filter (more CPU intensive)
    let filter = patch.add("filter", DiodeLadderFilter::new(sample_rate));

    // Envelope
    let adsr = patch.add("adsr", Adsr::new(sample_rate));

    // VCA
    let vca = patch.add("vca", Vca::new());

    // Mixer for oscillators
    let mixer = patch.add("mixer", Mixer::new(2));

    // Output
    let output = patch.add("output", StereoOutput::new());

    // Mix oscillators
    patch.connect(vco1.out("saw"), mixer.in_("ch0")).unwrap();
    patch.connect(vco2.out("sqr"), mixer.in_("ch1")).unwrap();

    // Through filter
    patch.connect(mixer.out("out"), filter.in_("in")).unwrap();

    // LFO modulation
    patch.connect(lfo1.out("sin"), filter.in_("fm")).unwrap();
    patch.connect(lfo2.out("tri"), vco2.in_("fm")).unwrap();

    // Through VCA
    patch.connect(filter.out("out"), vca.in_("in")).unwrap();
    patch.connect(adsr.out("env"), vca.in_("cv")).unwrap();

    // To output
    patch.connect(vca.out("out"), output.in_("left")).unwrap();
    patch.connect(vca.out("out"), output.in_("right")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();
    patch
}

// ============================================================================
// Individual Module Benchmarks
// ============================================================================

fn bench_vco(c: &mut Criterion) {
    let mut group = c.benchmark_group("modules/vco");

    for sample_rate in SAMPLE_RATES {
        let sr_name = format!("{}kHz", sample_rate as u32 / 1000);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("tick", &sr_name),
            &sample_rate,
            |b, &sr| {
                let mut vco = Vco::new(sr);
                let inputs = PortValues::new();
                let mut outputs = PortValues::new();

                b.iter(|| {
                    vco.tick(black_box(&inputs), &mut outputs);
                    outputs.get(10).unwrap_or(0.0)
                });
            },
        );
    }

    group.finish();
}

fn bench_svf(c: &mut Criterion) {
    let mut group = c.benchmark_group("modules/svf");

    for sample_rate in SAMPLE_RATES {
        let sr_name = format!("{}kHz", sample_rate as u32 / 1000);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("tick", &sr_name),
            &sample_rate,
            |b, &sr| {
                let mut svf = Svf::new(sr);
                let mut inputs = PortValues::new();
                inputs.set(0, 1.0); // Audio input
                inputs.set(1, 0.5); // Cutoff
                inputs.set(2, 0.3); // Resonance
                let mut outputs = PortValues::new();

                b.iter(|| {
                    svf.tick(black_box(&inputs), &mut outputs);
                    outputs.get(10).unwrap_or(0.0)
                });
            },
        );
    }

    group.finish();
}

fn bench_diode_ladder(c: &mut Criterion) {
    let mut group = c.benchmark_group("modules/diode_ladder");

    for sample_rate in SAMPLE_RATES {
        let sr_name = format!("{}kHz", sample_rate as u32 / 1000);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("tick", &sr_name),
            &sample_rate,
            |b, &sr| {
                let mut filter = DiodeLadderFilter::new(sr);
                let mut inputs = PortValues::new();
                inputs.set(0, 1.0); // Audio input
                inputs.set(1, 0.5); // Cutoff
                inputs.set(2, 0.7); // Resonance
                inputs.set(6, 0.3); // Drive
                let mut outputs = PortValues::new();

                b.iter(|| {
                    filter.tick(black_box(&inputs), &mut outputs);
                    outputs.get(10).unwrap_or(0.0)
                });
            },
        );
    }

    group.finish();
}

fn bench_adsr(c: &mut Criterion) {
    let mut group = c.benchmark_group("modules/adsr");

    for sample_rate in SAMPLE_RATES {
        let sr_name = format!("{}kHz", sample_rate as u32 / 1000);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("tick", &sr_name),
            &sample_rate,
            |b, &sr| {
                let mut adsr = Adsr::new(sr);
                let mut inputs = PortValues::new();
                inputs.set(0, 5.0); // Gate on
                inputs.set(2, 0.1); // Attack
                inputs.set(3, 0.2); // Decay
                inputs.set(4, 0.7); // Sustain
                inputs.set(5, 0.3); // Release
                let mut outputs = PortValues::new();

                b.iter(|| {
                    adsr.tick(black_box(&inputs), &mut outputs);
                    outputs.get(10).unwrap_or(0.0)
                });
            },
        );
    }

    group.finish();
}

fn bench_lfo(c: &mut Criterion) {
    let mut group = c.benchmark_group("modules/lfo");

    for sample_rate in SAMPLE_RATES {
        let sr_name = format!("{}kHz", sample_rate as u32 / 1000);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("tick", &sr_name),
            &sample_rate,
            |b, &sr| {
                let mut lfo = Lfo::new(sr);
                let inputs = PortValues::new();
                let mut outputs = PortValues::new();

                b.iter(|| {
                    lfo.tick(black_box(&inputs), &mut outputs);
                    outputs.get(10).unwrap_or(0.0)
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Sample Rate Benchmarks
// ============================================================================

fn bench_sample_rate_simple_patch(c: &mut Criterion) {
    let mut group = c.benchmark_group("sample_rate/simple_patch");

    for sample_rate in SAMPLE_RATES {
        let sr_name = format!("{}kHz", sample_rate as u32 / 1000);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("tick", &sr_name),
            &sample_rate,
            |b, &sr| {
                let mut patch = create_simple_patch(sr);
                b.iter(|| black_box(patch.tick()));
            },
        );
    }

    group.finish();
}

fn bench_sample_rate_modulated_patch(c: &mut Criterion) {
    let mut group = c.benchmark_group("sample_rate/modulated_patch");

    for sample_rate in SAMPLE_RATES {
        let sr_name = format!("{}kHz", sample_rate as u32 / 1000);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("tick", &sr_name),
            &sample_rate,
            |b, &sr| {
                let mut patch = create_modulated_patch(sr);
                b.iter(|| black_box(patch.tick()));
            },
        );
    }

    group.finish();
}

fn bench_sample_rate_complex_patch(c: &mut Criterion) {
    let mut group = c.benchmark_group("sample_rate/complex_patch");

    for sample_rate in SAMPLE_RATES {
        let sr_name = format!("{}kHz", sample_rate as u32 / 1000);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("tick", &sr_name),
            &sample_rate,
            |b, &sr| {
                let mut patch = create_complex_patch(sr);
                b.iter(|| black_box(patch.tick()));
            },
        );
    }

    group.finish();
}

// ============================================================================
// Buffer Processing Benchmarks (Real-Time Validation)
// ============================================================================

fn bench_buffer_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_processing");

    for sample_rate in SAMPLE_RATES {
        for buffer_size in BUFFER_SIZES {
            let sr_name = format!("{}kHz", sample_rate as u32 / 1000);
            let name = format!("{}/{}samples", sr_name, buffer_size);

            // Calculate time budget for this buffer
            let time_budget_us = (buffer_size as f64 / sample_rate) * 1_000_000.0;

            group.throughput(Throughput::Elements(buffer_size as u64));
            group.bench_with_input(
                BenchmarkId::new("simple_patch", &name),
                &(sample_rate, buffer_size),
                |b, &(sr, buf_size)| {
                    let mut patch = create_simple_patch(sr);
                    b.iter(|| {
                        for _ in 0..buf_size {
                            black_box(patch.tick());
                        }
                    });
                },
            );

            // Print budget info for reference (only visible in verbose mode)
            eprintln!(
                "  {} @ {} samples: budget = {:.2}µs",
                sr_name, buffer_size, time_budget_us
            );
        }
    }

    group.finish();
}

fn bench_buffer_processing_complex(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_processing_complex");

    for sample_rate in SAMPLE_RATES {
        for buffer_size in BUFFER_SIZES {
            let sr_name = format!("{}kHz", sample_rate as u32 / 1000);
            let name = format!("{}/{}samples", sr_name, buffer_size);

            group.throughput(Throughput::Elements(buffer_size as u64));
            group.bench_with_input(
                BenchmarkId::new("complex_patch", &name),
                &(sample_rate, buffer_size),
                |b, &(sr, buf_size)| {
                    let mut patch = create_complex_patch(sr);
                    b.iter(|| {
                        for _ in 0..buf_size {
                            black_box(patch.tick());
                        }
                    });
                },
            );
        }
    }

    group.finish();
}

// ============================================================================
// Polyphony Benchmarks
// ============================================================================

fn bench_polyphony_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("polyphony/voice_scaling");

    let sample_rate = 48000.0;

    for &num_voices in &VOICE_COUNTS {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("tick", num_voices),
            &num_voices,
            |b, &voices| {
                let mut poly = PolyPatch::new(voices, sample_rate);
                poly.compile().unwrap();

                // Activate all voices
                for i in 0..voices {
                    poly.note_on(60 + i as u8, 100);
                }

                b.iter(|| black_box(poly.tick()));
            },
        );
    }

    group.finish();
}

fn bench_polyphony_with_buffer(c: &mut Criterion) {
    let mut group = c.benchmark_group("polyphony/buffer_processing");

    let sample_rate = 48000.0;
    let buffer_size = 256;

    for &num_voices in &VOICE_COUNTS {
        group.throughput(Throughput::Elements(buffer_size as u64));
        group.bench_with_input(
            BenchmarkId::new("256_samples", num_voices),
            &num_voices,
            |b, &voices| {
                let mut poly = PolyPatch::new(voices, sample_rate);
                poly.compile().unwrap();

                // Activate all voices
                for i in 0..voices {
                    poly.note_on(60 + i as u8, 100);
                }

                b.iter(|| {
                    for _ in 0..buffer_size {
                        black_box(poly.tick());
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_voice_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("polyphony/voice_allocation");

    for &num_voices in &VOICE_COUNTS {
        group.bench_with_input(
            BenchmarkId::new("note_on_off", num_voices),
            &num_voices,
            |b, &voices| {
                let mut allocator = VoiceAllocator::new(voices);

                b.iter(|| {
                    // Allocate a voice
                    let idx = allocator.note_on(black_box(60), black_box(0.8));
                    black_box(idx);

                    // Release it
                    allocator.note_off(60);
                    allocator.tick();

                    // Reset for next iteration
                    allocator.panic();
                });
            },
        );
    }

    group.finish();
}

fn bench_voice_stealing(c: &mut Criterion) {
    let mut group = c.benchmark_group("polyphony/voice_stealing");

    // Test with 8 voices and various stealing modes
    let num_voices = 8;

    let modes = [
        ("round_robin", AllocationMode::RoundRobin),
        ("oldest_steal", AllocationMode::OldestSteal),
        ("quietest_steal", AllocationMode::QuietestSteal),
    ];

    for (mode_name, mode) in modes {
        group.bench_with_input(
            BenchmarkId::new("mode", mode_name),
            &mode,
            |b, &alloc_mode| {
                let mut allocator = VoiceAllocator::new(num_voices);
                allocator.set_mode(alloc_mode);

                // Fill all voices
                for i in 0..num_voices {
                    allocator.note_on(60 + i as u8, 0.8);
                }

                b.iter(|| {
                    // This should trigger voice stealing
                    let idx = allocator.note_on(black_box(80), black_box(0.8));
                    black_box(idx);
                    allocator.note_off(80);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Unison Benchmarks
// ============================================================================

fn bench_unison_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("polyphony/unison");

    let sample_rate = 48000.0;
    let unison_counts = [1, 2, 4, 8];

    for unison_voices in unison_counts {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("voices", unison_voices),
            &unison_voices,
            |b, &unison| {
                let mut poly = PolyPatch::new(4, sample_rate);
                poly.set_unison(UnisonConfig::new(unison, 10.0));
                poly.compile().unwrap();

                // Activate one voice with unison
                poly.note_on(60, 100);

                b.iter(|| black_box(poly.tick()));
            },
        );
    }

    group.finish();
}

// ============================================================================
// Patch Compilation Benchmarks
// ============================================================================

fn bench_patch_compilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("patch/compilation");

    let sample_rate = 48000.0;

    // Simple patch
    group.bench_function("simple", |b| {
        b.iter(|| {
            let mut patch = Patch::new(sample_rate);
            let vco = patch.add("vco", Vco::new(sample_rate));
            let output = patch.add("output", StereoOutput::new());
            patch.connect(vco.out("saw"), output.in_("left")).unwrap();
            patch.set_output(output.id());
            black_box(patch.compile().unwrap());
        });
    });

    // Modulated patch
    group.bench_function("modulated", |b| {
        b.iter(|| {
            let patch = create_modulated_patch(sample_rate);
            black_box(&patch);
        });
    });

    // Complex patch
    group.bench_function("complex", |b| {
        b.iter(|| {
            let patch = create_complex_patch(sample_rate);
            black_box(&patch);
        });
    });

    group.finish();
}

// ============================================================================
// SIMD Block Processing Benchmarks
// ============================================================================

fn bench_audio_block_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd/audio_block");

    let block_sizes = [64, 128, 256, 512];

    for size in block_sizes {
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(BenchmarkId::new("add_scalar", size), &size, |b, &sz| {
            let mut block = AudioBlock::new(sz);
            for i in 0..sz {
                block.set(i, i as f64 * 0.001);
            }

            b.iter(|| {
                block.add_scalar(black_box(0.5));
                block.get(0)
            });
        });

        group.bench_with_input(BenchmarkId::new("mul_scalar", size), &size, |b, &sz| {
            let mut block = AudioBlock::new(sz);
            for i in 0..sz {
                block.set(i, i as f64 * 0.001);
            }

            b.iter(|| {
                block.mul_scalar(black_box(0.5));
                block.get(0)
            });
        });

        group.bench_with_input(BenchmarkId::new("soft_clip", size), &size, |b, &sz| {
            let mut block = AudioBlock::new(sz);
            for i in 0..sz {
                block.set(i, (i as f64 - sz as f64 / 2.0) * 0.02);
            }

            b.iter(|| {
                block.soft_clip(black_box(1.5));
                block.get(0)
            });
        });

        group.bench_with_input(BenchmarkId::new("peak", size), &size, |b, &sz| {
            let mut block = AudioBlock::new(sz);
            for i in 0..sz {
                block.set(i, (i as f64 * 0.1).sin());
            }

            b.iter(|| black_box(block.peak()));
        });

        group.bench_with_input(BenchmarkId::new("rms", size), &size, |b, &sz| {
            let mut block = AudioBlock::new(sz);
            for i in 0..sz {
                block.set(i, (i as f64 * 0.1).sin());
            }

            b.iter(|| black_box(block.rms()));
        });
    }

    group.finish();
}

// ============================================================================
// Real-Time Compliance Benchmarks
// ============================================================================

/// This benchmark specifically measures whether we can meet real-time deadlines
fn bench_realtime_compliance(c: &mut Criterion) {
    let mut group = c.benchmark_group("realtime_compliance");

    // Common pro-audio configurations
    let configs = [
        ("44.1kHz/256", 44100.0, 256), // ~5.8ms budget
        ("48kHz/256", 48000.0, 256),   // ~5.3ms budget
        ("48kHz/128", 48000.0, 128),   // ~2.7ms budget - tighter
        ("96kHz/256", 96000.0, 256),   // ~2.7ms budget
        ("96kHz/128", 96000.0, 128),   // ~1.3ms budget - very tight
        ("192kHz/256", 192000.0, 256), // ~1.3ms budget
    ];

    for (name, sample_rate, buffer_size) in configs {
        let time_budget_ns = (buffer_size as f64 / sample_rate) * 1_000_000_000.0;

        group.throughput(Throughput::Elements(buffer_size as u64));
        group.bench_with_input(
            BenchmarkId::new("complex_patch", name),
            &(sample_rate, buffer_size),
            |b, &(sr, buf_size)| {
                let mut patch = create_complex_patch(sr);

                b.iter(|| {
                    for _ in 0..buf_size {
                        black_box(patch.tick());
                    }
                });
            },
        );

        eprintln!(
            "  {}: budget = {:.0}ns ({:.2}ms)",
            name,
            time_budget_ns,
            time_budget_ns / 1_000_000.0
        );
    }

    group.finish();
}

/// Benchmark polyphonic processing under real-time constraints
fn bench_polyphonic_realtime(c: &mut Criterion) {
    let mut group = c.benchmark_group("realtime_polyphonic");

    let sample_rate = 48000.0;
    let buffer_size = 256;
    let time_budget_ns = (buffer_size as f64 / sample_rate) * 1_000_000_000.0;

    eprintln!(
        "\n48kHz/256 buffer time budget: {:.0}ns ({:.2}ms)",
        time_budget_ns,
        time_budget_ns / 1_000_000.0
    );

    for &num_voices in &VOICE_COUNTS {
        group.throughput(Throughput::Elements(buffer_size as u64));
        group.bench_with_input(
            BenchmarkId::new("voices", num_voices),
            &num_voices,
            |b, &voices| {
                let mut poly = PolyPatch::new(voices, sample_rate);
                poly.compile().unwrap();

                // Activate all voices with different notes
                for i in 0..voices {
                    poly.note_on(48 + (i as u8 % 24), 100);
                }

                b.iter(|| {
                    for _ in 0..buffer_size {
                        black_box(poly.tick());
                    }
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Throughput Benchmarks
// ============================================================================

/// Measure raw sample throughput (samples per second)
fn bench_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");

    let sample_rate = 48000.0;
    let one_second_samples = sample_rate as usize;

    // Simple patch throughput
    group.throughput(Throughput::Elements(one_second_samples as u64));
    group.bench_function("simple_1sec", |b| {
        let mut patch = create_simple_patch(sample_rate);
        b.iter(|| {
            for _ in 0..one_second_samples {
                black_box(patch.tick());
            }
        });
    });

    // Complex patch throughput
    group.throughput(Throughput::Elements(one_second_samples as u64));
    group.bench_function("complex_1sec", |b| {
        let mut patch = create_complex_patch(sample_rate);
        b.iter(|| {
            for _ in 0..one_second_samples {
                black_box(patch.tick());
            }
        });
    });

    // Polyphonic throughput (8 voices)
    group.throughput(Throughput::Elements(one_second_samples as u64));
    group.bench_function("poly8_1sec", |b| {
        let mut poly = PolyPatch::new(8, sample_rate);
        poly.compile().unwrap();
        for i in 0..8 {
            poly.note_on(60 + i as u8, 100);
        }

        b.iter(|| {
            for _ in 0..one_second_samples {
                black_box(poly.tick());
            }
        });
    });

    group.finish();
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    module_benches,
    bench_vco,
    bench_svf,
    bench_diode_ladder,
    bench_adsr,
    bench_lfo,
);

criterion_group!(
    sample_rate_benches,
    bench_sample_rate_simple_patch,
    bench_sample_rate_modulated_patch,
    bench_sample_rate_complex_patch,
);

criterion_group!(
    buffer_benches,
    bench_buffer_processing,
    bench_buffer_processing_complex,
);

criterion_group!(
    polyphony_benches,
    bench_polyphony_scaling,
    bench_polyphony_with_buffer,
    bench_voice_allocation,
    bench_voice_stealing,
    bench_unison_processing,
);

criterion_group!(simd_benches, bench_audio_block_operations,);

criterion_group!(
    realtime_benches,
    bench_realtime_compliance,
    bench_polyphonic_realtime,
);

criterion_group!(patch_benches, bench_patch_compilation, bench_throughput,);

criterion_main!(
    module_benches,
    sample_rate_benches,
    buffer_benches,
    polyphony_benches,
    simd_benches,
    realtime_benches,
    patch_benches,
);
