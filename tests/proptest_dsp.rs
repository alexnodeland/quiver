//! Property-based tests (Q165).
//!
//! The existing `_bounded` tests use a handful of hand-picked point values. These
//! proptest generators sweep random parameter combinations:
//!   * filters (Svf, DiodeLadderFilter): random cutoff / resonance / input
//!     amplitude must keep the output finite and bounded over many ticks;
//!   * quantizers: any input voltage must map to a valid scale note, and the
//!     mapping must be monotonic (non-decreasing).
#![cfg(feature = "std")]

use proptest::prelude::*;
use quiver::prelude::*;

/// Tick `module` with a sine of amplitude `amp` at `cutoff_cv`/`res` and return
/// the maximum absolute output over `n` samples (NaN/Inf collapse to NaN).
fn filter_max_abs<M: GraphModule>(
    module: &mut M,
    cutoff_cv: f64,
    res: f64,
    amp: f64,
    n: usize,
) -> f64 {
    let mut inputs = PortValues::new();
    let mut outputs = PortValues::new();
    inputs.set(1, cutoff_cv);
    inputs.set(2, res);
    let dt = 330.0 / 44_100.0;
    let mut phase = 0.0f64;
    let mut max_abs = 0.0f64;
    for _ in 0..n {
        let s = (core::f64::consts::TAU * phase).sin() * amp;
        phase += dt;
        if phase >= 1.0 {
            phase -= 1.0;
        }
        inputs.set(0, s);
        module.tick(&inputs, &mut outputs);
        let out = outputs.get(10).unwrap();
        if !out.is_finite() {
            return f64::NAN;
        }
        max_abs = max_abs.max(out.abs());
    }
    max_abs
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn svf_stays_finite_and_bounded(
        cutoff in 0.0f64..1.0,
        res in 0.0f64..1.0,
        amp in 0.1f64..10.0,
    ) {
        let mut svf = Svf::new(44_100.0);
        let m = filter_max_abs(&mut svf, cutoff, res, amp, 1500);
        prop_assert!(m.is_finite(), "Svf produced non-finite output (cutoff={cutoff}, res={res}, amp={amp})");
        // Even at self-oscillation the SVF is designed to stay bounded; allow
        // generous resonant gain but catch true blow-ups.
        prop_assert!(m < 1.0e3, "Svf output exploded to {m} (cutoff={cutoff}, res={res}, amp={amp})");
    }

    #[test]
    fn diode_ladder_stays_finite_and_bounded(
        cutoff in 0.0f64..1.0,
        res in 0.0f64..1.0,
        amp in 0.1f64..10.0,
    ) {
        let mut dlf = DiodeLadderFilter::new(44_100.0);
        let m = filter_max_abs(&mut dlf, cutoff, res, amp, 1500);
        prop_assert!(m.is_finite(), "DiodeLadder produced non-finite output (cutoff={cutoff}, res={res}, amp={amp})");
        prop_assert!(m < 1.0e3, "DiodeLadder output exploded to {m} (cutoff={cutoff}, res={res}, amp={amp})");
    }
}

/// Major scale degrees within an octave (semitones from root).
const MAJOR: [i64; 7] = [0, 2, 4, 5, 7, 9, 11];

/// Quantize one voltage on a fresh quantizer (no hysteresis carry-over).
fn quantize_once(scale: Scale, voltage: f64) -> f64 {
    let mut q = Quantizer::new(scale);
    let mut inputs = PortValues::new();
    let mut outputs = PortValues::new();
    inputs.set(0, voltage);
    q.tick(&inputs, &mut outputs);
    outputs.get(10).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn quantizer_output_is_a_valid_major_scale_note(v in -5.0f64..5.0) {
        let out = quantize_once(Scale::Major, v);
        prop_assert!(out.is_finite());
        // Output must be an exact multiple of one semitone (1/12 V).
        let semis = (out * 12.0).round();
        prop_assert!((out * 12.0 - semis).abs() < 1e-9, "output {out}V is not a semitone multiple");
        // ...and its pitch class must be a major-scale degree.
        let pitch_class = (semis as i64).rem_euclid(12);
        prop_assert!(MAJOR.contains(&pitch_class), "output {out}V (pc {pitch_class}) is not in the major scale");
    }

    #[test]
    fn quantizer_is_monotonic(a in -5.0f64..5.0, delta in 0.0f64..5.0) {
        // A larger input never yields a lower quantized pitch.
        let lo = quantize_once(Scale::Major, a);
        let hi = quantize_once(Scale::Major, a + delta);
        prop_assert!(hi >= lo - 1e-9, "monotonicity violated: q({})={lo} > q({})={hi}", a, a + delta);
    }
}
