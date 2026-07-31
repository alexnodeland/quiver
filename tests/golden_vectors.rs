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
//! Coverage is chosen to exercise every path in `Patch::tick_step`:
//!
//! | patch | exercises |
//! |---|---|
//! | `subtractive` | VCO -> SVF -> VCA chain gated by an ADSR, multi-edge inputs |
//! | `diode_ladder` | nonlinear ladder filter, attenuated + offset cables |
//! | `lfo_modulated` | one LFO fanned out to three destinations (`mult`) |
//! | `delay_chorus` | cycle-breaker (`DelayLine`) scheduling and stereo scatter |
//! | `noise` | seeded global RNG, partially consumed multi-output module |
//!
//! `StereoOutput.right` is normalled to `left` in three of the five patches, so the
//! normalled-input resolution pass is covered too.
#![cfg(feature = "std")]

use quiver::modules::{Chorus, DelayLine};
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
    patch.compile().expect("golden patch must compile");
    let mut hasher = SampleHasher::new();
    let mut peak = 0.0f64;
    for _ in 0..FRAMES {
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
    assert_golden(
        "delay_chorus",
        render_hash(&mut patch),
        0x0bd3_4a0e_47ac_ee37,
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
