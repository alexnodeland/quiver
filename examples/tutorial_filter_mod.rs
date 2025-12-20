//! Tutorial: Filter Modulation
//!
//! Demonstrates LFO modulation of filter cutoff - the classic "wobble"
//! that brings patches to life.
//!
//! Run with: cargo run --example tutorial_filter_mod

use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    // Sound source - sawtooth oscillator
    let vco = patch.add("vco", Vco::new(sample_rate));

    // LFO for modulation (runs at sub-audio rate)
    let lfo = patch.add("lfo", Lfo::new(sample_rate));

    // Filter - we'll modulate its cutoff
    let vcf = patch.add("vcf", Svf::new(sample_rate));

    // Base cutoff offset
    let cutoff_base = patch.add("cutoff_base", Offset::new(3.0));

    // Output
    let output = patch.add("output", StereoOutput::new());

    // Audio path: VCO → Filter → Output
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(vcf.out("lp"), output.in_("left")).unwrap();

    // Modulation: LFO → Filter cutoff (with base offset)
    patch.connect(cutoff_base.out("out"), vcf.in_("cutoff")).unwrap();
    patch.connect(lfo.out("sin"), vcf.in_("fm")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    println!("=== Filter Modulation Demo ===\n");
    println!("LFO modulating filter cutoff creates the classic 'wobble' effect.\n");

    // Generate 2 seconds of audio to hear multiple LFO cycles
    let duration = 2.0;
    let total_samples = (sample_rate * duration) as usize;

    // Track the signal envelope over time
    let block_size = (sample_rate / 10.0) as usize; // 100ms blocks
    let mut time = 0.0;

    println!("Time(s)  | Peak Level | Character");
    println!("---------|------------|----------");

    for block in 0..(total_samples / block_size) {
        let mut peak = 0.0_f64;

        for _ in 0..block_size {
            let (left, _) = patch.tick();
            peak = peak.max(left.abs());
        }

        // Describe the sound character based on peak
        let character = if peak > 4.0 {
            "Bright (filter open)"
        } else if peak > 2.0 {
            "Medium"
        } else {
            "Dark (filter closed)"
        };

        if block % 5 == 0 {
            println!("{:7.2}  | {:10.2}V | {}", time, peak, character);
        }

        time += block_size as f64 / sample_rate;
    }

    println!("\nThe LFO creates a periodic sweep of the filter,");
    println!("cycling between bright (open) and dark (closed) states.");
    println!("\nTry different LFO waveforms:");
    println!("  - sin: smooth, natural sweep");
    println!("  - tri: linear ramp up and down");
    println!("  - saw: slow rise, fast drop");
    println!("  - sqr: instant toggle between states");
}
