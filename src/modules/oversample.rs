//! Oversampling / anti-aliasing helper for nonlinear stages (Q143).
//!
//! Nonlinear waveshapers (distortion, wavefolding) generate harmonics well above
//! the input band. At the base sample rate those harmonics fold back below Nyquist
//! as inharmonic aliasing. Running the nonlinearity at 2x or 4x the sample rate
//! pushes the fold-back point higher, and a lowpass before decimation removes the
//! newly generated ultrasonic content before it can alias.
//!
//! This module provides a shared, `no_std`, allocation-free (after construction)
//! [`Oversampler`]. It performs `upsample -> per-sample process -> decimate`
//! around a caller-supplied per-sample closure, using a single linear-phase FIR
//! anti-imaging/anti-aliasing filter in polyphase form for both the interpolation
//! and decimation stages.
//!
//! # Filter design
//!
//! The FIR is a windowed-sinc lowpass with cutoff `0.5 / factor` (normalized to the
//! *oversampled* rate) — i.e. the original Nyquist — using a Blackman window. The
//! design is fixed and deterministic (fixed length, fixed window, fixed cutoff):
//! `L = 31` taps at 2x and `L = 63` taps at 4x. A Blackman window yields roughly
//! **-74 dB** peak sidelobe / stopband attenuation, with the transition band
//! centered on the original Nyquist. Coefficients are evaluated once in
//! [`Oversampler::new`] into fixed-size arrays; no allocation occurs during
//! processing.

use libm::Libm;

/// Oversampling factor for nonlinear stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Oversample {
    /// No oversampling — the nonlinearity runs at the base sample rate.
    #[default]
    Off,
    /// 2x oversampling.
    X2,
    /// 4x oversampling.
    X4,
}

impl Oversample {
    /// Integer factor (1, 2, or 4).
    #[inline]
    pub fn factor(self) -> usize {
        match self {
            Oversample::Off => 1,
            Oversample::X2 => 2,
            Oversample::X4 => 4,
        }
    }
}

/// Largest supported FIR length (4x uses 63 taps).
const MAX_TAPS: usize = 63;
/// Largest supported input delay-line length (`ceil(MAX_TAPS / factor)` is 16 for
/// both 2x and 4x).
const MAX_INPUT_TAPS: usize = 16;

/// Polyphase FIR oversampler wrapping a per-sample nonlinear function.
///
/// Zero allocation after construction: all state lives in fixed-size arrays. Use
/// [`Oversampler::process`] to run a closure at the oversampled rate.
#[derive(Debug, Clone)]
pub struct Oversampler {
    factor: usize,
    len: usize,
    in_len: usize,
    /// FIR coefficients (DC gain normalized to 1.0), first `len` entries valid.
    h: [f64; MAX_TAPS],
    /// Ring of recent input-rate samples.
    in_ring: [f64; MAX_INPUT_TAPS],
    in_pos: usize,
    /// Ring of recent oversampled processed samples.
    os_ring: [f64; MAX_TAPS],
    os_pos: usize,
}

impl Oversampler {
    /// Create an oversampler for the given [`Oversample`] factor.
    pub fn new(mode: Oversample) -> Self {
        let factor = mode.factor();
        let (len, cutoff) = match mode {
            Oversample::Off => (1, 0.5),
            Oversample::X2 => (31, 0.25),
            Oversample::X4 => (63, 0.125),
        };

        let mut h = [0.0; MAX_TAPS];
        if factor > 1 {
            design_lowpass(&mut h[..len], cutoff);
        }

        // Input delay line needs one tap per input sample referenced by the
        // interpolation polyphase: `ceil(len / factor)`, but never more than the
        // filter length. Clamp to the fixed capacity.
        let in_len = if factor > 1 {
            len.div_ceil(factor).clamp(1, MAX_INPUT_TAPS)
        } else {
            1
        };

        Self {
            factor,
            len,
            in_len,
            h,
            in_ring: [0.0; MAX_INPUT_TAPS],
            in_pos: 0,
            os_ring: [0.0; MAX_TAPS],
            os_pos: 0,
        }
    }

    /// Current oversampling factor (1, 2, or 4).
    #[inline]
    pub fn factor(&self) -> usize {
        self.factor
    }

    /// Clear all internal delay lines.
    pub fn reset(&mut self) {
        self.in_ring = [0.0; MAX_INPUT_TAPS];
        self.os_ring = [0.0; MAX_TAPS];
        self.in_pos = 0;
        self.os_pos = 0;
    }

    /// Process one input sample, running `f` at the oversampled rate.
    ///
    /// When the factor is 1 (`Oversample::Off`) this is exactly `f(x)` with no
    /// filtering or added latency, so existing behavior is preserved bit-for-bit
    /// when oversampling is disabled.
    #[inline]
    pub fn process<F: FnMut(f64) -> f64>(&mut self, x: f64, mut f: F) -> f64 {
        if self.factor == 1 {
            return f(x);
        }

        let n = self.factor;
        let len = self.len;

        // Push the new input sample into the input ring.
        self.in_pos = (self.in_pos + 1) % self.in_len;
        self.in_ring[self.in_pos] = x;

        // Interpolate to `n` oversampled samples via the polyphase decomposition of
        // the FIR, apply the nonlinearity, and push each into the oversampled ring.
        for p in 0..n {
            let mut acc = 0.0;
            let mut tap = p;
            let mut i = 0;
            while tap < len {
                let idx = (self.in_pos + self.in_len - i) % self.in_len;
                acc += self.h[tap] * self.in_ring[idx];
                tap += n;
                i += 1;
            }
            // Gain of `n` compensates the zero-stuffing energy loss.
            let up = acc * (n as f64);
            let processed = f(up);
            self.os_pos = (self.os_pos + 1) % len;
            self.os_ring[self.os_pos] = processed;
        }

        // Decimate: FIR-filter the oversampled stream and keep the phase-0 sample
        // (the first of the `n` just pushed), which sits `n - 1` positions back.
        let center = (self.os_pos + len - (n - 1)) % len;
        let mut out = 0.0;
        for (j, &hj) in self.h[..len].iter().enumerate() {
            let idx = (center + len - j) % len;
            out += hj * self.os_ring[idx];
        }
        out
    }
}

/// Fill `h` with a Blackman-windowed sinc lowpass of the given normalized `cutoff`
/// (cycles/sample, `0 < cutoff < 0.5`) and normalize its DC gain to 1.0.
fn design_lowpass(h: &mut [f64], cutoff: f64) {
    let l = h.len();
    let m = (l - 1) as f64 / 2.0;
    let pi = core::f64::consts::PI;
    let mut sum = 0.0;
    for (n, hn) in h.iter_mut().enumerate() {
        let x = n as f64 - m;
        let sinc = if Libm::<f64>::fabs(x) < 1e-9 {
            2.0 * cutoff
        } else {
            Libm::<f64>::sin(2.0 * pi * cutoff * x) / (pi * x)
        };
        let nn = n as f64;
        let denom = (l - 1) as f64;
        let window = 0.42 - 0.5 * Libm::<f64>::cos(2.0 * pi * nn / denom)
            + 0.08 * Libm::<f64>::cos(4.0 * pi * nn / denom);
        *hn = sinc * window;
        sum += *hn;
    }
    if sum.abs() > 1e-12 {
        for hn in h.iter_mut() {
            *hn /= sum;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oversample_factor() {
        assert_eq!(Oversample::Off.factor(), 1);
        assert_eq!(Oversample::X2.factor(), 2);
        assert_eq!(Oversample::X4.factor(), 4);
        assert_eq!(Oversample::default(), Oversample::Off);
    }

    #[test]
    fn test_off_is_transparent() {
        let mut os = Oversampler::new(Oversample::Off);
        // Identity closure: Off must pass through exactly, no latency.
        for &x in &[0.0, 1.0, -0.5, 3.3, -2.2] {
            let y = os.process(x, |v| v);
            assert!((y - x).abs() < 1e-12);
        }
    }

    #[test]
    fn test_dc_gain_unity_2x() {
        let mut os = Oversampler::new(Oversample::X2);
        let mut last = 0.0;
        for _ in 0..2000 {
            last = os.process(1.0, |v| v);
        }
        assert!((last - 1.0).abs() < 1e-3, "2x DC gain not unity: {last}");
    }

    #[test]
    fn test_dc_gain_unity_4x() {
        let mut os = Oversampler::new(Oversample::X4);
        let mut last = 0.0;
        for _ in 0..4000 {
            last = os.process(1.0, |v| v);
        }
        assert!((last - 1.0).abs() < 1e-3, "4x DC gain not unity: {last}");
    }

    #[test]
    fn test_passband_sine_preserved_2x() {
        // A low-frequency sine (well inside the passband) should pass at ~unity
        // amplitude through the identity-processed oversampler.
        let sr = 44100.0;
        let freq = 200.0;
        let mut os = Oversampler::new(Oversample::X2);
        let n = 4000;
        let mut max_out: f64 = 0.0;
        // Skip the filter warm-up transient before measuring.
        for i in 0..n {
            let t = i as f64 / sr;
            let x = Libm::<f64>::sin(2.0 * core::f64::consts::PI * freq * t);
            let y = os.process(x, |v| v);
            if i > 2000 {
                max_out = max_out.max(y.abs());
            }
        }
        assert!(
            (max_out - 1.0).abs() < 0.05,
            "passband sine amplitude altered: {max_out}"
        );
    }

    #[test]
    fn test_reset_clears_state() {
        let mut os = Oversampler::new(Oversample::X4);
        for _ in 0..100 {
            os.process(1.0, |v| v);
        }
        os.reset();
        // First sample after reset with zero input and identity is ~0.
        let y = os.process(0.0, |v| v);
        assert!(y.abs() < 1e-9);
    }
}
