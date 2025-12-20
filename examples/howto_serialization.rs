//! How-To: Serialize and Save Patches
//!
//! Demonstrates saving patches to JSON and loading them back.
//! Essential for preset management and patch storage.
//!
//! Run with: cargo run --example howto_serialization

use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;

    println!("=== Patch Serialization Demo ===\n");

    // Build a patch
    let mut patch = Patch::new(sample_rate);

    let vco = patch.add("vco", Vco::new(sample_rate));
    let vcf = patch.add("vcf", Svf::new(sample_rate));
    let vca = patch.add("vca", Vca::new());
    let env = patch.add("env", Adsr::new(sample_rate));
    let lfo = patch.add("lfo", Lfo::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    // Audio path
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
    patch.connect(vca.out("out"), output.in_("left")).unwrap();
    patch.connect(vca.out("out"), output.in_("right")).unwrap();

    // Modulation
    patch.connect(env.out("env"), vcf.in_("cutoff")).unwrap();
    patch.connect(env.out("env"), vca.in_("cv")).unwrap();
    patch.connect(lfo.out("sin"), vcf.in_("fm")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    println!(
        "Original patch: {} modules, {} cables\n",
        patch.node_count(),
        patch.cable_count()
    );

    // Serialize to JSON
    let mut def = patch.to_def("Warm Pad");
    def.author = Some("Quiver Documentation".to_string());
    def.description = Some("A warm pad with LFO filter modulation".to_string());
    def.tags = vec!["pad".into(), "warm".into(), "modulated".into()];

    let json = def.to_json().expect("Serialization failed");

    println!("--- Serialized JSON ---");
    println!("{}\n", json);

    // Deserialize and rebuild
    println!("--- Deserializing ---");
    let loaded_def = PatchDef::from_json(&json).expect("Deserialization failed");

    println!("Loaded patch: {}", loaded_def.name);
    println!("  Author: {:?}", loaded_def.author);
    println!("  Description: {:?}", loaded_def.description);
    println!("  Tags: {:?}", loaded_def.tags);
    println!("  Modules: {}", loaded_def.modules.len());
    println!("  Cables: {}", loaded_def.cables.len());

    // Rebuild the patch using the registry
    let registry = ModuleRegistry::new();
    let mut reloaded_patch =
        Patch::from_def(&loaded_def, &registry, sample_rate).expect("Failed to rebuild patch");

    println!(
        "\nRebuilt patch: {} modules, {} cables",
        reloaded_patch.node_count(),
        reloaded_patch.cable_count()
    );

    // Verify it works by generating audio
    println!("\n--- Testing reloaded patch ---");

    let mut peak = 0.0_f64;
    for _ in 0..(sample_rate * 0.5) as usize {
        let (left, _) = reloaded_patch.tick();
        peak = peak.max(left.abs());
    }

    println!("Generated 0.5s of audio, peak: {:.2}V", peak);
    println!("\nRound-trip serialization successful!");

    // Show available presets using static methods
    println!("\n--- Built-in Presets ---");

    // Get all presets
    let all_presets = PresetLibrary::list();
    println!("\nTotal available presets: {}", all_presets.len());

    // Filter by category using static method
    println!("\nBass presets:");
    for preset in PresetLibrary::by_category(PresetCategory::Bass) {
        let desc = if preset.description.is_empty() {
            "No description"
        } else {
            &preset.description
        };
        println!("  {} - {}", preset.name, desc);
    }

    println!("\nPad presets:");
    for preset in PresetLibrary::by_category(PresetCategory::Pad) {
        let desc = if preset.description.is_empty() {
            "No description"
        } else {
            &preset.description
        };
        println!("  {} - {}", preset.name, desc);
    }

    println!("\nLead presets:");
    for preset in PresetLibrary::by_category(PresetCategory::Lead) {
        let desc = if preset.description.is_empty() {
            "No description"
        } else {
            &preset.description
        };
        println!("  {} - {}", preset.name, desc);
    }
}
