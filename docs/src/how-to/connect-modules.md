# Connect Modules

This guide covers the ways to connect modules in a `Patch`, from basic patching to
attenuated and modulated cables. Every connection is made between two **port
references** obtained from the `NodeHandle` that `patch.add(...)` returns.

## Basic Connection

The fundamental operation connects an output port to an input port and returns a stable
`CableId`:

```rust,ignore
let vco = patch.add("vco", Vco::new(44100.0));
let vcf = patch.add("vcf", Svf::new(44100.0));

let cable_id = patch.connect(vco.out("saw"), vcf.in_("in"))?;
```

- `vco.out("saw")` — an output jack, returns a `PortRef`.
- `vcf.in_("in")` — an input jack. Spelled `in_()` because `in` is a Rust keyword.

`connect` returns `Result<CableId, PatchError>`; the `CableId` stays valid even after
other cables are removed, so hold onto it if you want to disconnect exactly this cable
later.

## Finding Port Names

Inspect a module's `PortSpec`. `inputs` and `outputs` are `Vec<PortDef>`; each `PortDef`
has an `id`, a `name`, and a `kind` ([`SignalKind`](../appendix/signal-cheatsheet.md)):

```rust,ignore
let vco = Vco::new(44100.0);
let spec = vco.port_spec();

for port in &spec.inputs {
    println!("in  {}: {} ({:?})", port.id, port.name, port.kind);
}
for port in &spec.outputs {
    println!("out {}: {} ({:?})", port.id, port.name, port.kind);
}
```

Common port names:

| Module | Inputs | Outputs |
|--------|--------|---------|
| `Vco` | `voct`, `fm`, `pw`, `sync`, `fm_lin` | `sin`, `tri`, `saw`, `sqr` |
| `Svf` | `in`, `cutoff`, `res`, `fm`, `keytrack`, `keytrack_amt` | `lp`, `bp`, `hp`, `notch` |
| `Adsr` | `gate`, `retrig`, `attack`, `decay`, `sustain`, `release`, `shape` | `env`, `inv`, `eoc` |
| `Vca` | `in`, `cv`, `response`, `gain` | `out` |
| `StereoOutput` | `left`, `right` | (patch output) |

See the [Module Reference](../reference/oscillators.md) for the full list per module.

## Connection with Attenuation

Scale a signal to 0–100% strength with `connect_attenuated`. The attenuation is clamped
to `0.0..=1.0`:

```rust,ignore
patch.connect_attenuated(
    lfo.out("sin"),
    vcf.in_("cutoff"),
    0.5, // 50% strength
)?;
```

## Connection with Attenuation and Offset

For full attenuverter-plus-offset control, use `connect_modulated`:

```rust,ignore
patch.connect_modulated(
    lfo.out("sin"),
    vcf.in_("cutoff"),
    0.3,  // attenuation: -2.0..=2.0 (negative inverts, >1.0 amplifies)
    5.0,  // offset:      -10.0..=10.0 V, added after attenuation
)?;
```

- **attenuation** is clamped to `-2.0..=2.0`. `1.0` is unity, `0.5` half strength,
  `-1.0` inverts, `2.0` doubles (watch for clipping).
- **offset** is clamped to `-10.0..=10.0` volts and is added *after* attenuation. The
  example above shifts the LFO's ±5 V swing to oscillate around +5 V.

## Multiple Outputs (Mult)

One output can feed many inputs — connect it repeatedly, or use `mult` for a slice of
destinations:

```rust,ignore
// The same gate triggers three envelopes:
patch.connect(gate.out("out"), env1.in_("gate"))?;
patch.connect(gate.out("out"), env2.in_("gate"))?;
patch.connect(gate.out("out"), env3.in_("gate"))?;

// Or in one call:
patch.mult(gate.out("out"), &[env1.in_("gate"), env2.in_("gate"), env3.in_("gate")])?;
```

## Multiple Inputs (Summing)

Several cables into one input are **summed**, modeling how CVs mix at a hardware jack:

```rust,ignore
// Two LFOs combined on the filter cutoff:
patch.connect(lfo1.out("sin"), vcf.in_("cutoff"))?;
patch.connect(lfo2.out("tri"), vcf.in_("cutoff"))?;
// The cutoff input receives lfo1 + lfo2.
```

## Normalled Inputs

Some modules declare **normalled** inputs in their port spec: when left unpatched, the
input falls back to another port's current value. For example, `StereoOutput`'s `right`
input is normalled to `left`, so patching only `left` produces centered mono. Normalling
is a property of the module definition (`PortDef::normalled_to`), resolved at compile
time — you get the behavior automatically by leaving the input unpatched.

## Validation Modes

Control how signal-kind mismatches are handled:

```rust,ignore
patch.set_validation_mode(ValidationMode::Strict); // error on mismatch
patch.set_validation_mode(ValidationMode::Warn);   // record a warning, allow it
patch.set_validation_mode(ValidationMode::None);   // no checking
```

The default is `Warn`, which flags questionable connections without blocking
experimentation. In `Strict` mode an incompatible connection returns
`PatchError::SignalMismatch`.

## Disconnecting

Remove a specific cable by its `CableId`:

```rust,ignore
let cable_id = patch.connect(vco.out("saw"), vcf.in_("in"))?;
patch.disconnect(cable_id)?;
```

Or remove the cable between two specific ports:

```rust,ignore
patch.disconnect_ports(vco.out("saw"), vcf.in_("in"))?;
```

## Error Handling

Connecting returns `Result<CableId, PatchError>`. `PatchError` is `#[non_exhaustive]`,
so a `match` needs a wildcard arm:

```rust,ignore
match patch.connect(a.out("x"), b.in_("y")) {
    Ok(cable_id) => println!("connected: {cable_id}"),
    Err(PatchError::InvalidPort { name, available, .. }) => {
        println!("no such port {name:?}; try one of {available:?}");
    }
    Err(PatchError::SignalMismatch { message, .. }) => {
        println!("signal mismatch: {message}");
    }
    Err(e) => println!("error: {e}"),
}
```

Note that feedback loops are not rejected at `connect` time — they surface as
`PatchError::CycleDetected` from `patch.compile()` unless the loop passes through a
cycle-breaking module such as `UnitDelay` or `DelayLine`.

## Inspecting Connections

Query the current cables. `cables()` returns `&[Cable]`, where each `Cable` has `id`,
`from`, `to`, and optional `attenuation` / `offset`:

```rust,ignore
for cable in patch.cables() {
    println!("#{}: {:?} -> {:?}", cable.id, cable.from, cable.to);
}
```

## Best Practices

1. **Name modules clearly** — `"filter_lfo"`, not `"lfo2"`.
2. **Keep `Warn` validation on** during development to catch signal-kind mistakes.
3. **Check port specs** when unsure of names, rather than guessing.
4. **Break feedback** with a `UnitDelay` (or `DelayLine`) so `compile()` succeeds.
5. **Prefer attenuation over amplification** to avoid clipping.
