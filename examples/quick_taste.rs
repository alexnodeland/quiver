//! Quick Taste Example
//!
//! A minimal example showing the core Quiver workflow: build a patch, run it,
//! and this time actually *hear* it — the render finishes by writing a real
//! `.wav` file to disk.
//!
//! Run with: cargo run --example quick_taste

use quiver::prelude::*;
use quiver::render::write_wav;
use std::path::Path;

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

    // Generate one second of audio. `render` (from `quiver::render`, re-exported
    // in the prelude) just repeatedly calls `patch.tick()` for you — it's the
    // same thing as a manual loop, but it's also what writes WAV files below.
    let (left, right) = render(&mut patch, 1.0);

    // Report the results
    let peak = left.iter().map(|s| s.abs()).fold(0.0_f64, f64::max);
    println!("Generated {} samples", left.len());
    println!("Peak amplitude: {:.2}V", peak);

    // --- Hear it! ---
    // Quiver's `Audio` ports use a modular-synth convention of +-5V, but a
    // `.wav` file's samples are full-scale +-1.0, so we divide by 5 before
    // writing (see the `# Sample scale` note on `quiver::render`).
    let to_full_scale = |buf: &[f64]| -> Vec<f64> { buf.iter().map(|s| s / 5.0).collect() };
    let wav_path = Path::new("target/quick_taste.wav");
    write_wav(
        wav_path,
        44100,
        &to_full_scale(&left),
        &to_full_scale(&right),
    )
    .expect("failed to write WAV file");
    println!(
        "\nWrote {} - play it in any audio player to hear Quiver's output!",
        wav_path.display()
    );
}
