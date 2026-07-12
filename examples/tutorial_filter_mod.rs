//! Tutorial: Filter Modulation
//!
//! Demonstrates LFO modulation of filter cutoff - the classic "wobble"
//! that brings patches to life.
//!
//! # Why modulating the cutoff creates movement
//!
//! A static filter cutoff makes a static timbre — useful, but lifeless.
//! Driving the cutoff with a Low-Frequency Oscillator (an LFO: an
//! oscillator running well below audible range, here a fraction of a Hz to
//! a few Hz) continuously changes *which harmonics survive* the lowpass,
//! so the same sawtooth cycles between dark and bright without anyone
//! touching a knob. This differs from an audio-rate oscillator only in
//! frequency, not in kind — Quiver's `Lfo` and `Vco` share the same
//! waveform shapes for exactly this reason. The LFO's *waveform* changes
//! the character of the sweep: a sine gives a smooth, natural swell; a
//! triangle gives a linear ramp; a square gives an instant on/off "gate"
//! effect instead of a sweep at all. The cutoff itself still follows the
//! `Svf`'s exponential CV-to-Hz mapping (see `tutorial_subtractive.rs`), so
//! equal LFO excursions produce equal *musical* (octave) jumps in cutoff,
//! not equal Hz jumps.
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

    // Base cutoff offset: the cutoff CV's useful range is 0-1 (see
    // tutorial_subtractive.rs), so 0.5 centers it at a medium brightness the
    // LFO can swing both up and down from.
    let cutoff_base = patch.add("cutoff_base", Offset::new(0.5));

    // Why an Attenuverter here: the LFO's `sin` output swings a full +-5V
    // (audio-signal scale), but the cutoff CV only usefully spans 0-1V. Fed
    // in raw, the sum would spend almost the whole cycle pinned at one
    // extreme (fully open or fully closed) instead of sweeping smoothly.
    // Scaling it down to +-0.5V keeps `cutoff_base +- lfo` inside [0, 1] for
    // the whole cycle, so the sweep is continuous rather than a hard switch.
    let lfo_depth = patch.add("lfo_depth", Attenuverter::new());
    // Attenuverter gain = level / 5V, so level = 0.5 gives gain = 0.1,
    // turning the +-5V LFO into a +-0.5V cutoff excursion.
    let lfo_depth_cv = patch.add("lfo_depth_cv", Offset::new(0.5));

    // Output
    let output = patch.add("output", StereoOutput::new());

    // Audio path: VCO → Filter → Output
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(vcf.out("lp"), output.in_("left")).unwrap();

    // Modulation: LFO → attenuator → Filter cutoff (with base offset)
    patch
        .connect(cutoff_base.out("out"), vcf.in_("cutoff"))
        .unwrap();
    patch
        .connect(lfo_depth_cv.out("out"), lfo_depth.in_("level"))
        .unwrap();
    patch.connect(lfo.out("sin"), lfo_depth.in_("in")).unwrap();
    patch.connect(lfo_depth.out("out"), vcf.in_("fm")).unwrap();

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

        // Describe the sound character based on peak. Peak amplitude is a
        // rough but effective proxy for brightness here: a sawtooth's energy
        // is concentrated in its lower harmonics, so cutting it down
        // (closing the filter) shaves off amplitude along with treble.
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
