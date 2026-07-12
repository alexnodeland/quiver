//! Tutorial: Envelope Shaping
//!
//! Demonstrates the ADSR envelope generator and how it shapes sound over time.
//! This is fundamental to giving synthesized sounds their character.
//!
//! # Why ADSR has four stages
//!
//! A held note isn't a static event — it has a beginning, a middle, and an
//! end, and each needs different timing:
//! - **Attack**: the time to rise from silence to full level once the gate
//!   opens. Fast (a few ms) reads as percussive/plucky; slow (100s of ms)
//!   reads as a swell/pad fade-in.
//! - **Decay**: the time to fall from that initial peak down to the
//!   **Sustain** level — not a duration but a *held level* (0-1), the
//!   volume the note stays at for as long as the gate remains open. This is
//!   what makes a plucked-string patch (high decay, low sustain — the pluck
//!   dies down to near-silence and stays there) sound different from an
//!   organ patch (sustain near 1.0 — it just holds).
//! - **Release**: the time to fall from wherever the level was when the
//!   gate closed back to zero. Short release = clipped/staccato; long
//!   release = notes bleed into each other.
//!
//! Quiver's `Adsr` maps each stage's CV input onto an exponential 1ms-10s
//! time range, so small CV changes near the low end make a big perceptual
//! difference (just like a real synth's time knobs).
//!
//! Run with: cargo run --example tutorial_envelope

use quiver::prelude::*;
use std::sync::Arc;

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    // Gate control - simulates key press
    let gate_cv = Arc::new(AtomicF64::new(0.0));
    let gate = patch.add("gate", ExternalInput::gate(Arc::clone(&gate_cv)));

    // Sound source
    let vco = patch.add("vco", Vco::new(sample_rate));

    // ADSR envelope generator
    let env = patch.add("env", Adsr::new(sample_rate));

    // Amplifier controlled by envelope
    let vca = patch.add("vca", Vca::new());

    // Output
    let output = patch.add("output", StereoOutput::new());

    // Time-constant CVs. `Adsr` maps each 0-1 CV onto an exponential
    // 1ms-10s range via `time = 0.001 * 10000^cv`, so these values were
    // picked by solving that formula for the target times noted below —
    // slow enough that the millisecond checkpoints in the loops below land
    // inside each stage instead of after it has already finished.
    let attack_cv = patch.add("attack_cv", Offset::new(0.62)); // ~300ms attack
    let decay_cv = patch.add("decay_cv", Offset::new(0.5)); // ~100ms decay
    let sustain_cv = patch.add("sustain_cv", Offset::new(0.5)); // hold at 50% level
    let release_cv = patch.add("release_cv", Offset::new(0.64)); // ~350ms release

    // Connections
    patch.connect(gate.out("out"), env.in_("gate")).unwrap();
    patch
        .connect(attack_cv.out("out"), env.in_("attack"))
        .unwrap();
    patch
        .connect(decay_cv.out("out"), env.in_("decay"))
        .unwrap();
    patch
        .connect(sustain_cv.out("out"), env.in_("sustain"))
        .unwrap();
    patch
        .connect(release_cv.out("out"), env.in_("release"))
        .unwrap();
    patch.connect(vco.out("saw"), vca.in_("in")).unwrap();
    patch.connect(env.out("env"), vca.in_("cv")).unwrap();
    patch.connect(vca.out("out"), output.in_("left")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    println!("=== ADSR Envelope Demo ===\n");

    // Helper: run the patch for `n` samples and return the peak amplitude
    // seen. A *peak* (not just the last sample) is what we want here — a
    // single instantaneous sample would just be wherever the sawtooth
    // carrier happened to be in its cycle, not a meaningful envelope
    // reading.
    fn run_samples(patch: &mut Patch, n: usize) -> f64 {
        let mut peak = 0.0_f64;
        for _ in 0..n {
            let (left, _) = patch.tick();
            peak = peak.max(left.abs());
        }
        peak
    }

    // Start with gate off
    println!("Initial state (gate off):");
    let level = run_samples(&mut patch, 100);
    println!("  Envelope level: {:.3}V\n", level);

    // Gate ON - trigger attack
    println!("Gate ON - Attack phase begins");
    gate_cv.set(5.0);

    // Sample the attack: with a ~300ms attack time, these checkpoints show
    // the level climbing rather than already sitting at full scale.
    for ms in [10, 25, 50, 100, 200] {
        let samples = (sample_rate * ms as f64 / 1000.0) as usize;
        let level = run_samples(&mut patch, samples);
        println!("  {}ms: level = {:.2}V", ms, level);
    }

    // Let it reach sustain
    println!("\nDecay → Sustain:");
    let level = run_samples(&mut patch, (sample_rate * 0.5) as usize);
    println!("  Sustain level: {:.2}V\n", level);

    // Gate OFF - trigger release
    println!("Gate OFF - Release phase begins");
    gate_cv.set(0.0);

    // With a ~350ms release, the level should ease down across these
    // checkpoints instead of hitting zero immediately.
    for ms in [50, 100, 200, 500] {
        let samples = (sample_rate * ms as f64 / 1000.0) as usize;
        let level = run_samples(&mut patch, samples);
        println!("  +{}ms: level = {:.3}V", ms, level);
    }

    println!("\nThe envelope has completed its cycle.");
    println!("Attack→Decay→Sustain (while held) →Release (when released)");
}
