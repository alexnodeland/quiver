//! How-To: Visualize Your Patch
//!
//! Demonstrates patch visualization including DOT export,
//! signal analysis, and metering.
//!
//! Run with: cargo run --example howto_visualization

use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;

    println!("=== Patch Visualization Demo ===\n");

    // Build a patch to visualize
    let mut patch = Patch::new(sample_rate);

    let vco = patch.add("vco", Vco::new(sample_rate));
    let lfo = patch.add("lfo", Lfo::new(sample_rate));
    let vcf = patch.add("vcf", Svf::new(sample_rate));
    let vca = patch.add("vca", Vca::new());
    let env = patch.add("env", Adsr::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    // Connections
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(lfo.out("sin"), vcf.in_("fm")).unwrap();
    patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
    patch.connect(env.out("env"), vcf.in_("cutoff")).unwrap();
    patch.connect(env.out("env"), vca.in_("cv")).unwrap();
    patch.connect(vca.out("out"), output.in_("left")).unwrap();
    patch.connect(vca.out("out"), output.in_("right")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    // Generate DOT visualization
    println!("--- DOT Graph Output ---");
    println!("(Save this to a .dot file and render with GraphViz)\n");

    let exporter = DotExporter::new(&patch);
    let dot = exporter.to_dot();
    println!("{}", dot);

    // Generate audio for analysis
    println!("\n--- Signal Analysis ---\n");

    // Collect samples
    let num_samples = (sample_rate * 0.5) as usize;
    let mut samples = Vec::with_capacity(num_samples);

    for _ in 0..num_samples {
        let (left, _) = patch.tick();
        samples.push(left);
    }

    // Basic statistics
    let peak = samples.iter().map(|s| s.abs()).fold(0.0_f64, f64::max);
    let rms = (samples.iter().map(|s| s * s).sum::<f64>() / num_samples as f64).sqrt();
    let dc_offset = samples.iter().sum::<f64>() / num_samples as f64;

    println!("Sample Statistics:");
    println!("  Samples: {}", num_samples);
    println!(
        "  Peak: {:.3}V ({:.1} dB)",
        peak,
        20.0 * (peak / 5.0).log10()
    );
    println!("  RMS: {:.3}V ({:.1} dB)", rms, 20.0 * (rms / 5.0).log10());
    println!("  DC Offset: {:.6}V", dc_offset);

    // Estimate frequency via zero crossings
    let mut zero_crossings = 0;
    for i in 1..samples.len() {
        if (samples[i] >= 0.0) != (samples[i - 1] >= 0.0) {
            zero_crossings += 1;
        }
    }
    let estimated_freq = zero_crossings as f64 / 2.0 / (num_samples as f64 / sample_rate);
    println!("  Estimated Frequency: {:.1} Hz", estimated_freq);

    // ASCII waveform visualization
    println!("\n--- Waveform (ASCII) ---\n");

    let display_samples = 80; // Characters wide
    let step = samples.len() / display_samples;

    for row in (0..11).rev() {
        let threshold = (row as f64 - 5.0) / 5.0 * peak;
        let mut line = String::new();

        for col in 0..display_samples {
            let sample = samples[col * step];
            if (sample >= threshold && row > 5) || (sample <= threshold && row < 5) {
                line.push('█');
            } else if row == 5 {
                line.push('─');
            } else {
                line.push(' ');
            }
        }

        let label = match row {
            10 => "+peak",
            5 => "  0V ",
            0 => "-peak",
            _ => "     ",
        };

        println!("{} |{}", label, line);
    }

    // Using the Scope module
    println!("\n--- Scope Analysis ---\n");

    let mut scope = Scope::new(sample_rate);

    // Recreate patch for fresh samples
    patch.compile().unwrap();

    // Fill scope buffer
    for _ in 0..1024 {
        let (left, _) = patch.tick();
        scope.process(&PortValues::new(), &mut PortValues::new());
    }

    let buffer = scope.buffer();
    println!("Scope buffer size: {} samples", buffer.len());

    // Using LevelMeter
    println!("\n--- Level Meter ---\n");

    let mut meter = LevelMeter::new(sample_rate);

    for _ in 0..(sample_rate * 0.1) as usize {
        let (left, _) = patch.tick();
        let mut inputs = PortValues::new();
        inputs.set("in", left);
        meter.process(&inputs, &mut PortValues::new());
    }

    println!("Level Meter:");
    println!("  RMS Level: {:.2}V", meter.rms());
    println!("  Peak Level: {:.2}V", meter.peak());

    // Module graph summary
    println!("\n--- Patch Summary ---\n");
    println!("Modules: {}", patch.node_count());
    println!("Cables: {}", patch.cable_count());
    println!("\nTo visualize graphically:");
    println!("  1. Save the DOT output above to 'patch.dot'");
    println!("  2. Run: dot -Tpng patch.dot -o patch.png");
    println!("  3. Open patch.png in an image viewer");
}
