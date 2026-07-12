//! Nonlinear and spectral processing modules.

use super::common::{env_coef, GATE_THRESHOLD_V};
use super::oversample::{Oversample, Oversampler};
use crate::analog::saturation;
use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use alloc::vec;
use alloc::vec::Vec;
use libm::Libm;

/// `no_std`-compatible equivalent of `f64::rem_euclid`, wrapping `x` into the
/// non-negative range `[0, |n|)`.
fn rem_euclid_f64(x: f64, n: f64) -> f64 {
    let r = Libm::<f64>::fmod(x, n);
    if r < 0.0 {
        r + Libm::<f64>::fabs(n)
    } else {
        r
    }
}

/// Bitcrusher
///
/// Lo-fi effect that reduces bit depth and sample rate.
pub struct Bitcrusher {
    hold_sample: f64,
    hold_counter: f64,
    spec: PortSpec,
}

impl Bitcrusher {
    pub fn new() -> Self {
        Self {
            hold_sample: 0.0,
            hold_counter: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "bits", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "downsample", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }
}

impl Default for Bitcrusher {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for Bitcrusher {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let bits_cv = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
        let downsample_cv = inputs.get_or(2, 0.0).clamp(0.0, 1.0);

        let bits = 1.0 + bits_cv * 15.0;
        let downsample_factor = 1.0 + downsample_cv * 63.0;

        // Q029: accumulate a fractional sample-and-hold phase. Subtracting the
        // factor on wrap (instead of resetting to 0) lets fractional ratios such
        // as 1.5 average correctly over time rather than rounding up to the next
        // integer period.
        self.hold_counter += 1.0;
        if self.hold_counter >= downsample_factor {
            self.hold_counter -= downsample_factor;
            self.hold_sample = input;
        }

        // Q032: mid-tread (rounding) quantizer over an integer number of codes.
        // Rounding is unbiased (no ~0.5 LSB DC offset). Using an integer step
        // count and clamping the normalized value maps full-scale exactly to the
        // top code instead of one step beyond the intended range.
        let levels = Libm::<f64>::round(Libm::<f64>::pow(2.0, bits)).max(2.0);
        let steps = levels - 1.0;
        let normalized = ((self.hold_sample / 5.0 + 1.0) * 0.5).clamp(0.0, 1.0);
        let quantized = Libm::<f64>::round(normalized * steps) / steps;
        outputs.set(10, (quantized * 2.0 - 1.0) * 5.0);
    }

    fn reset(&mut self) {
        self.hold_sample = 0.0;
        self.hold_counter = 0.0;
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "bitcrusher"
    }
}

/// Lowest cutoff of the distortion tone control (tone CV = 0).
const DISTORTION_TONE_MIN_HZ: f64 = 500.0;
/// Highest cutoff of the distortion tone control (tone CV = 1, ~transparent).
const DISTORTION_TONE_MAX_HZ: f64 = 18_000.0;

/// Distortion
///
/// Waveshaping distortion with multiple algorithms:
/// - Soft clip (bounded `tanh`)
/// - Hard clip
/// - Foldback
/// - Asymmetric (tube-style)
///
/// All shapers operate in the normalized ±1 domain (the Audio convention is
/// ±5V) so their saturation points match the signal level, and every algorithm
/// stays within ±5V. The `tone` control is a real one-pole low-pass whose
/// cutoff is swept from `DISTORTION_TONE_MIN_HZ` (dark) to
/// `DISTORTION_TONE_MAX_HZ` (≈ transparent).
pub struct Distortion {
    /// One-pole low-pass state for the tone control (Q025).
    tone_lp: f64,
    sample_rate: f64,
    /// Opt-in oversampler for the waveshaping stage (Q143). Default `Off` keeps
    /// the base-rate behavior (and thus every existing test) bit-for-bit.
    oversampler: Oversampler,
    spec: PortSpec,
}

impl Distortion {
    pub fn new(sample_rate: f64) -> Self {
        let sample_rate = if sample_rate > 0.0 {
            sample_rate
        } else {
            44100.0
        };
        Self {
            tone_lp: 0.0,
            sample_rate,
            oversampler: Oversampler::new(Oversample::Off),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "drive", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "tone", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(3, "mode", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(4, "mix", SignalKind::CvUnipolar)
                        .with_default(1.0)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }

    // Soft clip using a genuinely bounded `tanh` (Q026). Operates in the
    // normalized ±1 domain then rescales to ±5V, so the output saturates at ±5V.
    fn soft_clip(x: f64, drive: f64) -> f64 {
        let gained = (x / 5.0) * (1.0 + drive * 10.0);
        Libm::<f64>::tanh(gained) * 5.0
    }

    // Hard clip (Q026): normalize, clamp to ±1, rescale to ±5V so its level
    // matches the surrounding ±5V modules.
    fn hard_clip(x: f64, drive: f64) -> f64 {
        let gained = (x / 5.0) * (1.0 + drive * 10.0);
        gained.clamp(-1.0, 1.0) * 5.0
    }

    // Foldback distortion (Q026 normalization + Q030 closed-form fold).
    fn foldback(x: f64, drive: f64) -> f64 {
        let gained = (x / 5.0) * (1.0 + drive * 5.0);
        Self::triangle_fold(gained, 1.0) * 5.0
    }

    /// Closed-form triangle foldback (Q030): reflects `x` back into
    /// `[-threshold, threshold]` via the periodic triangle identity, replacing
    /// a data-dependent `while` loop with constant-time arithmetic. It is
    /// mathematically identical to repeatedly reflecting about ±threshold.
    fn triangle_fold(x: f64, threshold: f64) -> f64 {
        let period = 4.0 * threshold;
        threshold - Libm::<f64>::fabs(rem_euclid_f64(x + threshold, period) - 2.0 * threshold)
    }

    // Asymmetric tube-style distortion (Q026): normalized, bounded to ±5V.
    fn asymmetric(x: f64, drive: f64) -> f64 {
        let gained = (x / 5.0) * (1.0 + drive * 8.0);
        let shaped = if gained >= 0.0 {
            // Softer positive knee, bounded to [0, 1).
            1.0 - Libm::<f64>::exp(-gained)
        } else {
            // Harder negative clipping via bounded tanh, bounded to (-1, 0].
            Libm::<f64>::tanh(gained)
        };
        shaped * 5.0
    }

    /// Select the oversampling factor for the waveshaping stage (Q143).
    ///
    /// Defaults to [`Oversample::Off`]. Enabling 2x/4x runs the (aliasing-prone)
    /// waveshaper at a higher internal rate and band-limits before decimation,
    /// materially reducing the inharmonic aliasing of hard/foldback modes at high
    /// input frequencies. The tone low-pass runs at the base rate, after
    /// decimation.
    pub fn set_oversample(&mut self, mode: Oversample) {
        self.oversampler = Oversampler::new(mode);
    }

    /// Current oversampling factor of the waveshaping stage (1 = off, 2, or 4).
    pub fn oversample_factor(&self) -> usize {
        self.oversampler.factor()
    }
}

impl Default for Distortion {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Distortion {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let drive = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
        let tone = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let mode = inputs.get_or(3, 0.0).clamp(0.0, 1.0);
        let mix = inputs.get_or(4, 1.0).clamp(0.0, 1.0);

        // Select distortion mode (quantized to 4 modes)
        let mode_idx = (mode * 3.99) as u8;
        // Run the waveshaper through the (opt-in) oversampler so its generated
        // harmonics are band-limited before folding back below Nyquist (Q143).
        // With `Oversample::Off` this is exactly the base-rate shaping call.
        let distorted = self.oversampler.process(input, |x| match mode_idx {
            0 => Self::soft_clip(x, drive),
            1 => Self::hard_clip(x, drive),
            2 => Self::foldback(x, drive),
            _ => Self::asymmetric(x, drive),
        });

        // Q025: real one-pole low-pass tone control with retained state. The
        // cutoff is swept logarithmically by the tone CV from
        // DISTORTION_TONE_MIN_HZ (dark) to DISTORTION_TONE_MAX_HZ (≈ transparent),
        // so higher tone genuinely preserves more high-frequency content.
        let cutoff = DISTORTION_TONE_MIN_HZ
            * Libm::<f64>::pow(DISTORTION_TONE_MAX_HZ / DISTORTION_TONE_MIN_HZ, tone);
        let alpha =
            1.0 - Libm::<f64>::exp(-2.0 * core::f64::consts::PI * cutoff / self.sample_rate);
        self.tone_lp += alpha * (distorted - self.tone_lp);
        let filtered = self.tone_lp;

        outputs.set(10, input * (1.0 - mix) + filtered * mix);
    }

    fn reset(&mut self) {
        self.tone_lp = 0.0;
        self.oversampler.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
        self.tone_lp = 0.0;
        self.oversampler.reset();
    }

    fn type_id(&self) -> &'static str {
        "distortion"
    }

    // Bridge the `oversample` internal parameter to live-patch introspection.
    crate::impl_introspect!();
}

// ============================================================================
// P3 Oscillators: Supersaw, Karplus-Strong
// ============================================================================

/// Ring Modulator
///
/// Multiplies two audio signals together, producing sum and difference frequencies.
/// Classic technique for metallic, bell-like, and atonal sounds.
pub struct RingModulator {
    spec: PortSpec,
}

impl RingModulator {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "carrier", SignalKind::Audio),
                    PortDef::new(1, "modulator", SignalKind::Audio),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }
}

impl Default for RingModulator {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for RingModulator {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let carrier = inputs.get_or(0, 0.0);
        let modulator = inputs.get_or(1, 0.0);

        // Ring modulation is simple multiplication
        // Normalize by 5.0 to keep output in ±5V range (both inputs are ±5V)
        let out = (carrier * modulator) / 5.0;
        outputs.set(10, out);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "ring_mod"
    }
}

/// Granular pitch shifter
///
/// Real-time pitch shifting using two overlapping grains with crossfade.
/// Uses a circular delay buffer with variable playback rate.
///
/// # Latency and aliasing (Q033)
/// The wet path is delayed: each grain reads from behind the write pointer by at
/// least half the window, and further behind for pitch-up (by `(rate-1)·window`)
/// so a grain's read pointer can never overtake the write pointer within its
/// lifetime. To keep that margin inside the ring buffer, the effective window is
/// automatically shortened at high pitch-up ratios. No oversampling is
/// performed, so the resampled grains alias; the effect is intended as a
/// character/lo-fi shifter, not a transparent one. Pitch is bounded to ±24
/// semitones (playback rate 0.25×–4×).
///
/// # Ports
/// - Input 0: Audio input
/// - Input 1: Pitch shift in semitones (-24 to +24, bipolar CV maps to range)
/// - Input 2: Window size (0-1 CV maps to 10-100ms)
/// - Input 3: Wet/dry mix (0-1)
/// - Output 10: Audio output
pub struct PitchShifter {
    /// Circular delay buffer (100ms at 48kHz max)
    buffer: [f64; 4800],
    /// Write position in buffer
    write_pos: usize,
    /// Two grain positions (fractional)
    grain_pos: [f64; 2],
    /// Two grain phases (0-1 for window position)
    grain_phase: [f64; 2],
    sample_rate: f64,
    spec: PortSpec,
}

impl PitchShifter {
    /// Maximum buffer size in samples (100ms at 48kHz)
    const BUFFER_SIZE: usize = 4800;

    pub fn new(sample_rate: f64) -> Self {
        let spec = PortSpec {
            inputs: vec![
                PortDef::new(0, "in", SignalKind::Audio),
                PortDef::new(1, "shift", SignalKind::CvBipolar).with_default(0.0),
                PortDef::new(2, "window", SignalKind::CvUnipolar).with_default(0.5),
                PortDef::new(3, "mix", SignalKind::CvUnipolar).with_default(1.0),
            ],
            outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
        };

        Self {
            buffer: [0.0; Self::BUFFER_SIZE],
            write_pos: 0,
            grain_pos: [0.0, 0.5 * Self::BUFFER_SIZE as f64], // Start 180° out of phase
            grain_phase: [0.0, 0.5],                          // 50% phase offset
            sample_rate,
            spec,
        }
    }

    /// Hann window function (0-1 maps to 0-1-0)
    fn hann_window(phase: f64) -> f64 {
        0.5 * (1.0 - Libm::<f64>::cos(phase * 2.0 * core::f64::consts::PI))
    }

    /// Read from circular buffer with linear interpolation
    fn read_buffer(&self, pos: f64) -> f64 {
        let pos = rem_euclid_f64(pos, Self::BUFFER_SIZE as f64);
        let idx0 = pos as usize;
        let idx1 = (idx0 + 1) % Self::BUFFER_SIZE;
        let frac = pos - Libm::<f64>::floor(pos);

        self.buffer[idx0] * (1.0 - frac) + self.buffer[idx1] * frac
    }
}

impl Default for PitchShifter {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for PitchShifter {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);

        // Map inputs
        // Shift: bipolar CV ±5V maps to ±24 semitones
        let shift_semitones = (inputs.get_or(1, 0.0) / 5.0) * 24.0;
        let shift_semitones = shift_semitones.clamp(-24.0, 24.0);

        // Window size: 10-100ms
        let window_cv = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let window_ms = 10.0 + window_cv * 90.0;
        let mut window_samples = (window_ms * self.sample_rate / 1000.0) as usize;
        window_samples = window_samples.min(Self::BUFFER_SIZE / 2);

        // Mix
        let mix = inputs.get_or(3, 1.0).clamp(0.0, 1.0);

        // Write input to circular buffer
        self.buffer[self.write_pos] = input / 5.0; // Normalize from audio
        self.write_pos = (self.write_pos + 1) % Self::BUFFER_SIZE;

        // Calculate playback rate
        let rate = Libm::<f64>::pow(2.0, shift_semitones / 12.0);

        // Q033: keep each grain's read pointer strictly behind the write pointer
        // for the grain's whole lifetime. Relative to the write pointer a grain
        // gains (rate-1) samples per sample, i.e. (rate-1)·window over a window;
        // we start it that far behind (plus a half-window cushion) and shorten
        // the window when pitching up so that margin fits inside the buffer.
        if rate > 1.0 {
            let max_lead = Self::BUFFER_SIZE as f64 * 0.4;
            let window_cap = (max_lead / (rate - 1.0)) as usize;
            window_samples = window_samples.min(window_cap);
        }
        window_samples = window_samples.max(1);
        let read_margin =
            (rate - 1.0).max(0.0) * window_samples as f64 + window_samples as f64 * 0.5;

        let phase_inc = 1.0 / window_samples as f64;

        // Process both grains
        let mut wet_output = 0.0;

        for i in 0..2 {
            // Read from buffer at grain position
            let sample = self.read_buffer(self.grain_pos[i]);

            // Apply Hann window
            let window = Self::hann_window(self.grain_phase[i]);
            wet_output += sample * window;

            // Advance grain position (write_pos - offset, at playback rate)
            // When rate > 1 (pitch up), read faster than write
            // When rate < 1 (pitch down), read slower than write
            self.grain_pos[i] += rate;

            // Wrap grain position
            if self.grain_pos[i] >= Self::BUFFER_SIZE as f64 {
                self.grain_pos[i] -= Self::BUFFER_SIZE as f64;
            } else if self.grain_pos[i] < 0.0 {
                self.grain_pos[i] += Self::BUFFER_SIZE as f64;
            }

            // Advance phase
            self.grain_phase[i] += phase_inc;

            // Reset grain when phase completes
            if self.grain_phase[i] >= 1.0 {
                self.grain_phase[i] -= 1.0;
                // Reset position behind the write pointer by the read margin so
                // the grain's read pointer cannot overtake the write pointer
                // (Q033).
                self.grain_pos[i] = rem_euclid_f64(
                    self.write_pos as f64 - read_margin,
                    Self::BUFFER_SIZE as f64,
                );
            }
        }

        // Mix wet and dry
        let dry = input / 5.0;
        let output = dry * (1.0 - mix) + wet_output * mix;

        outputs.set(10, output * 5.0); // Scale back to audio
    }

    fn reset(&mut self) {
        self.buffer = [0.0; Self::BUFFER_SIZE];
        self.write_pos = 0;
        self.grain_pos = [0.0, Self::BUFFER_SIZE as f64 * 0.5];
        self.grain_phase = [0.0, 0.5];
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.reset();
    }

    fn type_id(&self) -> &'static str {
        "pitch_shifter"
    }
}

/// Maximum number of vocoder bands
const MAX_VOCODER_BANDS: usize = 16;

/// Minimum frequency for vocoder bands (Hz)
const VOCODER_FREQ_MIN: f64 = 100.0;

/// Maximum frequency for vocoder bands (Hz)
const VOCODER_FREQ_MAX: f64 = 8000.0;

/// Largest Chamberlin SVF coefficient `f = 2·sin(π·freq/sr)` a band center is
/// allowed to produce. The filter clamps the coefficient at 0.99 for stability;
/// keeping every band strictly below that (Q027) guarantees the top bands stay
/// distinct instead of collapsing onto the clamp. The corresponding maximum
/// band center is `asin(coef/2)·sr/π`, which is sample-rate dependent.
const VOCODER_MAX_SVF_COEF: f64 = 0.95;

/// Spectral vocoder with configurable band count
///
/// Uses bandpass filter banks for both analysis (modulator) and synthesis
/// (carrier), with envelope followers to extract amplitude from the modulator
/// and apply it to the carrier.
///
/// # Ports
/// - Input 0: Carrier input (typically oscillator)
/// - Input 1: Modulator input (typically voice)
/// - Input 2: Number of bands (CV 0-1 maps to 4-16 bands)
/// - Input 3: Envelope attack (0-1)
/// - Input 4: Envelope release (0-1)
/// - Output 10: Vocoded output
pub struct Vocoder {
    // Analysis (modulator) filters - state variable filter state [LP, HP] per band
    analysis_state: [[f64; 2]; MAX_VOCODER_BANDS],
    // Synthesis (carrier) filters
    synthesis_state: [[f64; 2]; MAX_VOCODER_BANDS],
    // Envelope followers for each band
    envelopes: [f64; MAX_VOCODER_BANDS],

    // Pre-computed band frequencies
    band_freqs: [f64; MAX_VOCODER_BANDS],

    sample_rate: f64,
    spec: PortSpec,
}

impl Vocoder {
    /// Create a new vocoder with the given sample rate
    pub fn new(sample_rate: f64) -> Self {
        let mut vocoder = Self {
            analysis_state: [[0.0; 2]; MAX_VOCODER_BANDS],
            synthesis_state: [[0.0; 2]; MAX_VOCODER_BANDS],
            envelopes: [0.0; MAX_VOCODER_BANDS],
            band_freqs: [0.0; MAX_VOCODER_BANDS],
            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "carrier", SignalKind::Audio),
                    PortDef::new(1, "modulator", SignalKind::Audio),
                    PortDef::new(2, "bands", SignalKind::CvUnipolar).with_default(1.0),
                    PortDef::new(3, "attack", SignalKind::CvUnipolar).with_default(0.3),
                    PortDef::new(4, "release", SignalKind::CvUnipolar).with_default(0.3),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        };
        vocoder.compute_band_freqs();
        vocoder
    }

    /// Compute logarithmically spaced band frequencies.
    ///
    /// Q027: the highest band center is capped relative to the sample rate so
    /// that the Chamberlin SVF coefficient stays below its stability clamp
    /// (0.99). Without this cap, at 44.1 kHz any band above ~7.3 kHz — and many
    /// more at lower sample rates — clamp to the same coefficient and collapse
    /// onto one another. The cap is `asin(VOCODER_MAX_SVF_COEF/2)·sr/π`.
    fn compute_band_freqs(&mut self) {
        let coef_limit_freq = Libm::<f64>::asin(VOCODER_MAX_SVF_COEF / 2.0) * self.sample_rate
            / core::f64::consts::PI;
        let freq_max = VOCODER_FREQ_MAX
            .min(coef_limit_freq)
            .max(VOCODER_FREQ_MIN * 2.0);

        let log_min = Libm::<f64>::log2(VOCODER_FREQ_MIN);
        let log_max = Libm::<f64>::log2(freq_max);

        for i in 0..MAX_VOCODER_BANDS {
            let t = i as f64 / (MAX_VOCODER_BANDS - 1) as f64;
            let log_freq = log_min + t * (log_max - log_min);
            self.band_freqs[i] = Libm::<f64>::exp2(log_freq);
        }
    }

    /// Process a single band using a state variable filter (bandpass)
    /// Returns the bandpass output
    #[inline]
    fn process_svf_bandpass(
        state: &mut [f64; 2],
        input: f64,
        freq: f64,
        q: f64,
        sample_rate: f64,
    ) -> f64 {
        // Frequency coefficient
        let f = 2.0 * Libm::<f64>::sin(core::f64::consts::PI * freq / sample_rate);
        let f = f.min(0.99); // Stability limit

        // Q factor (resonance)
        let q_inv = 1.0 / q;

        // State variable filter
        let low = state[0];
        let high = input - low - q_inv * state[1];
        let band = f * high + state[1];
        let new_low = f * band + low;

        state[0] = new_low;
        state[1] = band;

        band
    }
}

impl Default for Vocoder {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Vocoder {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let carrier = inputs.get_or(0, 0.0);
        let modulator = inputs.get_or(1, 0.0);
        let bands_cv = inputs.get_or(2, 1.0).clamp(0.0, 1.0);
        let attack_cv = inputs.get_or(3, 0.3).clamp(0.0, 1.0);
        let release_cv = inputs.get_or(4, 0.3).clamp(0.0, 1.0);

        // Map CV to band count (4-16)
        let num_bands = Libm::<f64>::round(4.0 + bands_cv * 12.0) as usize;
        let num_bands = num_bands.min(MAX_VOCODER_BANDS);

        // Compute envelope coefficients (10ms to 200ms range)
        let attack_time = 0.01 + attack_cv * 0.19;
        let release_time = 0.01 + release_cv * 0.19;
        let attack_coef = env_coef(attack_time, self.sample_rate);
        let release_coef = env_coef(release_time, self.sample_rate);

        // Q factor for bandpass filters
        let q = 2.0;

        let mut output = 0.0;

        for i in 0..num_bands {
            let freq = self.band_freqs[i * MAX_VOCODER_BANDS / num_bands];

            // Analysis path: filter modulator and extract envelope
            let analysis_band = Self::process_svf_bandpass(
                &mut self.analysis_state[i],
                modulator,
                freq,
                q,
                self.sample_rate,
            );

            // Envelope follower
            let rectified = analysis_band.abs();
            if rectified > self.envelopes[i] {
                self.envelopes[i] =
                    attack_coef * self.envelopes[i] + (1.0 - attack_coef) * rectified;
            } else {
                self.envelopes[i] =
                    release_coef * self.envelopes[i] + (1.0 - release_coef) * rectified;
            }

            // Synthesis path: filter carrier and apply envelope
            let synthesis_band = Self::process_svf_bandpass(
                &mut self.synthesis_state[i],
                carrier,
                freq,
                q,
                self.sample_rate,
            );

            // Apply envelope to carrier band
            output += synthesis_band * self.envelopes[i];
        }

        // Normalize by number of bands to prevent clipping
        output /= num_bands as f64;

        // Scale output
        outputs.set(10, output * 4.0);
    }

    fn reset(&mut self) {
        self.analysis_state = [[0.0; 2]; MAX_VOCODER_BANDS];
        self.synthesis_state = [[0.0; 2]; MAX_VOCODER_BANDS];
        self.envelopes = [0.0; MAX_VOCODER_BANDS];
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.compute_band_freqs();
        self.reset();
    }

    fn type_id(&self) -> &'static str {
        "vocoder"
    }
}

// =============================================================================
// Granular - Granular Synthesis/Processing Engine
// =============================================================================

/// Maximum number of concurrent grains
const MAX_GRAINS: usize = 16;

/// Granular buffer size (2 seconds at 48kHz)
const GRANULAR_BUFFER_SIZE: usize = 96000;

/// Represents a single active grain
#[derive(Clone, Copy)]
struct Grain {
    /// Whether this grain is active
    active: bool,
    /// Start position in the buffer (samples)
    start_pos: usize,
    /// Current phase within the grain (0.0 to 1.0)
    phase: f64,
    /// Grain size in samples
    size: usize,
    /// Playback speed (1.0 = normal, 2.0 = octave up)
    speed: f64,
}

impl Default for Grain {
    fn default() -> Self {
        Self {
            active: false,
            start_pos: 0,
            phase: 0.0,
            size: 4410, // 100ms default
            speed: 1.0,
        }
    }
}

/// Granular synthesis/processing engine
///
/// Records input audio into a circular buffer and plays back overlapping
/// grains with individual pitch shifting and envelope shaping.
///
/// # Ports
/// - Input 0: Audio input
/// - Input 1: Playback position (0-1 maps to buffer position)
/// - Input 2: Grain size (0-1 maps to 10ms-500ms)
/// - Input 3: Density (0-1 maps to 1-20 grains per second)
/// - Input 4: Pitch shift (bipolar CV ±5V maps to ±24 semitones, i.e. playback
///   speed 0.25×–4×). Grain size is bounded so a grain's read span can never
///   exceed the buffer length at the chosen speed.
/// - Input 5: Spray (position randomization, 0-1)
/// - Input 6: Freeze (gate > 2.5V stops recording)
/// - Output 10: Processed output
pub struct Granular {
    /// Circular input buffer
    buffer: Vec<f64>,
    /// Write position in buffer
    write_pos: usize,

    /// Pool of grains
    grains: [Grain; MAX_GRAINS],

    /// Timer for spawning new grains (counts down)
    spawn_timer: usize,

    /// Random number generator for spray and density jitter
    rng: crate::rng::Rng,

    /// Smoothed constant-power normalization divisor (Q028). Tracks the expected
    /// steady-state grain overlap rather than the instantaneous active count,
    /// removing the per-sample amplitude zipper.
    norm_smooth: f64,

    sample_rate: f64,
    spec: PortSpec,
}

impl Granular {
    /// Create a new granular processor
    pub fn new(sample_rate: f64) -> Self {
        Self {
            buffer: vec![0.0; GRANULAR_BUFFER_SIZE],
            write_pos: 0,
            grains: [Grain::default(); MAX_GRAINS],
            spawn_timer: 0,
            rng: crate::rng::Rng::from_seed(42),
            norm_smooth: 1.0,
            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "position", SignalKind::CvUnipolar).with_default(0.5),
                    PortDef::new(2, "size", SignalKind::CvUnipolar).with_default(0.3),
                    PortDef::new(3, "density", SignalKind::CvUnipolar).with_default(0.5),
                    PortDef::new(4, "pitch", SignalKind::CvBipolar).with_default(0.0),
                    PortDef::new(5, "spray", SignalKind::CvUnipolar).with_default(0.1),
                    PortDef::new(6, "freeze", SignalKind::Gate).with_default(0.0),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }

    /// Compute Hann window value for grain envelope
    #[inline]
    fn hann_window(phase: f64) -> f64 {
        0.5 * (1.0 - Libm::<f64>::cos(2.0 * core::f64::consts::PI * phase))
    }

    /// Read from buffer with linear interpolation
    #[inline]
    pub fn read_buffer(&self, pos: f64) -> f64 {
        let pos = pos % GRANULAR_BUFFER_SIZE as f64;
        let index = pos as usize;
        let frac = pos - index as f64;

        let s0 = self.buffer[index % GRANULAR_BUFFER_SIZE];
        let s1 = self.buffer[(index + 1) % GRANULAR_BUFFER_SIZE];

        s0 + frac * (s1 - s0)
    }

    /// Spawn a new grain
    fn spawn_grain(&mut self, position: f64, size: usize, speed: f64, spray: f64) {
        // Find an inactive grain slot
        for grain in &mut self.grains {
            if !grain.active {
                // Calculate position with spray randomization
                let spray_offset = if spray > 0.0 {
                    (self.rng.next_f64() - 0.5) * spray * GRANULAR_BUFFER_SIZE as f64 * 0.5
                } else {
                    0.0
                };

                let base_pos = position * GRANULAR_BUFFER_SIZE as f64;
                let pos = (base_pos + spray_offset) as usize % GRANULAR_BUFFER_SIZE;

                grain.active = true;
                grain.start_pos = pos;
                grain.phase = 0.0;
                grain.size = size.max(100); // Minimum 100 samples
                grain.speed = speed;
                break;
            }
        }
    }
}

impl Default for Granular {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Granular {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let position = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
        let size_cv = inputs.get_or(2, 0.3).clamp(0.0, 1.0);
        let density_cv = inputs.get_or(3, 0.5).clamp(0.0, 1.0);
        let pitch_cv = inputs.get_or(4, 0.0).clamp(-5.0, 5.0);
        let spray = inputs.get_or(5, 0.1).clamp(0.0, 1.0);
        let freeze = inputs.get_or(6, 0.0);

        // Density: 1-20 grains per second
        let grains_per_sec = 1.0 + density_cv * 19.0;
        let spawn_interval = (self.sample_rate / grains_per_sec) as usize;

        // Q031: pitch shift ±5V maps to ±24 semitones (playback speed 0.25×–4×),
        // matching the documented range instead of the previous ±60 semitones.
        let semitones = (pitch_cv * 4.8).clamp(-24.0, 24.0);
        let speed = Libm::<f64>::exp2(semitones / 12.0);

        // Grain size: 10ms to 500ms, bounded so a grain's read span
        // (size × speed) can never exceed the buffer length (Q031). This keeps
        // fast (pitched-up) grains from lapping the circular buffer and reading
        // stale/aliased content.
        let max_size = (GRANULAR_BUFFER_SIZE as f64 / speed) as usize;
        let size_samples = (((0.01 + size_cv * 0.49) * self.sample_rate) as usize).min(max_size);

        // Record to buffer (unless frozen)
        if freeze <= GATE_THRESHOLD_V {
            self.buffer[self.write_pos] = input;
            self.write_pos = (self.write_pos + 1) % GRANULAR_BUFFER_SIZE;
        }

        // Spawn new grains based on density
        if self.spawn_timer == 0 {
            self.spawn_grain(position, size_samples, speed, spray);

            // Add jitter to spawn interval (±20%)
            let jitter = 1.0 + (self.rng.next_f64() - 0.5) * 0.4;
            self.spawn_timer = ((spawn_interval as f64) * jitter) as usize;
        } else {
            self.spawn_timer -= 1;
        }

        // Process all active grains
        let mut output = 0.0;

        for i in 0..MAX_GRAINS {
            if self.grains[i].active {
                let grain = &self.grains[i];

                // Calculate read position
                let read_offset = grain.phase * grain.size as f64 * grain.speed;
                let read_pos = grain.start_pos as f64 + read_offset;

                // Apply Hann window envelope
                let envelope = Self::hann_window(grain.phase);

                // Read from buffer (inline to avoid borrow issues)
                let pos = read_pos % GRANULAR_BUFFER_SIZE as f64;
                let index = pos as usize;
                let frac = pos - index as f64;
                let s0 = self.buffer[index % GRANULAR_BUFFER_SIZE];
                let s1 = self.buffer[(index + 1) % GRANULAR_BUFFER_SIZE];
                let sample = s0 + frac * (s1 - s0);

                output += sample * envelope;

                // Advance phase and check completion
                let new_phase = self.grains[i].phase + 1.0 / self.grains[i].size as f64;
                self.grains[i].phase = new_phase;

                if new_phase >= 1.0 {
                    self.grains[i].active = false;
                }
            }
        }

        // Q028: constant-power normalization by the *expected* steady-state
        // overlap (density × grain length), one-pole smoothed. Grains fade in
        // and out through the Hann window, so the summed output is already
        // continuous; normalizing by the smoothed expected overlap — rather than
        // the discretely-changing sqrt(active_count) that also over-counted
        // near-silent grains — removes the per-sample amplitude zipper.
        let grain_seconds = size_samples as f64 / self.sample_rate;
        let expected_overlap = grains_per_sec * grain_seconds;
        // Never amplify: only attenuate once grains routinely overlap.
        let target_norm = Libm::<f64>::sqrt(expected_overlap).max(1.0);
        let smooth = env_coef(0.05, self.sample_rate); // ~50ms smoothing
        self.norm_smooth = smooth * self.norm_smooth + (1.0 - smooth) * target_norm;
        output /= self.norm_smooth.max(1.0);

        outputs.set(10, output);
    }

    fn reset(&mut self) {
        self.buffer.iter_mut().for_each(|x| *x = 0.0);
        self.write_pos = 0;
        self.grains = [Grain::default(); MAX_GRAINS];
        self.spawn_timer = 0;
        self.rng = crate::rng::Rng::from_seed(42);
        self.norm_smooth = 1.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.reset();
    }

    fn type_id(&self) -> &'static str {
        "granular"
    }
}

/// Wavefolder module.
///
/// This is the canonical home of `Wavefolder` (Q149): it lives here in
/// `modules::nonlinear` alongside [`Distortion`] and the other waveshapers, and is
/// re-exported from [`crate::analog`] for backward compatibility. Like
/// [`Distortion`], it supports opt-in oversampling via
/// [`Wavefolder::set_oversample`].
pub struct Wavefolder {
    pub(crate) threshold: f64,
    /// Opt-in oversampler for the folding stage (Q143). Default `Off` preserves
    /// the base-rate behavior.
    oversampler: Oversampler,
    spec: PortSpec,
}

impl Wavefolder {
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold: threshold.max(0.1),
            oversampler: Oversampler::new(Oversample::Off),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "threshold", SignalKind::CvUnipolar)
                        .with_default(threshold)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }

    /// Select the oversampling factor for the folding stage (Q143).
    ///
    /// Defaults to [`Oversample::Off`]. Wavefolding is one of the most
    /// alias-prone nonlinearities; 2x/4x oversampling substantially reduces the
    /// inharmonic aliasing it produces at high input frequencies.
    pub fn set_oversample(&mut self, mode: Oversample) {
        self.oversampler = Oversampler::new(mode);
    }

    /// Current oversampling factor of the folding stage (1 = off, 2, or 4).
    pub fn oversample_factor(&self) -> usize {
        self.oversampler.factor()
    }
}

impl Default for Wavefolder {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl GraphModule for Wavefolder {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let threshold = inputs.get_or(1, self.threshold).max(0.1);

        // Fold through the opt-in oversampler; `Oversample::Off` is exactly the
        // base-rate fold call (Q143).
        let folded = self
            .oversampler
            .process(input, |x| saturation::fold(x / 5.0, threshold) * 5.0);
        outputs.set(10, folded);
    }

    fn reset(&mut self) {
        self.oversampler.reset();
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "wavefolder"
    }

    // Bridge the `oversample` internal parameter to live-patch introspection.
    crate::impl_introspect!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitcrusher() {
        let mut bc = Bitcrusher::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 2.5);
        inputs.set(1, 0.3); // Low bit depth
        inputs.set(2, 0.5); // Some downsampling
        bc.tick(&inputs, &mut outputs);

        let out = outputs.get(10).unwrap();
        assert!(out.is_finite());
    }
    #[test]
    fn test_bitcrusher_default() {
        let bc = Bitcrusher::default();
        assert_eq!(bc.type_id(), "bitcrusher");
    }
    #[test]
    fn test_ring_modulator() {
        let mut rm = RingModulator::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Both at +5V: should produce positive output
        inputs.set(0, 5.0); // Carrier
        inputs.set(1, 5.0); // Modulator
        rm.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 5.0).abs() < 0.1);

        // Opposite polarity: should produce negative output
        inputs.set(0, 5.0);
        inputs.set(1, -5.0);
        rm.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - (-5.0)).abs() < 0.1);

        // Zero modulator: should produce zero
        inputs.set(0, 5.0);
        inputs.set(1, 0.0);
        rm.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap()).abs() < 0.01);
    }
    #[test]
    fn test_ring_modulator_default_reset_sample_rate() {
        let mut rm = RingModulator::default();
        rm.reset();
        rm.set_sample_rate(48000.0);
        assert_eq!(rm.type_id(), "ring_mod");
    }
    #[test]
    fn test_pitch_shifter_default_reset_sample_rate() {
        let mut ps = PitchShifter::default();
        assert_eq!(ps.sample_rate, 44100.0);

        // Process some samples
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 2.5); // Audio input
        for _ in 0..100 {
            ps.tick(&inputs, &mut outputs);
        }

        // Verify buffer was written to
        assert!(ps.write_pos > 0);

        // Reset
        ps.reset();
        assert_eq!(ps.write_pos, 0);
        assert_eq!(ps.grain_phase, [0.0, 0.5]);

        // Set sample rate
        ps.set_sample_rate(48000.0);
        assert_eq!(ps.sample_rate, 48000.0);

        assert_eq!(ps.type_id(), "pitch_shifter");
        assert_eq!(ps.port_spec().inputs.len(), 4);
        assert_eq!(ps.port_spec().outputs.len(), 1);
    }
    #[test]
    fn test_pitch_shifter_hann_window() {
        // Test window function
        let start = PitchShifter::hann_window(0.0);
        let peak = PitchShifter::hann_window(0.5);
        let end = PitchShifter::hann_window(1.0);

        assert!(start.abs() < 0.01, "Window should start at 0: {}", start);
        assert!(
            (peak - 1.0).abs() < 0.01,
            "Window should peak at 1: {}",
            peak
        );
        assert!(end.abs() < 0.01, "Window should end at 0: {}", end);
    }
    #[test]
    fn test_pitch_shifter_passthrough() {
        let mut ps = PitchShifter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // No shift, full mix - should output delayed version of input
        inputs.set(1, 0.0); // No shift
        inputs.set(3, 1.0); // Full wet

        // Feed a sine wave
        let mut sum_out = 0.0;
        for i in 0..1000 {
            let input = Libm::<f64>::sin(i as f64 * 0.1) * 5.0;
            inputs.set(0, input);
            ps.tick(&inputs, &mut outputs);
            sum_out += outputs.get(10).unwrap().abs();
        }

        // Should have significant output
        assert!(sum_out > 100.0, "Should have output signal: {}", sum_out);
    }
    #[test]
    fn test_pitch_shifter_dry_wet_mix() {
        let mut ps = PitchShifter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Full dry - output should be close to input (after normalization)
        inputs.set(1, 0.0);
        inputs.set(3, 0.0); // Full dry

        let input_val = 2.5; // Some audio signal
        inputs.set(0, input_val);

        ps.tick(&inputs, &mut outputs);
        let dry_out = outputs.get(10).unwrap();

        // Dry output should be the input
        assert!(
            (dry_out - input_val).abs() < 0.1,
            "Dry output should match input: {} vs {}",
            dry_out,
            input_val
        );
    }
    #[test]
    fn test_pitch_shifter_shift_changes_output() {
        let mut ps = PitchShifter::new(44100.0);

        // Feed a signal and collect output with different shift values
        let collect_output = |ps: &mut PitchShifter, shift_cv: f64| -> f64 {
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(1, shift_cv);
            inputs.set(3, 1.0);
            ps.reset();

            let mut sum = 0.0;
            for i in 0..2000 {
                let input = Libm::<f64>::sin(i as f64 * 0.05) * 5.0;
                inputs.set(0, input);
                ps.tick(&inputs, &mut outputs);
                sum += outputs.get(10).unwrap();
            }
            sum
        };

        let sum_no_shift = collect_output(&mut ps, 0.0);
        let sum_up_octave = collect_output(&mut ps, 2.5); // +12 semitones
        let sum_down_octave = collect_output(&mut ps, -2.5); // -12 semitones

        // Different shifts should produce different outputs
        assert!(
            (sum_no_shift - sum_up_octave).abs() > 1.0,
            "Up shift should differ"
        );
        assert!(
            (sum_no_shift - sum_down_octave).abs() > 1.0,
            "Down shift should differ"
        );
    }
    #[test]
    fn test_pitch_shifter_buffer_wraparound() {
        let mut ps = PitchShifter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 2.5);
        inputs.set(1, 0.0);
        inputs.set(3, 1.0);

        // Process more samples than buffer size to test wraparound
        for _ in 0..10000 {
            ps.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            assert!(out.is_finite(), "Output should be finite");
        }

        // Write position should have wrapped
        assert!(ps.write_pos < PitchShifter::BUFFER_SIZE);
    }
    #[test]
    fn test_vocoder_default_reset_sample_rate() {
        let mut vocoder = Vocoder::default();
        assert_eq!(vocoder.sample_rate, 44100.0);

        // Feed some signal
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.5); // carrier
        inputs.set(1, 0.5); // modulator
        vocoder.tick(&inputs, &mut outputs);

        // Reset should clear state
        vocoder.reset();
        assert_eq!(vocoder.envelopes, [0.0; MAX_VOCODER_BANDS]);

        // Sample rate change
        vocoder.set_sample_rate(48000.0);
        assert_eq!(vocoder.sample_rate, 48000.0);

        assert_eq!(vocoder.type_id(), "vocoder");
        assert_eq!(vocoder.port_spec().inputs.len(), 5);
        assert_eq!(vocoder.port_spec().outputs.len(), 1);
    }
    #[test]
    fn test_vocoder_band_frequencies() {
        let vocoder = Vocoder::new(44100.0);

        // Check logarithmic spacing
        assert!(vocoder.band_freqs[0] >= VOCODER_FREQ_MIN - 1.0);
        assert!(vocoder.band_freqs[MAX_VOCODER_BANDS - 1] <= VOCODER_FREQ_MAX + 1.0);

        // Frequencies should be ascending
        for i in 1..MAX_VOCODER_BANDS {
            assert!(
                vocoder.band_freqs[i] > vocoder.band_freqs[i - 1],
                "Band frequencies should be ascending"
            );
        }
    }
    #[test]
    fn test_vocoder_silent_when_no_modulator() {
        let mut vocoder = Vocoder::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Carrier only, no modulator
        inputs.set(0, 0.8);
        inputs.set(1, 0.0);

        // Run for a while
        for _ in 0..1000 {
            vocoder.tick(&inputs, &mut outputs);
        }

        let out = outputs.get(10).unwrap();
        // Without modulator, output should be near zero (envelopes decay)
        assert!(
            out.abs() < 0.1,
            "Output should be near zero without modulator, got {}",
            out
        );
    }
    #[test]
    fn test_vocoder_output_when_both_active() {
        let mut vocoder = Vocoder::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Both carrier and modulator active
        let mut total_output = 0.0;
        for i in 0..2000 {
            let phase = i as f64 * 0.05;
            inputs.set(0, Libm::<f64>::sin(phase)); // carrier (oscillator)
            inputs.set(1, Libm::<f64>::sin(phase * 0.1)); // modulator (lower freq)
            vocoder.tick(&inputs, &mut outputs);
            total_output += outputs.get(10).unwrap().abs();
        }

        assert!(
            total_output > 1.0,
            "Should produce output when both signals active, got {}",
            total_output
        );
    }
    #[test]
    fn test_vocoder_band_count() {
        let mut vocoder_few = Vocoder::new(44100.0);
        let mut vocoder_many = Vocoder::new(44100.0);
        let mut inputs_few = PortValues::new();
        let mut inputs_many = PortValues::new();
        let mut outputs_few = PortValues::new();
        let mut outputs_many = PortValues::new();

        // Set up with different band counts
        inputs_few.set(2, 0.0); // Minimum bands (4)
        inputs_many.set(2, 1.0); // Maximum bands (16)

        // Both get same carrier and modulator
        let mut total_few = 0.0;
        let mut total_many = 0.0;

        for i in 0..1000 {
            let phase = i as f64 * 0.05;
            let carrier = Libm::<f64>::sin(phase);
            let modulator = Libm::<f64>::sin(phase * 0.2);

            inputs_few.set(0, carrier);
            inputs_few.set(1, modulator);
            inputs_many.set(0, carrier);
            inputs_many.set(1, modulator);

            vocoder_few.tick(&inputs_few, &mut outputs_few);
            vocoder_many.tick(&inputs_many, &mut outputs_many);

            total_few += outputs_few.get(10).unwrap().abs();
            total_many += outputs_many.get(10).unwrap().abs();
        }

        // Both should produce output (different character but both work)
        assert!(total_few > 0.5, "Few bands should produce output");
        assert!(total_many > 0.5, "Many bands should produce output");
    }
    #[test]
    fn test_vocoder_envelope_attack_release() {
        let mut vocoder = Vocoder::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test with different attack/release settings
        inputs.set(0, 1.0); // carrier
        inputs.set(1, 1.0); // modulator
        inputs.set(3, 0.0); // Fast attack
        inputs.set(4, 0.0); // Fast release

        // Run a few ticks to build up envelope
        for _ in 0..100 {
            vocoder.tick(&inputs, &mut outputs);
        }
        let fast_envelope = vocoder.envelopes[0];

        vocoder.reset();
        inputs.set(3, 1.0); // Slow attack

        for _ in 0..100 {
            vocoder.tick(&inputs, &mut outputs);
        }
        let slow_envelope = vocoder.envelopes[0];

        // Fast attack should build up faster
        assert!(
            fast_envelope > slow_envelope,
            "Fast attack should build envelope faster"
        );
    }
    #[test]
    fn test_granular_default_reset_sample_rate() {
        let mut granular = Granular::default();
        assert_eq!(granular.sample_rate, 44100.0);

        // Feed some signal
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.5);
        granular.tick(&inputs, &mut outputs);

        // Should have written to buffer
        assert_eq!(granular.write_pos, 1);

        // Reset should clear everything
        granular.reset();
        assert_eq!(granular.write_pos, 0);
        assert!(granular.grains.iter().all(|g| !g.active));

        // Sample rate change
        granular.set_sample_rate(48000.0);
        assert_eq!(granular.sample_rate, 48000.0);

        assert_eq!(granular.type_id(), "granular");
        assert_eq!(granular.port_spec().inputs.len(), 7);
        assert_eq!(granular.port_spec().outputs.len(), 1);
    }
    #[test]
    fn test_granular_hann_window() {
        // Hann window should be 0 at edges and 1 at center
        assert!(Granular::hann_window(0.0).abs() < 0.001);
        assert!((Granular::hann_window(0.5) - 1.0).abs() < 0.001);
        assert!(Granular::hann_window(1.0).abs() < 0.001);
    }
    #[test]
    fn test_granular_records_to_buffer() {
        let mut granular = Granular::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Feed a specific pattern
        for i in 0..100 {
            inputs.set(0, i as f64 * 0.01);
            granular.tick(&inputs, &mut outputs);
        }

        // Check buffer has recorded values
        assert!((granular.buffer[50] - 0.5).abs() < 0.01);
    }
    #[test]
    fn test_granular_freeze_stops_recording() {
        let mut granular = Granular::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Record some audio
        inputs.set(0, 1.0);
        for _ in 0..100 {
            granular.tick(&inputs, &mut outputs);
        }
        let pos_before = granular.write_pos;

        // Freeze
        inputs.set(6, 5.0); // Gate high

        // Should not advance write position
        for _ in 0..100 {
            granular.tick(&inputs, &mut outputs);
        }

        assert_eq!(granular.write_pos, pos_before);
    }
    #[test]
    fn test_granular_produces_output() {
        let mut granular = Granular::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set position to read from start of buffer where we'll write
        inputs.set(1, 0.05); // Read near the start where we're recording

        // Fill buffer with signal
        for i in 0..10000 {
            let phase = i as f64 * 0.01;
            inputs.set(0, Libm::<f64>::sin(phase));
            granular.tick(&inputs, &mut outputs);
        }

        // Continue and check output
        let mut total_output = 0.0;
        for _ in 0..5000 {
            inputs.set(0, 0.0);
            granular.tick(&inputs, &mut outputs);
            total_output += outputs.get(10).unwrap().abs();
        }

        assert!(
            total_output > 1.0,
            "Granular should produce output, got {}",
            total_output
        );
    }
    #[test]
    fn test_granular_density_affects_grain_count() {
        let mut granular_low = Granular::new(44100.0);
        let mut granular_high = Granular::new(44100.0);
        let mut inputs_low = PortValues::new();
        let mut inputs_high = PortValues::new();
        let mut outputs = PortValues::new();

        inputs_low.set(3, 0.0); // Low density
        inputs_high.set(3, 1.0); // High density

        // Fill buffers
        for i in 0..5000 {
            let sample = Libm::<f64>::sin(i as f64 * 0.05);
            inputs_low.set(0, sample);
            inputs_high.set(0, sample);
            granular_low.tick(&inputs_low, &mut outputs);
            granular_high.tick(&inputs_high, &mut outputs);
        }

        // Count active grains
        let active_low = granular_low.grains.iter().filter(|g| g.active).count();
        let active_high = granular_high.grains.iter().filter(|g| g.active).count();

        // High density should tend to have more active grains
        // (Note: due to randomness and grain lifetimes, this isn't guaranteed on every run)
        assert!(
            active_high >= active_low || (active_low == 0 && active_high == 0),
            "Higher density should produce more concurrent grains"
        );
    }
    #[test]
    fn test_granular_buffer_interpolation() {
        let granular = Granular::new(44100.0);

        // Manually set some buffer values
        let mut granular = granular;
        granular.buffer[0] = 0.0;
        granular.buffer[1] = 1.0;

        // Read at fractional position should interpolate
        let val = granular.read_buffer(0.5);
        assert!(
            (val - 0.5).abs() < 0.01,
            "Interpolation should give 0.5, got {}",
            val
        );
    }
    #[test]
    fn test_grain_default() {
        let grain = Grain::default();
        assert!(!grain.active);
        assert_eq!(grain.phase, 0.0);
        assert_eq!(grain.speed, 1.0);
    }

    // ------------------------------------------------------------------
    // Wave B remediation tests
    // ------------------------------------------------------------------

    /// Q025: the tone control is a real frequency-dependent low-pass, not a
    /// static gain. At minimum it attenuates highs far more than lows; at
    /// maximum it is essentially transparent.
    #[test]
    fn test_distortion_tone_is_real_filter() {
        let sr = 44100.0;
        // RMS of the output for a sine of `freq` Hz at the given tone setting,
        // using near-linear settings (drive = 0) so the filter dominates.
        let rms = |freq: f64, tone: f64| -> f64 {
            let mut d = Distortion::new(sr);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(1, 0.0); // drive = 0 (near-linear)
            inputs.set(2, tone); // tone CV
            inputs.set(3, 0.0); // soft clip
            inputs.set(4, 1.0); // full wet
            let n = 8000usize;
            let mut sumsq = 0.0;
            for i in 0..n {
                let x = Libm::<f64>::sin(2.0 * core::f64::consts::PI * freq * i as f64 / sr);
                inputs.set(0, x); // ±1V sine
                d.tick(&inputs, &mut outputs);
                let out = outputs.get(10).unwrap();
                if i >= n / 2 {
                    sumsq += out * out;
                }
            }
            Libm::<f64>::sqrt(sumsq / (n / 2) as f64)
        };

        let input_rms = 1.0 / Libm::<f64>::sqrt(2.0); // ±1V sine

        // Tone at minimum: a 5 kHz sine is attenuated much more than 200 Hz.
        let high_at_min = rms(5000.0, 0.0);
        let low_at_min = rms(200.0, 0.0);
        assert!(
            high_at_min < 0.5 * low_at_min,
            "tone min should attenuate highs more than lows: high={high_at_min} low={low_at_min}"
        );

        // Tone at maximum: the same 5 kHz sine passes ~transparently.
        let high_at_max = rms(5000.0, 1.0);
        assert!(
            high_at_max > 0.8 * input_rms,
            "tone max should be ~transparent: out_rms={high_at_max} in_rms={input_rms}"
        );
        assert!(
            high_at_max > 3.0 * high_at_min,
            "tone max should pass highs that tone min blocks: max={high_at_max} min={high_at_min}"
        );
    }

    /// Q026: every algorithm keeps a ±5V input bounded to ≤5.05V at maximum
    /// drive, and passes small signals through near unity at low drive.
    #[test]
    fn test_distortion_all_algorithms_bounded() {
        // Direct shaper bound over a wide input sweep (well beyond ±5V).
        for drive in [0.0, 0.5, 1.0] {
            let mut x = -12.0;
            while x <= 12.0 {
                for out in [
                    Distortion::soft_clip(x, drive),
                    Distortion::hard_clip(x, drive),
                    Distortion::foldback(x, drive),
                    Distortion::asymmetric(x, drive),
                ] {
                    assert!(
                        out.is_finite() && out.abs() <= 5.05,
                        "shaper out {out} exceeds ±5.05 at x={x} drive={drive}"
                    );
                }
                x += 0.05;
            }
        }

        // Full-module bound: constant ±5V at max drive settles ≤5.05V for each mode.
        for mode_cv in [0.0f64, 0.34, 0.67, 1.0] {
            for &v in &[5.0f64, -5.0] {
                let mut d = Distortion::new(44100.0);
                let mut inputs = PortValues::new();
                let mut outputs = PortValues::new();
                inputs.set(1, 1.0); // max drive
                inputs.set(2, 1.0); // tone transparent
                inputs.set(3, mode_cv);
                inputs.set(4, 1.0); // full wet
                inputs.set(0, v);
                let mut out = 0.0;
                for _ in 0..500 {
                    d.tick(&inputs, &mut outputs);
                    out = outputs.get(10).unwrap();
                }
                assert!(
                    out.abs() <= 5.05,
                    "mode {mode_cv} at {v}V max drive should stay ≤5.05V, got {out}"
                );
            }
        }
    }

    /// Q026: at low drive small signals pass through close to unity (no ±1V
    /// level-drop and no unbounded gain).
    #[test]
    fn test_distortion_unity_at_low_drive() {
        // hard_clip is exactly linear inside ±5V at drive 0.
        assert!((Distortion::hard_clip(0.5, 0.0) - 0.5).abs() < 1e-9);
        // soft_clip: 1V input -> 5*tanh(0.2) ≈ 0.986V (mild, near unity).
        let out = Distortion::soft_clip(1.0, 0.0);
        assert!((out - 1.0).abs() < 0.05, "soft_clip near unity, got {out}");
    }

    /// Q030: the closed-form triangle fold is identical to the original
    /// data-dependent reflection loop across a value sweep including extremes.
    #[test]
    fn test_triangle_fold_matches_reference_loop() {
        fn reference(gained: f64, threshold: f64) -> f64 {
            let mut folded = gained;
            while folded > threshold || folded < -threshold {
                if folded > threshold {
                    folded = 2.0 * threshold - folded;
                } else if folded < -threshold {
                    folded = -2.0 * threshold - folded;
                }
            }
            folded
        }
        let threshold = 1.0;
        let mut x = -1000.0;
        while x <= 1000.0 {
            let a = Distortion::triangle_fold(x, threshold);
            let b = reference(x, threshold);
            assert!((a - b).abs() < 1e-6, "fold mismatch at {x}: {a} vs {b}");
            x += 0.05;
        }
        for &x in &[1000.0, -1000.0, 5.0, -5.0, 3.0, -3.0, 1.0, -1.0, 0.0] {
            let a = Distortion::triangle_fold(x, threshold);
            let b = reference(x, threshold);
            assert!(
                (a - b).abs() < 1e-6,
                "fold mismatch at extreme {x}: {a} vs {b}"
            );
        }
    }

    /// Q027: all vocoder band SVF coefficients are strictly increasing (no two
    /// bands collapse onto the 0.99 stability clamp), at 44.1k and lower rates.
    #[test]
    fn test_vocoder_band_coefficients_strictly_increasing() {
        for &sr in &[44100.0, 22050.0, 32000.0] {
            let v = Vocoder::new(sr);
            let mut prev = -1.0;
            for i in 0..MAX_VOCODER_BANDS {
                let coef = (2.0 * Libm::<f64>::sin(core::f64::consts::PI * v.band_freqs[i] / sr))
                    .min(0.99);
                assert!(
                    coef > prev + 1e-9,
                    "band {i} coef {coef} not strictly greater than {prev} at sr {sr}"
                );
                prev = coef;
            }
        }
    }

    /// Q028: with grains continually spawning and dying, the output envelope has
    /// no per-sample amplitude jumps (the old sqrt(active_count) zipper).
    #[test]
    fn test_granular_no_amplitude_zipper() {
        let mut g = Granular::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 1.0); // constant DC so buffer reads are uniform
        inputs.set(1, 0.5); // position
        inputs.set(2, 0.3); // grain size
        inputs.set(3, 1.0); // max density -> frequent spawn/die
        inputs.set(5, 0.0); // no spray

        // Fill the whole buffer with the DC value.
        for _ in 0..(GRANULAR_BUFFER_SIZE + 20000) {
            g.tick(&inputs, &mut outputs);
        }

        let mut prev = outputs.get(10).unwrap();
        let mut max_delta = 0.0f64;
        for _ in 0..30000 {
            g.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            max_delta = max_delta.max((out - prev).abs());
            prev = out;
        }
        assert!(
            max_delta < 0.05,
            "granular output should have no zipper jumps, max delta {max_delta}"
        );
    }

    /// Q029: a fractional downsample factor of 1.5 yields an average hold period
    /// of ~1.5 samples (the old truncating logic rounded it up to 2).
    #[test]
    fn test_bitcrusher_fractional_downsample_period() {
        let mut bc = Bitcrusher::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        // downsample_factor = 1 + cv*63 = 1.5  ->  cv = 0.5/63
        inputs.set(2, 0.5 / 63.0);
        inputs.set(1, 1.0); // 16 bits -> fine quantization, distinct per update

        let n = 3000usize;
        let mut transitions = 0usize;
        let mut prev = f64::NAN;
        for i in 0..n {
            inputs.set(0, i as f64 * 0.001); // monotonic ramp, 0..3V
            bc.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            if i > 0 && (out - prev).abs() > 1e-9 {
                transitions += 1;
            }
            prev = out;
        }
        let avg_period = n as f64 / transitions as f64;
        assert!(
            (avg_period - 1.5).abs() < 0.1,
            "fractional downsample average period should be ~1.5, got {avg_period}"
        );
    }

    /// Q032: the rounding quantizer is unbiased (a zero-mean sine quantizes with
    /// ~0 DC, unlike the old flooring quantizer), and full-scale maps in range.
    #[test]
    fn test_bitcrusher_no_dc_bias() {
        let mut bc = Bitcrusher::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 2.0 / 15.0); // bits = 3 (coarse)
        inputs.set(2, 0.0); // no downsampling
        let n = 20000usize;
        let mut sum = 0.0;
        for i in 0..n {
            let v = Libm::<f64>::sin(i as f64 * 0.01) * 4.0; // zero-mean, within ±5V
            inputs.set(0, v);
            bc.tick(&inputs, &mut outputs);
            sum += outputs.get(10).unwrap();
        }
        let mean = sum / n as f64;
        assert!(
            mean.abs() < 0.1,
            "quantizer DC bias should be ~0, got {mean}"
        );
    }

    #[test]
    fn test_bitcrusher_full_scale_maps_in_range() {
        let mut bc = Bitcrusher::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 0.3);
        inputs.set(2, 0.0); // no downsampling -> hold updates every sample
        for &(v, expected) in &[(5.0, 5.0), (-5.0, -5.0)] {
            inputs.set(0, v);
            bc.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            assert!(
                out.abs() <= 5.0 + 1e-9 && (out - expected).abs() < 1e-9,
                "full-scale {v}V should map to {expected}V in range, got {out}"
            );
        }
    }

    /// Q031: pitch CV ±5 maps to ±24 semitones (speed 0.25×–4×), and grain read
    /// spans stay within the buffer so extreme pitch stays bounded and sane.
    #[test]
    fn test_granular_pitch_clamped_and_bounded() {
        let mut g = Granular::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 0.1); // position
        inputs.set(2, 1.0); // max grain size
        inputs.set(4, 5.0); // pitch +5V -> +24 st
        inputs.set(5, 0.0); // no spray
        inputs.set(0, 1.0);
        g.tick(&inputs, &mut outputs); // first tick spawns a grain

        let grain = g
            .grains
            .iter()
            .find(|gr| gr.active)
            .expect("a grain should be active after the first tick");
        assert!(
            (grain.speed - 4.0).abs() < 1e-6,
            "pitch +5V should be +24 st (speed 4), got speed {}",
            grain.speed
        );
        assert!(
            grain.size as f64 * grain.speed <= GRANULAR_BUFFER_SIZE as f64,
            "grain read span {} must not exceed buffer {}",
            grain.size as f64 * grain.speed,
            GRANULAR_BUFFER_SIZE
        );

        // Run at both pitch extremes: output stays finite, bounded, non-silent.
        for &pitch in &[5.0f64, -5.0] {
            let mut g = Granular::new(44100.0);
            inputs.set(4, pitch);
            let mut total = 0.0;
            let mut max_abs = 0.0f64;
            for i in 0..20000 {
                inputs.set(0, Libm::<f64>::sin(i as f64 * 0.05) * 5.0);
                g.tick(&inputs, &mut outputs);
                let out = outputs.get(10).unwrap();
                assert!(out.is_finite(), "granular output must be finite");
                max_abs = max_abs.max(out.abs());
                total += out.abs();
            }
            assert!(max_abs < 50.0, "output should stay bounded, got {max_abs}");
            assert!(total > 1.0, "output should be non-silent, got {total}");
        }
    }

    /// Q033: maximum pitch-up (+24 st, rate 4) stays finite, bounded near ±5V,
    /// and non-silent — the grain read pointer never overtakes the write pointer.
    #[test]
    fn test_pitch_shifter_max_pitch_up_bounded() {
        let mut ps = PitchShifter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 5.0); // +24 semitones (rate 4)
        inputs.set(3, 1.0); // full wet

        let mut total = 0.0;
        let mut max_abs = 0.0f64;
        for i in 0..10000 {
            inputs.set(0, Libm::<f64>::sin(i as f64 * 0.1) * 5.0);
            ps.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            assert!(out.is_finite(), "pitch-up output must be finite");
            max_abs = max_abs.max(out.abs());
            total += out.abs();
        }
        assert!(
            max_abs <= 5.5,
            "wet output should stay near ±5V (COLA), got {max_abs}"
        );
        assert!(
            total > 10.0,
            "pitch-up output should be non-silent, got {total}"
        );
    }

    // ================================================================
    // Q143: oversampling / anti-aliasing for nonlinear stages
    // ================================================================

    /// Naive DFT magnitude at integer bin `k` over `sig`.
    fn dft_mag(sig: &[f64], k: usize) -> f64 {
        let n = sig.len();
        let mut re = 0.0;
        let mut im = 0.0;
        for (i, &s) in sig.iter().enumerate() {
            let ang = -core::f64::consts::TAU * (k as f64) * (i as f64) / (n as f64);
            re += s * Libm::<f64>::cos(ang);
            im += s * Libm::<f64>::sin(ang);
        }
        Libm::<f64>::sqrt(re * re + im * im) / (n as f64)
    }

    /// Sum of DFT magnitude over the non-harmonic bins (aliased energy). `fund`
    /// is the fundamental bin; harmonics are its integer multiples.
    fn alias_energy(sig: &[f64], fund: usize) -> f64 {
        let n = sig.len();
        let mut total = 0.0;
        for k in 1..(n / 2) {
            if k % fund != 0 {
                total += dft_mag(sig, k);
            }
        }
        total
    }

    /// Drive a hard-clipping [`Distortion`] with a high-frequency sine and return
    /// the captured output (steady state, after warm-up).
    fn distortion_hardclip_capture(mode: Oversample, n: usize) -> Vec<f64> {
        let sr = 44100.0;
        let mut d = Distortion::new(sr);
        d.set_oversample(mode);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 1.0); // full drive
        inputs.set(2, 1.0); // tone fully open (minimize the post low-pass masking)
        inputs.set(3, 0.4); // mode 1 = hard clip (0.4 * 3.99 = 1.59 -> 1)
        inputs.set(4, 1.0); // fully wet

        // 4200 Hz lands exactly on DFT bin 42 for N=441 at 44.1k.
        let freq = 4200.0;
        let mut out = Vec::with_capacity(n);
        // Warm-up to fill the oversampler / tone-filter state.
        for i in 0..(n * 3) {
            let t = i as f64 / sr;
            let x = Libm::<f64>::sin(core::f64::consts::TAU * freq * t) * 5.0;
            inputs.set(0, x);
            d.tick(&inputs, &mut outputs);
            if i >= n * 2 {
                out.push(outputs.get(10).unwrap());
            }
        }
        out
    }

    #[test]
    fn test_distortion_oversampling_reduces_aliasing() {
        let n = 441;
        let fund = 42;
        let off = distortion_hardclip_capture(Oversample::Off, n);
        let x4 = distortion_hardclip_capture(Oversample::X4, n);

        let a_off = alias_energy(&off, fund);
        let a_x4 = alias_energy(&x4, fund);

        assert!(
            a_x4 < 0.7 * a_off,
            "4x oversampling should materially reduce alias energy: off={a_off} x4={a_x4}"
        );
    }

    #[test]
    fn test_distortion_oversample_off_is_default_and_transparent() {
        // Two Distortions, one explicitly Off, must produce identical output.
        let sr = 44100.0;
        let mut a = Distortion::new(sr);
        let mut b = Distortion::new(sr);
        b.set_oversample(Oversample::Off);
        let mut ia = PortValues::new();
        let mut oa = PortValues::new();
        let mut ib = PortValues::new();
        let mut ob = PortValues::new();
        for i in 0..500 {
            let x = Libm::<f64>::sin(i as f64 * 0.3) * 5.0;
            ia.set(0, x);
            ib.set(0, x);
            a.tick(&ia, &mut oa);
            b.tick(&ib, &mut ob);
            assert!((oa.get(10).unwrap() - ob.get(10).unwrap()).abs() < 1e-12);
        }
    }

    #[test]
    fn test_wavefolder_oversampling_reduces_aliasing() {
        let sr = 44100.0;
        let n = 441;
        let fund = 42;
        let freq = 4200.0;

        let capture = |mode: Oversample| -> Vec<f64> {
            let mut wf = Wavefolder::new(0.3);
            wf.set_oversample(mode);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            let mut out = Vec::with_capacity(n);
            for i in 0..(n * 3) {
                let t = i as f64 / sr;
                inputs.set(0, Libm::<f64>::sin(core::f64::consts::TAU * freq * t) * 5.0);
                wf.tick(&inputs, &mut outputs);
                if i >= n * 2 {
                    out.push(outputs.get(10).unwrap());
                }
            }
            out
        };

        let a_off = alias_energy(&capture(Oversample::Off), fund);
        let a_x4 = alias_energy(&capture(Oversample::X4), fund);
        assert!(
            a_x4 < 0.7 * a_off,
            "wavefolder 4x oversampling should reduce alias energy: off={a_off} x4={a_x4}"
        );
    }
}
