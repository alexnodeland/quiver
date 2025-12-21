# Connect Modules

This guide covers all the ways to connect modules in Quiver, from basic patching to advanced cable configuration.

## Basic Connection

The fundamental operation:

```rust,ignore
patch.connect(source.out("output_name"), dest.in_("input_name"))?;
```

> **Note:** Use `in_()` because `in` is a Rust reserved word.

## Finding Port Names

Check the module's `PortSpec`:

```rust,ignore
let vco = Vco::new(44100.0);
let spec = vco.port_spec();

println!("Inputs: {:?}", spec.inputs.keys().collect::<Vec<_>>());
println!("Outputs: {:?}", spec.outputs.keys().collect::<Vec<_>>());
```

Common port names:

| Module | Inputs | Outputs |
|--------|--------|---------|
| Vco | `voct`, `fm`, `pw`, `sync` | `sin`, `tri`, `saw`, `sqr` |
| Svf | `in`, `cutoff`, `resonance`, `fm` | `lp`, `bp`, `hp`, `notch` |
| Adsr | `gate`, `attack`, `decay`, `sustain`, `release` | `env` |
| Vca | `in`, `cv` | `out` |

## Connection with Attenuation

Scale the signal strength:

```rust,ignore
patch.connect_with(
    lfo.out("sin"),
    vcf.in_("cutoff"),
    Cable::new().with_attenuation(0.5),  // 50% strength
)?;
```

Attenuation range: **-2.0 to +2.0**
- 1.0 = unity (no change)
- 0.5 = half strength
- -1.0 = inverted
- 2.0 = doubled (with clipping risk)

## Connection with Offset

Add a DC offset to the signal:

```rust,ignore
patch.connect_with(
    lfo.out("sin"),
    vcf.in_("cutoff"),
    Cable::new()
        .with_attenuation(0.3)
        .with_offset(5.0),  // Center at 5V
)?;
```

This shifts the LFO's ±5V swing to oscillate around 5V.

## Multiple Outputs (Mult)

One output can feed multiple inputs:

```rust,ignore
// Same gate triggers multiple envelopes
patch.connect(gate.out("out"), env1.in_("gate"))?;
patch.connect(gate.out("out"), env2.in_("gate"))?;
patch.connect(gate.out("out"), env3.in_("gate"))?;
```

Quiver handles the signal distribution automatically.

## Multiple Inputs (Summing)

Multiple cables to one input are summed:

```rust,ignore
// Two LFOs combined on filter cutoff
patch.connect(lfo1.out("sin"), vcf.in_("cutoff"))?;
patch.connect(lfo2.out("tri"), vcf.in_("cutoff"))?;
// Result: filter cutoff receives lfo1 + lfo2
```

This models analog behavior where CVs mix at the input.

## Validation Modes

Control how signal type mismatches are handled:

```rust,ignore
// Strict: error on mismatch
patch.set_validation_mode(ValidationMode::Strict);

// Warn: log warning but allow
patch.set_validation_mode(ValidationMode::Warn);

// None: no checking
patch.set_validation_mode(ValidationMode::None);
```

Default is `Warn`, which helps catch mistakes without blocking experimentation.

## Disconnecting

Remove a specific connection:

```rust,ignore
let cable_id = patch.connect(vco.out("saw"), vcf.in_("in"))?;
patch.disconnect(cable_id);
```

Or disconnect by ports:

```rust,ignore
patch.disconnect_port(vcf.in_("in"));  // Remove all cables to this input
```

## Error Handling

Connection can fail for several reasons:

```rust,ignore
match patch.connect(a.out("x"), b.in_("y")) {
    Ok(cable_id) => println!("Connected: {:?}", cable_id),
    Err(PatchError::PortNotFound(port)) => {
        println!("Port '{}' doesn't exist", port);
    }
    Err(PatchError::CycleDetected) => {
        println!("Would create a feedback loop");
    }
    Err(e) => println!("Error: {:?}", e),
}
```

## Checking Connections

Query existing cables:

```rust,ignore
// All cables in patch
for cable in patch.cables() {
    println!("{:?} → {:?}", cable.from, cable.to);
}

// Cables to a specific input
for cable in patch.cables_to(vcf.in_("cutoff")) {
    println!("Modulated by: {:?}", cable.from);
}
```

## Best Practices

1. **Name modules clearly**: `"filter_lfo"` not `"lfo2"`
2. **Use validation mode Warn** during development
3. **Check port specs** if unsure about names
4. **Apply attenuation** rather than amplification to avoid clipping
