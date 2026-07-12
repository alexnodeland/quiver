# Create Custom Modules

Extend Quiver with your own DSP modules using the Module Development Kit (MDK).

## The GraphModule Trait

Every module in Layer 3 implements `GraphModule`:

```rust,ignore
pub trait GraphModule: Send {
    fn port_spec(&self) -> PortSpec;
    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues);
    fn reset(&mut self);
    fn set_sample_rate(&mut self, sample_rate: f64);
}
```

## Constructor & Sample-Rate Convention

`GraphModule` always provides `set_sample_rate`, and `Patch::add` calls it with
the patch's sample rate the moment a module is inserted. **The graph is the
single source of truth for sample rate** — whatever value a module was built
with is overwritten before its first `tick`. Given that, constructors follow one
rule so a module's sample-rate dependence is readable straight off its
signature:

- **Sample-rate-dependent modules take `sample_rate` in `new`.** Anything whose
  DSP needs the rate to initialize correctly-sized state — phase increments,
  delay/reverb buffers, envelope/filter coefficients — accepts it, e.g.
  `Vco::new(sample_rate)`, `Svf::new(sample_rate)`, `DelayLine::new(sample_rate)`.
  The value seeds initial state; `set_sample_rate` stores the rate and (if the
  module caches coefficients) recomputes them so a later rate change stays
  correct.
- **Sample-rate-independent modules take `new()`** (or only their value
  parameters). Gain, mixing, logic, and trigger/clock-driven modules do not need
  the rate: `Vca::new()`, `StereoOutput::new()`, `Mixer::new(num_channels)`,
  `Offset::new(offset)`. Their `set_sample_rate` is a no-op:
  `fn set_sample_rate(&mut self, _: f64) {}`.

Do **not** accept `sample_rate` "just in case": an unused constructor parameter
is misleading. `MyDistortion` below stores `sample_rate` because its DSP is
sample-rate-dependent (an antialiasing/DC-blocking stage would use it) — a pure
waveshaper that never reads the rate should take `new()` instead.

## Step 1: Define Your Ports

```rust,ignore
use quiver::prelude::*;

pub struct MyDistortion {
    sample_rate: f64,
    drive: f64,
}

impl MyDistortion {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            drive: 1.0,
        }
    }
}
```

## Step 2: Implement GraphModule

```rust,ignore
impl GraphModule for MyDistortion {
    fn port_spec(&self) -> PortSpec {
        PortSpec::new()
            .with_input("in", PortDef::audio())
            .with_input("drive", PortDef::cv_unipolar().with_default(5.0))
            .with_output("out", PortDef::audio())
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get("in");
        let drive = inputs.get("drive") / 5.0;  // Normalize CV

        // Soft clipping distortion
        let driven = input * (1.0 + drive * 4.0);
        let output = driven.tanh() * 5.0;  // Back to ±5V range

        outputs.set("out", output);
    }

    fn reset(&mut self) {
        self.drive = 1.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }
}
```

## Step 3: Use Your Module

```rust,ignore
let mut patch = Patch::new(44100.0);

let vco = patch.add("vco", Vco::new(44100.0));
let dist = patch.add("dist", MyDistortion::new(44100.0));
let output = patch.add("output", StereoOutput::new());

patch.connect(vco.out("saw"), dist.in_("in"))?;
patch.connect(dist.out("out"), output.in_("left"))?;
```

## Using Module Templates

The MDK provides templates for common module types:

```rust,ignore
use quiver::mdk::*;

let template = ModuleTemplate::new("BitCrusher", ModuleCategory::Effect)
    .with_input(PortTemplate::audio("in"))
    .with_input(PortTemplate::cv_unipolar("bits").with_default(8.0))
    .with_input(PortTemplate::cv_unipolar("rate").with_default(10.0))
    .with_output(PortTemplate::audio("out"));

// Generate skeleton code
let code = template.generate_rust_code();
println!("{}", code);
```

## Testing Custom Modules

Use the testing harness:

```rust,ignore
let mut harness = ModuleTestHarness::new(MyDistortion::new(44100.0));

// Test reset behavior
let result = harness.test_reset();
assert!(result.passed, "Reset test: {}", result.message);

// Test sample rate handling
let result = harness.test_sample_rate_change(48000.0);
assert!(result.passed, "Sample rate test: {}", result.message);

// Test output bounds
let result = harness.test_output_bounds(-10.0..=10.0);
assert!(result.passed, "Bounds test: {}", result.message);
```

## Signal Analysis

Analyze your module's output:

```rust,ignore
let analysis = AudioAnalysis::new(44100.0);

// Collect samples
let samples: Vec<f64> = (0..44100)
    .map(|_| module.tick(&inputs, &mut outputs))
    .collect();

println!("RMS Level: {:.2} dB", analysis.rms_db(&samples));
println!("Peak: {:.2}V", analysis.peak(&samples));
println!("DC Offset: {:.4}V", analysis.dc_offset(&samples));
println!("Estimated Frequency: {:.1} Hz", analysis.frequency_estimate(&samples));
```

## Documentation Generation

Auto-generate docs for your module:

```rust,ignore
let doc_gen = DocGenerator::new(&my_module);

// Markdown format
let markdown = doc_gen.generate(DocFormat::Markdown);
println!("{}", markdown);

// HTML format
let html = doc_gen.generate(DocFormat::Html);
```

## Example: Complete Custom Module

```rust,ignore
{{#include ../../../examples/howto_custom_module.rs}}
```

## Registering for Serialization

Add your module to the registry:

```rust,ignore
let mut registry = ModuleRegistry::new();

registry.register("my_distortion", |sr| {
    Box::new(MyDistortion::new(sr))
});

// Now patches with "my_distortion" can be loaded
let patch = Patch::from_def(&def, &registry, 44100.0)?;
```

## Best Practices

1. **Validate inputs**: Clamp CV values to expected ranges
2. **Handle edge cases**: Zero crossings, near-zero values
3. **Avoid allocations**: No heap allocations in `tick()`
4. **Document signal ranges**: Specify expected voltage ranges
5. **Test thoroughly**: Use the test harness before shipping
