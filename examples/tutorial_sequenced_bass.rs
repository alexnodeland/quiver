//! Tutorial: Building a Sequenced Bass
//!
//! A step sequencer driving a classic subtractive bass voice.
//! This pattern is the foundation of house, techno, and many other genres.
//!
//! Run with: cargo run --example tutorial_sequenced_bass

use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    // Master clock - sets the tempo
    let clock = patch.add("clock", Clock::new(sample_rate));

    // Step sequencer - stores our bassline pattern
    let seq = patch.add("seq", StepSequencer::new());

    // Bass voice: VCO → VCF → VCA
    let vco = patch.add("vco", Vco::new(sample_rate));
    let vcf = patch.add("vcf", Svf::new(sample_rate));
    let vca = patch.add("vca", Vca::new());
    let env = patch.add("env", Adsr::new(sample_rate));

    // Output
    let output = patch.add("output", StereoOutput::new());

    // Clock → Sequencer
    patch.connect(clock.out("div_8"), seq.in_("clock")).unwrap();

    // Sequencer → Voice
    patch.connect(seq.out("cv"), vco.in_("voct")).unwrap();
    patch.connect(seq.out("gate"), env.in_("gate")).unwrap();

    // Audio path
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
    patch.connect(vca.out("out"), output.in_("left")).unwrap();
    patch.connect(vca.out("out"), output.in_("right")).unwrap();

    // Envelope → Filter & VCA
    patch.connect(env.out("env"), vcf.in_("cutoff")).unwrap();
    patch.connect(env.out("env"), vca.in_("cv")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    println!("=== Sequenced Bass Demo ===\n");

    // The sequencer has 8 steps with default values
    // In a real application, you'd set the step CVs programmatically

    // Convert MIDI note to V/Oct
    fn midi_to_voct(note: u8) -> f64 {
        (note as f64 - 60.0) / 12.0
    }

    // Our bassline: C3, D3, rest, G2, C3, rest, E3, D3
    let pattern = [
        (48, true),   // C3
        (50, true),   // D3
        (0, false),   // rest
        (43, true),   // G2
        (48, true),   // C3
        (0, false),   // rest
        (52, true),   // E3
        (50, true),   // D3
    ];

    println!("Bassline pattern:");
    for (i, (note, active)) in pattern.iter().enumerate() {
        if *active {
            let voct = midi_to_voct(*note);
            let note_name = match note % 12 {
                0 => "C", 1 => "C#", 2 => "D", 3 => "D#",
                4 => "E", 5 => "F", 6 => "F#", 7 => "G",
                8 => "G#", 9 => "A", 10 => "A#", 11 => "B",
                _ => "?"
            };
            let octave = (note / 12) - 1;
            println!("  Step {}: {}{} ({:.3}V)", i + 1, note_name, octave, voct);
        } else {
            println!("  Step {}: rest", i + 1);
        }
    }

    println!("\nRunning sequencer for 2 seconds...\n");

    // Run the patch
    let total_samples = (sample_rate * 2.0) as usize;
    let step_samples = total_samples / 16; // ~8 steps at default tempo

    for step in 0..16 {
        let mut peak = 0.0_f64;

        for _ in 0..step_samples {
            let (left, _) = patch.tick();
            peak = peak.max(left.abs());
        }

        let step_num = (step % 8) + 1;
        let bar = "█".repeat((peak * 10.0) as usize);
        println!("Step {}: {:5.2}V |{}", step_num, peak, bar);
    }

    println!("\nThe sequencer cycles through the pattern,");
    println!("triggering the envelope on each gated step.");
}
