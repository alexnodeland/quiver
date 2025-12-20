//! First Patch Example
//!
//! A complete subtractive synthesizer voice demonstrating the core
//! Quiver workflow: VCO → VCF → VCA with ADSR envelope shaping.
//!
//! Run with: cargo run --example first_patch

use quiver::prelude::*;
use std::sync::Arc;

fn main() {
    // CD-quality sample rate
    let sample_rate = 44100.0;

    // Create our patch (virtual modular case)
    let mut patch = Patch::new(sample_rate);

    // External control: gate signal for envelope triggering
    let gate_cv = Arc::new(AtomicF64::new(0.0));

    // Add modules to the patch
    let gate = patch.add("gate", ExternalInput::gate(Arc::clone(&gate_cv)));
    let vco = patch.add("vco", Vco::new(sample_rate));
    let vcf = patch.add("vcf", Svf::new(sample_rate));
    let vca = patch.add("vca", Vca::new());
    let env = patch.add("env", Adsr::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    // Patch cables: signal flow
    // Gate triggers the envelope
    patch.connect(gate.out("out"), env.in_("gate")).unwrap();

    // VCO → VCF → VCA → Output (main audio path)
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
    patch.connect(vca.out("out"), output.in_("left")).unwrap();
    patch.connect(vca.out("out"), output.in_("right")).unwrap();

    // Envelope modulates both filter and amplitude
    patch.connect(env.out("env"), vcf.in_("cutoff")).unwrap();
    patch.connect(env.out("env"), vca.in_("cv")).unwrap();

    // Compile the patch for processing
    patch.set_output(output.id());
    patch.compile().unwrap();

    println!(
        "Patch compiled: {} modules, {} cables",
        patch.node_count(),
        patch.cable_count()
    );
    println!();

    // Play a note: gate on
    println!("Note ON - Gate rises to +5V");
    gate_cv.set(5.0);

    // Process attack phase (0.5 seconds)
    let attack_samples = (sample_rate * 0.5) as usize;
    let mut peak = 0.0_f64;

    for _ in 0..attack_samples {
        let (left, _) = patch.tick();
        peak = peak.max(left.abs());
    }
    println!("  Attack complete, peak level: {:.2}V", peak);

    // Release the note: gate off
    println!("Note OFF - Gate falls to 0V");
    gate_cv.set(0.0);

    // Process release phase
    let release_samples = (sample_rate * 1.0) as usize;
    let mut release_peak = 0.0_f64;

    for _ in 0..release_samples {
        let (left, _) = patch.tick();
        release_peak = release_peak.max(left.abs());
    }
    println!("  Release complete, final level: {:.4}V", release_peak);

    println!();
    println!("Subtractive synthesis voice complete!");
}
