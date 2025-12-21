//! Tutorial: Envelope Shaping
//!
//! Demonstrates the ADSR envelope generator and how it shapes sound over time.
//! This is fundamental to giving synthesized sounds their character.
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

    // Connections
    patch.connect(gate.out("out"), env.in_("gate")).unwrap();
    patch.connect(vco.out("saw"), vca.in_("in")).unwrap();
    patch.connect(env.out("env"), vca.in_("cv")).unwrap();
    patch.connect(vca.out("out"), output.in_("left")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    println!("=== ADSR Envelope Demo ===\n");

    // Helper to get envelope level
    fn run_samples(patch: &mut Patch, n: usize) -> f64 {
        let mut last = 0.0;
        for _ in 0..n {
            let (left, _) = patch.tick();
            last = left.abs();
        }
        last
    }

    // Start with gate off
    println!("Initial state (gate off):");
    let level = run_samples(&mut patch, 100);
    println!("  Envelope level: {:.3}V\n", level);

    // Gate ON - trigger attack
    println!("Gate ON - Attack phase begins");
    gate_cv.set(5.0);

    // Sample the attack
    for ms in [10, 25, 50, 100, 200] {
        let samples = (sample_rate * ms as f64 / 1000.0) as usize;
        let level = run_samples(&mut patch, samples);
        println!("  {}ms: level = {:.2}V", ms, level * 5.0); // scale for display
    }

    // Let it reach sustain
    println!("\nDecay → Sustain:");
    let level = run_samples(&mut patch, (sample_rate * 0.5) as usize);
    println!("  Sustain level: {:.2}V\n", level * 5.0);

    // Gate OFF - trigger release
    println!("Gate OFF - Release phase begins");
    gate_cv.set(0.0);

    for ms in [50, 100, 200, 500] {
        let samples = (sample_rate * ms as f64 / 1000.0) as usize;
        let level = run_samples(&mut patch, samples);
        println!("  +{}ms: level = {:.3}V", ms, level * 5.0);
    }

    println!("\nThe envelope has completed its cycle.");
    println!("Attack→Decay→Sustain (while held) →Release (when released)");
}
