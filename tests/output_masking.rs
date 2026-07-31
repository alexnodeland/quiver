//! Proof that dead-output elimination is invisible to the outputs that survive.
//!
//! [`GraphModule::tick_masked`] lets the graph tell a module which of its outputs anything
//! actually reads, so the module can skip producing the rest. That is only sound if a
//! skipped output changes *nothing* for the ones still wanted — neither their values on this
//! sample nor, through retained state, their values on any later one.
//!
//! Each test below renders a module twice over a long run: once with an all-ones mask (what
//! plain `tick` does) and once with only one output bit set, comparing the surviving port
//! **bit-for-bit** on every sample. Divergence in a phase accumulator, a PolyBLEP history,
//! a pink-noise filter or the shared RNG stream would show up within a handful of samples
//! and is guaranteed to show up over the run lengths used here.
//!
//! The graph-level counterpart is `tests/golden_vectors.rs`, whose patches consume a strict
//! subset of each source's outputs and must keep hashing identically.
#![cfg(feature = "std")]

use quiver::prelude::*;
use quiver::rng;

const SAMPLE_RATE: f64 = 44100.0;
const FRAMES: usize = 8192;

/// Run `module` for `FRAMES` samples under `wanted`, collecting the bits of `port` (and
/// `None` for a sample where the port was not written at all).
fn run_masked<M: GraphModule>(
    module: &mut M,
    driver: impl Fn(usize, &mut PortValues),
    port: PortId,
    wanted: u32,
) -> Vec<Option<u64>> {
    let mut inputs = PortValues::new();
    let mut outputs = PortValues::new();
    let mut trace = Vec::with_capacity(FRAMES);
    for frame in 0..FRAMES {
        driver(frame, &mut inputs);
        outputs.clear();
        module.tick_masked(&inputs, &mut outputs, wanted);
        trace.push(outputs.get(port).map(f64::to_bits));
    }
    trace
}

/// Assert that masking every output except `port` leaves `port` bit-identical to the
/// unmasked run, and that it is genuinely being produced (not silently absent).
fn assert_mask_transparent<M: GraphModule>(
    label: &str,
    mut full: M,
    mut partial: M,
    driver: impl Fn(usize, &mut PortValues) + Copy,
    port: PortId,
    only: u32,
) {
    let unmasked = run_masked(&mut full, driver, port, u32::MAX);
    let masked = run_masked(&mut partial, driver, port, only);

    assert!(
        unmasked.iter().all(Option::is_some),
        "{label}: port {port} was not written under a full mask"
    );
    assert!(
        unmasked.iter().any(|s| *s != Some(0.0f64.to_bits())),
        "{label}: port {port} produced nothing but zeros — the test proves nothing"
    );
    for (frame, (a, b)) in unmasked.iter().zip(masked.iter()).enumerate() {
        assert_eq!(
            a, b,
            "{label}: port {port} diverged at sample {frame} when the other outputs were \
             masked off ({a:?} vs {b:?})"
        );
    }
}

/// A VCO's saw output must not care whether sine/triangle/square are wanted — including
/// across hard-sync resets, whose PolyBLEP correction is shared between saw and square.
#[test]
fn vco_saw_is_unaffected_by_masking_the_other_waveforms() {
    let driver = |frame: usize, inputs: &mut PortValues| {
        inputs.set(0, 0.25);
        inputs.set(1, 0.0);
        inputs.set(2, 0.35);
        // Hard-sync pulses at an unrelated rate, exercising the sync-reset branch.
        inputs.set(3, if frame % 311 < 8 { 5.0 } else { 0.0 });
        inputs.set(4, 0.0);
    };
    assert_mask_transparent(
        "Vco",
        Vco::new(SAMPLE_RATE),
        Vco::new(SAMPLE_RATE),
        driver,
        12, // saw
        1 << 2,
    );
}

/// The same, for the square output (the other consumer of the sync correction).
#[test]
fn vco_square_is_unaffected_by_masking_the_other_waveforms() {
    let driver = |frame: usize, inputs: &mut PortValues| {
        inputs.set(0, -0.5);
        inputs.set(1, 0.0);
        inputs.set(2, 0.7);
        inputs.set(3, if frame % 257 < 8 { 5.0 } else { 0.0 });
        inputs.set(4, 0.0);
    };
    assert_mask_transparent(
        "Vco",
        Vco::new(SAMPLE_RATE),
        Vco::new(SAMPLE_RATE),
        driver,
        13, // sqr
        1 << 3,
    );
}

/// An LFO's unipolar sine must not care whether the four bipolar shapes are wanted.
#[test]
fn lfo_sin_uni_is_unaffected_by_masking_the_other_waveforms() {
    let driver = |frame: usize, inputs: &mut PortValues| {
        inputs.set(0, 0.8);
        inputs.set(1, 10.0);
        inputs.set(2, if frame % 1777 == 0 { 5.0 } else { 0.0 });
    };
    assert_mask_transparent(
        "Lfo",
        Lfo::new(SAMPLE_RATE),
        Lfo::new(SAMPLE_RATE),
        driver,
        14, // sin_uni
        1 << 4,
    );
}

/// White noise must not care whether pink/white2/pink2 are wanted — the delicate case,
/// since both channels draw from one RNG stream and each pink source has its own filter
/// state. Both runs start from the same seed, and the two RNG draws per sample stay
/// unconditional, so the streams stay in lockstep.
#[test]
fn noise_white_is_unaffected_by_masking_the_other_outputs() {
    const SEED: u64 = 0x1234_5678_9abc_def0;
    let driver = |_: usize, inputs: &mut PortValues| inputs.set(0, 0.42);

    rng::seed(SEED);
    let mut full = NoiseGenerator::new();
    let unmasked = run_masked(&mut full, driver, 10, u32::MAX);

    rng::seed(SEED);
    let mut partial = NoiseGenerator::new();
    let masked = run_masked(&mut partial, driver, 10, 1 << 0);

    assert!(unmasked.iter().all(Option::is_some));
    assert_eq!(
        unmasked, masked,
        "white noise diverged once the correlated/pink outputs were masked off — the RNG \
         stream or a pink filter must have fallen out of step"
    );
}

/// The mask is really in force in a compiled patch — without this the equivalence test
/// below could pass vacuously — and this is what that costs: an unread output's routing
/// slot is never written, so `get_output_value` reports its initial `0.0` instead of a live
/// sample. Metering an unpatched port of a mask-aware module requires patching it, or
/// pinning it with `keep_output_live` (see the tests that follow).
#[test]
fn unread_outputs_are_not_produced_into_the_routing_buffer() {
    let mut patch = Patch::new(SAMPLE_RATE);
    let vco = patch.add("vco", Vco::new(SAMPLE_RATE));
    let out = patch.add("out", StereoOutput::new());
    patch.connect(vco.out("saw"), out.in_("left")).unwrap();
    patch.set_output(out.id());
    patch.compile().unwrap();

    let mut saw_moved = false;
    for _ in 0..1024 {
        patch.tick();
        if patch.get_output_value(vco.id(), 12) != Some(0.0) {
            saw_moved = true;
        }
        // `tri` (port 11) has no consumer: never computed, never scattered.
        assert_eq!(patch.get_output_value(vco.id(), 11), Some(0.0));
    }
    assert!(saw_moved, "the consumed saw output should be live");
}

/// Masking must also be transparent *through the graph*: a patch that reads one port of a
/// mask-aware source renders exactly the same whether or not its siblings are also patched.
#[test]
fn graph_render_matches_when_unread_outputs_are_also_patched() {
    fn render(also_patch_siblings: bool) -> Vec<u64> {
        let mut patch = Patch::new(SAMPLE_RATE);
        let vco = patch.add("vco", Vco::new(SAMPLE_RATE));
        let svf = patch.add("svf", Svf::new(SAMPLE_RATE));
        let out = patch.add("out", StereoOutput::new());
        patch.connect(vco.out("saw"), svf.in_("in")).unwrap();
        patch.connect(svf.out("lp"), out.in_("left")).unwrap();

        if also_patch_siblings {
            // Give the other three waveforms consumers that cannot affect the output: a
            // dead-end attenuverter each. This flips their `wanted` bits on.
            for (i, name) in ["sin", "tri", "sqr"].iter().enumerate() {
                let sink = patch.add(format!("sink{i}"), Attenuverter::new());
                patch.connect(vco.out(name), sink.in_("in")).unwrap();
            }
        }

        patch.set_output(out.id());
        patch.compile().unwrap();
        (0..FRAMES)
            .map(|_| {
                let (left, _) = patch.tick();
                left.to_bits()
            })
            .collect()
    }

    assert_eq!(
        render(false),
        render(true),
        "the consumed port changed depending on whether its siblings were also consumed"
    );
}

// ---------------------------------------------------------------------------
// The escape hatch: `Patch::keep_output_live` re-enables an unread output for metering.
// ---------------------------------------------------------------------------

/// Build the reference patch (VCO saw -> output) and run it for `frames`, returning the
/// left channel's bits and the per-sample values of the VCO's `tri` port (11), which no
/// cable ever reads. `pin_tri` decides whether `tri` is pinned live.
fn saw_patch_trace(pin_tri: bool, frames: usize) -> (Vec<u64>, Vec<Option<f64>>) {
    let mut patch = Patch::new(SAMPLE_RATE);
    let vco = patch.add("vco", Vco::new(SAMPLE_RATE));
    let out = patch.add("out", StereoOutput::new());
    patch.connect(vco.out("saw"), out.in_("left")).unwrap();
    patch.set_output(out.id());
    if pin_tri {
        assert!(patch.keep_output_live(vco.id(), 11));
    }
    patch.compile().unwrap();

    let mut left = Vec::with_capacity(frames);
    let mut tri = Vec::with_capacity(frames);
    for _ in 0..frames {
        let (l, _) = patch.tick();
        left.push(l.to_bits());
        tri.push(patch.get_output_value(vco.id(), 11));
    }
    (left, tri)
}

/// Pinning a port nobody patches puts it back in the mask, so metering it reads live audio
/// again instead of the flat `0.0` the test above pins down.
#[test]
fn keep_output_live_restores_metering_of_an_unpatched_output() {
    let (_, tri) = saw_patch_trace(true, 1024);
    assert!(
        tri.iter().all(Option::is_some),
        "a pinned port must be present in the routing buffer on every sample"
    );
    assert!(
        tri.iter().any(|v| *v != Some(0.0)),
        "a pinned tri output should be live, not stuck at its initial 0.0"
    );
}

/// ...and doing so must not perturb a single sample of what the patch renders. The pinned
/// port feeds nothing, and producing it is the same code path the module would take if a
/// cable were attached, so the audio is bit-identical.
#[test]
fn keep_output_live_does_not_change_the_rendered_output() {
    let (unpinned, _) = saw_patch_trace(false, FRAMES);
    let (pinned, _) = saw_patch_trace(true, FRAMES);
    assert_eq!(
        unpinned, pinned,
        "keeping an unread output live changed the rendered signal"
    );
}

/// The pin is releasable, and releasing it returns the port to the dead set on the next
/// compile. Repeat marks and repeat releases are no-ops.
#[test]
fn release_output_live_returns_the_port_to_the_dead_set() {
    let mut patch = Patch::new(SAMPLE_RATE);
    let vco = patch.add("vco", Vco::new(SAMPLE_RATE));
    let out = patch.add("out", StereoOutput::new());
    patch.connect(vco.out("saw"), out.in_("left")).unwrap();
    patch.set_output(out.id());

    assert!(patch.keep_output_live(vco.id(), 11));
    assert!(
        !patch.keep_output_live(vco.id(), 11),
        "marking is idempotent"
    );
    assert_eq!(patch.kept_live_outputs().len(), 1);
    patch.compile().unwrap();
    for _ in 0..256 {
        patch.tick();
    }
    let live = patch.get_output_value(vco.id(), 11);

    assert!(patch.release_output_live(vco.id(), 11));
    assert!(
        !patch.release_output_live(vco.id(), 11),
        "releasing twice is a no-op"
    );
    assert!(patch.kept_live_outputs().is_empty());
    patch.compile().unwrap();
    for _ in 0..256 {
        patch.tick();
    }

    assert_ne!(
        live,
        Some(0.0),
        "the pinned port should have been live before release"
    );
    assert_eq!(
        patch.get_output_value(vco.id(), 11),
        Some(0.0),
        "after release the port is dead again (compile zeroed the routing buffer)"
    );
    assert!(!patch.clear_kept_live_outputs(), "nothing left to clear");
}

/// Removing a node drops its pins, so a long-lived editor session cannot accumulate
/// keep-alive entries for nodes that no longer exist.
#[test]
fn removing_a_node_drops_its_keep_alive_entries() {
    let mut patch = Patch::new(SAMPLE_RATE);
    let vco = patch.add("vco", Vco::new(SAMPLE_RATE));
    let lfo = patch.add("lfo", Lfo::new(SAMPLE_RATE));
    assert!(patch.keep_output_live(vco.id(), 11));
    assert!(patch.keep_output_live(lfo.id(), 11));
    assert_eq!(patch.kept_live_outputs().len(), 2);

    patch.remove(vco.id()).unwrap();
    assert_eq!(patch.kept_live_outputs().len(), 1);
    assert_eq!(patch.kept_live_outputs()[0].node, lfo.id());
}

/// The observer applies the pin for every port it meters, which is what keeps
/// `Engine.subscribe` reporting live values for an unpatched port. Without the sync the
/// subscription reads a flat zero.
#[test]
fn observer_keepalive_makes_an_unsubscribed_port_meterable() {
    use quiver::observer::{StateObserver, SubscriptionTarget};

    let mut patch = Patch::new(SAMPLE_RATE);
    let vco = patch.add("vco", Vco::new(SAMPLE_RATE));
    let out = patch.add("out", StereoOutput::new());
    patch.connect(vco.out("saw"), out.in_("left")).unwrap();
    patch.set_output(out.id());

    let mut observer = StateObserver::new();
    observer.add_subscriptions(vec![
        // Metering an unpatched port of a mask-aware module...
        SubscriptionTarget::Level {
            node_id: "vco".into(),
            port_id: 11,
        },
        // ...and a Param target, which names no port and must be skipped.
        SubscriptionTarget::Param {
            node_id: "vco".into(),
            param_id: "waveform".into(),
        },
    ]);
    assert_eq!(observer.metered_ports().count(), 1);

    observer.sync_output_keepalive(&mut patch);
    assert_eq!(patch.kept_live_outputs().len(), 1);
    patch.compile().unwrap();

    let mut moved = false;
    for _ in 0..1024 {
        patch.tick();
        moved |= patch.get_output_value(vco.id(), 11) != Some(0.0);
    }
    assert!(moved, "the metered port should be live after the sync");

    // Dropping the subscription releases the pin on the next sync.
    observer.clear_subscriptions();
    observer.sync_output_keepalive(&mut patch);
    assert!(patch.kept_live_outputs().is_empty());
}
