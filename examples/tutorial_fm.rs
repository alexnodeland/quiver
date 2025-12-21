//! Tutorial: FM Synthesis Basics
//!
//! Frequency Modulation synthesis using two oscillators.
//! The modulator's output modulates the carrier's frequency,
//! creating rich, complex timbres from simple sine waves.
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

    // FM connection: modulator → carrier's FM input
    patch
        .connect(modulator.out("sin"), mod_depth.in_("in"))
        .unwrap();
    patch
        .connect(mod_depth.out("out"), carrier.in_("fm"))
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

    // Test different modulation indices
    for (name, ratio, depth) in [
        ("Pure carrier (no FM)", 1.0, 0.0),
        ("Subtle FM (index ~1)", 1.0, 0.5),
        ("Medium FM (index ~3)", 1.0, 1.5),
        ("Heavy FM (index ~5)", 1.0, 2.5),
        ("Bell (1:√2 ratio)", 1.414, 2.0),
        ("Metallic (1:3.5 ratio)", 3.5, 2.0),
    ] {
        // Reset and reconfigure
        let mut test_patch = Patch::new(sample_rate);

        let carrier = test_patch.add("carrier", Vco::new(sample_rate));
        let modulator = test_patch.add("modulator", Vco::new(sample_rate));
        let mod_depth_node = test_patch.add("mod_depth", Attenuverter::new());
        let ratio_mult = test_patch.add("ratio", Attenuverter::new()); // Scale modulator pitch
        let output = test_patch.add("output", StereoOutput::new());

        // Set up FM with the given parameters
        test_patch
            .connect(modulator.out("sin"), mod_depth_node.in_("in"))
            .unwrap();
        test_patch
            .connect(mod_depth_node.out("out"), carrier.in_("fm"))
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

        println!("{}", name);
        println!("  C:M ratio = 1:{:.3}, mod depth = {:.1}", ratio, depth);
        println!("  Peak: {:.2}V, Zero-crossing rate: {:.0} Hz", peak, zcr);
        println!();
    }

    println!("FM synthesis creates complex timbres from simple oscillators.");
    println!("The carrier:modulator ratio determines harmonic vs inharmonic sound.");
    println!("The modulation index (depth) controls brightness and complexity.");
}
