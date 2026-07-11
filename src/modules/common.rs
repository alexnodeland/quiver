//! Shared DSP helpers and constants used across the module submodules.
//!
//! Everything here is `no_std`-compatible: transcendental math goes through the
//! [`libm::Libm`] shim rather than the std-only inherent `f64` methods.

use libm::Libm;

/// Frequency of middle C (C4) in Hz, used as the 0V reference for V/Oct pitch.
pub const C4_HZ: f64 = 261.625_565_300_598_6;

/// Nominal "high" voltage emitted on gate, trigger, and clock outputs.
pub const GATE_HIGH_V: f64 = 5.0;

/// Threshold at which a gate/trigger/clock signal is considered "high".
pub const GATE_THRESHOLD_V: f64 = 2.5;

/// Convert a 1V/octave pitch signal to frequency in Hz (0V = C4).
#[inline]
pub fn voct_to_hz(voct: f64) -> f64 {
    C4_HZ * Libm::<f64>::pow(2.0, voct)
}

/// One-pole smoothing coefficient for an envelope of `time_seconds` at
/// `sample_rate`: `exp(-1 / (time_seconds * sample_rate))`.
///
/// Guards against a non-positive time constant (which would otherwise divide by
/// zero); such a degenerate case collapses to an instantaneous response (`0.0`),
/// matching the `exp(-inf)` limit.
#[inline]
pub fn env_coef(time_seconds: f64, sample_rate: f64) -> f64 {
    let denom = time_seconds * sample_rate;
    if denom <= 0.0 {
        return 0.0;
    }
    Libm::<f64>::exp(-1.0 / denom)
}

/// Read a fractional delay from a circular buffer using linear interpolation.
///
/// `write_pos` is the index that will next be written; `delay_samples` is the
/// (possibly fractional) number of samples back to read. Read positions wrap
/// around the buffer length.
#[inline]
pub fn read_interpolated(buffer: &[f64], write_pos: usize, delay_samples: f64) -> f64 {
    let buffer_len = buffer.len();
    let delay_int = delay_samples as usize;
    let frac = delay_samples - delay_int as f64;

    let read_pos1 = (write_pos + buffer_len - delay_int) % buffer_len;
    let read_pos2 = (write_pos + buffer_len - delay_int - 1) % buffer_len;

    let sample1 = buffer[read_pos1];
    let sample2 = buffer[read_pos2];
    sample1 * (1.0 - frac) + sample2 * frac
}

/// Convert a decibel value to a linear gain: `10^(db / 20)`.
#[inline]
pub fn db_to_gain(db: f64) -> f64 {
    Libm::<f64>::pow(10.0, db / 20.0)
}

/// Convert a linear gain to decibels: `20 * log10(gain)`.
#[inline]
pub fn gain_to_db(gain: f64) -> f64 {
    20.0 * Libm::<f64>::log10(gain)
}

/// Flush denormalized numbers to zero to avoid CPU denormal penalties.
///
/// Defined here for Wave B, which wires it into the DSP paths; not yet called.
#[allow(dead_code)]
#[inline]
pub fn flush_denorm(x: f64) -> f64 {
    if Libm::<f64>::fabs(x) < 1e-20 {
        0.0
    } else {
        x
    }
}

/// PolyBLEP residual for bandlimiting a discontinuity at phase `t` with step
/// `dt` (normalized frequency). Returns the correction to add/subtract from a
/// naive waveform to reduce aliasing.
#[inline]
pub fn polyblep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let t = t / dt;
        2.0 * t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

/// Rising-edge detector for gate/trigger/clock signals.
///
/// Tracks the previous sample and reports a rising edge when the signal crosses
/// its threshold from low to high.
#[derive(Debug, Default, Clone, Copy)]
pub struct EdgeDetector {
    prev: f64,
}

impl EdgeDetector {
    /// Create a detector with the previous sample initialized to `0.0`.
    pub fn new() -> Self {
        Self { prev: 0.0 }
    }

    /// Report whether `v` is a rising edge across [`GATE_THRESHOLD_V`], then
    /// record `v` as the new previous sample.
    #[inline]
    pub fn rising(&mut self, v: f64) -> bool {
        self.rising_above(v, GATE_THRESHOLD_V)
    }

    /// Report whether `v` is a rising edge across an explicit `threshold`, then
    /// record `v` as the new previous sample.
    #[inline]
    pub fn rising_above(&mut self, v: f64, threshold: f64) -> bool {
        let rising = v > threshold && self.prev <= threshold;
        self.prev = v;
        rising
    }

    /// Reset the detector's previous sample to `0.0`.
    #[inline]
    pub fn reset(&mut self) {
        self.prev = 0.0;
    }
}

#[cfg(test)]
pub(crate) const SAFE_AUDIO_LIMIT: f64 = 10.0; // Max safe output voltage

/// Helper to run a module for N samples and track max output.
#[cfg(test)]
pub(crate) fn measure_max_output<F>(samples: usize, mut tick_fn: F) -> f64
where
    F: FnMut() -> f64,
{
    let mut max_abs = 0.0f64;
    for _ in 0..samples {
        let out = tick_fn();
        max_abs = max_abs.max(out.abs());
    }
    max_abs
}
