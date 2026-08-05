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

/// Widest V/Oct magnitude `voct_to_hz` will evaluate, in octaves either side
/// of C4.
///
/// Chosen so that **no reachable musical value is affected** and overflow is
/// impossible. At +32 the result is ~1.1 THz and at −32 it is ~6e-8 Hz (a
/// period of about 190 days); the audible band sits inside ±11, and even a
/// 192 kHz Nyquist is only +8.5. Overflow needs roughly 2^1024, so this clamps
/// thirty octaves before anything can go non-finite.
pub const MAX_ABS_VOCT: f64 = 32.0;

/// Convert a 1V/octave pitch signal to frequency in Hz (0V = C4).
///
/// The input is clamped to ±[`MAX_ABS_VOCT`] so the result is always finite for
/// a finite input. Without it, `2^1100` is `inf`, which propagates into a phase
/// increment and then into the accumulator — the modules recover from that
/// (Q198: `wrap_phase` re-seeds rather than latching NaN), but recovering from
/// a value is not the same as never producing it, and only one of the two is a
/// property you can state.
///
/// A NaN input stays NaN: `f64::clamp` passes it through, and the downstream
/// recovery path exists precisely for inputs that carry no pitch at all. This
/// clamp is about *finite* garbage.
///
/// Deliberately **not** a Nyquist clamp. Bounding aliasing needs the sample
/// rate, which this function does not take and should not — that is a per-
/// oscillator decision, and folding it in here would silently change the
/// frequency of every module at high pitch rather than only preventing
/// overflow.
#[inline]
pub fn voct_to_hz(voct: f64) -> f64 {
    C4_HZ * Libm::<f64>::pow(2.0, voct.clamp(-MAX_ABS_VOCT, MAX_ABS_VOCT))
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

/// Sanitize an audio input sample so a non-finite value can never enter a
/// module's feedback state (Q160).
///
/// Stateful/feedback modules (SVF integrators, ladder stages, delay/reverb/
/// chorus buffers) would otherwise write a single `NaN`/`Inf` input into their
/// state and circulate it forever, permanently poisoning all future output.
/// Calling this on the audio input at the top of `tick` costs a single branch
/// per sample and keeps a finite sample bit-for-bit unchanged, so a clean signal
/// fed after a poisoned one recovers immediately. Non-finite samples are treated
/// as silence (`0.0`).
#[inline]
pub fn sanitize_audio(x: f64) -> f64 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
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

/// Wrap a phase accumulator into `[0, 1)`, recovering from non-finite values.
///
/// Q198: oscillator phase accumulators are recursive state, so a single
/// non-finite increment (e.g. `voct_to_hz` overflowing to `inf` under an
/// extreme V/Oct input) would otherwise latch the phase to NaN permanently
/// (`NaN - floor(NaN)` is NaN), or — for the `while phase >= 1.0` wrap style —
/// spin the audio thread forever (`inf - 1.0` is `inf`). Non-finite phases
/// reset to `0.0`; finite phases of any magnitude wrap in O(1) via `floor`.
#[inline]
pub fn wrap_phase(x: f64) -> f64 {
    if !x.is_finite() {
        return 0.0;
    }
    let wrapped = x - Libm::<f64>::floor(x);
    // `x - floor(x)` lies in [0, 1) for finite x (including negatives), but
    // guard the rounding edge case where it lands on exactly 1.0.
    if wrapped >= 1.0 {
        0.0
    } else {
        wrapped
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

/// PolyBLAMP residual for bandlimiting a *slope* discontinuity (a corner) at
/// phase `t` with step `dt` (normalized frequency).
///
/// This is the integral of [`polyblep`] and peaks at `1/3` exactly at the
/// corner. Add `delta_slope_per_sample * polyblamp(t, dt)` to a naive waveform
/// to round a corner where the per-sample slope changes by
/// `delta_slope_per_sample`. Used for bandlimited triangles (which have slope,
/// not value, discontinuities).
#[inline]
pub fn polyblamp(t: f64, dt: f64) -> f64 {
    if t < dt {
        let t = t / dt - 1.0;
        -1.0 / 3.0 * t * t * t
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt + 1.0;
        1.0 / 3.0 * t * t * t
    } else {
        0.0
    }
}

/// Bit-exact memo for a pure "inputs → derived coefficients" computation.
///
/// Module `tick()` implementations derive coefficients from their parameter
/// inputs with transcendental math (cv→Hz maps, filter prewarp, envelope time
/// maps) every sample, even though in a typical patch those inputs are wired to
/// constants. `Memo` caches the last derived value and recomputes only when a
/// key changes, comparing keys by **bit pattern**:
///
/// - On a miss the caller's closure runs *exactly* the original computation, so
///   a hit returns bit-for-bit the value a recompute would produce for the same
///   key bits. Memoization is observationally invisible except in CPU time
///   (the downstream determinism contract — bit-identical samples — holds).
/// - Bit comparison makes `+0.0`/`-0.0` distinct keys (conservative: at worst
///   an extra recompute, never a wrong hit) and lets a NaN key hit against an
///   identical NaN (a recompute from the same NaN bits would return the same
///   value bits).
/// - The initial state matches no key (explicit `valid` flag), so the first
///   call always computes.
///
/// If the derived value depends on the sample rate, include the sample rate as
/// one of the keys; a `set_sample_rate` change then misses naturally, with no
/// per-module invalidation hook to forget. `reset()` intentionally does *not*
/// clear memos: the cached values are input-derived, not audio state.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Memo<const N: usize, V> {
    /// Bit patterns of the key inputs from the last recompute.
    keys: [u64; N],
    /// Cached derived value from the last recompute.
    val: V,
    /// Whether `keys`/`val` hold a real computation (false until first use).
    valid: bool,
    /// Number of misses (recomputes) performed, for cache-behavior tests.
    #[cfg(test)]
    recomputes: u64,
}

impl<const N: usize, V: Copy> Memo<N, V> {
    /// Create an empty memo. `placeholder` is never observable: the `valid`
    /// flag forces the first call to recompute.
    pub(crate) fn new(placeholder: V) -> Self {
        Self {
            keys: [0; N],
            val: placeholder,
            valid: false,
            #[cfg(test)]
            recomputes: 0,
        }
    }

    /// Return the cached value if `keys` bit-match the previous call, otherwise
    /// run `compute`, cache its result under `keys`, and return it.
    #[inline]
    pub(crate) fn get_or_compute(&mut self, keys: [f64; N], compute: impl FnOnce() -> V) -> V {
        let bits = keys.map(f64::to_bits);
        if !self.valid || bits != self.keys {
            self.val = compute();
            self.keys = bits;
            self.valid = true;
            #[cfg(test)]
            {
                self.recomputes += 1;
            }
        }
        self.val
    }

    /// Force the next call to recompute. Test hook: ticking a module with its
    /// memos invalidated every sample executes the pre-memoization code path,
    /// which equivalence tests compare bitwise against the memoized path.
    #[cfg(test)]
    pub(crate) fn invalidate(&mut self) {
        self.valid = false;
    }

    /// Number of recomputes performed so far (cache-behavior tests).
    #[cfg(test)]
    pub(crate) fn recompute_count(&self) -> u64 {
        self.recomputes
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

    /// Like [`rising`](Self::rising), but on a rising edge also returns the
    /// estimated fractional sample position of the threshold crossing.
    ///
    /// The returned `frac` is in `[0, 1]`: `0` means the crossing happened right
    /// at the previous sample instant, `1` means it happened right at the
    /// current sample. It is derived by linearly interpolating between the
    /// previous and current sample. Records `v` as the new previous sample.
    #[inline]
    pub fn rising_frac(&mut self, v: f64) -> Option<f64> {
        self.rising_frac_above(v, GATE_THRESHOLD_V)
    }

    /// [`rising_frac`](Self::rising_frac) with an explicit `threshold`.
    #[inline]
    pub fn rising_frac_above(&mut self, v: f64, threshold: f64) -> Option<f64> {
        let prev = self.prev;
        self.prev = v;
        if v > threshold && prev <= threshold {
            let denom = v - prev;
            let frac = if denom > 0.0 {
                ((threshold - prev) / denom).clamp(0.0, 1.0)
            } else {
                1.0
            };
            Some(frac)
        } else {
            None
        }
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

#[cfg(test)]
mod tests {
    use super::{voct_to_hz, Memo, C4_HZ};
    use libm::Libm;

    #[test]
    fn test_memo_first_call_always_computes() {
        let mut memo: Memo<1, f64> = Memo::new(123.456);
        // The placeholder must never be observable, even if the first key's
        // bits happen to equal the zeroed initial key state.
        let v = memo.get_or_compute([0.0], || 7.0);
        assert_eq!(v.to_bits(), 7.0f64.to_bits());
        assert_eq!(memo.recompute_count(), 1);
    }

    #[test]
    fn test_memo_hit_and_miss() {
        let mut memo: Memo<2, f64> = Memo::new(0.0);
        let v1 = memo.get_or_compute([1.0, 2.0], || 3.0);
        let v2 = memo.get_or_compute([1.0, 2.0], || unreachable!("must hit"));
        assert_eq!(v1.to_bits(), v2.to_bits());
        assert_eq!(memo.recompute_count(), 1);

        let v3 = memo.get_or_compute([1.0, 2.5], || 4.0);
        assert_eq!(v3.to_bits(), 4.0f64.to_bits());
        assert_eq!(memo.recompute_count(), 2);
    }

    #[test]
    fn test_memo_signed_zero_keys_are_distinct() {
        // Bit comparison: -0.0 and +0.0 must not alias (conservative miss).
        let mut memo: Memo<1, f64> = Memo::new(0.0);
        memo.get_or_compute([0.0], || 1.0);
        let v = memo.get_or_compute([-0.0], || 2.0);
        assert_eq!(v.to_bits(), 2.0f64.to_bits());
        assert_eq!(memo.recompute_count(), 2);
    }

    #[test]
    fn test_memo_nan_key_hits_identical_nan() {
        // A NaN key with identical bits may hit: a recompute from the same
        // input bits would produce the same output bits (pure computation).
        let mut memo: Memo<1, f64> = Memo::new(0.0);
        let v1 = memo.get_or_compute([f64::NAN], || 9.0);
        let v2 = memo.get_or_compute([f64::NAN], || unreachable!("must hit"));
        assert_eq!(v1.to_bits(), v2.to_bits());
        assert_eq!(memo.recompute_count(), 1);
    }

    #[test]
    fn test_memo_invalidate_forces_recompute() {
        let mut memo: Memo<1, f64> = Memo::new(0.0);
        memo.get_or_compute([5.0], || 1.0);
        memo.invalidate();
        let v = memo.get_or_compute([5.0], || 2.0);
        assert_eq!(v.to_bits(), 2.0f64.to_bits());
        assert_eq!(memo.recompute_count(), 2);
    }

    /// **A finite V/Oct always yields a finite frequency.** Before the clamp,
    /// `2^1100` was `inf`, which reached the phase accumulator as a non-finite
    /// increment; the modules recovered, but the property could not be stated.
    #[test]
    fn voct_to_hz_is_finite_for_any_finite_input() {
        for &v in &[
            0.0, 1.0, -1.0, 8.5, -14.7, 32.0, -32.0, 1100.0, -1100.0, 1e300, -1e300,
        ] {
            let hz = voct_to_hz(v);
            assert!(hz.is_finite(), "voct_to_hz({v}) = {hz}");
            assert!(
                hz > 0.0,
                "voct_to_hz({v}) = {hz} is not a positive frequency"
            );
        }
    }

    /// The clamp must not touch anything a musician can reach. The audible band
    /// is inside ±11 octaves of C4 and even a 192 kHz Nyquist is +8.5, so every
    /// value in that range must come back exactly as `C4 * 2^v`.
    #[test]
    fn voct_to_hz_is_untouched_across_the_reachable_range() {
        for step in -110..=110 {
            let v = step as f64 / 10.0;
            let expected = C4_HZ * Libm::<f64>::pow(2.0, v);
            assert_eq!(
                voct_to_hz(v).to_bits(),
                expected.to_bits(),
                "voct_to_hz({v}) moved"
            );
        }
    }

    /// NaN carries no pitch, so it is passed through rather than clamped into
    /// a plausible-looking frequency — the downstream recovery path exists for
    /// exactly that case.
    #[test]
    fn voct_to_hz_passes_nan_through() {
        assert!(voct_to_hz(f64::NAN).is_nan());
    }
}
