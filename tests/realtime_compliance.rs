//! Real-time compliance test (Q114).
//!
//! Criterion benchmarks *print* timings but never fail the build when a patch
//! blows its deadline. This test closes that gap: it measures wall-clock time
//! for processing one second of audio and **asserts** the work stays inside the
//! real-time budget. CI runs it via
//! `cargo test --release --test realtime_compliance`, so the deadline is
//! genuinely gated.
//!
//! ## Debug vs. release
//!
//! The strict wall-clock assertion only fires in optimized builds
//! (`!cfg!(debug_assertions)`). Debug builds run the DSP unoptimized — routinely
//! 10x+ slower than real time — so a hard deadline there would be meaningless
//! and flaky. In debug builds this test still runs the *entire* workload and
//! asserts the patches produce non-silent output, i.e. it degrades to a
//! functional smoke test. Run it in release to actually measure headroom.
//!
//! The budget fraction (80%) is deliberately generous: shared CI runners are
//! noisy and we care about "comfortably real-time", not micro-margins.

use quiver::modules::{Chorus, DelayLine, Supersaw};
use quiver::prelude::*;
use std::time::Instant;

/// Sample rate the deadlines are measured against.
const SAMPLE_RATE: f64 = 48_000.0;

/// Maximum fraction of the real-time budget the worst case may consume.
const BUDGET_FRACTION: f64 = 0.8;

/// Seconds of audio processed per measurement.
const SECONDS: f64 = 1.0;

// ---------------------------------------------------------------------------
// Patch builders (mirror the benchmark worst cases)
// ---------------------------------------------------------------------------

/// Worst-case single-voice chain: `Supersaw → DiodeLadderFilter → Chorus →
/// DelayLine → Reverb → StereoOutput` (matches `heavy_fx/chain` in the benches).
fn heavy_fx_patch(sample_rate: f64) -> Patch {
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

/// One realistic voice: `ctrl → Vco → Svf → Vca`, with an `Adsr` (driven by the
/// controller gate) modulating the VCA. Identical to the benches' voice.
fn build_synth_voice(patch: &mut Patch, ctrl: &NodeHandle) -> Result<(), PatchError> {
    let sr = patch.sample_rate();
    let vco = patch.add("vco", Vco::new(sr));
    let svf = patch.add("svf", Svf::new(sr));
    let vca = patch.add("vca", Vca::new());
    let adsr = patch.add("adsr", Adsr::new(sr));
    let out = patch.add("out", StereoOutput::new());

    patch.connect(ctrl.out("voct"), vco.in_("voct"))?;
    patch.connect(ctrl.out("gate"), adsr.in_("gate"))?;
    patch.connect(vco.out("saw"), svf.in_("in"))?;
    patch.connect(svf.out("lp"), vca.in_("in"))?;
    patch.connect(adsr.out("env"), vca.in_("cv"))?;
    patch.connect(vca.out("out"), out.in_("left"))?;
    patch.connect(vca.out("out"), out.in_("right"))?;

    patch.set_output(out.id());
    Ok(())
}

fn poly_synth(num_voices: usize, sample_rate: f64) -> PolyPatch {
    PolyPatch::with_voice_fn(num_voices, sample_rate, build_synth_voice)
        .expect("voice graph must build")
}

// ---------------------------------------------------------------------------
// Measurement helpers
// ---------------------------------------------------------------------------

/// Result of one measurement run.
struct Measurement {
    elapsed_secs: f64,
    energy: f64,
}

impl Measurement {
    /// Fraction of real-time consumed (elapsed / audio-duration).
    fn ratio(&self) -> f64 {
        self.elapsed_secs / SECONDS
    }
}

/// Process `SECONDS` of the heavy-FX chain via `tick_block`, timing only the
/// steady-state loop (after a warm-up block).
fn measure_heavy() -> Measurement {
    let mut patch = heavy_fx_patch(SAMPLE_RATE);
    let total = (SAMPLE_RATE * SECONDS) as usize;
    let block = 64usize;
    let mut left = vec![0.0f64; block];
    let mut right = vec![0.0f64; block];

    // Warm up (fills delay/reverb buffers, settles denormals) — untimed.
    patch.tick_block(&mut left, &mut right);

    let mut energy = 0.0;
    let start = Instant::now();
    let mut done = 0;
    while done < total {
        let n = block.min(total - done);
        patch.tick_block(&mut left[..n], &mut right[..n]);
        for (l, r) in left[..n].iter().zip(&right[..n]) {
            energy += l.abs() + r.abs();
        }
        done += n;
    }
    let elapsed_secs = start.elapsed().as_secs_f64();

    Measurement {
        elapsed_secs,
        energy,
    }
}

/// Process `SECONDS` of a populated `num_voices`-voice polyphonic synth,
/// per-sample, timing only the steady-state loop (after a warm-up).
fn measure_poly(num_voices: usize) -> Measurement {
    let mut poly = poly_synth(num_voices, SAMPLE_RATE);
    for i in 0..num_voices {
        poly.note_on(48 + (i as u8 % 24), 100);
    }

    // Warm up so envelopes are past their attack transient — untimed.
    for _ in 0..64 {
        poly.tick();
    }

    let total = (SAMPLE_RATE * SECONDS) as usize;
    let mut energy = 0.0;
    let start = Instant::now();
    for _ in 0..total {
        let (l, r) = poly.tick();
        energy += l.abs() + r.abs();
    }
    let elapsed_secs = start.elapsed().as_secs_f64();

    Measurement {
        elapsed_secs,
        energy,
    }
}

/// Shared assertion: always require non-silent output; only enforce the
/// wall-clock deadline in optimized builds.
fn check(label: &str, m: &Measurement) {
    assert!(
        m.energy > 1.0,
        "{label}: produced (near-)silence over {SECONDS}s (energy = {}); the graph is empty or mis-wired",
        m.energy
    );

    let ratio = m.ratio();
    println!(
        "[realtime] {label}: {SECONDS:.1}s of audio @ {SAMPLE_RATE:.0} Hz in {:.4}s \
         => {:.1}% of budget ({:.1}% headroom)",
        m.elapsed_secs,
        ratio * 100.0,
        (1.0 - ratio) * 100.0
    );

    if cfg!(debug_assertions) {
        println!(
            "[realtime] {label}: debug build — timing assertion skipped (run --release to gate it)"
        );
        return;
    }

    assert!(
        ratio < BUDGET_FRACTION,
        "{label}: used {:.1}% of the real-time budget (limit {:.0}%) — not real-time safe",
        ratio * 100.0,
        BUDGET_FRACTION * 100.0
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The worst-case heavy-FX chain must process 1 second of 48 kHz audio in well
/// under 1 second of wall-clock time (release builds).
#[test]
fn heavy_fx_chain_meets_realtime_deadline() {
    let m = measure_heavy();
    check("heavy-fx chain", &m);
}

/// Eight populated voices (VCO→VCF→VCA + ADSR each) must also stay comfortably
/// real-time.
#[test]
fn polyphonic_8_voices_meets_realtime_deadline() {
    let m = measure_poly(8);
    check("poly-8 voices", &m);
}
