//! How-To: Plugin Integration
//!
//! Demonstrates using Quiver patches with plugin and Web Audio wrappers.
//!
//! Run with: cargo run --example howto_plugin

use quiver::prelude::*;

fn main() {
    println!("=== Plugin Integration Demo ===\n");

    // Define plugin metadata using the actual PluginInfo structure
    let info = PluginInfo {
        id: "com.quiver.synth".to_string(),
        name: "QuiverSynth".to_string(),
        vendor: "Quiver Audio".to_string(),
        version: "1.0.0".to_string(),
        category: PluginCategory::Instrument,
        is_synth: true,
        sample_rates: vec![44100.0, 48000.0, 96000.0],
        max_block_size: 512,
        latency: 0,
    };

    println!("Plugin Info:");
    println!("  ID: {}", info.id);
    println!("  Name: {}", info.name);
    println!("  Vendor: {}", info.vendor);
    println!("  Version: {}", info.version);
    println!("  Category: {:?}", info.category);
    println!("  Is Synth: {}", info.is_synth);
    println!("  Supported sample rates: {:?}", info.sample_rates);

    // Define parameters that would be exposed to the DAW
    let parameters = vec![
        PluginParameter {
            id: 0,
            name: "Cutoff".to_string(),
            short_name: "Cut".to_string(),
            default: 0.5,
            min: 0.0,
            max: 1.0,
            unit: "%".to_string(),
            steps: 0, // Continuous
        },
        PluginParameter {
            id: 1,
            name: "Resonance".to_string(),
            short_name: "Res".to_string(),
            default: 0.25,
            min: 0.0,
            max: 1.0,
            unit: "%".to_string(),
            steps: 0,
        },
        PluginParameter {
            id: 2,
            name: "Attack".to_string(),
            short_name: "Atk".to_string(),
            default: 0.01,
            min: 0.001,
            max: 2.0,
            unit: "s".to_string(),
            steps: 0,
        },
        PluginParameter {
            id: 3,
            name: "Release".to_string(),
            short_name: "Rel".to_string(),
            default: 0.3,
            min: 0.01,
            max: 5.0,
            unit: "s".to_string(),
            steps: 0,
        },
    ];

    println!("\nExposed Parameters:");
    for param in &parameters {
        println!(
            "  {} ({}): {} - {} {} (default: {})",
            param.name, param.short_name, param.min, param.max, param.unit, param.default
        );
    }

    // Define audio bus configuration
    let bus_config = AudioBusConfig {
        inputs: 0,
        outputs: 2,
        name: "Main".to_string(),
    };

    // Create the plugin wrapper
    let sample_rate = 44100.0;
    let _wrapper = PluginWrapper::new(info, bus_config.clone());

    println!("\nPlugin wrapper created.");

    // Demonstrate Web Audio configuration
    println!("\n--- Web Audio Configuration ---\n");

    let web_config = WebAudioConfig {
        input_channels: 0,
        output_channels: 2,
        sample_rate: 44100.0,
        block_size: 128,
    };

    println!("WebAudioConfig:");
    println!("  Input Channels: {}", web_config.input_channels);
    println!("  Output Channels: {}", web_config.output_channels);
    println!("  Sample Rate: {} Hz", web_config.sample_rate);
    println!("  Block Size: {} samples", web_config.block_size);

    // Create a patch for the web audio processor
    let mut patch = Patch::new(sample_rate);

    let vco = patch.add("vco", Vco::new(sample_rate));
    let vcf = patch.add("vcf", Svf::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(vcf.out("lp"), output.in_("left")).unwrap();
    patch.connect(vcf.out("lp"), output.in_("right")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    // Process audio using the patch directly
    // (In a real web audio scenario, you'd implement WebAudioProcessor trait)
    let mut left_out = vec![0.0_f32; 128];
    let mut right_out = vec![0.0_f32; 128];

    // Simulate processing a block (like in AudioWorklet)
    for i in 0..128 {
        let (l, r) = patch.tick();
        left_out[i] = l as f32;
        right_out[i] = r as f32;
    }

    let peak_l = left_out.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
    let peak_r = right_out.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);

    println!("\nProcessed 128-sample block:");
    println!("  Left peak: {:.3}V", peak_l);
    println!("  Right peak: {:.3}V", peak_r);
    println!("\n  (WebAudioProcessor is a trait - implement it for your wrapper type)");

    // Demonstrate OSC control setup
    println!("\n--- OSC Control Setup ---\n");

    // Create atomic values that can be controlled via OSC
    use quiver::AtomicF64;
    use std::sync::Arc;

    let cutoff_value = Arc::new(AtomicF64::new(5.0));
    let resonance_value = Arc::new(AtomicF64::new(0.25));

    // Create OSC bindings with pattern matching and scaling
    let cutoff_binding =
        OscBinding::new("/synth/cutoff", Arc::clone(&cutoff_value)).with_scale(10.0); // Scale 0-1 to 0-10V

    let resonance_binding = OscBinding::new("/synth/resonance", Arc::clone(&resonance_value));

    println!("OSC Bindings configured:");
    println!("  /synth/cutoff -> cutoff CV (scaled 0-10V)");
    println!("  /synth/resonance -> resonance CV");

    // Demonstrate OSC message parsing
    let msg = OscMessage {
        address: "/synth/cutoff".to_string(),
        args: vec![OscValue::Float(0.75)],
    };

    println!("\nExample OSC message:");
    println!("  Address: {}", msg.address);
    println!("  Value: {:?}", msg.args);

    // Pattern matching
    let pattern = OscPattern::new("/synth/*");
    println!(
        "\nPattern '{}' matches '{}': {}",
        "/synth/*",
        msg.address,
        pattern.matches(&msg.address)
    );

    // Show current values
    println!("\nCurrent atomic values:");
    println!("  Cutoff: {:.2}V", cutoff_value.get());
    println!("  Resonance: {:.2}", resonance_value.get());

    // Demonstrate the bindings are ready for use
    let _ = (cutoff_binding, resonance_binding);

    println!("\n--- Audio Bus Configuration ---\n");

    let sidechain_config = AudioBusConfig {
        inputs: 2,
        outputs: 0,
        name: "Sidechain".to_string(),
    };

    println!("Main Bus Configuration:");
    println!("  Inputs: {}", bus_config.inputs);
    println!("  Outputs: {}", bus_config.outputs);
    println!("  Name: {}", bus_config.name);

    println!("\nSidechain Bus Configuration:");
    println!("  Inputs: {}", sidechain_config.inputs);
    println!("  Outputs: {}", sidechain_config.outputs);
    println!("  Name: {}", sidechain_config.name);

    println!("\nPlugin integration demo complete.");
    println!("\nTo build a real plugin:");
    println!("  1. Use a framework like vst-rs, nih-plug, or clap-rs");
    println!("  2. Wrap Quiver's PluginWrapper in the framework's traits");
    println!("  3. Build as a shared library (.vst3, .component, .clap)");
}
