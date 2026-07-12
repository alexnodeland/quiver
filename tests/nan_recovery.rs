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

use quiver::modules::{Chorus, DelayLine};
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
