//! `Patch::tick_block` against `Patch::tick`, bit for bit.
//!
//! The block engine runs most nodes node-major (each through a whole block before the
//! next), through their `GraphModule::tick_frames` where they have one, and the nodes on a
//! feedback cycle or sharing the thread-wide random stream sample by sample. Whatever the
//! schedule, a block of `n` frames must be exactly `n` calls to `tick`: every sample, and
//! every output port's value afterwards (`get_output_value`, which observers read).
//!
//! Each test builds a patch twice, renders one by `tick` and the other by `tick_block` in
//! blocks of 1, 7, 64, 128 and 1000 frames (and ragged mixes), and compares every sample's
//! bits and every port's bits after. Edits (`set_param_by_id`, a cable, a new node) land
//! at the same frame on both sides: between ticks on one, between blocks on the other.
#![cfg(feature = "std")]

use std::sync::Arc;

use quiver::io::ExternalOutput;
use quiver::modules::{
    Bitcrusher, Chorus, Clock, Comparator, Compressor, Crossfader, DelayLine, Distortion,
    EnvelopeFollower, Euclidean, FormantOsc, KarplusStrong, Limiter, Max, Min, Mixer, NoiseGate,
    ParametricEq, Quantizer, Reverb, RingModulator, Scale, ScaleQuantizer, Supersaw, Tremolo,
    UnitDelay, Vibrato, Wavefolder, Wavetable,
};
use quiver::port::{BlockInputs, BlockOutputs};
use quiver::prelude::*;

const SR: f64 = 44_100.0;
const BLOCKS: [usize; 5] = [1, 7, 64, 128, 1000];

type Edit = Box<dyn Fn(&mut Patch)>;

/// Every output port of every node, as bits (`None` for a port with no routing slot).
fn port_bits(patch: &Patch) -> Vec<(String, u32, Option<u64>)> {
    let mut ports = Vec::new();
    for (id, name, module) in patch.nodes() {
        for out in &module.port_spec().outputs {
            ports.push((
                name.to_string(),
                out.id,
                patch.get_output_value(id, out.id).map(f64::to_bits),
            ));
        }
    }
    ports.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    ports
}

/// Render `frames` frames by `tick`, applying each edit just before its frame.
fn by_ticks(patch: &mut Patch, frames: usize, edits: &[(usize, Edit)]) -> Vec<(u64, u64)> {
    let mut out = Vec::with_capacity(frames);
    for frame in 0..frames {
        for (at, edit) in edits {
            if *at == frame {
                edit(patch);
            }
        }
        let (l, r) = patch.tick();
        out.push((l.to_bits(), r.to_bits()));
    }
    out
}

/// Render `frames` frames by `tick_block`, `block` frames a call (cut short at each edit,
/// which lands between calls), cycling through `block` sizes.
fn by_blocks(
    patch: &mut Patch,
    frames: usize,
    blocks: &[usize],
    edits: &[(usize, Edit)],
) -> Vec<(u64, u64)> {
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    let mut done = 0;
    let mut sizes = blocks.iter().cycle();
    while done < frames {
        for (at, edit) in edits {
            if *at == done {
                edit(patch);
            }
        }
        let next_edit = edits
            .iter()
            .map(|(at, _)| *at)
            .filter(|&at| at > done)
            .min()
            .unwrap_or(frames);
        let n = (*sizes.next().unwrap()).min(next_edit - done);
        patch.tick_block(&mut left[done..done + n], &mut right[done..done + n]);
        done += n;
    }
    left.iter()
        .zip(&right)
        .map(|(l, r)| (l.to_bits(), r.to_bits()))
        .collect()
}

/// Build the patch twice and hold `tick_block` to `tick` at every block size.
fn check_with(
    name: &str,
    build: &dyn Fn() -> Patch,
    frames: usize,
    edits: &dyn Fn() -> Vec<(usize, Edit)>,
    before_render: &dyn Fn(),
) {
    let mut a = build();
    before_render();
    let expected = by_ticks(&mut a, frames, &edits());
    let expected_ports = port_bits(&a);
    // A patch that renders silence everywhere would prove little.
    let mut sizes: Vec<Vec<usize>> = BLOCKS.iter().map(|&b| vec![b]).collect();
    sizes.push(vec![256, 1, 1000, 37, 4096, 63, 65]);
    for blocks in sizes {
        let mut b = build();
        before_render();
        let got = by_blocks(&mut b, frames, &blocks, &edits());
        if let Some(first) = expected.iter().zip(&got).position(|(e, g)| e != g) {
            panic!(
                "`{name}`, blocks {blocks:?}: frame {first} differs: tick {:?}, tick_block {:?}",
                (
                    f64::from_bits(expected[first].0),
                    f64::from_bits(expected[first].1)
                ),
                (f64::from_bits(got[first].0), f64::from_bits(got[first].1)),
            );
        }
        assert_eq!(expected.len(), got.len());
        assert_eq!(
            expected_ports,
            port_bits(&b),
            "`{name}`, blocks {blocks:?}: output ports differ after the render"
        );
    }
}

fn check(name: &str, build: &dyn Fn() -> Patch, frames: usize) {
    check_with(name, build, frames, &Vec::new, &|| {});
}

/// The output's samples are not all silence (so a comparison means something).
fn audible(build: &dyn Fn() -> Patch, frames: usize) -> bool {
    let mut p = build();
    (0..frames).any(|_| p.tick().0 != 0.0)
}

/// A gate that opens and closes: an LFO's square through an offset cable.
fn gate_lfo(patch: &mut Patch, name: &str, rate: f64) -> NodeHandle {
    let lfo = patch.add(name, Lfo::new(SR));
    assert!(patch.set_param_by_id(lfo.id(), "rate", rate));
    lfo
}

/// VCO -> SVF -> VCA -> out, ADSR on the VCA, an LFO fanned out to the filter and the
/// VCO's pulse width, an external pitch, unpatched inputs at their defaults.
fn subtractive() -> Patch {
    let mut p = Patch::new(SR);
    let pitch = Arc::new(AtomicF64::new(0.25));
    let voct = p.add("voct", ExternalInput::voct(pitch));
    let vco = p.add("vco", Vco::new(SR));
    let svf = p.add("svf", Svf::new(SR));
    let vca = p.add("vca", Vca::new());
    let env = p.add("env", Adsr::new(SR));
    let gate = gate_lfo(&mut p, "gate", 0.45);
    let lfo = p.add("lfo", Lfo::new(SR));
    let out = p.add("out", StereoOutput::new());
    p.connect(voct.out("out"), vco.in_("voct")).unwrap();
    p.connect(vco.out("saw"), svf.in_("in")).unwrap();
    p.connect_attenuated(lfo.out("sin"), svf.in_("fm"), 0.1)
        .unwrap();
    p.connect_attenuated(lfo.out("sin_uni"), vco.in_("pw"), 0.05)
        .unwrap();
    p.connect(svf.out("lp"), vca.in_("in")).unwrap();
    p.connect(gate.out("sqr"), env.in_("gate")).unwrap();
    p.connect(env.out("env"), vca.in_("cv")).unwrap();
    p.connect(vca.out("out"), out.in_("left")).unwrap();
    p.set_output(out.id());
    p.compile().unwrap();
    p
}

#[test]
fn a_chain_with_fan_out_and_defaults() {
    assert!(audible(&subtractive, 20_000));
    check("subtractive", &subtractive, 30_000);
}

/// Several sources summed into one input (plain, attenuated, offset), a mixer fanning in,
/// a stereo output with both sides patched.
fn fan_in() -> Patch {
    let mut p = Patch::new(SR);
    let a = p.add("a", Vco::new(SR));
    let b = p.add("b", Vco::new(SR));
    let c = p.add("c", Lfo::new(SR));
    let mix = p.add("mix", Mixer::new(3));
    let att = p.add("att", Attenuverter::new());
    let off = p.add("off", Offset::new(0.25));
    let out = p.add("out", StereoOutput::new());
    p.connect(a.out("sin"), b.in_("fm")).unwrap();
    p.connect(a.out("tri"), mix.in_("ch0")).unwrap();
    p.connect_attenuated(b.out("sqr"), mix.in_("ch0"), -0.5)
        .unwrap();
    p.connect_modulated(c.out("tri"), mix.in_("ch1"), 0.3, 0.7)
        .unwrap();
    p.connect(b.out("saw"), mix.in_("ch2")).unwrap();
    p.connect(mix.out("out"), att.in_("in")).unwrap();
    p.connect(c.out("saw"), att.in_("level")).unwrap();
    p.connect(att.out("out"), off.in_("in")).unwrap();
    p.connect(off.out("out"), out.in_("left")).unwrap();
    p.connect(mix.out("out"), out.in_("right")).unwrap();
    p.set_output(out.id());
    p.compile().unwrap();
    p
}

#[test]
fn fan_in_and_cable_arithmetic() {
    assert!(audible(&fan_in, 1_000));
    check("fan_in", &fan_in, 20_000);
}

/// One module of a kind, every input driven (gates from LFO squares, CVs from sines folded
/// into `0..1` by the cable, one from seeded noise) except the last, which keeps its
/// default; its first output to the left channel, its last to the right, the rest
/// unpatched (masked off where the module honours the mask).
fn one_of(kind: &'static str, make: fn() -> Box<dyn GraphModule>) -> impl Fn() -> Patch {
    move || {
        let mut p = Patch::new(SR);
        let module = make();
        let inputs: Vec<String> = module
            .port_spec()
            .inputs
            .iter()
            .map(|d| d.name.clone())
            .collect();
        let outputs: Vec<String> = module
            .port_spec()
            .outputs
            .iter()
            .map(|d| d.name.clone())
            .collect();
        let m = p.add_boxed(kind, module);
        let noise = p.add("noise", NoiseGenerator::new());
        let pitch = p.add("pitch", Vco::new(SR));
        let out = p.add("out", StereoOutput::new());
        let driven = inputs.len().saturating_sub(1).max(inputs.len().min(1));
        for (k, input) in inputs.iter().take(driven).enumerate() {
            let to = m.in_(input);
            match k % 4 {
                0 => {
                    // The signal input: an audible oscillator, plus a little noise.
                    p.connect(pitch.out("saw"), to).unwrap();
                    p.connect_attenuated(noise.out("white"), to, 0.1).unwrap();
                }
                1 => {
                    let lfo = gate_lfo(&mut p, &format!("lfo{k}"), 0.3 + 0.07 * k as f64);
                    p.connect_modulated(lfo.out("sin"), to, 0.1, 0.5).unwrap();
                }
                2 => {
                    let lfo = gate_lfo(&mut p, &format!("lfo{k}"), 0.55 + 0.05 * k as f64);
                    p.connect(lfo.out("sqr"), to).unwrap();
                }
                _ => {
                    p.connect_modulated(noise.out("pink"), to, 0.05, 0.4)
                        .unwrap();
                }
            }
        }
        if let Some(first) = outputs.first() {
            p.connect(m.out(first), out.in_("left")).unwrap();
        }
        if let Some(last) = outputs.last() {
            p.connect(m.out(last), out.in_("right")).unwrap();
        }
        p.set_output(out.id());
        p.seed(0xB10C);
        p.compile().unwrap();
        p
    }
}

#[test]
fn every_kind_with_a_block_path_and_some_without() {
    type Make = fn() -> Box<dyn GraphModule>;
    let kinds: Vec<(&'static str, Make)> = vec![
        // With a `tick_frames` override.
        ("vco", || Box::new(Vco::new(SR))),
        ("lfo", || Box::new(Lfo::new(SR))),
        ("noise", || Box::new(NoiseGenerator::new())),
        ("svf", || Box::new(Svf::new(SR))),
        ("adsr", || Box::new(Adsr::new(SR))),
        ("vca", || Box::new(Vca::new())),
        ("limiter", || Box::new(Limiter::new(SR))),
        ("mixer", || Box::new(Mixer::new(4))),
        ("offset", || Box::new(Offset::new(-1.5))),
        ("attenuverter", || Box::new(Attenuverter::new())),
        ("stereo_output", || Box::new(StereoOutput::new())),
        ("slew_limiter", || Box::new(SlewLimiter::new(SR))),
        ("sample_hold", || Box::new(SampleAndHold::new())),
        ("supersaw", || Box::new(Supersaw::new(SR))),
        ("karplus_strong", || Box::new(KarplusStrong::new(SR))),
        ("wavetable", || Box::new(Wavetable::new(SR))),
        ("formant_osc", || Box::new(FormantOsc::new(SR))),
        ("noise_gate", || Box::new(NoiseGate::new(SR))),
        ("compressor", || Box::new(Compressor::new(SR))),
        ("envelope_follower", || Box::new(EnvelopeFollower::new(SR))),
        ("parametric_eq", || Box::new(ParametricEq::new(SR))),
        ("bitcrusher", || Box::new(Bitcrusher::new())),
        ("distortion", || Box::new(Distortion::new(SR))),
        ("ring_mod", || Box::new(RingModulator::new())),
        ("wavefolder", || Box::new(Wavefolder::new(2.0))),
        ("unit_delay", || Box::new(UnitDelay::new())),
        ("delay_line", || Box::new(DelayLine::new(SR))),
        ("chorus", || Box::new(Chorus::new(SR))),
        ("tremolo", || Box::new(Tremolo::new(SR))),
        ("vibrato", || Box::new(Vibrato::new(SR))),
        ("reverb", || Box::new(Reverb::new(SR))),
        ("scale_quantizer", || Box::new(ScaleQuantizer::new(SR))),
        ("euclidean", || Box::new(Euclidean::new(SR))),
        ("quantizer", || Box::new(Quantizer::new(Scale::Major))),
        ("clock", || Box::new(Clock::new(SR))),
        ("crossfader", || Box::new(Crossfader::new())),
        ("comparator", || Box::new(Comparator::new())),
        ("min", || Box::new(Min::new())),
        ("max", || Box::new(Max::new())),
        ("external_input", || {
            Box::new(ExternalInput::cv(Arc::new(AtomicF64::new(0.375))))
        }),
        // Without one: the per-sample path inside a block.
        ("diode_ladder", || Box::new(DiodeLadderFilter::new(SR))),
        ("ring_bernoulli", || Box::new(BernoulliGate::new())),
        ("crosstalk", || Box::new(Crosstalk::new(SR))),
        ("multiple", || Box::new(Multiple::new())),
        ("analog_vco", || Box::new(AnalogVco::new(SR))),
    ];
    let with_block = kinds.iter().position(|k| k.0 == "diode_ladder").unwrap();
    for (index, (kind, make)) in kinds.into_iter().enumerate() {
        // The block path is really taken: driven the way the patch drives it (its spec's
        // port counts and first ids), the module accepts the block, so a drift in an
        // override's port counts or numbering cannot silently lose the fast path.
        assert_eq!(
            takes_blocks(make()),
            index < with_block,
            "`{kind}`: tick_frames accepted a block?"
        );
        let build = one_of(kind, make);
        check(kind, &build, 6_000);
    }
}

/// Whether `module` accepts a block laid out as `Patch::tick_block` lays out its ports.
fn takes_blocks(mut module: Box<dyn GraphModule>) -> bool {
    let spec = module.port_spec().clone();
    let (ins, outs) = (spec.inputs.len(), spec.outputs.len());
    let frames = 4;
    let input_rows = vec![0.25; ins.max(1) * frames];
    let mut output_rows = vec![0.0; outs.max(1) * frames];
    let inputs = BlockInputs::new(
        &input_rows,
        frames,
        frames,
        ins,
        spec.inputs.first().map_or(0, |p| p.id),
    );
    let mut outputs = BlockOutputs::new(
        &mut output_rows,
        frames,
        frames,
        outs,
        spec.outputs.first().map_or(0, |p| p.id),
    );
    module.tick_frames(&inputs, &mut outputs, u32::MAX)
}

/// A `Mixer <-> DelayLine` feedback loop between a VCO and a filter: the loop runs sample
/// by sample, the VCO before it and the filter after it node-major. Plus a `UnitDelay`
/// patched into itself, and a node downstream of the loop on a second branch.
fn loop_in_a_chain() -> Patch {
    let mut p = Patch::new(SR);
    let vco = p.add("vco", Vco::new(SR));
    let mix = p.add("mix", Mixer::new(2));
    let delay = p.add("delay", DelayLine::new(SR));
    let svf = p.add("svf", Svf::new(SR));
    let tail = p.add("tail", Vca::new());
    let echo = p.add("echo", UnitDelay::new());
    let out = p.add("out", StereoOutput::new());
    p.connect(vco.out("saw"), mix.in_("ch0")).unwrap();
    p.connect(mix.out("out"), delay.in_("in")).unwrap();
    p.connect_attenuated(delay.out("out"), mix.in_("ch1"), 0.6)
        .unwrap();
    p.connect(delay.out("out"), svf.in_("in")).unwrap();
    p.connect(svf.out("lp"), tail.in_("in")).unwrap();
    p.connect(vco.out("sqr"), echo.in_("in")).unwrap();
    p.connect_attenuated(echo.out("out"), echo.in_("in"), 0.5)
        .unwrap();
    p.connect(tail.out("out"), out.in_("left")).unwrap();
    p.connect(echo.out("out"), out.in_("right")).unwrap();
    p.set_output(out.id());
    p.compile().unwrap();
    p
}

#[test]
fn a_feedback_loop_in_the_middle_of_a_chain() {
    assert!(audible(&loop_in_a_chain, 5_000));
    check("loop_in_a_chain", &loop_in_a_chain, 20_000);
}

/// A breaker whose deferred input comes from a node off its own cycle (scheduled late
/// because it waits on a second loop): the deferred edge still reads a frame late.
fn deferred_from_off_the_cycle() -> Patch {
    let mut p = Patch::new(SR);
    // Added first, so the sort picks it as the breaker when everything stalls.
    let d = p.add("d", UnitDelay::new());
    let q = p.add("q", Attenuverter::new());
    let r = p.add("r", Mixer::new(2));
    let s = p.add("s", DelayLine::new(SR));
    let late = p.add("late", Vca::new());
    let vco = p.add("vco", Vco::new(SR));
    let out = p.add("out", StereoOutput::new());
    // d <-> q, a loop through the first breaker.
    p.connect(d.out("out"), q.in_("in")).unwrap();
    p.connect(q.out("out"), d.in_("in")).unwrap();
    // r <-> s, a second loop, which `late` hangs off and feeds `d`.
    p.connect(vco.out("tri"), r.in_("ch0")).unwrap();
    p.connect(r.out("out"), s.in_("in")).unwrap();
    p.connect_attenuated(s.out("out"), r.in_("ch1"), 0.5)
        .unwrap();
    p.connect(s.out("out"), late.in_("in")).unwrap();
    p.connect_attenuated(late.out("out"), d.in_("in"), 0.25)
        .unwrap();
    p.connect(q.out("out"), out.in_("left")).unwrap();
    p.connect(late.out("out"), out.in_("right")).unwrap();
    p.set_output(out.id());
    p.compile().unwrap();
    p
}

#[test]
fn a_deferred_edge_from_off_the_cycle() {
    assert!(audible(&deferred_from_off_the_cycle, 5_000));
    check(
        "deferred_from_off_the_cycle",
        &deferred_from_off_the_cycle,
        10_000,
    );
}

/// Two *unseeded* noise sources share the thread-wide stream, with node-major work
/// between and around them: they must still draw in `tick`'s interleaved order.
fn two_unseeded_noises() -> Patch {
    let mut p = Patch::new(SR);
    let n1 = p.add("n1", NoiseGenerator::new());
    let f1 = p.add("f1", Svf::new(SR));
    let n2 = p.add("n2", NoiseGenerator::new());
    let ks = p.add("ks", KarplusStrong::new(SR));
    let gate = gate_lfo(&mut p, "gate", 0.8);
    let mix = p.add("mix", Mixer::new(3));
    let vca = p.add("vca", Vca::new());
    let out = p.add("out", StereoOutput::new());
    p.connect(n1.out("white"), f1.in_("in")).unwrap();
    p.connect(f1.out("bp"), mix.in_("ch0")).unwrap();
    p.connect(n2.out("pink"), mix.in_("ch1")).unwrap();
    p.connect(gate.out("sqr"), ks.in_("trigger")).unwrap();
    p.connect(ks.out("out"), mix.in_("ch2")).unwrap();
    p.connect(mix.out("out"), vca.in_("in")).unwrap();
    p.connect(vca.out("out"), out.in_("left")).unwrap();
    p.set_output(out.id());
    p.compile().unwrap();
    p
}

#[test]
fn unseeded_noise_sources_draw_in_tick_order() {
    let reseed = || quiver::rng::seed(0x5EED);
    check_with(
        "two_unseeded_noises",
        &two_unseeded_noises,
        10_000,
        &Vec::new,
        &reseed,
    );
    // And seeded, where each draws from its own stream and runs node-major.
    let seeded = || {
        let mut p = two_unseeded_noises();
        p.seed(42);
        p
    };
    check("two_seeded_noises", &seeded, 10_000);
}

#[test]
fn edits_between_blocks_land_on_the_same_frame() {
    let edits = || -> Vec<(usize, Edit)> {
        vec![
            // A knob turned (applied in place), twice, and a default restored.
            (
                1_000,
                Box::new(|p: &mut Patch| {
                    let svf = p.get_node_id_by_name("svf").unwrap();
                    assert!(p.set_param_by_id(svf, "cutoff", 0.8));
                    let env = p.get_node_id_by_name("env").unwrap();
                    assert!(p.set_param_by_id(env, "release", 0.05));
                }),
            ),
            (
                3_001,
                Box::new(|p: &mut Patch| {
                    let vca = p.get_node_id_by_name("vca").unwrap();
                    assert!(p.set_param_by_id(vca, "gain", 1.5));
                }),
            ),
            // A recompile mid-run: a new node and cable (dirty, compiled by the next call).
            (
                5_555,
                Box::new(|p: &mut Patch| {
                    let lfo = p.get_handle_by_name("lfo").unwrap();
                    let svf = p.get_handle_by_name("svf").unwrap();
                    let slew = p.add("slew", SlewLimiter::new(SR));
                    p.connect(lfo.out("tri"), slew.in_("in")).unwrap();
                    p.connect_attenuated(slew.out("out"), svf.in_("res"), 0.1)
                        .unwrap();
                }),
            ),
            // An explicit compile, which keeps the module states.
            (
                7_000,
                Box::new(|p: &mut Patch| {
                    p.compile().unwrap();
                }),
            ),
        ]
    };
    check_with("edits", &subtractive, 10_000, &edits, &|| {});
}

#[test]
fn tick_and_tick_block_interleave() {
    let mut a = loop_in_a_chain();
    let expected: Vec<(u64, u64)> = (0..5_000)
        .map(|_| {
            let (l, r) = a.tick();
            (l.to_bits(), r.to_bits())
        })
        .collect();
    let mut b = loop_in_a_chain();
    let mut got = Vec::new();
    let mut l = [0.0; 300];
    let mut r = [0.0; 300];
    while got.len() < 5_000 {
        // 300 frames by block, then 17 by tick.
        b.tick_block(&mut l, &mut r);
        got.extend(l.iter().zip(&r).map(|(l, r)| (l.to_bits(), r.to_bits())));
        for _ in 0..17 {
            let (l, r) = b.tick();
            got.push((l.to_bits(), r.to_bits()));
        }
    }
    got.truncate(5_000);
    assert!(expected == got, "mixing tick and tick_block diverged");
    assert_eq!(port_bits(&a), {
        // Bring `a` level with `b` before comparing ports.
        let mut a2 = loop_in_a_chain();
        for _ in 0..got.len() {
            a2.tick();
        }
        port_bits(&a2)
    });
}

#[test]
fn kept_live_and_masked_ports_read_what_tick_leaves() {
    // A VCO whose unpatched outputs are masked off (they keep 0.0), one of them pinned live.
    let build = || {
        let mut p = Patch::new(SR);
        let vco = p.add("vco", Vco::new(SR));
        let lfo = p.add("lfo", Lfo::new(SR));
        let out = p.add("out", StereoOutput::new());
        p.connect(vco.out("saw"), out.in_("left")).unwrap();
        p.connect(lfo.out("tri"), vco.in_("fm")).unwrap();
        let sin = vco.out("sin");
        p.keep_output_live(sin.node, sin.port);
        p.set_output(out.id());
        p.compile().unwrap();
        p
    };
    check("kept_live", &build, 3_000);
}

/// A module of no fixed layout: input ids that do not count up by one, so the patch never
/// offers it blocks, and a module that declines them.
struct Odd {
    spec: PortSpec,
    acc: f64,
}

impl Odd {
    fn new(ids: [u32; 2]) -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(ids[0], "a", SignalKind::Audio),
                    PortDef::new(ids[1], "b", SignalKind::CvBipolar).with_default(0.5),
                ],
                outputs: vec![
                    PortDef::new(20, "x", SignalKind::Audio),
                    PortDef::new(21, "y", SignalKind::Audio),
                ],
            },
            acc: 0.0,
        }
    }
}

impl GraphModule for Odd {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }
    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let a = inputs.get_or(self.spec.inputs[0].id, 0.0);
        let b = inputs.get_or(self.spec.inputs[1].id, 0.0);
        self.acc = self.acc * 0.99 + a * b;
        outputs.set(20, self.acc);
        // `y` only on some samples: unwritten ones keep the last value.
        if a > 0.0 {
            outputs.set(21, a - b);
        }
    }
    fn reset(&mut self) {
        self.acc = 0.0;
    }
    fn set_sample_rate(&mut self, _: f64) {}
}

#[test]
fn modules_without_a_block_path() {
    for ids in [[0, 1], [3, 7]] {
        let build = move || {
            let mut p = Patch::new(SR);
            let vco = p.add("vco", Vco::new(SR));
            let odd = p.add("odd", Odd::new(ids));
            let vca = p.add("vca", Vca::new());
            let out = p.add("out", StereoOutput::new());
            p.connect(vco.out("sin"), odd.in_("a")).unwrap();
            p.connect(odd.out("x"), vca.in_("in")).unwrap();
            p.connect(odd.out("y"), out.in_("right")).unwrap();
            p.connect(vca.out("out"), out.in_("left")).unwrap();
            p.set_output(out.id());
            p.compile().unwrap();
            p
        };
        check("odd", &build, 4_000);
    }
}

#[test]
fn silence_without_an_output_or_a_schedule() {
    // No output node: silence, but the modules still run, as with tick.
    let no_output = || {
        let mut p = subtractive();
        let vco = p.get_node_id_by_name("vco").unwrap();
        p.set_output(vco);
        p
    };
    check("vco_as_output", &no_output, 2_000);

    let mut p = Patch::new(SR);
    let a = p.add("a", Vca::new());
    let b = p.add("b", Vca::new());
    p.connect(a.out("out"), b.in_("in")).unwrap();
    p.connect(b.out("out"), a.in_("in")).unwrap();
    let mut l = [1.0; 100];
    let mut r = [1.0; 100];
    p.tick_block(&mut l, &mut r);
    assert!(p.last_compile_error().is_some(), "a cycle with no breaker");
    assert!(l.iter().chain(&r).all(|&x| x == 0.0));

    let mut empty = Patch::new(SR);
    let mut l = [1.0; 10];
    let mut r = [1.0; 5];
    empty.tick_block(&mut l, &mut r);
    assert!(r.iter().all(|&x| x == 0.0));
    assert!(
        l[..5].iter().all(|&x| x == 0.0),
        "only the common length is written"
    );
    assert!(l[5..].iter().all(|&x| x == 1.0));
}

#[test]
fn a_module_tick_frames_can_be_driven_by_hand() {
    // The public block buffers, outside a patch: a VCA's block equals its ticks.
    let mut by_tick = Vca::new();
    let mut by_block = Vca::new();
    let frames = 5;
    let ins: Vec<f64> = (0..4 * 8)
        .map(|i| match i / 8 {
            0 => (i % 8) as f64 - 2.0,
            1 => 7.5,
            2 => 0.0,
            _ => 1.5,
        })
        .collect();
    let mut outs = vec![9.0; 8];
    {
        let inputs = BlockInputs::new(&ins, 8, frames, 4, 0);
        let mut outputs = BlockOutputs::new(&mut outs, 8, frames, 1, 10);
        assert!(by_block.tick_frames(&inputs, &mut outputs, u32::MAX));
        assert_eq!(outputs.written(), 1);
    }
    for t in 0..frames {
        let mut pin = PortValues::new();
        for k in 0..4 {
            pin.set(k as u32, ins[k * 8 + t]);
        }
        let mut pout = PortValues::new();
        by_tick.tick(&pin, &mut pout);
        assert_eq!(pout.get(10).unwrap().to_bits(), outs[t].to_bits());
    }
    assert_eq!(outs[frames], 9.0, "nothing past the block is touched");
    // A layout the module does not have is declined, untouched.
    let inputs = BlockInputs::new(&ins, 8, frames, 3, 0);
    let mut outputs = BlockOutputs::new(&mut outs, 8, frames, 1, 10);
    assert!(!by_block.tick_frames(&inputs, &mut outputs, u32::MAX));
    assert_eq!(outputs.written(), 0);
}

#[test]
fn reset_seed_remove_and_disconnect_between_blocks() {
    let edits = || -> Vec<(usize, Edit)> {
        vec![
            (1_500, Box::new(|p: &mut Patch| p.reset())),
            (
                2_222,
                Box::new(|p: &mut Patch| {
                    let lfo = p.get_node_id_by_name("lfo").unwrap();
                    p.remove(lfo).unwrap();
                }),
            ),
            (
                4_000,
                Box::new(|p: &mut Patch| {
                    let svf = p.get_handle_by_name("svf").unwrap();
                    let vca = p.get_handle_by_name("vca").unwrap();
                    p.disconnect_ports(svf.out("lp"), vca.in_("in")).unwrap();
                    p.connect(svf.out("bp"), vca.in_("in")).unwrap();
                }),
            ),
            (5_001, Box::new(|p: &mut Patch| p.reset())),
        ]
    };
    check_with(
        "reset_remove_disconnect",
        &subtractive,
        7_000,
        &edits,
        &|| {},
    );

    // Seeding after compile: the unseeded noise sources' group is kept until the next
    // recompile (here, a disconnect), and both renders agree throughout.
    let edits = || -> Vec<(usize, Edit)> {
        vec![
            (1_000, Box::new(|p: &mut Patch| p.seed(7))),
            (2_500, Box::new(|p: &mut Patch| p.reset())),
            (
                4_100,
                Box::new(|p: &mut Patch| {
                    let n2 = p.get_handle_by_name("n2").unwrap();
                    let mix = p.get_handle_by_name("mix").unwrap();
                    p.disconnect_ports(n2.out("pink"), mix.in_("ch1")).unwrap();
                }),
            ),
        ]
    };
    check_with(
        "seed_after_compile",
        &two_unseeded_noises,
        6_000,
        &edits,
        &|| quiver::rng::seed(0x5EED),
    );
}

/// An `ExternalOutput` writing the cell an `ExternalInput` of the same patch reads: the
/// reader must see the writer's value frame by frame, as with `tick` — both name the cell
/// (`shares_state`), so they run as one group. Two plain readers on host knobs alongside
/// share nothing anyone writes.
fn loopback() -> Patch {
    let mut p = Patch::new(SR);
    let cell = Arc::new(AtomicF64::new(0.0));
    let knob = Arc::new(AtomicF64::new(0.5));
    let lfo = p.add("lfo", Lfo::new(SR));
    let tx = p.add(
        "tx",
        ExternalOutput::new(Arc::clone(&cell), SignalKind::Audio),
    );
    let rx = p.add(
        "rx",
        ExternalInput::new(Arc::clone(&cell), SignalKind::Audio),
    );
    let k1 = p.add("k1", ExternalInput::cv(Arc::clone(&knob)));
    let k2 = p.add("k2", ExternalInput::cv(knob));
    let vca = p.add("vca", Vca::new());
    let out = p.add("out", StereoOutput::new());
    p.connect(lfo.out("sin"), tx.in_("in")).unwrap();
    p.connect(rx.out("out"), vca.in_("in")).unwrap();
    p.connect(k1.out("out"), vca.in_("gain")).unwrap();
    p.connect(vca.out("out"), out.in_("left")).unwrap();
    p.connect(k2.out("out"), out.in_("right")).unwrap();
    p.set_output(out.id());
    p.compile().unwrap();
    p
}

#[test]
fn an_external_output_looped_into_an_external_input() {
    assert!(audible(&loopback, 2_000));
    check("loopback", &loopback, 5_000);
    // Built the other way round (the reader added first) too.
    let reversed = || {
        let mut p = Patch::new(SR);
        let cell = Arc::new(AtomicF64::new(0.0));
        let rx = p.add(
            "rx",
            ExternalInput::new(Arc::clone(&cell), SignalKind::Audio),
        );
        let out = p.add("out", StereoOutput::new());
        let lfo = p.add("lfo", Lfo::new(SR));
        let tx = p.add("tx", ExternalOutput::new(cell, SignalKind::Audio));
        p.connect(lfo.out("tri"), tx.in_("in")).unwrap();
        p.connect(rx.out("out"), out.in_("left")).unwrap();
        p.set_output(out.id());
        p.compile().unwrap();
        p
    };
    check("loopback_reversed", &reversed, 5_000);
}

#[test]
#[should_panic(expected = "a block of frames must fit in its stride")]
fn block_outputs_reject_frames_longer_than_their_stride() {
    let mut out = [0.0; 8];
    let _ = BlockOutputs::new(&mut out, 0, 8, 1, 10);
}

#[test]
#[should_panic(expected = "a block of frames must fit in its stride")]
fn block_inputs_reject_frames_longer_than_their_stride() {
    let data = [0.0; 8];
    let _ = BlockInputs::new(&data, 2, 8, 1, 0);
}

#[test]
fn a_single_port_block_with_a_wide_stride_drives_by_hand() {
    // One port, its frames at the front of a wider stride: the built-ins take it.
    let ins = [1.0, 2.0, 3.0, 9.0];
    let mut outs = [7.0; 6];
    let inputs = BlockInputs::new(&ins, 6, 3, 1, 0);
    let mut outputs = BlockOutputs::new(&mut outs, 6, 3, 1, 10);
    assert!(Offset::new(1.0).tick_frames(&inputs, &mut outputs, u32::MAX));
    assert_eq!(outs, [2.0, 3.0, 4.0, 7.0, 7.0, 7.0]);
}
