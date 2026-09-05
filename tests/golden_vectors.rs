//! Golden-vector bit-exactness gate for the graph interpreter.
//!
//! Downstream consumers depend on `(patch, sample_rate) -> bit-identical samples`: the same
//! compiled patch must render byte-for-byte the same audio across refactors of the routing
//! engine. Interpreter-level optimizations (dense port storage, scatter-by-slice, hoisted
//! module handles, dead-output elimination, …) are all supposed to be *purely* internal, so
//! any change in these hashes is a regression, not a rebaseline.
//!
//! Each test renders a representative patch for one second at 44.1 kHz and hashes the raw
//! IEEE-754 bit patterns of every stereo sample with FNV-1a/64. The constants below were
//! captured from the pre-optimization engine; **do not update them** to make a failing test
//! pass — a mismatch means the numerics moved.
//!
//! **Deliberate rebaseline, 0.4.0 (Q-N4):** `delay_chorus` changed from
//! `0x0bd3_4a0e_47ac_ee37` to `0x8b20_c2b4_31f7_8812`. Until 0.3.x the scheduler dropped
//! *every* cable into a cycle-breaker from the topological sort, so in this acyclic patch the
//! `DelayLine` was scheduled before the VCO (the delay was `add`ed first) and read the VCO's
//! *previous* sample — one sample of latency that depended on node insertion order. The
//! scheduler now defers only cables that close a cycle, so the delay reads the current sample.
//! The other four patches contain no cycle-breaker and are bit-identical to 0.1.1.
//!
//! Coverage is chosen to exercise every path in `Patch::tick_step`:
//!
//! | patch | exercises |
//! |---|---|
//! | `subtractive` | VCO -> SVF -> VCA chain gated by an ADSR (one cable per input) |
//! | `diode_ladder` | nonlinear ladder filter, attenuated + offset cables |
//! | `lfo_modulated` | one LFO fanned out to three destinations (`mult`) |
//! | `delay_chorus` | a `DelayLine` on an acyclic path (no deferred edge) and stereo scatter |
//! | `noise` | seeded global RNG, partially consumed multi-output module |
//! | `multi_cable` | several cables summed into one input, plain / attenuated / offset |
//! | `feedback_loop` | a genuine `mixer <-> delay` cycle: the one deferred (previous-tick) edge |
//! | `poly` | a four-voice `PolyPatch` with overlapping note-ons/offs |
//! | `set_param_mid_render` | `set_param_by_id` after compile, applied in place mid-render |
//!
//! `tick_block_matches_tick_bitwise` additionally checks that the block entry point is
//! the per-sample engine bit for bit, with ragged block sizes, over every patch here.
//!
//! `StereoOutput.right` is normalled to `left` in most patches, so the normalled-input
//! resolution pass is covered too.
#![cfg(feature = "std")]

use quiver::modules::{Chorus, DelayLine, Mixer};
use quiver::prelude::*;
use quiver::rng;

const SAMPLE_RATE: f64 = 44100.0;
/// One second of audio per patch — long enough for envelopes, LFO cycles and the delay
/// line to fully engage, short enough to stay well inside the test-suite time budget.
const FRAMES: usize = 44100;

/// FNV-1a/64 over the little-endian bit patterns of every rendered sample.
///
/// Hashing `f64::to_bits` (not the float itself) is what makes this a *bit*-exactness
/// gate: `-0.0`, `+0.0` and every NaN payload hash differently.
#[derive(Debug)]
struct SampleHasher {
    state: u64,
}

impl SampleHasher {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self {
            state: Self::OFFSET_BASIS,
        }
    }

    fn write_sample(&mut self, sample: f64) {
        for byte in sample.to_bits().to_le_bytes() {
            self.state ^= byte as u64;
            self.state = self.state.wrapping_mul(Self::PRIME);
        }
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

/// Render `FRAMES` stereo samples and return their FNV-1a/64 hash.
///
/// Also asserts the render is audible and finite: a hash on its own would happily lock in
/// silence or a wall of NaNs, so a patch that stops producing signal must fail loudly
/// rather than merely fail to match.
fn render_hash(patch: &mut Patch) -> u64 {
    render_hash_with(patch, |_, _| {})
}

/// [`render_hash`] with a hook run before each frame (for mid-render edits).
fn render_hash_with(patch: &mut Patch, mut before_frame: impl FnMut(&mut Patch, usize)) -> u64 {
    patch.compile().expect("golden patch must compile");
    let mut hasher = SampleHasher::new();
    let mut peak = 0.0f64;
    for frame in 0..FRAMES {
        before_frame(patch, frame);
        let (left, right) = patch.tick();
        assert!(
            left.is_finite() && right.is_finite(),
            "golden patch produced a non-finite sample"
        );
        peak = peak.max(left.abs()).max(right.abs());
        hasher.write_sample(left);
        hasher.write_sample(right);
    }
    assert!(peak > 1e-3, "golden patch rendered (near-)silence: {peak}");
    hasher.finish()
}

/// Assert a rendered hash, reporting the observed value so a genuine numeric change can be
/// diagnosed (never blindly pasted back in).
fn assert_golden(name: &str, actual: u64, expected: u64) {
    assert_eq!(
        actual, expected,
        "golden vector `{name}` changed: expected {expected:#018x}, got {actual:#018x}. \
         The interpreter is supposed to be bit-exact — investigate before rebaselining."
    );
}

/// VCO -> SVF -> VCA, with an ADSR gated by a square LFO driving the VCA's CV.
fn patch_subtractive() -> Patch {
    let mut patch = Patch::new(SAMPLE_RATE);

    let pitch = patch.add("pitch", Offset::new(-0.25));
    let vco = patch.add("vco", Vco::new(SAMPLE_RATE));
    let svf = patch.add("svf", Svf::new(SAMPLE_RATE));
    let gate_lfo = patch.add("gate_lfo", Lfo::new(SAMPLE_RATE));
    let adsr = patch.add("adsr", Adsr::new(SAMPLE_RATE));
    let vca = patch.add("vca", Vca::new());
    let out = patch.add("out", StereoOutput::new());

    patch.connect(pitch.out("out"), vco.in_("voct")).unwrap();
    patch.connect(vco.out("saw"), svf.in_("in")).unwrap();
    patch.connect(svf.out("lp"), vca.in_("in")).unwrap();
    // ±5 V square -> 0/10 V gate.
    patch
        .connect_modulated(gate_lfo.out("sqr"), adsr.in_("gate"), 1.0, 5.0)
        .unwrap();
    patch.connect(adsr.out("env"), vca.in_("cv")).unwrap();
    patch.connect(vca.out("out"), out.in_("left")).unwrap();

    patch.set_param_by_id(gate_lfo.id(), "rate", 0.75);
    patch.set_param_by_id(svf.id(), "cutoff", 0.45);
    patch.set_param_by_id(svf.id(), "res", 0.6);
    patch.set_param_by_id(adsr.id(), "attack", 0.15);
    patch.set_param_by_id(adsr.id(), "decay", 0.3);
    patch.set_param_by_id(adsr.id(), "sustain", 0.4);
    patch.set_param_by_id(adsr.id(), "release", 0.35);

    patch.set_output(out.id());
    patch
}

/// A square VCO through the analog-modelled diode ladder, its cutoff swept by an LFO
/// through an attenuated + offset cable.
fn patch_diode_ladder() -> Patch {
    let mut patch = Patch::new(SAMPLE_RATE);

    let pitch = patch.add("pitch", Offset::new(0.5));
    let vco = patch.add("vco", Vco::new(SAMPLE_RATE));
    let lfo = patch.add("lfo", Lfo::new(SAMPLE_RATE));
    let ladder = patch.add("ladder", DiodeLadderFilter::new(SAMPLE_RATE));
    let out = patch.add("out", StereoOutput::new());

    patch.connect(pitch.out("out"), vco.in_("voct")).unwrap();
    patch.connect(vco.out("sqr"), ladder.in_("in")).unwrap();
    patch
        .connect_modulated(lfo.out("sin"), ladder.in_("fm"), 0.4, 0.5)
        .unwrap();
    patch.connect(ladder.out("out"), out.in_("left")).unwrap();

    patch.set_param_by_id(lfo.id(), "rate", 0.55);
    patch.set_param_by_id(ladder.id(), "cutoff", 0.35);
    patch.set_param_by_id(ladder.id(), "res", 0.7);
    patch.set_param_by_id(ladder.id(), "drive", 0.6);

    patch.set_output(out.id());
    patch
}

/// One LFO fanned out to a VCO's linear FM, an SVF's cutoff and a VCA's CV.
fn patch_lfo_modulated() -> Patch {
    let mut patch = Patch::new(SAMPLE_RATE);

    let pitch = patch.add("pitch", Offset::new(-1.0));
    let lfo = patch.add("lfo", Lfo::new(SAMPLE_RATE));
    let vco = patch.add("vco", Vco::new(SAMPLE_RATE));
    let svf = patch.add("svf", Svf::new(SAMPLE_RATE));
    let vca = patch.add("vca", Vca::new());
    let out = patch.add("out", StereoOutput::new());

    patch.connect(pitch.out("out"), vco.in_("voct")).unwrap();
    patch.connect(vco.out("tri"), svf.in_("in")).unwrap();
    patch.connect(svf.out("bp"), vca.in_("in")).unwrap();
    patch
        .mult(lfo.out("sin"), &[vco.in_("fm_lin"), svf.in_("fm")])
        .unwrap();
    patch
        .connect_modulated(lfo.out("sin_uni"), vca.in_("cv"), 0.5, 2.5)
        .unwrap();
    patch.connect(vca.out("out"), out.in_("left")).unwrap();

    patch.set_param_by_id(lfo.id(), "rate", 0.62);
    patch.set_param_by_id(svf.id(), "cutoff", 0.55);
    patch.set_param_by_id(svf.id(), "res", 0.35);

    patch.set_output(out.id());
    patch
}

/// VCO -> delay line (a feedback cycle-breaker) -> chorus, taken out in true stereo.
fn patch_delay_chorus() -> Patch {
    let mut patch = Patch::new(SAMPLE_RATE);

    let pitch = patch.add("pitch", Offset::new(0.25));
    let vco = patch.add("vco", Vco::new(SAMPLE_RATE));
    let delay = patch.add("delay", DelayLine::new(SAMPLE_RATE));
    let chorus = patch.add("chorus", Chorus::new(SAMPLE_RATE));
    let out = patch.add("out", StereoOutput::new());

    patch.connect(pitch.out("out"), vco.in_("voct")).unwrap();
    patch.connect(vco.out("tri"), delay.in_("in")).unwrap();
    patch.connect(delay.out("out"), chorus.in_("in")).unwrap();
    patch.connect(chorus.out("left"), out.in_("left")).unwrap();
    patch
        .connect(chorus.out("right"), out.in_("right"))
        .unwrap();

    patch.set_param_by_id(delay.id(), "time", 0.3);
    patch.set_param_by_id(delay.id(), "feedback", 0.55);
    patch.set_param_by_id(delay.id(), "mix", 0.5);
    patch.set_param_by_id(chorus.id(), "rate", 0.4);
    patch.set_param_by_id(chorus.id(), "depth", 0.6);
    patch.set_param_by_id(chorus.id(), "mix", 0.5);

    patch.set_output(out.id());
    patch
}

/// Seeded white noise through a resonant SVF. Only one of the noise generator's four
/// outputs is consumed, mirroring how downstream consumers use multi-output sources.
fn patch_noise() -> Patch {
    let mut patch = Patch::new(SAMPLE_RATE);

    let noise = patch.add("noise", NoiseGenerator::new());
    let lfo = patch.add("lfo", Lfo::new(SAMPLE_RATE));
    let svf = patch.add("svf", Svf::new(SAMPLE_RATE));
    let out = patch.add("out", StereoOutput::new());

    patch.connect(noise.out("white"), svf.in_("in")).unwrap();
    patch
        .connect_modulated(lfo.out("tri"), svf.in_("fm"), 0.6, 0.0)
        .unwrap();
    patch.connect(svf.out("bp"), out.in_("left")).unwrap();

    patch.set_param_by_id(lfo.id(), "rate", 0.5);
    patch.set_param_by_id(svf.id(), "cutoff", 0.4);
    patch.set_param_by_id(svf.id(), "res", 0.8);

    patch.set_output(out.id());
    patch
}

/// Two audio cables summed into one filter input (one plain, one attenuated), two CV
/// cables summed into the filter's `fm` (attenuated, and attenuated + offset), and two
/// into the VCA's `cv` (an offset bias plus a wobble): hardware-style input mixing, the
/// `has_connection` sum path with every coefficient kind.
fn patch_multi_cable() -> Patch {
    let mut patch = Patch::new(SAMPLE_RATE);

    let pitch = patch.add("pitch", Offset::new(-0.5));
    let vco = patch.add("vco", Vco::new(SAMPLE_RATE));
    let vco2 = patch.add("vco2", Vco::new(SAMPLE_RATE));
    let lfo = patch.add("lfo", Lfo::new(SAMPLE_RATE));
    let lfo2 = patch.add("lfo2", Lfo::new(SAMPLE_RATE));
    let svf = patch.add("svf", Svf::new(SAMPLE_RATE));
    let vca = patch.add("vca", Vca::new());
    let out = patch.add("out", StereoOutput::new());

    patch.connect(pitch.out("out"), vco.in_("voct")).unwrap();
    // Same pitch a fifth up (7 semitones) through an offset cable.
    patch
        .connect_modulated(pitch.out("out"), vco2.in_("voct"), 1.0, 7.0 / 12.0)
        .unwrap();
    // Two audio cables into one input.
    patch.connect(vco.out("saw"), svf.in_("in")).unwrap();
    patch
        .connect_attenuated(vco2.out("sqr"), svf.in_("in"), 0.4)
        .unwrap();
    // Two CV cables into one input: attenuated, and attenuated + offset.
    patch
        .connect_attenuated(lfo.out("sin"), svf.in_("fm"), 0.3)
        .unwrap();
    patch
        .connect_modulated(lfo2.out("tri"), svf.in_("fm"), 0.2, 0.25)
        .unwrap();
    // Two CV cables into the VCA: a biased slow wobble plus a square chop.
    patch
        .connect_modulated(lfo.out("sin_uni"), vca.in_("cv"), 0.4, 3.0)
        .unwrap();
    patch
        .connect_attenuated(lfo2.out("sqr"), vca.in_("cv"), 0.2)
        .unwrap();
    patch.connect(svf.out("lp"), vca.in_("in")).unwrap();
    patch.connect(vca.out("out"), out.in_("left")).unwrap();

    patch.set_param_by_id(lfo.id(), "rate", 0.4);
    patch.set_param_by_id(lfo2.id(), "rate", 0.7);
    patch.set_param_by_id(svf.id(), "cutoff", 0.5);
    patch.set_param_by_id(svf.id(), "res", 0.5);

    patch.set_output(out.id());
    patch
}

/// A genuine feedback loop: the delay's output is summed back into its own input through a
/// mixer, so the `mixer <-> delay` cycle compiles only because `DelayLine` breaks it. The
/// deferred edge (`delay.out -> mix.ch1`) is the one place the engine reads a previous-tick
/// value; the dry VCO on the right channel pins the acyclic scatter next to it.
fn patch_feedback_loop() -> Patch {
    let mut patch = Patch::new(SAMPLE_RATE);

    let pitch = patch.add("pitch", Offset::new(-0.25));
    let vco = patch.add("vco", Vco::new(SAMPLE_RATE));
    let mix = patch.add("mix", Mixer::new(2));
    let delay = patch.add("delay", DelayLine::new(SAMPLE_RATE));
    let out = patch.add("out", StereoOutput::new());

    patch.connect(pitch.out("out"), vco.in_("voct")).unwrap();
    patch.connect(vco.out("tri"), mix.in_("ch0")).unwrap();
    // The feedback edge: 45% of the delayed signal back into the delay's input.
    patch
        .connect_attenuated(delay.out("out"), mix.in_("ch1"), 0.45)
        .unwrap();
    patch.connect(mix.out("out"), delay.in_("in")).unwrap();
    patch.connect(delay.out("out"), out.in_("left")).unwrap();
    patch.connect(vco.out("tri"), out.in_("right")).unwrap();

    // The loop supplies the feedback; the module's own feedback stays off and the output
    // is fully wet so the deferred edge is what the left channel hears.
    patch.set_param_by_id(delay.id(), "time", 0.25);
    patch.set_param_by_id(delay.id(), "feedback", 0.0);
    patch.set_param_by_id(delay.id(), "mix", 1.0);

    patch.set_output(out.id());
    patch
}

/// A four-voice `PolyPatch` (VCO -> ADSR-gated VCA per voice) playing a C-major arpeggio
/// with overlapping notes and releases: voice allocation, per-voice controllers, mixing
/// and the smoothed polyphony gain all feed the hash.
fn poly_render_hash() -> u64 {
    let mut poly = PolyPatch::with_voice_fn(4, SAMPLE_RATE, |patch, ctrl| {
        let sr = patch.sample_rate();
        let vco = patch.add("vco", Vco::new(sr));
        let adsr = patch.add("adsr", Adsr::new(sr));
        let vca = patch.add("vca", Vca::new());
        let out = patch.add("out", StereoOutput::new());
        patch.connect(ctrl.out("voct"), vco.in_("voct"))?;
        patch.connect(ctrl.out("gate"), adsr.in_("gate"))?;
        patch.connect(vco.out("saw"), vca.in_("in"))?;
        patch.connect(adsr.out("env"), vca.in_("cv"))?;
        patch.connect(vca.out("out"), out.in_("left"))?;
        patch.set_param_by_id(adsr.id(), "attack", 0.05);
        patch.set_param_by_id(adsr.id(), "decay", 0.2);
        patch.set_param_by_id(adsr.id(), "sustain", 0.5);
        patch.set_param_by_id(adsr.id(), "release", 0.25);
        patch.set_output(out.id());
        Ok(())
    })
    .expect("golden poly patch must build");

    // (frame, note, on?) — an arpeggio whose tails overlap the next onsets.
    const EVENTS: &[(usize, u8, bool)] = &[
        (0, 60, true),
        (4410, 64, true),
        (8820, 67, true),
        (13230, 72, true),
        (17640, 60, false),
        (22050, 64, false),
        (26460, 67, false),
        (30870, 72, false),
        (35280, 55, true),
        (39690, 55, false),
    ];

    let mut hasher = SampleHasher::new();
    let mut peak = 0.0f64;
    let mut next_event = 0;
    for frame in 0..FRAMES {
        while next_event < EVENTS.len() && EVENTS[next_event].0 == frame {
            let (_, note, on) = EVENTS[next_event];
            if on {
                poly.note_on(note, 100);
            } else {
                poly.note_off(note);
            }
            next_event += 1;
        }
        let (left, right) = poly.tick();
        assert!(left.is_finite() && right.is_finite());
        peak = peak.max(left.abs()).max(right.abs());
        hasher.write_sample(left);
        hasher.write_sample(right);
    }
    assert!(
        peak > 1e-3,
        "golden poly patch rendered (near-)silence: {peak}"
    );
    hasher.finish()
}

#[test]
fn golden_subtractive() {
    let mut patch = patch_subtractive();
    assert_golden(
        "subtractive",
        render_hash(&mut patch),
        0x20b7_2443_0004_7c91,
    );
}

#[test]
fn golden_diode_ladder() {
    let mut patch = patch_diode_ladder();
    assert_golden(
        "diode_ladder",
        render_hash(&mut patch),
        0xae58_26e6_0315_0055,
    );
}

#[test]
fn golden_lfo_modulated() {
    let mut patch = patch_lfo_modulated();
    assert_golden(
        "lfo_modulated",
        render_hash(&mut patch),
        0x1d5d_d077_2b74_5b25,
    );
}

#[test]
fn golden_delay_chorus() {
    let mut patch = patch_delay_chorus();
    // Rebaselined in 0.4.0 — see the module docs (Q-N4). Previously 0x0bd3_4a0e_47ac_ee37.
    assert_golden(
        "delay_chorus",
        render_hash(&mut patch),
        0x8b20_c2b4_31f7_8812,
    );
}

#[test]
fn golden_noise() {
    // The noise generator draws from the thread-local global RNG; seeding it here (each
    // test runs on its own thread) makes the render reproducible.
    rng::seed(0x5EED_C0FF_EE12_3456);
    let mut patch = patch_noise();
    assert_golden("noise", render_hash(&mut patch), 0x0af6_6bdf_536f_e7a1);
}

#[test]
fn golden_multi_cable() {
    let mut patch = patch_multi_cable();
    // Captured from 0.4.0 (Q-N5), the first release to pin this path.
    assert_golden(
        "multi_cable",
        render_hash(&mut patch),
        0x6309_03cf_780d_9bd1,
    );
}

#[test]
fn golden_feedback_loop() {
    let mut patch = patch_feedback_loop();
    // Captured from 0.4.0 (Q-N5), under the back-edge-only scheduler (Q-N4).
    assert_golden(
        "feedback_loop",
        render_hash(&mut patch),
        0xdbf6_f30b_9884_64e7,
    );
}

#[test]
fn golden_poly() {
    // Captured from 0.4.0 (Q-N5).
    assert_golden("poly", poly_render_hash(), 0x2aa8_96e2_3408_82a9);
}

/// `set_param_by_id` half-way through a render: the value must land on the very next
/// sample, through the in-place plan patch (Q-N3), without a recompile blanking anything.
#[test]
fn golden_set_param_mid_render() {
    let mut patch = patch_subtractive();
    let svf = patch.get_node_id_by_name("svf").unwrap();
    let adsr = patch.get_node_id_by_name("adsr").unwrap();
    let hash = render_hash_with(&mut patch, |patch, frame| {
        if frame == FRAMES / 2 {
            assert!(patch.set_param_by_id(svf, "cutoff", 0.7));
            assert!(patch.set_param_by_id(adsr, "release", 0.1));
        }
    });
    // Captured from 0.4.0 (Q-N5). Differs from `subtractive`, so the edit did land.
    assert_golden("set_param_mid_render", hash, 0xbce4_5797_7c9c_f461);
}

/// `tick_block` is the same engine as `tick`, sample for sample, bit for bit — including
/// ragged final blocks and across every patch in this file.
#[test]
fn tick_block_matches_tick_bitwise() {
    type Builder = fn() -> Patch;
    let builders: [(&str, Builder); 7] = [
        ("subtractive", patch_subtractive),
        ("diode_ladder", patch_diode_ladder),
        ("lfo_modulated", patch_lfo_modulated),
        ("delay_chorus", patch_delay_chorus),
        ("noise", patch_noise),
        ("multi_cable", patch_multi_cable),
        ("feedback_loop", patch_feedback_loop),
    ];
    for (name, build) in builders {
        // The noise patch tracks the global stream unless seeded; seed both renders.
        let mut a = build();
        a.seed(0x7E57);
        a.compile().unwrap();
        let per_sample: Vec<(u64, u64)> = (0..FRAMES)
            .map(|_| {
                let (l, r) = a.tick();
                (l.to_bits(), r.to_bits())
            })
            .collect();

        let mut b = build();
        b.seed(0x7E57);
        b.compile().unwrap();
        let mut left = vec![0.0; FRAMES];
        let mut right = vec![0.0; FRAMES];
        let mut done = 0;
        for block in [256usize, 1, 1000, 37, 4096].iter().cycle() {
            if done >= FRAMES {
                break;
            }
            let n = (*block).min(FRAMES - done);
            b.tick_block(&mut left[done..done + n], &mut right[done..done + n]);
            done += n;
        }
        let blocked: Vec<(u64, u64)> = left
            .iter()
            .zip(&right)
            .map(|(l, r)| (l.to_bits(), r.to_bits()))
            .collect();
        assert!(
            per_sample == blocked,
            "tick_block diverged from tick for `{name}`"
        );
    }
}
