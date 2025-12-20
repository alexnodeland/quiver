//! Quick Taste Example
//!
//! A minimal example showing the core Quiver workflow.
//!
//! Run with: cargo run --example quick_taste

use quiver::prelude::*;

fn main() {
    // Create a patch at CD-quality sample rate
    let mut patch = Patch::new(44100.0);

    // Add an oscillator and output
    let vco = patch.add("vco", Vco::new(44100.0));
    let output = patch.add("out", StereoOutput::new());

    // Connect the sawtooth wave to both channels
    patch.connect(vco.out("saw"), output.in_("left")).unwrap();
    patch.connect(vco.out("saw"), output.in_("right")).unwrap();

    // Compile the patch for processing
    patch.set_output(output.id());
    patch.compile().unwrap();

    // Generate one second of audio
    let mut samples = Vec::new();
    for _ in 0..44100 {
        let (left, _right) = patch.tick();
        samples.push(left);
    }

    // Report the results
    let peak = samples.iter().map(|s| s.abs()).fold(0.0_f64, f64::max);
    println!("Generated {} samples", samples.len());
    println!("Peak amplitude: {:.2}V", peak);
}
