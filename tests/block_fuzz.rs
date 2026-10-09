//! Random patches, rendered by `tick` and by `tick_block`, bit for bit.
//!
//! A deterministic, seeded property test over every module the registry builds: random
//! nodes, random cables (mostly forward, some backward into cycles, some attenuated and
//! offset, several into one input), some inputs left unpatched, some knobs set before
//! compile and some mid-run between blocks, some outputs kept live, the patch seeded or
//! left on the thread-wide random stream. Each patch is rendered for 700 frames by `tick`
//! and by `tick_block` in ragged blocks (1, 7, 64, 65, 128, 3, 200 frames), and every
//! sample and every output port's value after must agree to the bit. A patch that does not
//! compile (a cycle with no breaker) must render silence both ways.
#![cfg(feature = "std")]

use quiver::prelude::*;
use quiver::rng::Rng;
use quiver::serialize::ModuleRegistry;

const SR: f64 = 44_100.0;
const PATCHES: u64 = 500;
const FRAMES: usize = 700;

/// A cable: from (node, output index) to (node, input index), with an optional
/// (attenuation, offset).
type CablePlan = (usize, usize, usize, usize, Option<(f64, f64)>);

struct Plan {
    types: Vec<String>,
    cables: Vec<CablePlan>,
    /// (node, output index) kept live.
    live: Vec<(usize, usize)>,
    /// (node, input index, value) set before the render.
    knobs: Vec<(usize, usize, f64)>,
    /// (frame, node, input index, value) set mid-render.
    edits: Vec<(usize, usize, usize, f64)>,
    seeded: bool,
    out: usize,
}

fn below(r: &mut Rng, n: usize) -> usize {
    (r.next_u64() % n as u64) as usize
}

fn make_plan(r: &mut Rng, reg: &ModuleRegistry, kinds: &[String]) -> Plan {
    let n = 2 + below(r, 9);
    let mut types: Vec<String> = (0..n)
        .map(|_| kinds[below(r, kinds.len())].clone())
        .collect();
    types.push("stereo_output".to_string());
    let out = types.len() - 1;
    let specs: Vec<PortSpec> = types
        .iter()
        .map(|t| reg.instantiate(t, SR).unwrap().port_spec().clone())
        .collect();

    let mut cables = Vec::new();
    for _ in 0..below(r, 3 * types.len() + 1) {
        let (mut a, mut b) = (below(r, types.len()), below(r, types.len()));
        // Mostly forward in insertion order; the rest may close cycles.
        if r.next_f64() < 0.95 && a > b {
            std::mem::swap(&mut a, &mut b);
        }
        // A node patched into itself is a cycle unless it is a delay: keep a few.
        if a == b && r.next_f64() < 0.9 {
            continue;
        }
        if specs[a].outputs.is_empty() || specs[b].inputs.is_empty() {
            continue;
        }
        let o = below(r, specs[a].outputs.len());
        let i = below(r, specs[b].inputs.len());
        let modulated =
            (r.next_f64() < 0.3).then(|| (r.next_f64() * 4.0 - 2.0, r.next_f64() * 2.0 - 1.0));
        cables.push((a, o, b, i, modulated));
    }
    // Something reaches the output, on each side.
    for side in 0..2 {
        let a = below(r, types.len() - 1);
        if !specs[a].outputs.is_empty() {
            cables.push((a, below(r, specs[a].outputs.len()), out, side, None));
        }
    }
    let mut live = Vec::new();
    let mut knobs = Vec::new();
    for (a, spec) in specs.iter().enumerate() {
        for o in 0..spec.outputs.len() {
            if r.next_f64() < 0.1 {
                live.push((a, o));
            }
        }
        for i in 0..spec.inputs.len() {
            if r.next_f64() < 0.2 {
                knobs.push((a, i, r.next_f64() * 10.0 - 2.0));
            }
        }
    }
    let mut edits = Vec::new();
    for _ in 0..below(r, 4) {
        let a = below(r, types.len());
        if !specs[a].inputs.is_empty() {
            let i = below(r, specs[a].inputs.len());
            edits.push((below(r, 600), a, i, r.next_f64() * 10.0 - 2.0));
        }
    }
    Plan {
        types,
        cables,
        live,
        knobs,
        edits,
        seeded: r.next_f64() < 0.3,
        out,
    }
}

fn spec_of(patch: &Patch, id: NodeId) -> PortSpec {
    patch
        .nodes()
        .find(|(i, _, _)| *i == id)
        .unwrap()
        .2
        .port_spec()
        .clone()
}

fn build(plan: &Plan, reg: &ModuleRegistry) -> (Patch, Vec<NodeId>) {
    let mut p = Patch::new(SR);
    let ids: Vec<NodeId> = plan
        .types
        .iter()
        .enumerate()
        .map(|(k, t)| {
            p.add_boxed(format!("n{k}"), reg.instantiate(t, SR).unwrap())
                .id()
        })
        .collect();
    for &(a, o, b, i, modulated) in &plan.cables {
        let from = PortRef {
            node: ids[a],
            port: spec_of(&p, ids[a]).outputs[o].id,
        };
        let to = PortRef {
            node: ids[b],
            port: spec_of(&p, ids[b]).inputs[i].id,
        };
        // A cable the validation refuses is simply not there, on both sides.
        let _ = match modulated {
            Some((att, off)) => p.connect_modulated(from, to, att, off),
            None => p.connect(from, to),
        };
    }
    for &(a, o) in &plan.live {
        let port = spec_of(&p, ids[a]).outputs[o].id;
        p.keep_output_live(ids[a], port);
    }
    for &(a, i, v) in &plan.knobs {
        let name = spec_of(&p, ids[a]).inputs[i].name.clone();
        p.set_param_by_id(ids[a], &name, v);
    }
    p.set_output(ids[plan.out]);
    if plan.seeded {
        p.seed(99);
    }
    (p, ids)
}

fn edit(patch: &mut Patch, ids: &[NodeId], plan: &Plan, frame: usize) {
    for &(at, n, i, v) in &plan.edits {
        if at == frame {
            let name = spec_of(patch, ids[n]).inputs[i].name.clone();
            patch.set_param_by_id(ids[n], &name, v);
        }
    }
}

fn port_bits(p: &Patch) -> Vec<(String, u32, Option<u64>)> {
    let mut ports = Vec::new();
    for (id, name, m) in p.nodes() {
        for o in &m.port_spec().outputs {
            ports.push((
                name.to_string(),
                o.id,
                p.get_output_value(id, o.id).map(f64::to_bits),
            ));
        }
    }
    ports.sort();
    ports
}

#[test]
fn random_patches_render_the_same_by_tick_and_by_tick_block() {
    let reg = ModuleRegistry::new();
    // Host-fed sources render nothing of their own here.
    let skip = ["audio_input", "external_input", "osc_input"];
    let kinds: Vec<String> = reg
        .list_modules()
        .map(|m| m.type_id.clone())
        .filter(|t| !skip.contains(&t.as_str()))
        .collect();
    assert!(kinds.len() > 40, "the registry lists the modules");

    let (mut audible, mut compiled) = (0, 0);
    let mut failures = Vec::new();
    for case in 0..PATCHES {
        let mut r = Rng::from_seed(case * 104_729 + 17);
        let plan = make_plan(&mut r, &reg, &kinds);

        quiver::rng::seed(12_345);
        let (mut a, ids_a) = build(&plan, &reg);
        let mut by_tick = Vec::with_capacity(FRAMES);
        for frame in 0..FRAMES {
            edit(&mut a, &ids_a, &plan, frame);
            let (l, r) = a.tick();
            by_tick.push((l.to_bits(), r.to_bits()));
        }

        quiver::rng::seed(12_345);
        let (mut b, ids_b) = build(&plan, &reg);
        let mut by_block = Vec::with_capacity(FRAMES);
        let sizes = [1usize, 7, 64, 65, 128, 3, 200];
        let (mut done, mut k) = (0, case as usize);
        let mut left = [0.0; 200];
        let mut right = [0.0; 200];
        while done < FRAMES {
            edit(&mut b, &ids_b, &plan, done);
            let mut n = sizes[k % sizes.len()].min(FRAMES - done);
            k += 1;
            if let Some(next) = plan.edits.iter().map(|e| e.0).filter(|&e| e > done).min() {
                n = n.min(next - done);
            }
            b.tick_block(&mut left[..n], &mut right[..n]);
            by_block.extend(
                left[..n]
                    .iter()
                    .zip(&right[..n])
                    .map(|(l, r)| (l.to_bits(), r.to_bits())),
            );
            done += n;
        }

        if a.last_compile_error().is_none() {
            compiled += 1;
        }
        if by_tick.iter().any(|&(l, _)| f64::from_bits(l) != 0.0) {
            audible += 1;
        }
        let first = by_tick.iter().zip(&by_block).position(|(x, y)| x != y);
        if first.is_some() || port_bits(&a) != port_bits(&b) {
            failures.push(format!(
                "case {case}: first differing frame {first:?}; types {:?}, seeded {}, \
                 cables {:?}, edits {:?}",
                plan.types,
                plan.seeded,
                plan.cables
                    .iter()
                    .map(|c| (c.0, c.1, c.2, c.3))
                    .collect::<Vec<_>>(),
                plan.edits,
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {PATCHES} patches differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // The set means something: plenty of patches compile (a random backward cable often
    // closes a cycle with no delay in it, which renders silence both ways) and make sound.
    eprintln!("{compiled} of {PATCHES} compiled, {audible} made sound");
    assert!(compiled > PATCHES / 2, "{compiled} of {PATCHES} compiled");
    assert!(audible > PATCHES / 5, "{audible} of {PATCHES} made sound");
}
