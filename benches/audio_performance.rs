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
// Modules not re-exported through the prelude (used by the heavy-FX and
// expensive-module benches).
use quiver::modules::{Chorus, DelayLine, KarplusStrong, Supersaw};

// ============================================================================
// Sample Rate Constants
// ============================================================================

const SAMPLE_RATES: [f64; 4] = [44100.0, 48000.0, 96000.0, 192000.0];
const BUFFER_SIZES: [usize; 4] = [64, 128, 256, 512];
const VOICE_COUNTS: [usize; 5] = [1, 4, 8, 16, 32];

// Extended constants for stress testing
const ULTRA_LOW_LATENCY_BUFFERS: [usize; 3] = [16, 32, 48];
const HIGH_POLYPHONY_COUNTS: [usize; 3] = [48, 64, 128];

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

// ----------------------------------------------------------------------------
// Populated polyphony voice (Q112)
// ----------------------------------------------------------------------------

/// Build one realistic monophonic voice graph for [`PolyPatch::with_voice_fn`]:
/// `ctrl → Vco → Svf → Vca`, with an `Adsr` (driven by the controller gate)
/// modulating the VCA. This is the DSP every polyphony benchmark must actually
/// measure — the old benches ran controller-only (empty) voices and therefore
/// measured *zero* synthesis work.
fn build_synth_voice(patch: &mut Patch, ctrl: &NodeHandle) -> Result<(), PatchError> {
    let sr = patch.sample_rate();
    let vco = patch.add("vco", Vco::new(sr));
    let svf = patch.add("svf", Svf::new(sr));
    let vca = patch.add("vca", Vca::new());
    let adsr = patch.add("adsr", Adsr::new(sr));
    let out = patch.add("out", StereoOutput::new());

    // Controller → pitch and envelope gate.
    patch.connect(ctrl.out("voct"), vco.in_("voct"))?;
    patch.connect(ctrl.out("gate"), adsr.in_("gate"))?;
    // Signal path: VCO saw → filter → VCA.
    patch.connect(vco.out("saw"), svf.in_("in"))?;
    patch.connect(svf.out("lp"), vca.in_("in"))?;
    // Envelope → VCA gain.
    patch.connect(adsr.out("env"), vca.in_("cv"))?;
    // VCA → stereo output (both channels).
    patch.connect(vca.out("out"), out.in_("left"))?;
    patch.connect(vca.out("out"), out.in_("right"))?;

    patch.set_output(out.id());
    Ok(())
}

/// Create a polyphonic synth whose voices actually make sound (see
/// [`build_synth_voice`]).
fn create_poly_synth(num_voices: usize, sample_rate: f64) -> PolyPatch {
    PolyPatch::with_voice_fn(num_voices, sample_rate, build_synth_voice)
        .expect("voice graph must build")
}

/// Warm up a freshly built polyphonic synth and assert it produces NON-SILENT
/// output. This makes an accidentally-empty or mis-wired voice graph fail the
/// benchmark loudly instead of silently measuring nothing (Q112). Leaves the
/// synth reset so the caller can set up its own note pattern.
fn assert_poly_non_silent(poly: &mut PolyPatch) {
    poly.note_on(60, 100);
    let mut energy = 0.0;
    for _ in 0..100 {
        let (l, r) = poly.tick();
        energy += l.abs() + r.abs();
    }
    assert!(
        energy > 1e-3,
        "polyphonic voice produced silence over 100 ticks (energy = {energy}); \
         the voice graph is empty or mis-wired — benches would measure nothing"
    );
    poly.reset();
}

/// Worst-case heavy-FX chain (Q119): `Supersaw → DiodeLadderFilter → Chorus →
/// DelayLine → Reverb → StereoOutput`. This is the most expensive single-voice
/// signal path in the library and doubles as the worst-case patch for the
/// real-time compliance test in `tests/realtime_compliance.rs`.
fn create_heavy_fx_patch(sample_rate: f64) -> Patch {
    let mut patch = Patch::new(sample_rate);

    let saw = patch.add("saw", Supersaw::new(sample_rate));
    let filter = patch.add("filter", DiodeLadderFilter::new(sample_rate));
    let chorus = patch.add("chorus", Chorus::new(sample_rate));
    let delay = patch.add("delay", DelayLine::new(sample_rate));
    let reverb = patch.add("reverb", Reverb::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    patch.connect(saw.out("out"), filter.in_("in")).unwrap();
    patch.connect(filter.out("out"), chorus.in_("in")).unwrap();
    patch.connect(chorus.out("out"), delay.in_("in")).unwrap();
    patch.connect(delay.out("out"), reverb.in_("in")).unwrap();
    patch
        .connect(reverb.out("left"), output.in_("left"))
        .unwrap();
    patch
        .connect(reverb.out("right"), output.in_("right"))
        .unwrap();

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
                let mut poly = create_poly_synth(voices, sample_rate);
                assert_poly_non_silent(&mut poly);

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
                let mut poly = create_poly_synth(voices, sample_rate);
                assert_poly_non_silent(&mut poly);

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
                let mut poly = create_poly_synth(4, sample_rate);
                poly.set_unison(UnisonConfig::new(unison, 10.0));
                assert_poly_non_silent(&mut poly);

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
            patch.compile().unwrap();
            black_box(());
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
// SIMD Block Processing Benchmarks (scalar vs. SIMD A/B — Q116)
// ============================================================================

/// Block-operation benchmarks. The four element-wise ops (`add_scalar`,
/// `mul_scalar`, `add_block`, `mul_block`) are feature-gated in `src/simd.rs`:
/// built with `--features simd` they run the `wide::f64x4` path, otherwise the
/// scalar path. The bench *names are identical* either way, so a criterion
/// baseline saved from the scalar build compares directly against the SIMD
/// build:
///
/// ```text
/// cargo bench -- simd --save-baseline scalar
/// cargo bench --features simd -- simd --baseline scalar
/// ```
///
/// (`make bench-simd` runs exactly that flow.)
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

        group.bench_with_input(BenchmarkId::new("add_block", size), &size, |b, &sz| {
            let mut block = AudioBlock::new(sz);
            let other = AudioBlock::constant(sz, 0.25);
            for i in 0..sz {
                block.set(i, i as f64 * 0.001);
            }

            b.iter(|| {
                block.add_block(black_box(&other));
                block.get(0)
            });
        });

        group.bench_with_input(BenchmarkId::new("mul_block", size), &size, |b, &sz| {
            let mut block = AudioBlock::new(sz);
            let other = AudioBlock::constant(sz, 0.999);
            for i in 0..sz {
                block.set(i, i as f64 * 0.001);
            }

            b.iter(|| {
                block.mul_block(black_box(&other));
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
                let mut poly = create_poly_synth(voices, sample_rate);
                assert_poly_non_silent(&mut poly);

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
        let mut poly = create_poly_synth(8, sample_rate);
        assert_poly_non_silent(&mut poly);
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
// Stress Test Benchmarks
// ============================================================================

/// Ultra-low latency benchmarks (16-48 sample buffers)
fn bench_ultra_low_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("stress/ultra_low_latency");

    let sample_rate = 48000.0;

    for buffer_size in ULTRA_LOW_LATENCY_BUFFERS {
        let time_budget_us = (buffer_size as f64 / sample_rate) * 1_000_000.0;

        group.throughput(Throughput::Elements(buffer_size as u64));
        group.bench_with_input(
            BenchmarkId::new("simple_patch", buffer_size),
            &buffer_size,
            |b, &buf_size| {
                let mut patch = create_simple_patch(sample_rate);
                b.iter(|| {
                    for _ in 0..buf_size {
                        black_box(patch.tick());
                    }
                });
            },
        );

        eprintln!(
            "  {} samples @ 48kHz: budget = {:.1}µs ({:.3}ms)",
            buffer_size,
            time_budget_us,
            time_budget_us / 1000.0
        );
    }

    group.finish();
}

/// High polyphony stress test (48-128 voices)
fn bench_high_polyphony(c: &mut Criterion) {
    let mut group = c.benchmark_group("stress/high_polyphony");

    let sample_rate = 48000.0;

    for &num_voices in &HIGH_POLYPHONY_COUNTS {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("tick", num_voices),
            &num_voices,
            |b, &voices| {
                let mut poly = create_poly_synth(voices, sample_rate);
                assert_poly_non_silent(&mut poly);

                // Activate all voices with different notes
                for i in 0..voices {
                    poly.note_on(36 + (i as u8 % 48), 100);
                }

                b.iter(|| black_box(poly.tick()));
            },
        );
    }

    group.finish();
}

/// High polyphony with buffer processing
fn bench_high_polyphony_buffer(c: &mut Criterion) {
    let mut group = c.benchmark_group("stress/high_polyphony_buffer");

    let sample_rate = 48000.0;
    let buffer_size = 128;

    for &num_voices in &HIGH_POLYPHONY_COUNTS {
        group.throughput(Throughput::Elements(buffer_size as u64));
        group.bench_with_input(
            BenchmarkId::new("128_samples", num_voices),
            &num_voices,
            |b, &voices| {
                let mut poly = create_poly_synth(voices, sample_rate);
                assert_poly_non_silent(&mut poly);

                for i in 0..voices {
                    poly.note_on(36 + (i as u8 % 48), 100);
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
// Additional Module Benchmarks
// ============================================================================

fn bench_noise_generator(c: &mut Criterion) {
    let mut group = c.benchmark_group("modules/noise");

    group.throughput(Throughput::Elements(1));
    group.bench_function("tick", |b| {
        let mut noise = NoiseGenerator::new();
        let inputs = PortValues::new();
        let mut outputs = PortValues::new();

        b.iter(|| {
            noise.tick(black_box(&inputs), &mut outputs);
            outputs.get(10).unwrap_or(0.0)
        });
    });

    group.finish();
}

fn bench_quantizer(c: &mut Criterion) {
    let mut group = c.benchmark_group("modules/quantizer");

    group.throughput(Throughput::Elements(1));
    group.bench_function("chromatic", |b| {
        let mut quantizer = Quantizer::chromatic();
        let mut inputs = PortValues::new();
        inputs.set(0, 1.234); // V/Oct input
        let mut outputs = PortValues::new();

        b.iter(|| {
            quantizer.tick(black_box(&inputs), &mut outputs);
            outputs.get(10).unwrap_or(0.0)
        });
    });

    group.finish();
}

fn bench_slew_limiter(c: &mut Criterion) {
    let mut group = c.benchmark_group("modules/slew_limiter");

    for sample_rate in SAMPLE_RATES {
        let sr_name = format!("{}kHz", sample_rate as u32 / 1000);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("tick", &sr_name),
            &sample_rate,
            |b, &sr| {
                let mut slew = SlewLimiter::new(sr);
                let mut inputs = PortValues::new();
                inputs.set(0, 5.0); // Input signal
                inputs.set(1, 0.5); // Rise rate
                inputs.set(2, 0.5); // Fall rate
                let mut outputs = PortValues::new();

                b.iter(|| {
                    slew.tick(black_box(&inputs), &mut outputs);
                    outputs.get(10).unwrap_or(0.0)
                });
            },
        );
    }

    group.finish();
}

fn bench_clock(c: &mut Criterion) {
    let mut group = c.benchmark_group("modules/clock");

    for sample_rate in SAMPLE_RATES {
        let sr_name = format!("{}kHz", sample_rate as u32 / 1000);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("tick", &sr_name),
            &sample_rate,
            |b, &sr| {
                let mut clock = Clock::new(sr);
                let mut inputs = PortValues::new();
                inputs.set(0, 120.0); // BPM
                let mut outputs = PortValues::new();

                b.iter(|| {
                    clock.tick(black_box(&inputs), &mut outputs);
                    outputs.get(10).unwrap_or(0.0)
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Filter Comparison Benchmarks
// ============================================================================

/// Compare SVF vs DiodeLadder filter performance
fn bench_filter_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison/filters");

    let sample_rate = 48000.0;
    let buffer_size = 256;

    // SVF filter
    group.throughput(Throughput::Elements(buffer_size as u64));
    group.bench_function("svf_256", |b| {
        let mut svf = Svf::new(sample_rate);
        let mut inputs = PortValues::new();
        inputs.set(0, 1.0);
        inputs.set(1, 0.5);
        inputs.set(2, 0.7);
        let mut outputs = PortValues::new();

        b.iter(|| {
            for _ in 0..buffer_size {
                svf.tick(black_box(&inputs), &mut outputs);
            }
            outputs.get(10).unwrap_or(0.0)
        });
    });

    // Diode Ladder filter
    group.throughput(Throughput::Elements(buffer_size as u64));
    group.bench_function("diode_ladder_256", |b| {
        let mut filter = DiodeLadderFilter::new(sample_rate);
        let mut inputs = PortValues::new();
        inputs.set(0, 1.0);
        inputs.set(1, 0.5);
        inputs.set(2, 0.7);
        inputs.set(6, 0.3);
        let mut outputs = PortValues::new();

        b.iter(|| {
            for _ in 0..buffer_size {
                filter.tick(black_box(&inputs), &mut outputs);
            }
            outputs.get(10).unwrap_or(0.0)
        });
    });

    group.finish();
}

// ============================================================================
// Patch Lifecycle Benchmarks
// ============================================================================

/// Benchmark patch creation and teardown
fn bench_patch_lifecycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("lifecycle");

    let sample_rate = 48000.0;

    // Simple patch creation
    group.bench_function("create_simple", |b| {
        b.iter(|| {
            let patch = create_simple_patch(sample_rate);
            black_box(patch)
        });
    });

    // Complex patch creation
    group.bench_function("create_complex", |b| {
        b.iter(|| {
            let patch = create_complex_patch(sample_rate);
            black_box(patch)
        });
    });

    // PolyPatch creation (8 populated voices — real build cost)
    group.bench_function("create_poly8", |b| {
        b.iter(|| {
            let poly = create_poly_synth(8, sample_rate);
            black_box(poly)
        });
    });

    // PolyPatch creation (32 populated voices — real build cost)
    group.bench_function("create_poly32", |b| {
        b.iter(|| {
            let poly = create_poly_synth(32, sample_rate);
            black_box(poly)
        });
    });

    group.finish();
}

// ============================================================================
// Maximum Throughput Benchmark
// ============================================================================

/// Find the maximum sustainable polyphony at 48kHz/256 samples
fn bench_max_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("stress/max_throughput");

    let sample_rate = 48000.0;
    let buffer_size = 256;
    let time_budget_ns = (buffer_size as f64 / sample_rate) * 1_000_000_000.0;

    eprintln!(
        "\nMax throughput test - budget: {:.0}ns ({:.2}ms)",
        time_budget_ns,
        time_budget_ns / 1_000_000.0
    );

    // Test increasingly complex scenarios
    let scenarios: &[(&str, usize, usize)] = &[
        ("8v_simple", 8, 1), // 8 voices, simple patch
        ("16v_simple", 16, 1),
        ("32v_simple", 32, 1),
        ("8v_unison4", 8, 4),   // 8 voices with 4x unison each
        ("16v_unison2", 16, 2), // 16 voices with 2x unison each
    ];

    for (name, voices, unison) in scenarios {
        group.throughput(Throughput::Elements(buffer_size as u64));
        group.bench_with_input(
            BenchmarkId::new("scenario", *name),
            &(*voices, *unison),
            |b, &(v, u)| {
                let mut poly = create_poly_synth(v, sample_rate);
                if u > 1 {
                    poly.set_unison(UnisonConfig::new(u, 15.0));
                }
                assert_poly_non_silent(&mut poly);

                for i in 0..v {
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
// Heavy-FX Worst-Case Benchmarks (Q119)
// ============================================================================

/// Worst-case signal path: `Supersaw → DiodeLadderFilter → Chorus → DelayLine →
/// Reverb` at 96 kHz, block-processed with `tick_block` into 32- and 64-sample
/// buffers (the tightest realistic real-time budgets). This is the single most
/// expensive chain the library can build and is the reference worst case for
/// the real-time compliance test.
fn bench_heavy_fx_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("heavy_fx/chain");

    let sample_rate = 96000.0;

    for buffer_size in [32usize, 64] {
        let time_budget_us = (buffer_size as f64 / sample_rate) * 1_000_000.0;

        group.throughput(Throughput::Elements(buffer_size as u64));
        group.bench_with_input(
            BenchmarkId::new("96kHz", buffer_size),
            &buffer_size,
            |b, &bs| {
                let mut patch = create_heavy_fx_patch(sample_rate);
                let mut left = vec![0.0f64; bs];
                let mut right = vec![0.0f64; bs];
                b.iter(|| {
                    patch.tick_block(black_box(&mut left), black_box(&mut right));
                });
            },
        );

        eprintln!(
            "  heavy_fx @ 96kHz / {} samples: budget = {:.2}µs",
            buffer_size, time_budget_us
        );
    }

    group.finish();
}

// ============================================================================
// Expensive Module Benchmarks (Q119)
// ============================================================================

/// Per-`tick()` cost of the individually expensive modules, at 48 kHz and
/// 96 kHz. These are the modules most likely to blow a real-time budget on
/// their own; benching them in isolation makes their per-sample cost explicit.
fn bench_expensive_modules(c: &mut Criterion) {
    let mut group = c.benchmark_group("modules/expensive");

    // Only measure sample rates where the per-sample budget matters most.
    let rates = [48000.0f64, 96000.0];

    for sr in rates {
        let sr_name = format!("{}kHz", sr as u32 / 1000);

        group.throughput(Throughput::Elements(1));

        // --- Oscillators / sources ---------------------------------------
        group.bench_with_input(BenchmarkId::new("supersaw", &sr_name), &sr, |b, &sr| {
            let mut m = Supersaw::new(sr);
            let mut inputs = PortValues::new();
            inputs.set(0, 0.0); // voct
            let mut outputs = PortValues::new();
            b.iter(|| {
                m.tick(black_box(&inputs), &mut outputs);
                outputs.get(10).unwrap_or(0.0)
            });
        });

        group.bench_with_input(BenchmarkId::new("wavetable", &sr_name), &sr, |b, &sr| {
            let mut m = Wavetable::new(sr);
            let mut inputs = PortValues::new();
            inputs.set(0, 0.0); // v_oct
            let mut outputs = PortValues::new();
            b.iter(|| {
                m.tick(black_box(&inputs), &mut outputs);
                outputs.get(10).unwrap_or(0.0)
            });
        });

        group.bench_with_input(
            BenchmarkId::new("karplus_strong", &sr_name),
            &sr,
            |b, &sr| {
                let mut m = KarplusStrong::new(sr);
                let mut inputs = PortValues::new();
                inputs.set(0, 0.0); // voct
                inputs.set(1, 5.0); // trigger (excite the string)
                let mut outputs = PortValues::new();
                b.iter(|| {
                    m.tick(black_box(&inputs), &mut outputs);
                    outputs.get(10).unwrap_or(0.0)
                });
            },
        );

        // --- Effects / processors ----------------------------------------
        group.bench_with_input(BenchmarkId::new("reverb", &sr_name), &sr, |b, &sr| {
            let mut m = Reverb::new(sr);
            let mut inputs = PortValues::new();
            inputs.set(0, 0.5); // audio in
            let mut outputs = PortValues::new();
            b.iter(|| {
                m.tick(black_box(&inputs), &mut outputs);
                outputs.get(10).unwrap_or(0.0)
            });
        });

        group.bench_with_input(BenchmarkId::new("granular", &sr_name), &sr, |b, &sr| {
            let mut m = Granular::new(sr);
            let mut inputs = PortValues::new();
            inputs.set(0, 0.5); // audio in
            let mut outputs = PortValues::new();
            b.iter(|| {
                m.tick(black_box(&inputs), &mut outputs);
                outputs.get(10).unwrap_or(0.0)
            });
        });

        group.bench_with_input(
            BenchmarkId::new("pitch_shifter", &sr_name),
            &sr,
            |b, &sr| {
                let mut m = PitchShifter::new(sr);
                let mut inputs = PortValues::new();
                inputs.set(0, 0.5); // audio in
                inputs.set(1, 0.5); // shift up
                let mut outputs = PortValues::new();
                b.iter(|| {
                    m.tick(black_box(&inputs), &mut outputs);
                    outputs.get(10).unwrap_or(0.0)
                });
            },
        );

        group.bench_with_input(BenchmarkId::new("vocoder", &sr_name), &sr, |b, &sr| {
            let mut m = Vocoder::new(sr);
            let mut inputs = PortValues::new();
            inputs.set(0, 0.5); // carrier
            inputs.set(1, 0.3); // modulator
            let mut outputs = PortValues::new();
            b.iter(|| {
                m.tick(black_box(&inputs), &mut outputs);
                outputs.get(10).unwrap_or(0.0)
            });
        });

        eprintln!(
            "  expensive modules @ {}: per-tick budget @ 96kHz single buffer already tight",
            sr_name
        );
    }

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
    extended_module_benches,
    bench_noise_generator,
    bench_quantizer,
    bench_slew_limiter,
    bench_clock,
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

criterion_group!(
    stress_benches,
    bench_ultra_low_latency,
    bench_high_polyphony,
    bench_high_polyphony_buffer,
    bench_max_throughput,
);

criterion_group!(comparison_benches, bench_filter_comparison,);

criterion_group!(lifecycle_benches, bench_patch_lifecycle,);

criterion_group!(heavy_benches, bench_heavy_fx_chain, bench_expensive_modules,);

criterion_main!(
    module_benches,
    extended_module_benches,
    sample_rate_benches,
    buffer_benches,
    polyphony_benches,
    simd_benches,
    realtime_benches,
    patch_benches,
    stress_benches,
    comparison_benches,
    lifecycle_benches,
    heavy_benches,
);
