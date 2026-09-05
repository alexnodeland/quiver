//! NaN / Infinity injection recovery tests (Q160).
//!
//! Stateful / feedback modules keep internal state (SVF integrators, ladder
//! stages, delay/reverb/chorus buffers). Before the Wave-F input-sanitization
//! fix, a single non-finite input sample would be written into that state and
//! then circulate forever, permanently poisoning every future output.
//!
//! Policy (documented on each module): the audio input of every feedback module
//! is sanitized with a single `is_finite` branch per sample; a non-finite input
//! is treated as `0.0` so it never enters the feedback state. A clean signal fed
//! afterward therefore recovers within a bounded number of samples.
//!
//! These tests feed exactly one `NaN` (and one `Infinity`) sample, then a clean
//! sine, and assert the module output returns to finite, bounded values.

use quiver::modules::{Chorus, DelayLine, Granular, KarplusStrong, PitchShifter};
use quiver::prelude::*;

/// Feed `poison` once on the audio input (port 0), then a clean sine for
/// `settle` samples, and return whether every sample of the final `tail`
/// outputs (on output port 10) is finite and bounded.
fn recovers<M: GraphModule>(
    module: &mut M,
    extra_inputs: &[(u32, f64)],
    poison: f64,
    settle: usize,
    tail: usize,
) -> bool {
    let mut inputs = PortValues::new();
    let mut outputs = PortValues::new();
    for &(port, value) in extra_inputs {
        inputs.set(port, value);
    }

    // One poison sample.
    inputs.set(0, poison);
    module.tick(&inputs, &mut outputs);

    // Clean sine afterwards.
    let mut phase = 0.0f64;
    let dt = 220.0 / 44100.0;
    let mut all_finite = true;
    for n in 0..settle {
        let s = (core::f64::consts::TAU * phase).sin();
        phase += dt;
        if phase >= 1.0 {
            phase -= 1.0;
        }
        inputs.set(0, s);
        module.tick(&inputs, &mut outputs);
        let out = outputs.get(10).unwrap();
        if n >= settle - tail && (!out.is_finite() || out.abs() > 1e6) {
            all_finite = false;
        }
    }
    all_finite
}

#[test]
fn svf_recovers_from_nan_and_inf() {
    // res moderate, cutoff mid; a NaN in the resonant integrator would latch.
    for &poison in &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut svf = Svf::new(44100.0);
        assert!(
            recovers(&mut svf, &[(1, 0.5), (2, 0.8)], poison, 2000, 500),
            "Svf latched {poison} into its integrator state"
        );
    }
}

#[test]
fn diode_ladder_recovers_from_nan_and_inf() {
    for &poison in &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut dlf = DiodeLadderFilter::new(44100.0);
        assert!(
            recovers(&mut dlf, &[(1, 0.5), (2, 0.9)], poison, 2000, 500),
            "DiodeLadderFilter latched {poison} into its ladder stages"
        );
    }
}

#[test]
fn delay_line_recovers_from_nan_and_inf() {
    // High feedback so a NaN would recirculate through the buffer forever.
    for &poison in &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut delay = DelayLine::new(44100.0);
        assert!(
            recovers(
                &mut delay,
                &[(1, 0.2), (2, 0.9), (3, 0.5)],
                poison,
                4000,
                500
            ),
            "DelayLine latched {poison} into its feedback buffer"
        );
    }
}

#[test]
fn reverb_recovers_from_nan_and_inf() {
    // Large room + low damping: the comb/allpass feedback network is the most
    // prone to permanently latching a poisoned sample.
    for &poison in &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut reverb = Reverb::new(44100.0);
        assert!(
            recovers(
                &mut reverb,
                &[(1, 0.9), (2, 0.1), (3, 0.5)],
                poison,
                8000,
                500
            ),
            "Reverb latched {poison} into its comb/allpass network"
        );
    }
}

#[test]
fn chorus_recovers_from_nan_and_inf() {
    for &poison in &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut chorus = Chorus::new(44100.0);
        assert!(
            recovers(
                &mut chorus,
                &[(1, 0.3), (2, 0.5), (3, 0.5)],
                poison,
                4000,
                500
            ),
            "Chorus latched {poison} into its modulated delay buffer"
        );
    }
}

// ---- Q-N6: the three modules that took unsanitised input ----------------
//
// These use a stricter criterion than `recovers`: with the input sanitised, a
// non-finite sample is silence, so **every** output from the poison onward must be
// finite — not merely the tail after the buffer has been overwritten. Without
// `sanitize_audio` each of these fails (a NaN circulates in the grain/circular
// buffer, or a single-sample NaN burst passes straight through the fold).

/// Poison the audio input (port 0) for `poison_len` samples, then feed a clean
/// sine; return whether every output sample (port 10), poison included, is finite.
fn stays_finite<M: GraphModule>(
    module: &mut M,
    extra_inputs: &[(u32, f64)],
    poison: f64,
    poison_len: usize,
    clean_len: usize,
) -> bool {
    let mut inputs = PortValues::new();
    let mut outputs = PortValues::new();
    for &(port, value) in extra_inputs {
        inputs.set(port, value);
    }
    let mut phase = 0.0f64;
    let dt = 220.0 / 44100.0;
    for n in 0..poison_len + clean_len {
        if n < poison_len {
            inputs.set(0, poison);
        } else {
            inputs.set(0, (core::f64::consts::TAU * phase).sin());
            phase = (phase + dt) % 1.0;
        }
        module.tick(&inputs, &mut outputs);
        if !outputs.get(10).unwrap().is_finite() {
            return false;
        }
    }
    true
}

#[test]
fn pitch_shifter_never_emits_non_finite() {
    for &poison in &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut ps = PitchShifter::new(44100.0);
        assert!(
            stays_finite(&mut ps, &[(1, 2.0), (2, 0.5), (3, 1.0)], poison, 8, 12000),
            "PitchShifter read {poison} back out of its grain buffer"
        );
    }
}

#[test]
fn granular_never_emits_non_finite() {
    for &poison in &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut gr = Granular::new(44100.0);
        assert!(
            stays_finite(
                &mut gr,
                &[(1, 0.5), (2, 0.3), (3, 0.9), (4, 0.0), (5, 0.1), (6, 0.0)],
                poison,
                2000,
                20000
            ),
            "Granular read {poison} back out of its buffer"
        );
    }
}

/// The audit's freeze case: a NaN written into the circular buffer and then frozen in
/// place would be read back by every grain for as long as the freeze lasts.
#[test]
fn granular_frozen_buffer_cannot_be_poisoned() {
    const BUFFER_FILL: usize = 4 * 44100; // longer than the internal buffer
    for &poison in &[f64::NAN, f64::INFINITY] {
        let mut gr = Granular::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        for &(port, value) in &[(1, 0.5), (2, 0.3), (3, 0.9), (4, 0.0), (5, 0.5)] {
            inputs.set(port, value);
        }
        // Unfrozen: fill the whole buffer with the poison...
        inputs.set(6, 0.0);
        inputs.set(0, poison);
        for _ in 0..BUFFER_FILL {
            gr.tick(&inputs, &mut outputs);
        }
        // ...then freeze it and play it back: nothing non-finite may come out.
        inputs.set(6, 5.0);
        inputs.set(0, 0.0);
        for _ in 0..8000 {
            gr.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            assert!(
                out.is_finite(),
                "frozen Granular played back {out} after {poison}"
            );
        }
    }
}

#[test]
fn wavefolder_never_emits_non_finite() {
    for &poison in &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        // Memoryless fold: a non-finite input used to pass straight through.
        let mut plain = Wavefolder::new(0.6);
        assert!(
            stays_finite(&mut plain, &[(1, 0.6)], poison, 1, 200),
            "Wavefolder passed {poison} through the fold"
        );
        // With oversampling the half-band filters carry state across samples.
        let mut wf = Wavefolder::new(0.6);
        wf.set_oversample(Oversample::X4);
        assert!(
            stays_finite(&mut wf, &[(1, 0.6)], poison, 1, 2000),
            "Wavefolder let {poison} into its oversampler"
        );
    }
}

/// `KarplusStrong` has no audio input; its exposure is through the CVs and its own
/// recirculating state. Non-finite CVs (including during a pluck) must neither poison
/// the loop nor leave the string silent for the next pluck.
#[test]
fn karplus_strong_recovers_from_non_finite_cvs() {
    for &poison in &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut ks = KarplusStrong::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        // Pluck with every CV poisoned, and hold the poison while it rings.
        for &port in &[0, 2, 3, 4] {
            inputs.set(port, poison);
        }
        inputs.set(1, 5.0);
        ks.tick(&inputs, &mut outputs);
        inputs.set(1, 0.0);
        for _ in 0..2000 {
            ks.tick(&inputs, &mut outputs);
            assert!(
                outputs.get(10).unwrap().is_finite(),
                "KS went non-finite on {poison} CVs"
            );
        }
        // Clean CVs, re-pluck: it must ring normally.
        inputs.set(0, 0.0);
        inputs.set(2, 0.9);
        inputs.set(3, 0.5);
        inputs.set(4, 0.0);
        inputs.set(1, 5.0);
        ks.tick(&inputs, &mut outputs);
        inputs.set(1, 0.0);
        let mut energy = 0.0;
        for _ in 0..2000 {
            ks.tick(&inputs, &mut outputs);
            let y = outputs.get(10).unwrap();
            assert!(y.is_finite());
            energy += y * y;
        }
        assert!(
            (energy / 2000.0).sqrt() > 0.05,
            "KS silent after recovering from {poison} CVs"
        );
    }
}
