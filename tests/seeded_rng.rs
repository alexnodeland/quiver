//! Per-module owned randomness and `Patch::seed` (Q-N2).
//!
//! Before 0.4.0 every stochastic module drew from one thread-global stream seeded
//! from the wall clock, so a render was reproducible only if the caller reseeded
//! that stream first, never shared a thread, and constructed no analog modules in
//! between. `Patch::seed` gives every node its own stream derived from one seed;
//! these tests pin the contract: two seeded renders are identical without touching
//! the global RNG, `reset()` replays them, two patches on one thread are
//! independent, `PolyPatch` propagates per-voice seeds, and an *unseeded* patch
//! still follows `quiver::rng::seed` exactly as it always did.
#![cfg(feature = "std")]

use quiver::modules::{BernoulliGate, KarplusStrong};
use quiver::prelude::*;
use quiver::rng;

const SR: f64 = 44_100.0;
const FRAMES: usize = 4096;

/// Every stochastic module in one graph: white noise, a plucked string (its
/// excitation is random), a Bernoulli gate flipping on an LFO, and an analog VCO
/// (drift noise per tick, plus four construction-time draws).
fn build_stochastic_patch() -> Patch {
    let mut patch = Patch::new(SR);
    let noise = patch.add("noise", NoiseGenerator::new());
    let lfo = patch.add("lfo", Lfo::new(SR));
    let ks = patch.add("ks", KarplusStrong::new(SR));
    let coin = patch.add("coin", BernoulliGate::new());
    let avco = patch.add("avco", AnalogVco::new(SR));
    let mix = patch.add("mix", Mixer::new(4));
    let out = patch.add("out", StereoOutput::new());

    patch.set_param_by_id(lfo.id(), "rate", 0.9);
    patch.connect(lfo.out("sqr"), ks.in_("trigger")).unwrap();
    patch.connect(lfo.out("sqr"), coin.in_("trig")).unwrap();
    patch
        .connect_attenuated(noise.out("white"), mix.in_("ch0"), 0.2)
        .unwrap();
    patch.connect(ks.out("out"), mix.in_("ch1")).unwrap();
    patch
        .connect_attenuated(coin.out("gate_a"), mix.in_("ch2"), 0.1)
        .unwrap();
    patch
        .connect_attenuated(avco.out("saw"), mix.in_("ch3"), 0.2)
        .unwrap();
    patch.connect(mix.out("out"), out.in_("left")).unwrap();
    patch.set_output(out.id());
    patch
}

fn render(patch: &mut Patch, frames: usize) -> Vec<(u64, u64)> {
    (0..frames)
        .map(|_| {
            let (l, r) = patch.tick();
            assert!(l.is_finite() && r.is_finite());
            (l.to_bits(), r.to_bits())
        })
        .collect()
}

fn has_signal(samples: &[(u64, u64)]) -> bool {
    samples.iter().any(|&(l, _)| f64::from_bits(l).abs() > 1e-3)
}

/// Scramble the thread-global stream (and consume construction-time draws) so
/// any leak from it into a seeded render would show.
fn perturb_global_rng() {
    rng::seed(rng::random().to_bits());
    let _ = AnalogVco::new(SR); // four global draws at construction
    for _ in 0..97 {
        let _ = rng::random();
    }
}

#[test]
fn seeded_renders_are_identical_without_touching_the_global_rng() {
    let mut a = build_stochastic_patch();
    a.seed(0xDEAD_BEEF);
    let first = render(&mut a, FRAMES);
    assert!(has_signal(&first), "stochastic patch rendered silence");

    perturb_global_rng();

    let mut b = build_stochastic_patch();
    b.seed(0xDEAD_BEEF);
    let second = render(&mut b, FRAMES);
    assert_eq!(first, second, "seeded render depends on the global RNG");

    // And the seed matters: a different one is a different render.
    let mut c = build_stochastic_patch();
    c.seed(0xDEAD_BEEF + 1);
    assert_ne!(first, render(&mut c, FRAMES));
    assert_eq!(c.seed_value(), Some(0xDEAD_BEEF + 1));
}

#[test]
fn reset_replays_a_seeded_render() {
    let mut patch = build_stochastic_patch();
    patch.seed(11);
    let first = render(&mut patch, FRAMES);
    perturb_global_rng();
    patch.reset();
    let second = render(&mut patch, FRAMES);
    assert_eq!(first, second, "reset() did not restore the seeded streams");
}

#[test]
fn two_seeded_patches_on_one_thread_are_independent() {
    // Reference: `a` alone.
    let mut solo = build_stochastic_patch();
    solo.seed(1);
    let reference = render(&mut solo, FRAMES);

    // Same patch interleaved sample-by-sample with a different seeded patch.
    let mut a = build_stochastic_patch();
    a.seed(1);
    let mut b = build_stochastic_patch();
    b.seed(2);
    let mut interleaved = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        let (l, r) = a.tick();
        interleaved.push((l.to_bits(), r.to_bits()));
        let _ = b.tick();
        let _ = rng::random(); // a third party consuming the global stream
    }
    assert_eq!(
        reference, interleaved,
        "patch B (or the global stream) leaked into A"
    );
}

#[test]
fn unseeded_patch_still_follows_the_global_seed() {
    // The pre-0.4.0 contract, kept for callers that reseed the thread themselves.
    rng::seed(0x5EED);
    let mut a = build_stochastic_patch();
    assert_eq!(a.seed_value(), None);
    let first = render(&mut a, 1024);

    rng::seed(0x5EED);
    let mut b = build_stochastic_patch();
    let second = render(&mut b, 1024);
    assert_eq!(first, second, "unseeded patch no longer tracks rng::seed");

    // ...and an unseeded patch IS affected by what else the thread draws, which is
    // exactly why `Patch::seed` exists.
    rng::seed(0x5EED);
    let _ = rng::random();
    let mut c = build_stochastic_patch();
    assert_ne!(first, render(&mut c, 1024));
}

#[test]
fn nodes_added_after_seeding_are_seeded_too() {
    let mut a = Patch::new(SR);
    let noise_a = a.add("noise", NoiseGenerator::new());
    let out_a = a.add("out", StereoOutput::new());
    a.connect(noise_a.out("white"), out_a.in_("left")).unwrap();
    a.set_output(out_a.id());
    a.seed(3);

    let mut b = Patch::new(SR);
    b.seed(3);
    let noise_b = b.add("noise", NoiseGenerator::new());
    let out_b = b.add("out", StereoOutput::new());
    b.connect(noise_b.out("white"), out_b.in_("left")).unwrap();
    b.set_output(out_b.id());

    assert_eq!(render(&mut a, 512), render(&mut b, 512));
}

fn build_noise_poly(voices: usize) -> PolyPatch {
    PolyPatch::with_voice_fn(voices, SR, |patch, ctrl| {
        let noise = patch.add("noise", NoiseGenerator::new());
        let vca = patch.add("vca", Vca::new());
        let out = patch.add("out", StereoOutput::new());
        patch.connect(noise.out("white"), vca.in_("in"))?;
        patch.connect(ctrl.out("gate"), vca.in_("cv"))?;
        patch.connect(vca.out("out"), out.in_("left"))?;
        patch.set_output(out.id());
        Ok(())
    })
    .unwrap()
}

fn render_poly(poly: &mut PolyPatch, frames: usize) -> Vec<u64> {
    (0..frames)
        .map(|_| {
            let (l, _) = poly.tick();
            assert!(l.is_finite());
            l.to_bits()
        })
        .collect()
}

#[test]
fn poly_patch_seeds_every_voice_distinctly() {
    let mut a = build_noise_poly(2);
    a.seed(99);
    a.note_on(60, 100);
    a.note_on(64, 100);
    let first = render_poly(&mut a, 2048);

    perturb_global_rng();

    let mut b = build_noise_poly(2);
    b.seed(99);
    b.note_on(60, 100);
    b.note_on(64, 100);
    assert_eq!(
        first,
        render_poly(&mut b, 2048),
        "seeded PolyPatch not reproducible"
    );

    // Each voice has its own stream: the two voice graphs render different noise.
    let mut c = build_noise_poly(2);
    c.seed(99);
    c.note_on(60, 100);
    c.note_on(64, 100);
    let mut differ = false;
    for _ in 0..256 {
        c.tick();
        let v0 = c.voice_patch(0).unwrap().get_output_value(
            c.voice_patch(0)
                .unwrap()
                .get_node_id_by_name("noise")
                .unwrap(),
            10,
        );
        let v1 = c.voice_patch(1).unwrap().get_output_value(
            c.voice_patch(1)
                .unwrap()
                .get_node_id_by_name("noise")
                .unwrap(),
            10,
        );
        differ |= v0 != v1;
    }
    assert!(differ, "voices share one noise stream");

    // The seed survives a voice rebuild.
    let mut d = build_noise_poly(2);
    d.seed(99);
    d.set_sample_rate(SR);
    d.note_on(60, 100);
    d.note_on(64, 100);
    assert_eq!(
        first,
        render_poly(&mut d, 2048),
        "seed lost across voice rebuild"
    );
}
