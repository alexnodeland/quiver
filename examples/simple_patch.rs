//! Simple Patch Example
//!
//! This example demonstrates building a basic synthesizer patch:
//! VCO -> VCF -> VCA -> Output
//!
//! Run with: cargo run --example simple_patch

use quiver::prelude::*;
use std::sync::Arc;

fn main() {
    let sample_rate = 44100.0;

    // Create a new patch
    let mut patch = Patch::new(sample_rate);

    // Create external inputs for MIDI control
    let pitch_value = Arc::new(AtomicF64::new(0.0)); // C4
    let gate_value = Arc::new(AtomicF64::new(0.0));

    // Add modules
    let pitch_in = patch.add("pitch_in", ExternalInput::voct(Arc::clone(&pitch_value)));
    let gate_in = patch.add("gate_in", ExternalInput::gate(Arc::clone(&gate_value)));

    let vco = patch.add("vco", Vco::new(sample_rate));
    let vcf = patch.add("vcf", Svf::new(sample_rate));
    let vca = patch.add("vca", Vca::new());
    let env = patch.add("env", Adsr::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    // Make connections
    // Pitch -> VCO
    patch.connect(pitch_in.out("out"), vco.in_("voct")).unwrap();

    // Gate -> Envelope
    patch.connect(gate_in.out("out"), env.in_("gate")).unwrap();

    // VCO -> VCF -> VCA -> Output
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
    patch.connect(vca.out("out"), output.in_("left")).unwrap();

    // Envelope -> VCF cutoff and VCA
    patch.connect(env.out("env"), vcf.in_("cutoff")).unwrap();
    patch.connect(env.out("env"), vca.in_("cv")).unwrap();

    // Set output node and compile
    patch.set_output(output.id());
    patch.compile().unwrap();

    println!("Patch created with {} modules and {} cables",
        patch.node_count(),
        patch.cable_count()
    );

    // Simulate playing a note
    println!("\nPlaying C4...");
    pitch_value.set(0.0); // C4
    gate_value.set(5.0);   // Gate on

    // Generate 0.5 seconds of audio
    let samples = (sample_rate * 0.5) as usize;
    let mut max_level = 0.0_f64;

    for _ in 0..samples {
        let (left, _right) = patch.tick();
        max_level = max_level.max(left.abs());
    }

    println!("Max level during attack: {:.2}V", max_level);

    // Release
    println!("\nReleasing note...");
    gate_value.set(0.0);

    for _ in 0..samples {
        let (left, _right) = patch.tick();
        max_level = max_level.max(left.abs());
    }

    // Serialize the patch
    let def = patch.to_def("Simple Synth");
    let json = def.to_json().unwrap();
    println!("\nPatch definition:\n{}", json);

    // Note: ExternalInput modules can't be reloaded from JSON since they require
    // Arc<AtomicF64> values that are created at runtime. For full serialization,
    // use only modules from the registry.
    println!("\nNote: This patch uses ExternalInput modules which require runtime Arc values.");
    println!("For patches that can be fully serialized/deserialized, use only registry modules.");

    // Demonstrate loading a patch without ExternalInput
    let reload_def = PatchDef::from_json(r#"{
        "version": 1,
        "name": "Simple Test",
        "author": null,
        "description": null,
        "tags": [],
        "modules": [
            {"name": "vco", "module_type": "vco", "position": null, "state": null},
            {"name": "vcf", "module_type": "svf", "position": null, "state": null},
            {"name": "output", "module_type": "stereo_output", "position": null, "state": null}
        ],
        "cables": [
            {"from": "vco.saw", "to": "vcf.in", "attenuation": null},
            {"from": "vcf.lp", "to": "output.left", "attenuation": null}
        ],
        "parameters": {}
    }"#).unwrap();

    let registry = ModuleRegistry::new();
    let _reloaded = Patch::from_def(&reload_def, &registry, sample_rate)
        .expect("Failed to reload patch");
    println!("Registry-only patch successfully serialized and reloaded!");
}
