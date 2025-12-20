//! Tutorial: Basic Subtractive Synthesis
//!
//! This example demonstrates the fundamentals of subtractive synthesis:
//! starting with a harmonically rich oscillator and shaping it with a filter.
//!
//! Run with: cargo run --example tutorial_subtractive

use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    // The oscillator: source of harmonics
    let vco = patch.add("vco", Vco::new(sample_rate));

    // The filter: subtracts harmonics
    let vcf = patch.add("vcf", Svf::new(sample_rate));

    // Output stage
    let output = patch.add("output", StereoOutput::new());

    // Offset module to set filter cutoff (in CV range)
    // 5V corresponds to a medium-high cutoff frequency
    let cutoff = patch.add("cutoff", Offset::new(5.0));

    // Connect: Saw wave → Filter → Output
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(cutoff.out("out"), vcf.in_("cutoff")).unwrap();
    patch.connect(vcf.out("lp"), output.in_("left")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    // Generate samples and analyze harmonic content
    println!("=== Subtractive Synthesis Demo ===\n");

    // Collect one period of audio (assuming ~261Hz C4)
    let period_samples = (sample_rate / 261.63) as usize;
    let mut samples: Vec<f64> = Vec::new();

    for _ in 0..period_samples * 10 {
        let (left, _) = patch.tick();
        samples.push(left);
    }

    // Analyze the filtered output
    let peak = samples.iter().map(|s| s.abs()).fold(0.0_f64, f64::max);
    let rms = (samples.iter().map(|s| s * s).sum::<f64>() / samples.len() as f64).sqrt();

    println!("Sawtooth → Lowpass Filter");
    println!("  Peak amplitude: {:.2}V", peak);
    println!("  RMS level: {:.2}V", rms);
    println!("  Samples generated: {}", samples.len());

    // Compare with unfiltered saw
    let mut raw_patch = Patch::new(sample_rate);
    let raw_vco = raw_patch.add("vco", Vco::new(sample_rate));
    let raw_out = raw_patch.add("output", StereoOutput::new());
    raw_patch.connect(raw_vco.out("saw"), raw_out.in_("left")).unwrap();
    raw_patch.set_output(raw_out.id());
    raw_patch.compile().unwrap();

    let mut raw_samples: Vec<f64> = Vec::new();
    for _ in 0..period_samples * 10 {
        let (left, _) = raw_patch.tick();
        raw_samples.push(left);
    }

    let raw_peak = raw_samples.iter().map(|s| s.abs()).fold(0.0_f64, f64::max);
    let raw_rms = (raw_samples.iter().map(|s| s * s).sum::<f64>() / raw_samples.len() as f64).sqrt();

    println!("\nRaw Sawtooth (unfiltered)");
    println!("  Peak amplitude: {:.2}V", raw_peak);
    println!("  RMS level: {:.2}V", raw_rms);

    println!("\nThe filter has smoothed the waveform by removing high harmonics.");
    println!("Notice the lower RMS - less high-frequency energy means a softer sound.");
}
