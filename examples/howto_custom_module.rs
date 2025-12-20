//! How-To: Create Custom Modules
//!
//! Demonstrates building a custom DSP module using the GraphModule trait.
//! This example creates a bit crusher effect.
//!
//! Run with: cargo run --example howto_custom_module

use quiver::prelude::*;

/// A bit crusher effect that reduces sample resolution and rate.
///
/// # Ports
///
/// ## Inputs
/// - `in`: Audio input (±5V)
/// - `bits`: Bit depth reduction (1-16 bits via 0-10V CV)
/// - `rate`: Sample rate reduction factor (1-64x via 0-10V CV)
///
/// ## Outputs
/// - `out`: Crushed audio output (±5V)
pub struct BitCrusher {
    sample_rate: f64,
    hold_sample: f64,
    hold_counter: f64,
}

impl BitCrusher {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            hold_sample: 0.0,
            hold_counter: 0.0,
        }
    }
}

impl GraphModule for BitCrusher {
    fn port_spec(&self) -> PortSpec {
        PortSpec::new()
            // Audio input
            .with_input("in", PortDef::audio())
            // Bit depth: 0V = 16 bits (clean), 10V = 1 bit (extreme)
            .with_input("bits", PortDef::cv_unipolar().with_default(0.0))
            // Rate reduction: 0V = 1x (clean), 10V = 64x reduction
            .with_input("rate", PortDef::cv_unipolar().with_default(0.0))
            // Audio output
            .with_output("out", PortDef::audio())
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get("in");
        let bits_cv = inputs.get("bits").clamp(0.0, 10.0);
        let rate_cv = inputs.get("rate").clamp(0.0, 10.0);

        // Convert CV to parameters
        // bits_cv: 0V = 16 bits, 10V = 1 bit
        let bits = 16.0 - (bits_cv / 10.0 * 15.0);
        let levels = 2.0_f64.powf(bits);

        // rate_cv: 0V = 1x, 10V = 64x reduction
        let rate_reduction = 1.0 + (rate_cv / 10.0 * 63.0);

        // Sample rate reduction (sample & hold)
        self.hold_counter += 1.0;
        if self.hold_counter >= rate_reduction {
            self.hold_counter = 0.0;
            self.hold_sample = input;
        }

        // Bit depth reduction (quantization)
        // Normalize to 0-1, quantize, scale back
        let normalized = (self.hold_sample + 5.0) / 10.0;  // 0 to 1
        let quantized = (normalized * levels).round() / levels;
        let output = quantized * 10.0 - 5.0;  // Back to ±5V

        outputs.set("out", output);
    }

    fn reset(&mut self) {
        self.hold_sample = 0.0;
        self.hold_counter = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }
}

fn main() {
    let sample_rate = 44100.0;

    println!("=== Custom Module Demo: BitCrusher ===\n");

    // Create a patch with our custom module
    let mut patch = Patch::new(sample_rate);

    let vco = patch.add("vco", Vco::new(sample_rate));
    let crusher = patch.add("crusher", BitCrusher::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    // CV control for the effect
    let bits_cv = patch.add("bits_cv", Offset::new(0.0));  // Start clean
    let rate_cv = patch.add("rate_cv", Offset::new(0.0));

    // Connections
    patch.connect(vco.out("sin"), crusher.in_("in")).unwrap();
    patch.connect(bits_cv.out("out"), crusher.in_("bits")).unwrap();
    patch.connect(rate_cv.out("out"), crusher.in_("rate")).unwrap();
    patch.connect(crusher.out("out"), output.in_("left")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    // Test at different settings
    println!("Testing BitCrusher at various settings:\n");

    // We'll simulate different CV values by creating new patches
    for (bits_v, rate_v, desc) in [
        (0.0, 0.0, "Clean (16-bit, no rate reduction)"),
        (5.0, 0.0, "8-bit, full rate"),
        (8.0, 0.0, "4-bit, full rate"),
        (0.0, 5.0, "16-bit, 32x rate reduction"),
        (7.0, 5.0, "Lo-fi (5-bit, 32x reduction)"),
        (9.0, 8.0, "Extreme (2-bit, 50x reduction)"),
    ] {
        let mut test_patch = Patch::new(sample_rate);

        let vco = test_patch.add("vco", Vco::new(sample_rate));
        let crusher = test_patch.add("crusher", BitCrusher::new(sample_rate));
        let bits = test_patch.add("bits", Offset::new(bits_v));
        let rate = test_patch.add("rate", Offset::new(rate_v));
        let output = test_patch.add("output", StereoOutput::new());

        test_patch.connect(vco.out("sin"), crusher.in_("in")).unwrap();
        test_patch.connect(bits.out("out"), crusher.in_("bits")).unwrap();
        test_patch.connect(rate.out("out"), crusher.in_("rate")).unwrap();
        test_patch.connect(crusher.out("out"), output.in_("left")).unwrap();

        test_patch.set_output(output.id());
        test_patch.compile().unwrap();

        // Generate samples and analyze
        let num_samples = (sample_rate * 0.1) as usize;
        let mut samples = Vec::with_capacity(num_samples);

        for _ in 0..num_samples {
            let (left, _) = test_patch.tick();
            samples.push(left);
        }

        let peak = samples.iter().map(|s| s.abs()).fold(0.0_f64, f64::max);
        let rms = (samples.iter().map(|s| s * s).sum::<f64>() / num_samples as f64).sqrt();

        // Count unique values (rough measure of bit reduction)
        let mut unique: Vec<i32> = samples.iter()
            .map(|s| (s * 1000.0) as i32)
            .collect();
        unique.sort();
        unique.dedup();

        println!("{}", desc);
        println!("  Bits CV: {:.1}V, Rate CV: {:.1}V", bits_v, rate_v);
        println!("  Peak: {:.2}V, RMS: {:.2}V, Unique levels: {}\n",
                 peak, rms, unique.len());
    }

    // Show the port specification
    let module = BitCrusher::new(sample_rate);
    let spec = module.port_spec();

    println!("--- Port Specification ---");
    println!("Inputs:");
    for (name, def) in &spec.inputs {
        println!("  {}: {:?}, default={:.1}V", name, def.kind, def.default);
    }
    println!("Outputs:");
    for (name, def) in &spec.outputs {
        println!("  {}: {:?}", name, def.kind);
    }
}
