//! Tutorial: Basic Subtractive Synthesis
//!
//! This example demonstrates the fundamentals of subtractive synthesis:
//! starting with a harmonically rich oscillator and shaping it with a filter.
//!
//! # Why subtractive synthesis works
//!
//! A sawtooth wave already contains *every* harmonic of its fundamental
//! (1st, 2nd, 3rd, ... all present, falling off as 1/n). Subtractive
//! synthesis doesn't add color, it removes it: a lowpass filter attenuates
//! everything above its cutoff, leaving a subset of that harmonic series.
//! Sweeping the cutoff changes which harmonics survive, which is why an
//! otherwise-static saw wave can sound like it "opens up" as the filter
//! tracks upward. The state-variable filter (`Svf`) used here also exposes
//! resonance (`res`), which boosts energy right at the cutoff frequency —
//! more resonance emphasizes that edge harmonic, giving the classic
//! "squelchy" synth-filter character, and at very high resonance the filter
//! can self-oscillate into a pure sine at the cutoff frequency.
//!
//! Run with: cargo run --example tutorial_subtractive

use quiver::prelude::*;
use quiver::render::write_wav;
use std::path::Path;

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    // The oscillator: source of harmonics
    let vco = patch.add("vco", Vco::new(sample_rate));

    // The filter: subtracts harmonics
    let vcf = patch.add("vcf", Svf::new(sample_rate));

    // Output stage
    let output = patch.add("output", StereoOutput::new());

    // Offset module to set filter cutoff (in CV range).
    // Why: the Svf maps its cutoff CV exponentially over its 0-1 input range
    // (20Hz .. ~20kHz), so a fixed voltage picks a fixed brightness rather
    // than a fixed Hz value — this mirrors how a hardware VCF's cutoff knob
    // works. 0.35 lands the cutoff a couple of harmonics above the C4
    // fundamental (~261Hz), audibly darkening the sawtooth without muting it.
    let cutoff = patch.add("cutoff", Offset::new(0.35));

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
    raw_patch
        .connect(raw_vco.out("saw"), raw_out.in_("left"))
        .unwrap();
    raw_patch.set_output(raw_out.id());
    raw_patch.compile().unwrap();

    let mut raw_samples: Vec<f64> = Vec::new();
    for _ in 0..period_samples * 10 {
        let (left, _) = raw_patch.tick();
        raw_samples.push(left);
    }

    let raw_peak = raw_samples.iter().map(|s| s.abs()).fold(0.0_f64, f64::max);
    let raw_rms =
        (raw_samples.iter().map(|s| s * s).sum::<f64>() / raw_samples.len() as f64).sqrt();

    println!("\nRaw Sawtooth (unfiltered)");
    println!("  Peak amplitude: {:.2}V", raw_peak);
    println!("  RMS level: {:.2}V", raw_rms);

    println!("\nThe filter has smoothed the waveform by removing high harmonics.");
    println!("Notice the lower RMS - less high-frequency energy means a softer sound.");

    // --- Hear it! ---
    // Render a couple more seconds from the same (already-compiled) filtered
    // patch to a real .wav file. Quiver's Audio ports are +-5V; WAV files are
    // full-scale +-1.0, so we scale down before writing (see the
    // `# Sample scale` note on `quiver::render`).
    let (wav_left, wav_right) = render(&mut patch, 2.0);
    let to_full_scale = |buf: &[f64]| -> Vec<f64> { buf.iter().map(|s| s / 5.0).collect() };
    let wav_path = Path::new("target/tutorial_subtractive.wav");
    write_wav(
        wav_path,
        sample_rate as u32,
        &to_full_scale(&wav_left),
        &to_full_scale(&wav_right),
    )
    .expect("failed to write WAV file");
    println!(
        "\nWrote {} - play it to hear the filtered sawtooth!",
        wav_path.display()
    );
}
