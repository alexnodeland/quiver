//! Simple Patch Example
//!
//! The smallest possible Quiver patch: one oscillator wired straight to the
//! output. No envelope, no gate, no filter — just the four lines of graph
//! setup every larger patch builds on.
//!
//! How this differs from `first_patch.rs`: that example is the *full* voice
//! (VCO -> VCF -> VCA, shaped by an ADSR envelope and a gate signal). This
//! one strips everything away except the minimum needed to hear a tone, so
//! you can see the bare `add` / `connect` / `compile` / `tick` skeleton
//! without any modulation logic in the way.
//!
//! Run with: cargo run --example simple_patch

use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;

    // 1. Create a patch — the container that owns every module and cable.
    let mut patch = Patch::new(sample_rate);

    // 2. Add modules. `add` takes a name (for debugging/serialization) and
    //    the module itself, and hands back a handle used to wire it up.
    let vco = patch.add("vco", Vco::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    // 3. Connect: the VCO's sawtooth output feeds directly into the output
    //    module's left channel. (The right channel is left unconnected —
    //    StereoOutput normals an unpatched right input to the left signal,
    //    so both speakers still play the same tone.)
    patch.connect(vco.out("saw"), output.in_("left")).unwrap();

    // 4. Compile: resolves the graph into a fixed processing order. Required
    //    once, after wiring, before the first `tick()`.
    patch.set_output(output.id());
    patch.compile().unwrap();

    println!(
        "The smallest Quiver patch: {} module(s), {} cable(s)",
        patch.node_count(),
        patch.cable_count()
    );

    // 5. Tick: each call advances the whole graph by exactly one sample.
    // The VCO free-runs at its default pitch (V/Oct input left at 0V = C4),
    // so a tone starts immediately — there is no gate to open here.
    let samples = (sample_rate * 0.5) as usize;
    let mut peak = 0.0_f64;

    for _ in 0..samples {
        let (left, _right) = patch.tick();
        peak = peak.max(left.abs());
    }

    println!("Generated {samples} samples, peak amplitude: {peak:.2}V");
    println!("\nThat's it: add -> connect -> compile -> tick.");
    println!("See first_patch.rs for a full voice with envelope + filter shaping.");
}
