//! Proof that the graph engine's tick paths are allocation-free.
//!
//! This integration test installs a counting global allocator (a wrapper around the system
//! allocator that increments an atomic on every `alloc`/`realloc` while armed). It builds a
//! representative patch (VCO -> SVF -> VCA -> StereoOutput, plus an LFO modulation cable and
//! a normalled input), warms it up, then asserts that a burst of `tick()` calls and a
//! `tick_block()` call perform **zero** heap allocations.
//!
//! It lives in its own integration-test binary (not a unit test) so the `#[global_allocator]`
//! only governs this process and does not perturb the main unit-test binary. It is std-only
//! because the counting allocator wraps `std::alloc::System`.
#![cfg(feature = "std")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
/// VCO -> SVF -> VCA -> StereoOutput, an LFO modulation cable into the filter cutoff, and a
/// normalled input (StereoOutput's `right` normals to `left`).
fn build_patch() -> Patch {
    let sr = 44_100.0;
    let mut patch = Patch::new(sr);

    let vco = patch.add("vco", Vco::new(sr));
    let lfo = patch.add("lfo", Lfo::new(sr));
    let svf = patch.add("svf", Svf::new(sr));
    let vca = patch.add("vca", Vca::new());
    let out = patch.add("out", StereoOutput::new());

    patch.connect(vco.out("saw"), svf.in_("in")).unwrap();
    // LFO modulation cable into the filter cutoff (CvBipolar -> CvUnipolar; allowed).
    patch.connect(lfo.out("sin"), svf.in_("cutoff")).unwrap();
    patch.connect(svf.out("lp"), vca.in_("in")).unwrap();
    // VCA -> StereoOutput left only; `right` is normalled to `left`, exercising the
    // two-pass normalled-input resolution.
    patch.connect(vca.out("out"), out.in_("left")).unwrap();

    patch.set_output(out.id());
    patch.compile().unwrap();
    patch
}

/// Both the per-sample `tick()` and the block `tick_block()` paths must allocate nothing
/// after compile + warmup.
///
/// A single test function keeps the counting windows strictly sequential; with a process-wide
/// counting allocator, two concurrently scheduled test threads could otherwise attribute one
/// another's allocations to the measured window.
#[test]
fn graph_tick_paths_are_allocation_free() {
    let mut patch = build_patch();

    // Warm up so every reusable buffer has reached steady-state capacity.
    for _ in 0..256 {
        black_box(patch.tick());
    }

    // 1000 per-sample ticks must not allocate.
    let per_sample = count_allocs(|| {
        for _ in 0..1000 {
            black_box(patch.tick());
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
    let set_param = count_allocs(|| {
        black_box(patch.set_param_by_id(svf, "res", 0.7));
        black_box(patch.set_param_by_id(svf, "res", 0.2));
        // `cutoff` has the LFO cable on it: shadowed, returns false, still no allocation.
        black_box(patch.set_param_by_id(svf, "cutoff", 0.4));
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
