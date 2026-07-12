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
  "output": "vca",
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
  "parameters": {
    "vcf.cutoff": 0.6
  }
}
```

The `output` field records which module drives the patch's stereo out. It is optional
and additive — patches written before it existed simply omit it, and `Patch::from_def`
falls back to an output-node heuristic. `parameters` maps `"module_name.param_id"` to a
value, where `param_id` is either a control-input port name (its unpatched base value) or
an internal parameter id exposed through introspection.

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

// Built-in modules are registered by default.
// For custom modules, register a factory with metadata:
registry.register_factory(
    "my_module",   // type_id
    "My Module",   // display name
    "effect",      // category
    "A custom effect", // description
    |sr| Box::new(MyCustomModule::new(sr)),
);
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

Describe a patch either on the live `Patch` via `PatchMeta` (which survives a
`to_def`/`from_def` round-trip) or directly on the `PatchDef`:

```rust,ignore
// On the live patch — carried through serialization:
patch.set_meta(PatchMeta {
    name: Some("Fat Bass".into()),
    author: Some("Sound Designer".into()),
    description: Some("Classic Moog-style bass with filter sweep".into()),
    tags: vec!["bass".into(), "moog".into(), "classic".into()],
});

// Or on the serialized def:
let mut def = patch.to_def("Fat Bass");
def.author = Some("Sound Designer".to_string());
def.description = Some("Classic Moog-style bass with filter sweep".to_string());
def.tags = vec!["bass".into(), "moog".into(), "classic".into()];
```

## Parameters Round-Trip

Module parameters are captured and restored via introspection. `to_def` records each
module's parameters into `PatchDef.parameters`, and `from_def` re-applies them with
`set_param_by_id`. You can inspect and drive parameters directly on a `Patch`:

```rust,ignore
// Discover a node's parameters:
for info in patch.param_infos(node_id) {
    println!("{} = {:?}", info.id, patch.get_param_by_id(node_id, &info.id));
}

// Set one by id (returns false if the id is unknown):
patch.set_param_by_id(node_id, "cutoff", 0.6);
```

Modules opt in to this by implementing `GraphModule::introspect`, which exposes their
parameters to the GUI/serialization layer.

## Versioning

`PatchDef.version` is checked against `CURRENT_PATCH_VERSION`. The format only grows
additively, so the policy is: **accept any patch with `version <= CURRENT_PATCH_VERSION`,
and reject anything newer**. `Patch::from_def` enforces this and returns an error for a
patch written by a future version:

```rust,ignore
use quiver::serialize::CURRENT_PATCH_VERSION;

let def = PatchDef::from_json(&json)?;
assert!(def.version <= CURRENT_PATCH_VERSION); // from_def rejects newer patches
```

## Preset Library

Use the built-in preset system:

```rust,ignore
let library = PresetLibrary::new();

// List all presets (associated functions — no receiver needed)
for preset in PresetLibrary::list() {
    println!("{}: {}", preset.name, preset.description);
}

// Get presets by category
let basses = PresetLibrary::by_category(PresetCategory::Bass);

// Search by tag
let acid = library.search_tags(&["acid", "303"]);

// Load and build a preset (get returns Option, build returns Result)
if let Some(preset) = library.get("303 Acid") {
    let patch = preset.build(44100.0)?;
}
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
