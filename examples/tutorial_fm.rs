//! Tutorial: FM Synthesis Basics
//!
//! Frequency Modulation synthesis using two oscillators.
//! The modulator's output modulates the carrier's frequency,
//! creating rich, complex timbres from simple sine waves.
//!
//! # Why FM sounds the way it does
//!
//! Modulating a carrier's instantaneous frequency with another oscillator
//! (the modulator) doesn't just add one extra pitch — it generates a whole
//! family of new frequencies called *sidebands*, symmetric around the
//! carrier: `fc ± n * fm` for every integer `n = 1, 2, 3, ...`, where `fc`
//! is the carrier frequency and `fm` is the modulator frequency. How many of
//! those sidebands carry audible energy (and how loud each one is) is
//! governed by the **modulation index**, `I = deviation / fm` — the peak
//! frequency deviation the modulator pushes the carrier through, divided by
//! the modulator's own frequency. `I = 0` is just the bare carrier; as `I`
//! grows, energy spreads into more and more sidebands (their amplitudes
//! follow Bessel functions `J_n(I)`), which is why increasing the
//! modulation index alone brightens/thickens a tone even with a fixed
//! carrier:modulator ratio.
//!
//! This example uses `fm_lin`, the VCO's *linear*, through-zero FM input,
//! specifically because linear FM is what produces the textbook sidebands
//! above — the alternative `fm` input is *exponential* (each volt is an
//! octave, good for vibrato/pitch bends) and does not follow the same
//! sideband math. The carrier:modulator frequency ratio determines whether
//! those sidebands land back on harmonics of the carrier (integer ratios,
//! e.g. 1:2, 1:3 — sounds "musical"/harmonic) or land in between them
//! (irrational/non-integer ratios, e.g. 1:1.414 — sounds bell-like or
//! metallic/inharmonic).
//!
//! Run with: cargo run --example tutorial_fm

use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    // Carrier oscillator - this is what we hear
    let carrier = patch.add("carrier", Vco::new(sample_rate));

    // Modulator oscillator - this modulates the carrier's frequency
    let modulator = patch.add("modulator", Vco::new(sample_rate));

    // Modulation index control (depth of FM effect)
    let mod_depth = patch.add("mod_depth", Attenuverter::new());

    // Output
    let output = patch.add("output", StereoOutput::new());

    // FM connection: modulator -> carrier's *linear* FM input (`fm_lin`).
    // Why `fm_lin` and not `fm`: only the linear input adds a frequency
    // deviation directly (`freq += (fm_lin / 5) * base_freq`), which is what
    // produces the `fc +- n*fm` sidebands described above. The exponential
    // `fm` input multiplies frequency instead, which is the right shape for
    // vibrato but doesn't follow the same sideband formula.
    patch
        .connect(modulator.out("sin"), mod_depth.in_("in"))
        .unwrap();
    patch
        .connect(mod_depth.out("out"), carrier.in_("fm_lin"))
        .unwrap();

    // Carrier to output (using sine for pure FM demonstration)
    patch
        .connect(carrier.out("sin"), output.in_("left"))
        .unwrap();
    patch
        .connect(carrier.out("sin"), output.in_("right"))
        .unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    println!("=== FM Synthesis Demo ===\n");
    println!("Two oscillators: Carrier (audible) + Modulator (creates harmonics)\n");

    // Generate samples at different modulation depths
    let samples_per_test = (sample_rate * 0.5) as usize;

    // Test different (carrier:modulator ratio, modulation depth) pairs. The
    // modulation index I = depth / ratio (see the doc comment above), so the
    // printed value is computed, not guessed.
    for (name, ratio, depth) in [
        ("Pure carrier (no FM)", 1.0_f64, 0.0_f64),
        ("Subtle FM", 1.0, 0.5),
        ("Medium FM", 1.0, 1.5),
        ("Heavy FM", 1.0, 3.0),
        ("Bell (1:sqrt(2) ratio)", 1.414, 2.0),
        ("Metallic (1:3.5 ratio)", 3.5, 2.0),
    ] {
        // Reset and reconfigure
        let mut test_patch = Patch::new(sample_rate);

        let carrier = test_patch.add("carrier", Vco::new(sample_rate));
        let modulator = test_patch.add("modulator", Vco::new(sample_rate));
        let mod_depth_node = test_patch.add("mod_depth", Attenuverter::new());
        // Sets the modulator's pitch to `ratio` times the carrier's: V/Oct is
        // logarithmic (1V = 1 octave = 2x frequency), so a frequency ratio
        // becomes a voltage offset of log2(ratio).
        let mod_ratio_cv = test_patch.add("mod_ratio_cv", Offset::new(ratio.log2()));
        // Attenuverter gain = level / 5V (see Attenuverter's doc), so driving
        // `level` with `depth * 5.0` makes the attenuverter's gain equal
        // `depth` directly.
        let depth_cv = test_patch.add("depth_cv", Offset::new(depth * 5.0));
        let output = test_patch.add("output", StereoOutput::new());

        // Set up FM with the given parameters
        test_patch
            .connect(mod_ratio_cv.out("out"), modulator.in_("voct"))
            .unwrap();
        test_patch
            .connect(depth_cv.out("out"), mod_depth_node.in_("level"))
            .unwrap();
        test_patch
            .connect(modulator.out("sin"), mod_depth_node.in_("in"))
            .unwrap();
        test_patch
            .connect(mod_depth_node.out("out"), carrier.in_("fm_lin"))
            .unwrap();
        test_patch
            .connect(carrier.out("sin"), output.in_("left"))
            .unwrap();

        test_patch.set_output(output.id());
        test_patch.compile().unwrap();

        // Generate samples
        let mut peak = 0.0_f64;
        let mut zero_crossings = 0;
        let mut last_sign = 0.0_f64;

        for i in 0..samples_per_test {
            let (left, _) = test_patch.tick();
            peak = peak.max(left.abs());

            // Count zero crossings (rough measure of harmonic content)
            if i > 0 {
                let current_sign = if left >= 0.0 { 1.0 } else { -1.0 };
                if current_sign != last_sign {
                    zero_crossings += 1;
                }
                last_sign = current_sign;
            }
        }

        // Zero crossing rate indicates harmonic complexity
        let zcr = zero_crossings as f64 / (samples_per_test as f64 / sample_rate);
        // I = deviation / modulator_frequency = depth / ratio (both already
        // expressed relative to the carrier frequency).
        let modulation_index = if ratio > 0.0 { depth / ratio } else { 0.0 };

        println!("{}", name);
        println!(
            "  C:M ratio = 1:{:.3}, modulation index I = {:.2}",
            ratio, modulation_index
        );
        println!("  Peak: {:.2}V, Zero-crossing rate: {:.0} Hz", peak, zcr);
        println!();
    }

    println!("FM synthesis creates complex timbres from simple oscillators.");
    println!("The carrier:modulator ratio determines harmonic vs inharmonic sound.");
    println!("The modulation index (I = deviation/fm) controls brightness and complexity.");
}
