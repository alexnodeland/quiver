//! How-To: Plugin Integration
//!
//! Demonstrates using Quiver patches with plugin and Web Audio wrappers.
//!
//! Run with: cargo run --example howto_plugin

use quiver::prelude::*;

fn main() {
    println!("=== Plugin Integration Demo ===\n");

    // Define plugin metadata
    let info = PluginInfo {
        name: "QuiverSynth".to_string(),
        vendor: "Quiver Audio".to_string(),
        unique_id: 0x51564152, // "QVAR"
        version: (1, 0, 0),
        category: PluginCategory::Instrument,
        num_inputs: 0,
        num_outputs: 2,
        has_editor: false,
    };

    println!("Plugin Info:");
    println!("  Name: {}", info.name);
    println!("  Vendor: {}", info.vendor);
    println!("  ID: 0x{:08X}", info.unique_id);
    println!(
        "  Version: {}.{}.{}",
        info.version.0, info.version.1, info.version.2
    );
    println!("  Category: {:?}", info.category);
    println!("  I/O: {} in, {} out", info.num_inputs, info.num_outputs);

    // Define parameters that would be exposed to the DAW
    let parameters = vec![
        PluginParameter {
            name: "Cutoff".to_string(),
            default: 0.5,
            min: 0.0,
            max: 1.0,
            unit: "%".to_string(),
        },
        PluginParameter {
            name: "Resonance".to_string(),
            default: 0.25,
            min: 0.0,
            max: 1.0,
            unit: "%".to_string(),
        },
        PluginParameter {
            name: "Attack".to_string(),
            default: 0.01,
            min: 0.001,
            max: 2.0,
            unit: "s".to_string(),
        },
        PluginParameter {
            name: "Release".to_string(),
            default: 0.3,
            min: 0.01,
            max: 5.0,
            unit: "s".to_string(),
        },
    ];

    println!("\nExposed Parameters:");
    for param in &parameters {
        println!(
            "  {}: {} - {} {} (default: {})",
            param.name, param.min, param.max, param.unit, param.default
        );
    }

    // Create the wrapper
    let sample_rate = 44100.0;
    let wrapper = PluginWrapper::new(info, sample_rate);

    println!("\nPlugin wrapper created.");

    // Demonstrate Web Audio configuration
    println!("\n--- Web Audio Configuration ---\n");

    let web_config = WebAudioConfig {
        sample_rate: 44100.0,
        channels: 2,
        buffer_size: 128,
    };

    println!("WebAudioConfig:");
    println!("  Sample Rate: {} Hz", web_config.sample_rate);
    println!("  Channels: {}", web_config.channels);
    println!("  Buffer Size: {} samples", web_config.buffer_size);

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

    // Demonstrate WebAudioProcessor
    let processor = WebAudioProcessor::new(patch);

    // Simulate processing a block (like in AudioWorklet)
    let mut left_out = vec![0.0_f32; 128];
    let mut right_out = vec![0.0_f32; 128];

    // In a real scenario, this would be called by the Web Audio thread
    for i in 0..128 {
        let (l, r) = processor.tick();
        left_out[i] = l as f32;
        right_out[i] = r as f32;
    }

    let peak_l = left_out.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
    let peak_r = right_out.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);

    println!("\nProcessed 128-sample block:");
    println!("  Left peak: {:.3}", peak_l);
    println!("  Right peak: {:.3}", peak_r);

    // Demonstrate OSC control setup
    println!("\n--- OSC Control Setup ---\n");

    let bindings = vec![
        OscBinding::new("/synth/cutoff", "vcf.cutoff", 0.0..10.0),
        OscBinding::new("/synth/resonance", "vcf.resonance", 0.0..1.0),
        OscBinding::new("/synth/attack", "env.attack", 0.001..2.0),
    ];

    println!("OSC Bindings:");
    for binding in &bindings {
        println!("  {} -> {}", binding.address(), binding.target());
    }

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

    println!("\n--- Audio Bus Configuration ---\n");

    let bus_config = AudioBusConfig {
        main_inputs: 0,
        main_outputs: 2,
        aux_inputs: vec![("Sidechain".to_string(), 2)],
        aux_outputs: vec![],
    };

    println!("Bus Configuration:");
    println!("  Main inputs: {}", bus_config.main_inputs);
    println!("  Main outputs: {}", bus_config.main_outputs);
    for (name, channels) in &bus_config.aux_inputs {
        println!("  Aux input '{}': {} channels", name, channels);
    }

    println!("\nPlugin integration demo complete.");
    println!("\nTo build a real plugin:");
    println!("  1. Use a framework like vst-rs, nih-plug, or clap-rs");
    println!("  2. Wrap Quiver's PluginWrapper in the framework's traits");
    println!("  3. Build as a shared library (.vst3, .component, .clap)");
}
