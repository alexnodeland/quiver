//! Long-run stability / decay tests for feedback-heavy modules (Q166).
//!
//! Short (100-3000 sample) decay tests can miss slow, denormal-driven DC drift
//! or creeping instability in feedback comb/allpass/ladder networks. These tests
//! excite each module with a single impulse and then run 60k-80k samples of
//! silence (~1.4-1.8s at 44.1kHz), asserting the tail decays to near-zero with
//! no re-growth (the peak never climbs back up after the initial transient).
//!
//! Runtimes are kept well under the ~10s bar (each module runs in a few ms in a
//! debug build), so nothing is `#[ignore]`d.

use quiver::modules::{Flanger, Phaser};
use quiver::prelude::*;

/// Excite `module` with a one-sample impulse, then run `n` silent samples.
/// Returns `(initial_peak, window_peaks)` where `window_peaks` are the peak
/// absolute output over consecutive `window`-sized blocks of the silent tail.
fn impulse_tail<M: GraphModule>(
    module: &mut M,
    extra_inputs: &[(u32, f64)],
    n: usize,
    window: usize,
) -> (f64, Vec<f64>) {
    let mut inputs = PortValues::new();
    let mut outputs = PortValues::new();
    for &(port, value) in extra_inputs {
        inputs.set(port, value);
    }

    // Impulse.
    inputs.set(0, 5.0);
    module.tick(&inputs, &mut outputs);
    let initial_peak = outputs.get(10).unwrap().abs();

    // Silent tail.
    inputs.set(0, 0.0);
    let mut window_peaks = Vec::new();
    let mut cur = 0.0f64;
    for i in 0..n {
        module.tick(&inputs, &mut outputs);
        let a = outputs.get(10).unwrap();
        assert!(a.is_finite(), "non-finite output at tail sample {i}");
        cur = cur.max(a.abs());
        if (i + 1) % window == 0 {
            window_peaks.push(cur);
            cur = 0.0;
        }
    }
    (initial_peak, window_peaks)
}

/// Assert a decaying tail: the last window is near-silent, and the peak in the
/// second half of the tail does not exceed the peak in the first half (a stable
/// decay loses energy over time; instability or DC re-growth would reverse this).
fn assert_decays(name: &str, _initial_peak: f64, window_peaks: &[f64]) {
    let last = *window_peaks.last().unwrap();
    assert!(
        last < 1e-3,
        "{name}: tail did not decay to near-zero (last window peak = {last})"
    );
    let half = window_peaks.len() / 2;
    let peak_of = |s: &[f64]| s.iter().cloned().fold(0.0_f64, f64::max);
    let first_half = peak_of(&window_peaks[..half]);
    let second_half = peak_of(&window_peaks[half..]);
    assert!(
        second_half <= first_half + 1e-6,
        "{name}: tail re-grew (second-half peak {second_half} > first-half peak {first_half})"
    );
}

#[test]
fn reverb_decays_over_long_tail() {
    let mut reverb = Reverb::new(44100.0);
    let (peak, windows) = impulse_tail(&mut reverb, &[(1, 0.5), (2, 0.5), (3, 1.0)], 80_000, 4000);
    assert_decays("reverb", peak, &windows);
}

#[test]
fn chorus_decays_over_long_tail() {
    use quiver::modules::Chorus;
    let mut chorus = Chorus::new(44100.0);
    let (peak, windows) = impulse_tail(&mut chorus, &[(1, 0.3), (2, 0.5), (3, 1.0)], 60_000, 4000);
    assert_decays("chorus", peak, &windows);
}

#[test]
fn diode_ladder_decays_over_long_tail() {
    // Moderate resonance (not self-oscillating) so the ring decays.
    let mut dlf = DiodeLadderFilter::new(44100.0);
    let (peak, windows) = impulse_tail(&mut dlf, &[(1, 0.5), (2, 0.5)], 60_000, 4000);
    assert_decays("diode_ladder", peak, &windows);
}

#[test]
fn flanger_decays_over_long_tail() {
    let mut flanger = Flanger::new(44100.0);
    let (peak, windows) = impulse_tail(
        &mut flanger,
        &[(1, 0.3), (2, 0.5), (3, 0.7), (4, 1.0)],
        60_000,
        4000,
    );
    assert_decays("flanger", peak, &windows);
}

#[test]
fn phaser_decays_over_long_tail() {
    let mut phaser = Phaser::new(44100.0);
    let (peak, windows) = impulse_tail(&mut phaser, &[(1, 0.3), (2, 0.7), (3, 0.7)], 60_000, 4000);
    assert_decays("phaser", peak, &windows);
}
