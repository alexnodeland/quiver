# Serialize and Save Patches

Save patches to JSON and reload them—essential for presets and patch management.

## Basic Serialization

Convert a patch to JSON:

```rust,ignore
// Create your patch
let mut patch = Patch::new(44100.0);
// ... add modules and connections ...

// Serialize to PatchDef
let def = patch.to_def("My Awesome Synth");

// Convert to JSON string
let json = def.to_json()?;
println!("{}", json);
```

## PatchDef Structure

The serialized format:

```json
{
  "version": 1,
  "name": "My Awesome Synth",
  "author": "Your Name",
  "description": "A warm analog-style bass",
  "tags": ["bass", "analog", "subtractive"],
  "modules": [
    {
      "name": "vco",
      "module_type": "vco",
      "position": [100, 200],
      "state": null
    }
  ],
  "cables": [
    {
      "from": "vco.saw",
      "to": "vcf.in",
      "attenuation": 1.0
    }
  ],
  "parameters": {}
}
```

## Loading Patches

Reconstruct a patch from JSON:

```rust,ignore
// Parse JSON
let def = PatchDef::from_json(&json_string)?;

// Create module registry
let registry = ModuleRegistry::new();

// Rebuild patch
let patch = Patch::from_def(&def, &registry, 44100.0)?;
```

## The Module Registry

The registry maps type names to constructors:

```rust,ignore
let mut registry = ModuleRegistry::new();

// Built-in modules are registered by default
// For custom modules, register them:
registry.register("my_module", |sr| {
    Box::new(MyCustomModule::new(sr))
});
```

Default registered modules:

| Type ID | Module |
|---------|--------|
| `vco` | Vco |
| `svf` | Svf |
| `adsr` | Adsr |
| `vca` | Vca |
| `lfo` | Lfo |
| `mixer` | Mixer |
| `stereo_output` | StereoOutput |
| ... | (many more) |

## File Operations

Save to and load from files:

```rust,ignore
use std::fs;

// Save
let json = patch.to_def("My Patch").to_json()?;
fs::write("my_patch.json", &json)?;

// Load
let json = fs::read_to_string("my_patch.json")?;
let def = PatchDef::from_json(&json)?;
let patch = Patch::from_def(&def, &registry, 44100.0)?;
```

## Handling External Inputs

`ExternalInput` modules require `Arc<AtomicF64>` values that can't serialize:

```rust,ignore
// These modules won't round-trip through JSON:
let pitch = patch.add("pitch", ExternalInput::voct(pitch_arc));

// After loading, you'll need to reconnect external inputs manually
```

**Solution**: Use `Offset` for static values, or re-add ExternalInputs after loading.

## Patch Metadata

Add descriptive information:

```rust,ignore
let mut def = patch.to_def("Fat Bass");
def.author = Some("Sound Designer".to_string());
def.description = Some("Classic Moog-style bass with filter sweep".to_string());
def.tags = vec!["bass".into(), "moog".into(), "classic".into()];
```

## Versioning

The `version` field enables migration:

```rust,ignore
let def = PatchDef::from_json(&json)?;

match def.version {
    1 => { /* current format */ }
    0 => { /* legacy format - migrate */ }
    _ => { /* unknown version */ }
}
```

## Preset Library

Use the built-in preset system:

```rust,ignore
let library = PresetLibrary::new();

// List all presets
for preset in library.list() {
    println!("{}: {}", preset.name, preset.description);
}

// Get presets by category
let basses = library.by_category(PresetCategory::Bass);

// Search by tag
let acid = library.search_tags(&["acid", "303"]);

// Load a preset
let preset = library.get("303 Acid")?;
let patch = preset.build(44100.0)?;
```

## Example: Patch Manager

```rust,ignore
{{#include ../../../examples/howto_serialization.rs}}
```

## Best Practices

1. **Version your patches**: Include version numbers for future compatibility
2. **Document parameters**: Use description fields liberally
3. **Test round-trips**: Verify patches load correctly after saving
4. **Handle missing modules**: Gracefully handle unknown module types
5. **Separate external I/O**: Document which external connections are needed
