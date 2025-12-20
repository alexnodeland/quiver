//! How-To: MIDI Input Integration
//!
//! Demonstrates connecting external MIDI to a Quiver patch using AtomicF64
//! for thread-safe communication between MIDI and audio threads.
//!
//! Run with: cargo run --example howto_midi

use quiver::prelude::*;
use std::sync::Arc;

fn main() {
    let sample_rate = 44100.0;

    // Thread-safe communication channels
    let pitch_cv = Arc::new(AtomicF64::new(0.0)); // V/Oct
    let gate_cv = Arc::new(AtomicF64::new(0.0)); // Gate
    let velocity_cv = Arc::new(AtomicF64::new(5.0)); // Velocity (0-10V)
    let mod_wheel_cv = Arc::new(AtomicF64::new(0.0)); // CC1 modulation

    // Create patch
    let mut patch = Patch::new(sample_rate);

    // External inputs
    let pitch = patch.add("midi_pitch", ExternalInput::voct(Arc::clone(&pitch_cv)));
    let gate = patch.add("midi_gate", ExternalInput::gate(Arc::clone(&gate_cv)));
    let velocity = patch.add("midi_vel", ExternalInput::cv(Arc::clone(&velocity_cv)));
    let mod_wheel = patch.add("mod_wheel", ExternalInput::cv(Arc::clone(&mod_wheel_cv)));

    // Synth voice
    let vco = patch.add("vco", Vco::new(sample_rate));
    let vcf = patch.add("vcf", Svf::new(sample_rate));
    let vca = patch.add("vca", Vca::new());
    let env = patch.add("env", Adsr::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    // MIDI → synth connections
    patch.connect(pitch.out("out"), vco.in_("voct")).unwrap();
    patch.connect(gate.out("out"), env.in_("gate")).unwrap();

    // Audio chain
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
    patch.connect(vca.out("out"), output.in_("left")).unwrap();
    patch.connect(vca.out("out"), output.in_("right")).unwrap();

    // Modulation routing
    patch.connect(env.out("env"), vcf.in_("cutoff")).unwrap();
    patch.connect(env.out("env"), vca.in_("cv")).unwrap();
    patch.connect(mod_wheel.out("out"), vcf.in_("fm")).unwrap(); // Mod wheel → filter

    patch.set_output(output.id());
    patch.compile().unwrap();

    println!("=== MIDI Integration Demo ===\n");

    // Simulate MIDI events (in real app, these come from MIDI callback)
    fn midi_note_to_voct(note: u8) -> f64 {
        (note as f64 - 60.0) / 12.0
    }

    fn midi_velocity_to_cv(velocity: u8) -> f64 {
        velocity as f64 / 127.0 * 10.0
    }

    fn midi_cc_to_cv(value: u8) -> f64 {
        value as f64 / 127.0 * 10.0
    }

    // Simulate playing a C4 note
    println!("Simulating MIDI Note On: C4 (60), velocity 100");
    pitch_cv.set(midi_note_to_voct(60));
    velocity_cv.set(midi_velocity_to_cv(100));
    gate_cv.set(5.0); // Gate high

    // Process some samples during note
    let attack_samples = (sample_rate * 0.3) as usize;
    for _ in 0..attack_samples {
        patch.tick();
    }
    println!("  Attack phase processed ({} samples)", attack_samples);

    // Simulate mod wheel movement
    println!("\nSimulating CC1 (Mod Wheel): 64");
    mod_wheel_cv.set(midi_cc_to_cv(64));

    // More processing
    for _ in 0..(sample_rate * 0.2) as usize {
        patch.tick();
    }

    // Simulate note off
    println!("\nSimulating MIDI Note Off");
    gate_cv.set(0.0); // Gate low

    // Process release
    let release_samples = (sample_rate * 0.5) as usize;
    for _ in 0..release_samples {
        patch.tick();
    }
    println!("  Release phase processed ({} samples)", release_samples);

    // Play a chord (demonstrating polyphony would need PolyPatch)
    println!("\n--- Playing ascending notes ---");
    for (note, name) in [(60, "C4"), (64, "E4"), (67, "G4"), (72, "C5")] {
        // Note on
        pitch_cv.set(midi_note_to_voct(note));
        gate_cv.set(5.0);

        // Play for 200ms
        let mut peak = 0.0_f64;
        for _ in 0..(sample_rate * 0.2) as usize {
            let (left, _) = patch.tick();
            peak = peak.max(left.abs());
        }

        // Note off
        gate_cv.set(0.0);
        for _ in 0..(sample_rate * 0.1) as usize {
            patch.tick();
        }

        println!(
            "  {} (MIDI {}): V/Oct = {:.3}V, peak = {:.2}V",
            name,
            note,
            midi_note_to_voct(note),
            peak
        );
    }

    println!("\nMIDI integration complete.");
    println!("In a real application:");
    println!("  1. Create AtomicF64 values for each MIDI parameter");
    println!("  2. Update them from your MIDI callback");
    println!("  3. The audio thread reads the latest values each tick");
}
