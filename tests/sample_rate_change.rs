//! Mid-stream `set_sample_rate` tests (Q159).
//!
//! The `*_default_reset_sample_rate` unit tests all call `set_sample_rate`
//! immediately after construction, before any audio has flowed. This exercises
//! the harder case: accumulate real state (buffered echoes, filter resonance),
//! then change the sample rate mid-stream and keep ticking. Buffer-backed
//! modules (DelayLine, Chorus, Reverb, PitchShifter) reallocate their buffers on
//! a rate change, so the index math must not panic and the output must stay
//! finite and bounded across the transition.

use quiver::modules::{Chorus, DelayLine};
use quiver::prelude::*;

/// Feed a sine into `module`, change the sample rate part-way through, and
/// return whether every output sample (port 10) stayed finite and bounded.
fn survives_rate_change<M: GraphModule>(
    module: &mut M,
    extra_inputs: &[(u32, f64)],
    old_rate: f64,
    new_rate: f64,
) -> bool {
    let mut inputs = PortValues::new();
    let mut outputs = PortValues::new();
    for &(port, value) in extra_inputs {
        inputs.set(port, value);
    }

    let mut ok = true;
    let mut phase = 0.0f64;
    let feed = |module: &mut M,
                inputs: &mut PortValues,
                outputs: &mut PortValues,
                phase: &mut f64,
                rate: f64,
                n: usize,
                ok: &mut bool| {
        let dt = 220.0 / rate;
        for _ in 0..n {
            let s = (core::f64::consts::TAU * *phase).sin();
            *phase += dt;
            if *phase >= 1.0 {
                *phase -= 1.0;
            }
            inputs.set(0, s);
            module.tick(inputs, outputs);
            let out = outputs.get(10).unwrap();
            if !out.is_finite() || out.abs() > 1e3 {
                *ok = false;
            }
        }
    };

    // Accumulate state at the original rate.
    feed(
        module,
        &mut inputs,
        &mut outputs,
        &mut phase,
        old_rate,
        4000,
        &mut ok,
    );
    // Change the rate mid-stream (reallocates buffers on delay-based modules).
    module.set_sample_rate(new_rate);
    // Keep ticking at the new rate.
    feed(
        module,
        &mut inputs,
        &mut outputs,
        &mut phase,
        new_rate,
        4000,
        &mut ok,
    );
    ok
}

#[test]
fn delay_line_survives_mid_stream_rate_change() {
    let mut delay = DelayLine::new(44100.0);
    assert!(survives_rate_change(
        &mut delay,
        &[(1, 0.3), (2, 0.7), (3, 0.5)],
        44100.0,
        48000.0
    ));
    // Also exercise a downward rate change (shrinks the buffer).
    let mut delay2 = DelayLine::new(48000.0);
    assert!(survives_rate_change(
        &mut delay2,
        &[(1, 0.3), (2, 0.7), (3, 0.5)],
        48000.0,
        22050.0
    ));
}

#[test]
fn chorus_survives_mid_stream_rate_change() {
    let mut chorus = Chorus::new(44100.0);
    assert!(survives_rate_change(
        &mut chorus,
        &[(1, 0.3), (2, 0.5), (3, 0.5)],
        44100.0,
        96000.0
    ));
}

#[test]
fn reverb_survives_mid_stream_rate_change() {
    let mut reverb = Reverb::new(44100.0);
    assert!(survives_rate_change(
        &mut reverb,
        &[(1, 0.7), (2, 0.3), (3, 0.5)],
        44100.0,
        88200.0
    ));
}

#[test]
fn pitch_shifter_survives_mid_stream_rate_change() {
    let mut ps = PitchShifter::new(44100.0);
    assert!(survives_rate_change(
        &mut ps,
        &[(1, 0.5), (2, 0.5), (3, 1.0)],
        44100.0,
        48000.0
    ));
}

#[test]
fn svf_survives_mid_stream_rate_change() {
    // Accumulate resonance, then change rate mid-decay.
    let mut svf = Svf::new(44100.0);
    assert!(survives_rate_change(
        &mut svf,
        &[(1, 0.6), (2, 0.9)],
        44100.0,
        48000.0
    ));
}
