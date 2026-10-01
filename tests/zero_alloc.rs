//! Proof that the graph engine's tick paths are allocation-free.
//!
//! This integration test installs a counting global allocator (a wrapper around the system
//! allocator that increments an atomic on every `alloc`/`realloc` while armed). It builds a
//! representative patch (VCO -> SVF -> VCA -> StereoOutput, plus an LFO modulation cable, a
//! normalled input, and a host `AudioInput` summed into the filter), warms it up, then asserts
//! that a burst of `tick()` calls and a `tick_block()` call — each preceded by the host writing
//! its input block — perform **zero** heap allocations.
//!
//! It lives in its own integration-test binary (not a unit test) so the `#[global_allocator]`
//! only governs this process and does not perturb the main unit-test binary. It is std-only
//! because the counting allocator wraps `std::alloc::System`.
#![cfg(feature = "std")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use quiver::prelude::*;

/// Global allocator that counts allocations while `COUNTING` is armed.
struct CountingAllocator;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static COUNTING: AtomicBool = AtomicBool::new(false);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Deallocations are irrelevant to the "does the audio path allocate?" question.
        System.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // A realloc (e.g. a Vec/HashMap growing) is an allocation for our purposes.
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// Run `f` with allocation counting armed and return the number of allocations it caused.
/// The arm/disarm brackets `f` tightly so the surrounding assert (which allocates its panic
/// message only on failure) is never counted.
fn count_allocs<F: FnOnce()>(f: F) -> usize {
    ALLOCS.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
    f();
    COUNTING.store(false, Ordering::Relaxed);
    ALLOCS.load(Ordering::Relaxed)
}

/// A representative patch exercising the routing engine end to end:
/// VCO -> SVF -> VCA -> StereoOutput, an LFO modulation cable into the filter cutoff, a
/// normalled input (StereoOutput's `right` normals to `left`), and host audio from an
/// `AudioInput` summed into the filter input, tracked by a `Track` that plays the VCO and
/// recorded by a `Capture` whose playback joins the filter input too.
fn build_patch() -> Rig {
    let sr = 44_100.0;
    let mut patch = Patch::new(sr);
    let input = Arc::new(AudioInputStream::new(2, 512));
    let record = Arc::new(AtomicF64::new(0.0));
    let play = Arc::new(AtomicF64::new(0.0));
    // A clip read at the host's frame (the frame-by-frame mode).
    let clip = Arc::new(AudioInputStream::with_host_clock(1, 2048));

    let vco = patch.add("vco", Vco::new(sr));
    let lfo = patch.add("lfo", Lfo::new(sr));
    let svf = patch.add("svf", Svf::new(sr));
    let vca = patch.add("vca", Vca::new());
    let out = patch.add("out", StereoOutput::new());

    patch.connect(vco.out("saw"), svf.in_("in")).unwrap();
    // Host audio summed into the filter input alongside the VCO.
    let mic = patch.add("mic", AudioInput::new(Arc::clone(&input)));
    patch.connect(mic.out("out"), svf.in_("in")).unwrap();
    // ...and pitch-tracked: the input plays the VCO.
    let track = patch.add("track", Track::new(sr));
    patch.connect(mic.out("out"), track.in_("in")).unwrap();
    patch.connect(track.out("voct"), vco.in_("voct")).unwrap();
    // ...and resampled: record and play gates come from the host.
    let capture = patch.add("capture", Capture::new(sr));
    let rec = patch.add("rec", ExternalInput::gate(Arc::clone(&record)));
    let gate = patch.add("play", ExternalInput::gate(Arc::clone(&play)));
    patch.connect(mic.out("out"), capture.in_("in")).unwrap();
    patch
        .connect(rec.out("out"), capture.in_("record"))
        .unwrap();
    patch.connect(gate.out("out"), capture.in_("gate")).unwrap();
    patch.connect(capture.out("out"), svf.in_("in")).unwrap();
    let clip_in = patch.add("clip", AudioInput::new(Arc::clone(&clip)));
    patch.connect(clip_in.out("out"), svf.in_("in")).unwrap();
    // LFO modulation cable into the filter cutoff (CvBipolar -> CvUnipolar; allowed).
    patch.connect(lfo.out("sin"), svf.in_("cutoff")).unwrap();
    patch.connect(svf.out("lp"), vca.in_("in")).unwrap();
    // VCA -> StereoOutput left only; `right` is normalled to `left`, exercising the
    // two-pass normalled-input resolution.
    patch.connect(vca.out("out"), out.in_("left")).unwrap();

    patch.set_output(out.id());
    patch.compile().unwrap();
    Rig {
        patch,
        input,
        record,
        play,
        clip,
    }
}

/// The patch and the host-side handles that feed it.
struct Rig {
    patch: Patch,
    input: Arc<AudioInputStream>,
    record: Arc<AtomicF64>,
    play: Arc<AtomicF64>,
    clip: Arc<AudioInputStream>,
}

/// Both the per-sample `tick()` and the block `tick_block()` paths must allocate nothing
/// after compile + warmup.
///
/// A single test function keeps the counting windows strictly sequential; with a process-wide
/// counting allocator, two concurrently scheduled test threads could otherwise attribute one
/// another's allocations to the measured window.
#[test]
fn graph_tick_paths_are_allocation_free() {
    let Rig {
        mut patch,
        input,
        record,
        play,
        clip,
    } = build_patch();
    let clip_samples: Vec<f32> = (0..2048).map(|i| (i as f32 * 0.003).sin()).collect();
    clip.write(&[&clip_samples[..]]);
    // The host's input: a continuous 220 Hz tone, refilled block by block into
    // preallocated buffers (planar and interleaved) before each render.
    let mut host_l = vec![0.0f32; 512];
    let mut host_r = vec![0.0f32; 512];
    let mut interleaved = vec![0.0f32; 1024];
    let mut phase = 0.0f32;
    let mut refill = |l: &mut [f32], r: &mut [f32], inter: &mut [f32]| {
        for i in 0..l.len() {
            let s = 0.5 * (phase * std::f32::consts::TAU).sin();
            phase = (phase + 220.0 / 44_100.0).fract();
            l[i] = s;
            r[i] = -s;
            inter[2 * i] = s;
            inter[2 * i + 1] = -s;
        }
    };

    // Warm up so every reusable buffer has reached steady-state capacity, and the
    // tracker is past its first frames (snapshots, estimates and gate changes all happen
    // inside the measured windows below).
    for _ in 0..16 {
        refill(&mut host_l, &mut host_r, &mut interleaved);
        input.write(&[&host_l[..], &host_r[..]]);
        for _ in 0..512 {
            black_box(patch.tick());
        }
    }

    // 1000 per-sample ticks, fed by host input blocks, must not allocate — while the
    // capture records (chunks 0-1), then plays its take back (chunks 2-3).
    let per_sample = count_allocs(|| {
        for chunk in 0..4 {
            refill(&mut host_l, &mut host_r, &mut interleaved);
            input.write(&[&host_l[..250], &host_r[..250]]);
            if chunk == 3 {
                input.write_interleaved(&interleaved[..500], 2);
            }
            record.set(if chunk < 2 { 5.0 } else { 0.0 });
            play.set(if chunk >= 2 { 5.0 } else { 0.0 });
            for _ in 0..250 {
                black_box(patch.tick());
                clip.advance();
            }
        }
    });
    assert_eq!(
        per_sample, 0,
        "tick() allocated {} time(s) across 1000 samples",
        per_sample
    );

    // A block tick over preallocated output slices must not allocate either.
    let mut left = [0.0_f64; 512];
    let mut right = [0.0_f64; 512];
    patch.tick_block(&mut left, &mut right); // warm the block path once
    let block = count_allocs(|| {
        refill(&mut host_l, &mut host_r, &mut interleaved);
        input.write(&[&host_l[..], &host_r[..]]);
        patch.tick_block(&mut left, &mut right);
        black_box((&left, &right));
    });
    assert_eq!(block, 0, "tick_block() allocated {} time(s)", block);

    // Q-N3: `set_param_by_id` on a compiled patch patches the routing plan in place —
    // no recompile, no `String` key, no growth — so a UI (or the WASM worklet's
    // `process()`) can turn knobs from the audio thread. Both a first-time set and a
    // repeat set of an unpatched control input, and a set on a cabled input (a recorded
    // no-op), must allocate nothing; nor may the ticks that follow (which would if the
    // patch had been invalidated and lazily recompiled).
    let svf = patch.get_node_id_by_name("svf").unwrap();
    let track = patch.get_node_id_by_name("track").unwrap();
    let mic = patch.get_node_id_by_name("mic").unwrap();
    let set_param = count_allocs(|| {
        black_box(patch.set_param_by_id(svf, "res", 0.7));
        black_box(patch.set_param_by_id(svf, "res", 0.2));
        // `cutoff` has the LFO cable on it: shadowed, returns false, still no allocation.
        black_box(patch.set_param_by_id(svf, "cutoff", 0.4));
        // Internal (introspection) selects: the tracker's band and the input's channel.
        // `range` re-plans the tracker's analysis in buffers sized for every band.
        black_box(patch.set_param_by_id(track, "range", 2.0));
        black_box(patch.set_param_by_id(mic, "channel", 0.0));
        for _ in 0..64 {
            black_box(patch.tick());
        }
        black_box(patch.set_param_by_id(track, "range", 0.0));
        for _ in 0..64 {
            black_box(patch.tick());
        }
    });
    assert_eq!(
        set_param, 0,
        "set_param_by_id() + ticks allocated {} time(s)",
        set_param
    );
}
