# Integrate with Plugins

Use Quiver patches as VST/AU plugins or integrate with Web Audio.

## Plugin Wrapper Overview

```mermaid
flowchart LR
    HOST[DAW Host] --> WRAP[Plugin Wrapper]
    WRAP --> PATCH[Quiver Patch]
    PATCH --> WRAP
    WRAP --> HOST
```

## VST/AU Plugin Wrapper

The `PluginWrapper` provides a bridge to plugin formats:

```rust,ignore
use quiver::prelude::*;

// Define your synth as a function
fn create_synth(sample_rate: f64) -> Patch {
    let mut patch = Patch::new(sample_rate);
    // ... build your patch ...
    patch
}

// Create plugin wrapper
let info = PluginInfo {
    name: "QuiverSynth".to_string(),
    vendor: "My Company".to_string(),
    unique_id: 12345,
    version: (1, 0, 0),
    category: PluginCategory::Instrument,
    num_inputs: 0,
    num_outputs: 2,
    has_editor: false,
};

let wrapper = PluginWrapper::new(info, create_synth);
```

## Plugin Parameters

Expose parameters to the DAW:

```rust,ignore
let params = vec![
    PluginParameter {
        name: "Cutoff".to_string(),
        default: 0.5,
        min: 0.0,
        max: 1.0,
        unit: "".to_string(),
    },
    PluginParameter {
        name: "Resonance".to_string(),
        default: 0.0,
        min: 0.0,
        max: 1.0,
        unit: "".to_string(),
    },
];

wrapper.set_parameters(params);
```

## Web Audio Integration

For browser-based applications:

```rust,ignore
use quiver::prelude::*;

// Configure for Web Audio
let config = WebAudioConfig {
    sample_rate: 44100.0,
    channels: 2,
    buffer_size: 128,
};

// Create processor
let processor = WebAudioProcessor::new(config, |sr| {
    let mut patch = Patch::new(sr);
    // ... build patch ...
    patch
});

// In your AudioWorklet process callback
fn process(inputs: &[f32], outputs: &mut [f32]) {
    processor.process(inputs, outputs);
}
```

## AudioWorklet Usage

Create a WebAssembly worklet:

```rust,ignore
let worklet = WebAudioWorklet::new(patch);

// Process interleaved stereo
let input: Vec<f32> = /* from Web Audio */;
let mut output: Vec<f32> = vec![0.0; 256];

worklet.process(&input, &mut output);

// output is now interleaved stereo [L, R, L, R, ...]
```

## OSC Control

Receive OSC messages for real-time control:

```rust,ignore
// Create OSC receiver
let receiver = OscReceiver::new("127.0.0.1:9000")?;

// Define parameter bindings
let bindings = vec![
    OscBinding {
        pattern: OscPattern::new("/synth/cutoff"),
        target: "vcf.cutoff".to_string(),
        range: (0.0, 10.0),
    },
    OscBinding {
        pattern: OscPattern::new("/synth/resonance"),
        target: "vcf.resonance".to_string(),
        range: (0.0, 1.0),
    },
];

// In your processing loop
while let Some(msg) = receiver.recv()? {
    for binding in &bindings {
        if binding.pattern.matches(&msg.address) {
            let value = binding.map_value(&msg);
            patch.set_parameter(&binding.target, value);
        }
    }
}
```

## OSC Input Module

Use OSC as a CV source:

```rust,ignore
let osc_in = patch.add("osc_cutoff", OscInput::new("/synth/cutoff"));
patch.connect(osc_in.out("out"), vcf.in_("cutoff"))?;
```

## Bus Configuration

Define audio bus layout:

```rust,ignore
let bus_config = AudioBusConfig {
    inputs: vec![
        AudioBus { name: "Sidechain".into(), channels: 2 },
    ],
    outputs: vec![
        AudioBus { name: "Main".into(), channels: 2 },
    ],
};
```

## Example: Complete Plugin

```rust,ignore
{{#include ../../../examples/howto_plugin.rs}}
```

## Format-Specific Notes

### VST3
- Uses `unique_id` for plugin identification
- Category maps to VST3 categories

### AU (Audio Unit)
- Requires bundle identifier
- Uses different parameter system

### LV2
- URI-based identification
- Turtle metadata generation needed

## Real-Time Safety

Plugin environments require strict real-time guarantees:

1. **No allocations** in process callback
2. **No blocking** operations
3. **Bounded execution time**

Quiver's design supports this:
- Pre-allocated buffers
- Lock-free atomics for parameters
- Predictable module processing
