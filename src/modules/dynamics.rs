//! Envelope, amplifier, and dynamics modules.

use super::common::{
    db_to_gain, env_coef, flush_denorm, gain_to_db, sanitize_audio, Memo, GATE_HIGH_V,
    GATE_THRESHOLD_V,
};
use crate::port::{
    GraphModule, ModulatedParam, ParamRange, PortDef, PortSpec, PortValues, SignalKind,
};
use alloc::vec;
use libm::Libm;

/// ADSR stage enumeration
#[derive(Debug, Clone, Copy, PartialEq)]
enum AdsrStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// ADSR Envelope Generator
///
/// A classic Attack-Decay-Sustain-Release envelope with gate and retrigger inputs.
/// Outputs normal and inverted envelope signals, plus end-of-cycle trigger.
///
/// # Segment timing
///
/// The `decay` and `release` parameters denote the true duration of their
/// respective segments (peak→sustain and current-level→zero), not the time to
/// traverse the full 0..1 span. Per-sample rates are therefore scaled by the
/// span actually traversed: `decay_rate = (1 - sustain) / (decay_time · fs)` and
/// `release_rate = release_start_level / (release_time · fs)`, where
/// `release_start_level` is captured at the instant the gate falls.
///
/// # Curve shape
///
/// The `shape` input selects the segment curve: `0V` (default) gives classic
/// linear ramps; a high level (`> GATE_THRESHOLD_V`, e.g. `5V`) selects an
/// exponential one-pole approach toward each stage's target (attack→1, decay→
/// sustain, release→0) using `env_coef` with the stage time as the time
/// constant.
///
/// # Retrigger semantics
///
/// A retrigger (or a fresh gate) restarts the contour at the **Attack** stage
/// but **continues from the current level** — it does not reset the level to
/// zero. Retriggering during Sustain therefore ramps back up from the sustain
/// level rather than restarting from silence.
pub struct Adsr {
    stage: AdsrStage,
    level: f64,
    sample_rate: f64,
    prev_gate: f64,
    prev_retrig: f64,
    /// Level captured when the gate falls, used to scale the release rate so the
    /// release duration equals the labeled release time regardless of the level
    /// the envelope was at when the gate was released.
    release_start_level: f64,
    /// Memoized segment times and one-pole coefficients: three `pow` and three
    /// `exp` per sample collapse to a key compare while the time CVs are static
    /// (the common case — most patches wire them to constants).
    /// `[attack_time, decay_time, release_time, attack_coef, decay_coef,
    /// release_coef]`.
    time_memo: Memo<4, [f64; 6]>,
    spec: PortSpec,
}

impl Adsr {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            stage: AdsrStage::Idle,
            level: 0.0,
            sample_rate,
            prev_gate: 0.0,
            prev_retrig: 0.0,
            release_start_level: 0.0,
            time_memo: Memo::new([0.0; 6]),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "gate", SignalKind::Gate),
                    PortDef::new(1, "retrig", SignalKind::Trigger),
                    PortDef::new(2, "attack", SignalKind::CvUnipolar)
                        .with_default(0.1)
                        .with_attenuverter(),
                    PortDef::new(3, "decay", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(4, "sustain", SignalKind::CvUnipolar)
                        .with_default(0.7)
                        .with_attenuverter(),
                    PortDef::new(5, "release", SignalKind::CvUnipolar)
                        .with_default(0.4)
                        .with_attenuverter(),
                    // Curve shape: 0V = linear (default), high = exponential.
                    // Appended as a new port id (6) so existing port numbering
                    // is preserved.
                    PortDef::new(6, "shape", SignalKind::Gate).with_default(0.0),
                ],
                outputs: vec![
                    PortDef::new(10, "env", SignalKind::CvUnipolar),
                    PortDef::new(11, "inv", SignalKind::CvUnipolar),
                    PortDef::new(12, "eoc", SignalKind::Trigger),
                ],
            },
        }
    }

    fn cv_to_time(cv: f64) -> f64 {
        // Map 0-1 CV to 1ms - 10s (exponential)
        0.001 * Libm::<f64>::pow(10000.0, cv.clamp(0.0, 1.0))
    }
}

impl Default for Adsr {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Adsr {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let gate = inputs.get_or(0, 0.0);
        let retrig = inputs.get_or(1, 0.0);
        let attack_cv = inputs.get_or(2, 0.1);
        let decay_cv = inputs.get_or(3, 0.3);
        let sustain_level = inputs.get_or(4, 0.7).clamp(0.0, 1.0);
        let release_cv = inputs.get_or(5, 0.4);
        let exp_mode = inputs.get_or(6, 0.0) > GATE_THRESHOLD_V;

        // Segment times and exponential one-pole coefficients, memoized on the
        // time CVs (bit-exact: the miss path is the original math). Each stage
        // time is the time constant of its one-pole coefficient.
        let sample_rate = self.sample_rate;
        let [attack_time, decay_time, release_time, attack_coef, decay_coef, release_coef] = self
            .time_memo
            .get_or_compute([attack_cv, decay_cv, release_cv, sample_rate], || {
                let attack_time = Self::cv_to_time(attack_cv);
                let decay_time = Self::cv_to_time(decay_cv);
                let release_time = Self::cv_to_time(release_cv);
                [
                    attack_time,
                    decay_time,
                    release_time,
                    env_coef(attack_time, sample_rate),
                    env_coef(decay_time, sample_rate),
                    env_coef(release_time, sample_rate),
                ]
            });

        let gate_high = gate > GATE_THRESHOLD_V;
        let gate_rising = gate_high && self.prev_gate <= GATE_THRESHOLD_V;
        let gate_falling = !gate_high && self.prev_gate > GATE_THRESHOLD_V;
        let retrig_rising = retrig > GATE_THRESHOLD_V && self.prev_retrig <= GATE_THRESHOLD_V;

        // State transitions. A retrigger/gate continues from the current level
        // (see the struct docs); it never resets `level` to zero.
        if gate_rising || (retrig_rising && gate_high) {
            self.stage = AdsrStage::Attack;
        } else if gate_falling && self.stage != AdsrStage::Idle {
            // Capture the level at gate-fall so the release rate can be scaled to
            // make the actual release duration equal the labeled release time.
            self.release_start_level = self.level;
            self.stage = AdsrStage::Release;
        }

        // Linear per-sample rates, scaled by the span each segment traverses so
        // the labeled decay/release times equal the real segment durations.
        let attack_rate = 1.0 / (attack_time * self.sample_rate);
        let decay_rate = (1.0 - sustain_level) / (decay_time * self.sample_rate);
        let release_rate = self.release_start_level / (release_time * self.sample_rate);

        // Distance from a one-pole target at which a segment is considered done.
        const EXP_DONE: f64 = 1e-3;

        // Process current stage
        let mut eoc = 0.0;
        match self.stage {
            AdsrStage::Idle => {
                self.level = 0.0;
            }
            AdsrStage::Attack => {
                if exp_mode {
                    self.level += (1.0 - self.level) * (1.0 - attack_coef);
                    if self.level >= 1.0 - EXP_DONE {
                        self.level = 1.0;
                        self.stage = AdsrStage::Decay;
                    }
                } else {
                    self.level += attack_rate;
                    if self.level >= 1.0 {
                        self.level = 1.0;
                        self.stage = AdsrStage::Decay;
                    }
                }
            }
            AdsrStage::Decay => {
                if exp_mode {
                    self.level += (sustain_level - self.level) * (1.0 - decay_coef);
                    if self.level - sustain_level <= EXP_DONE {
                        self.level = sustain_level;
                        self.stage = AdsrStage::Sustain;
                    }
                } else {
                    self.level -= decay_rate;
                    if self.level <= sustain_level {
                        self.level = sustain_level;
                        self.stage = AdsrStage::Sustain;
                    }
                }
            }
            AdsrStage::Sustain => {
                self.level = sustain_level;
            }
            AdsrStage::Release => {
                if exp_mode {
                    self.level += (0.0 - self.level) * (1.0 - release_coef);
                    if self.level <= EXP_DONE {
                        self.level = 0.0;
                        self.stage = AdsrStage::Idle;
                        eoc = GATE_HIGH_V; // End-of-cycle trigger
                    }
                } else {
                    self.level -= release_rate;
                    if self.level <= 0.0 {
                        self.level = 0.0;
                        self.stage = AdsrStage::Idle;
                        eoc = GATE_HIGH_V; // End-of-cycle trigger
                    }
                }
            }
        }

        self.prev_gate = gate;
        self.prev_retrig = retrig;

        // Output scaled to standard modular levels
        outputs.set(10, self.level * 10.0); // 0-10V unipolar
        outputs.set(11, (1.0 - self.level) * 10.0); // Inverted
        outputs.set(12, eoc);
    }

    fn reset(&mut self) {
        self.stage = AdsrStage::Idle;
        self.level = 0.0;
        self.prev_gate = 0.0;
        self.prev_retrig = 0.0;
        self.release_start_level = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "adsr"
    }
}

/// Voltage-Controlled Amplifier (VCA)
///
/// An amplifier with CV control, useful for amplitude modulation.
///
/// # Control voltage
///
/// `cv` in `[0, 10]V` maps to a base control amount in `[0, 1]` (values outside
/// the range are clamped). With the default `cv` of `10V` the base amount is
/// unity.
///
/// # Response curve
///
/// The `response` input selects how the control amount maps to gain: `0V`
/// (default) is linear (`gain = cv/10`); a high level (`> GATE_THRESHOLD_V`,
/// e.g. `5V`) selects an exponential (square-law) taper `gain = (cv/10)²`. The
/// exponential curve is monotonic with matched endpoints (`0→0`, `1→1`) and a
/// documented midpoint of `0.25` at `cv = 5V`.
///
/// # Boost
///
/// The `gain` input is a post-response scale in `[0, 2]` (default `1.0`),
/// allowing up to `+6 dB` of boost/overdrive headroom. With all inputs at their
/// defaults the VCA is bit-for-bit identical to a plain `out = in · cv/10`.
pub struct Vca {
    spec: PortSpec,
}

impl Vca {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "cv", SignalKind::CvUnipolar)
                        .with_default(10.0)
                        .with_attenuverter(),
                    // Response curve: 0V = linear (default), high = exponential.
                    PortDef::new(2, "response", SignalKind::Gate).with_default(0.0),
                    // Post-response gain scale in [0, 2] for boost headroom.
                    PortDef::new(3, "gain", SignalKind::CvUnipolar).with_default(1.0),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }
}

impl Default for Vca {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for Vca {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let cv = inputs.get_or(1, 10.0).clamp(0.0, 10.0) / 10.0;
        let exp_response = inputs.get_or(2, 0.0) > GATE_THRESHOLD_V;
        let gain_scale = inputs.get_or(3, 1.0).clamp(0.0, 2.0);

        // Exponential (square-law) taper: monotonic, endpoints 0->0 and 1->1,
        // midpoint 0.25 at cv=5V. Linear is the default and preserves the
        // original `out = in * cv/10` behavior bit-for-bit.
        let base_gain = if exp_response { cv * cv } else { cv };

        outputs.set(10, input * base_gain * gain_scale);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "vca"
    }
}

/// Limiter
///
/// A dynamics processor that prevents signals from exceeding a threshold.
/// Supports both hard and soft limiting modes.
pub struct Limiter {
    sample_rate: f64,
    envelope: f64,
    /// Memoized release coefficient (one `exp` per sample while static).
    release_memo: Memo<2, f64>,
    spec: PortSpec,
}

impl Limiter {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            envelope: 0.0,
            release_memo: Memo::new(0.0),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "threshold", SignalKind::CvUnipolar)
                        .with_default(0.8)
                        .with_attenuverter(),
                    PortDef::new(2, "release", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(3, "soft", SignalKind::Gate).with_default(5.0),
                    // Q148: external sidechain/key. Following the Compressor
                    // convention, an unpatched sidechain reads back the main input
                    // (`get_or(4, input)`), so behavior is unchanged unless keyed.
                    PortDef::new(4, "sidechain", SignalKind::Audio),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "gr", SignalKind::CvUnipolar),
                ],
            },
        }
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Limiter {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize audio + sidechain so a non-finite sample cannot latch
        // the envelope detector (a one-pole feedback state) to NaN permanently.
        let input = sanitize_audio(inputs.get_or(0, 0.0));
        let threshold = inputs.get_or(1, 0.8).clamp(0.01, 1.0) * 5.0;
        let release_cv = inputs.get_or(2, 0.3).clamp(0.0, 1.0);
        let soft_mode = inputs.get_or(3, 5.0) > GATE_THRESHOLD_V;
        // Q148: detect on the sidechain; unpatched it mirrors the main input.
        let sidechain = sanitize_audio(inputs.get_or(4, input));

        // Release coefficient memoized on its driving CV (bit-exact miss path).
        let sample_rate = self.sample_rate;
        let release_coef = self
            .release_memo
            .get_or_compute([release_cv, sample_rate], || {
                let release_ms = 10.0 + release_cv * 990.0;
                env_coef(release_ms / 1000.0, sample_rate)
            });

        let abs_input = Libm::<f64>::fabs(sidechain);

        if abs_input > self.envelope {
            self.envelope = abs_input;
        } else {
            self.envelope = release_coef * self.envelope + (1.0 - release_coef) * abs_input;
        }
        // Q017: flush the detector one-pole so it settles to exactly 0 at
        // silence instead of leaving a denormal tail.
        self.envelope = flush_denorm(self.envelope);

        let gain = if soft_mode {
            // C0/C1-continuous soft knee. Unity gain until the envelope reaches
            // `knee_start` (half the threshold), then a scaled `tanh` that
            // leaves `knee_start` with unit slope and asymptotically approaches
            // the `threshold` ceiling from below. The value *and* slope match at
            // `knee_start`, so there is no output step as the envelope crosses
            // the threshold — the old static curve was unity below threshold but
            // jumped to `threshold * tanh(1) ≈ 0.762 * threshold` just above it,
            // a ~24% instant drop. The knee still never reaches the threshold
            // (`tanh < 1`), so the brick-wall guarantee holds.
            let knee_start = 0.5 * threshold;
            if self.envelope > knee_start {
                let span = threshold - knee_start; // = 0.5 * threshold, > 0
                let target =
                    knee_start + span * Libm::<f64>::tanh((self.envelope - knee_start) / span);
                target / self.envelope
            } else {
                1.0
            }
        } else if self.envelope > threshold {
            threshold / self.envelope
        } else {
            1.0
        };

        // Final hard clamp at +/-threshold so the "brick-wall" guarantee is
        // literally enforced regardless of the knee shape.
        let out = (input * gain).clamp(-threshold, threshold);
        outputs.set(10, out);
        outputs.set(11, (1.0 - gain) * 10.0);
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "limiter"
    }
}

/// Noise Gate
///
/// A dynamics processor that attenuates signals below a threshold.
///
/// # Gate ballistics
///
/// The open/close decision uses hysteresis (a close threshold at `0.7×` the
/// open threshold) plus a **hold time** (`NoiseGate::HOLD_MS`, default 10 ms):
/// the gate stays open for the hold time after the last supra-threshold sample,
/// so a signal dithering around the threshold does not chatter. The gate's
/// anti-click fade uses an **independent** fade time (`NoiseGate::FADE_MS`,
/// default 5 ms) rather than the level-detector's attack/release coefficients,
/// so the fade rate does not change with the detector ballistics. The fade
/// state is flushed to zero (Q017) so it settles to exactly 0 rather than
/// lingering in the denormal range.
pub struct NoiseGate {
    sample_rate: f64,
    envelope: f64,
    gate_state: f64,
    /// Latched open/closed decision (drives hysteresis in the threshold band).
    gate_open: bool,
    /// Samples remaining in the hold window after the last supra-threshold
    /// sample; while non-zero the gate is kept open.
    hold_counter: u32,
    /// Memoized `[attack_coef, release_coef]` (two `exp` per sample while the
    /// ballistics CVs are static).
    coef_memo: Memo<3, [f64; 2]>,
    /// Anti-click fade coefficient. `FADE_MS` is a constant, so this depends
    /// only on the sample rate; it is derived in `new`/`set_sample_rate` instead
    /// of recomputing the `exp` every sample.
    fade_coef: f64,
    spec: PortSpec,
}

impl NoiseGate {
    /// Anti-click gate fade time (ms), independent of the detector ballistics.
    const FADE_MS: f64 = 5.0;
    /// Hold time (ms): the gate stays open this long after the last
    /// supra-threshold sample to prevent chatter near the threshold.
    const HOLD_MS: f64 = 10.0;

    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            envelope: 0.0,
            gate_state: 0.0,
            gate_open: false,
            hold_counter: 0,
            coef_memo: Memo::new([0.0; 2]),
            fade_coef: env_coef(Self::FADE_MS / 1000.0, sample_rate),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "threshold", SignalKind::CvUnipolar)
                        .with_default(0.1)
                        .with_attenuverter(),
                    PortDef::new(2, "attack", SignalKind::CvUnipolar)
                        .with_default(0.1)
                        .with_attenuverter(),
                    PortDef::new(3, "release", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(4, "range", SignalKind::CvUnipolar)
                        .with_default(1.0)
                        .with_attenuverter(),
                    // Q148: external sidechain/key. Unpatched it mirrors the main
                    // input (`get_or(5, input)`), matching the Compressor
                    // convention, so behavior is unchanged unless keyed.
                    PortDef::new(5, "sidechain", SignalKind::Audio),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "gate", SignalKind::Gate),
                ],
            },
        }
    }
}

impl Default for NoiseGate {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for NoiseGate {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize audio + sidechain to keep a non-finite sample out of
        // the envelope detector's feedback state.
        let input = sanitize_audio(inputs.get_or(0, 0.0));
        let threshold = inputs.get_or(1, 0.1).clamp(0.0, 1.0) * 5.0;
        let attack_cv = inputs.get_or(2, 0.1).clamp(0.0, 1.0);
        let release_cv = inputs.get_or(3, 0.3).clamp(0.0, 1.0);
        let range = inputs.get_or(4, 1.0).clamp(0.0, 1.0);
        // Q148: detect on the sidechain; unpatched it mirrors the main input.
        let sidechain = sanitize_audio(inputs.get_or(5, input));

        // Ballistics coefficients memoized on their CVs (bit-exact miss path).
        let sample_rate = self.sample_rate;
        let [attack_coef, release_coef] =
            self.coef_memo
                .get_or_compute([attack_cv, release_cv, sample_rate], || {
                    let attack_ms = 0.1 + attack_cv * 49.9;
                    let release_ms = 10.0 + release_cv * 490.0;
                    [
                        env_coef(attack_ms / 1000.0, sample_rate),
                        env_coef(release_ms / 1000.0, sample_rate),
                    ]
                });

        let abs_input = Libm::<f64>::fabs(sidechain);
        if abs_input > self.envelope {
            self.envelope = attack_coef * self.envelope + (1.0 - attack_coef) * abs_input;
        } else {
            self.envelope = release_coef * self.envelope + (1.0 - release_coef) * abs_input;
        }
        // Q017: flush the detector so it reaches exactly 0 at silence.
        self.envelope = flush_denorm(self.envelope);

        let open_threshold = threshold;
        let close_threshold = threshold * 0.7;

        // Hysteresis + hold: opening (re)arms the hold window; the gate only
        // closes once the hold has expired AND the envelope has fallen back
        // below the (lower) close threshold. In the band between the two
        // thresholds the previous decision latches.
        let hold_samples = (Self::HOLD_MS * self.sample_rate / 1000.0) as u32;
        if self.envelope > open_threshold {
            self.gate_open = true;
            self.hold_counter = hold_samples;
        } else if self.hold_counter > 0 {
            self.hold_counter -= 1;
        } else if self.envelope < close_threshold {
            self.gate_open = false;
        }

        // Independent anti-click fade toward the target, unrelated to the
        // detector's attack/release coefficients (Q016).
        let fade_coef = self.fade_coef;
        let target = if self.gate_open { 1.0 } else { 0.0 };
        self.gate_state = fade_coef * self.gate_state + (1.0 - fade_coef) * target;
        // Q016/Q017: flush the fade state so a closed gate reaches exactly 0.
        self.gate_state = flush_denorm(self.gate_state);

        let gain = (1.0 - range) + range * self.gate_state;
        outputs.set(10, input * gain);
        outputs.set(
            11,
            if self.gate_state > 0.5 {
                GATE_HIGH_V
            } else {
                0.0
            },
        );
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
        self.gate_state = 0.0;
        self.gate_open = false;
        self.hold_counter = 0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        // The fade time constant is fixed; its coefficient tracks the rate.
        self.fade_coef = env_coef(Self::FADE_MS / 1000.0, sample_rate);
    }

    fn type_id(&self) -> &'static str {
        "noise_gate"
    }
}

/// Compressor
///
/// A dynamics processor that reduces the dynamic range of audio signals.
pub struct Compressor {
    sample_rate: f64,
    envelope: f64,
    /// Memoized `[attack_coef, release_coef]` (two `exp` per sample while the
    /// ballistics CVs are static).
    coef_memo: Memo<3, [f64; 2]>,
    spec: PortSpec,
}

impl Compressor {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            envelope: 0.0,
            coef_memo: Memo::new([0.0; 2]),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "threshold", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "ratio", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(3, "attack", SignalKind::CvUnipolar)
                        .with_default(0.2)
                        .with_attenuverter(),
                    PortDef::new(4, "release", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(5, "makeup", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(6, "sidechain", SignalKind::Audio),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "gr", SignalKind::CvUnipolar),
                ],
            },
        }
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Compressor {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize audio + sidechain to keep a non-finite sample out of
        // the envelope detector's feedback state.
        let input = sanitize_audio(inputs.get_or(0, 0.0));
        let threshold_cv = inputs.get_or(1, 0.5).clamp(0.0, 1.0);
        let ratio_cv = inputs.get_or(2, 0.5).clamp(0.0, 1.0);
        let attack_cv = inputs.get_or(3, 0.2).clamp(0.0, 1.0);
        let release_cv = inputs.get_or(4, 0.3).clamp(0.0, 1.0);
        let makeup_cv = inputs.get_or(5, 0.0).clamp(0.0, 1.0);
        let sidechain = sanitize_audio(inputs.get_or(6, input));

        let threshold = threshold_cv * 5.0;
        let ratio = 1.0 + ratio_cv * 19.0;
        let makeup_gain = 1.0 + makeup_cv * 3.0;

        // Ballistics coefficients memoized on their CVs (bit-exact miss path).
        let sample_rate = self.sample_rate;
        let [attack_coef, release_coef] =
            self.coef_memo
                .get_or_compute([attack_cv, release_cv, sample_rate], || {
                    let attack_ms = 0.1 + attack_cv * 99.9;
                    let release_ms = 10.0 + release_cv * 990.0;
                    [
                        env_coef(attack_ms / 1000.0, sample_rate),
                        env_coef(release_ms / 1000.0, sample_rate),
                    ]
                });

        let abs_sidechain = Libm::<f64>::fabs(sidechain);
        if abs_sidechain > self.envelope {
            self.envelope = attack_coef * self.envelope + (1.0 - attack_coef) * abs_sidechain;
        } else {
            self.envelope = release_coef * self.envelope + (1.0 - release_coef) * abs_sidechain;
        }
        // Q017: flush the detector so it settles to exactly 0 at silence.
        self.envelope = flush_denorm(self.envelope);

        let gain = if self.envelope > threshold && threshold > 0.0 {
            let over_db = gain_to_db(self.envelope / threshold);
            let compressed_db = over_db / ratio;
            let gain_reduction_db = over_db - compressed_db;
            db_to_gain(-gain_reduction_db)
        } else {
            1.0
        };

        outputs.set(10, input * gain * makeup_gain);
        outputs.set(11, (1.0 - gain) * 10.0);
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "compressor"
    }
}

/// Ducker (Q148)
///
/// A dedicated sidechain ducking processor: a `key` (sidechain) input drives gain
/// reduction on the main signal. When the key envelope is at or above the
/// threshold, the main signal is attenuated by up to `amount`; the reduction
/// tracks the key level with independent attack/release ballistics and recovers
/// when the key falls silent.
///
/// # Parameter reads via [`ModulatedParam`] (Q147)
///
/// `amount` and `threshold` are read through [`ModulatedParam`]: the panel knob is
/// the `base`, and the corresponding bipolar CV input is summed in through the
/// attenuverter on the `ModulatedParam` ±5 V scale. This makes `ModulatedParam` a
/// live knob+CV read path rather than an unused export.
pub struct Ducker {
    sample_rate: f64,
    /// Smoothed |key| envelope.
    envelope: f64,
    /// Duck depth (0..1), knob + CV.
    amount: ModulatedParam,
    /// Key level (volts) at which full ducking is reached, knob + CV.
    threshold: ModulatedParam,
    /// Memoized `[attack_coef, release_coef]` (two `exp` per sample while the
    /// ballistics CVs are static).
    coef_memo: Memo<3, [f64; 2]>,
    spec: PortSpec,
}

impl Ducker {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate: if sample_rate > 0.0 {
                sample_rate
            } else {
                44100.0
            },
            envelope: 0.0,
            // Full ducking depth by default (base knob = 1.0 -> up to unity reduction).
            amount: ModulatedParam::new(ParamRange::Linear { min: 0.0, max: 1.0 }).with_base(1.0),
            // Threshold spans 0..5 V; default knob ~0.2 -> ~1 V key level for full duck.
            threshold: ModulatedParam::new(ParamRange::Linear { min: 0.0, max: 5.0 })
                .with_base(0.2),
            coef_memo: Memo::new([0.0; 2]),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "key", SignalKind::Audio),
                    PortDef::new(2, "amount", SignalKind::CvBipolar).with_attenuverter(),
                    PortDef::new(3, "threshold", SignalKind::CvBipolar).with_attenuverter(),
                    PortDef::new(4, "attack", SignalKind::CvUnipolar)
                        .with_default(0.1)
                        .with_attenuverter(),
                    PortDef::new(5, "release", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "gr", SignalKind::CvUnipolar),
                ],
            },
        }
    }

    /// Set the duck-depth knob (0..1), the `base` of the amount [`ModulatedParam`].
    pub fn set_amount(&mut self, amount: f64) {
        self.amount.base = amount.clamp(0.0, 1.0);
    }

    /// Current duck-depth knob (0..1).
    pub fn amount(&self) -> f64 {
        self.amount.base
    }

    /// Set the threshold knob (0..1), the `base` of the threshold
    /// [`ModulatedParam`] (mapped to a `0..5 V` key level).
    pub fn set_threshold(&mut self, threshold: f64) {
        self.threshold.base = threshold.clamp(0.0, 1.0);
    }

    /// Current threshold knob (0..1).
    pub fn threshold(&self) -> f64 {
        self.threshold.base
    }
}

impl Default for Ducker {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for Ducker {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize audio + key so a non-finite sample cannot latch the
        // key-envelope detector (a one-pole feedback state) to NaN permanently.
        let input = sanitize_audio(inputs.get_or(0, 0.0));
        let key = sanitize_audio(inputs.get_or(1, 0.0));
        let amount_cv = inputs.get_or(2, 0.0);
        let threshold_cv = inputs.get_or(3, 0.0);
        let attack_cv = inputs.get_or(4, 0.1).clamp(0.0, 1.0);
        let release_cv = inputs.get_or(5, 0.3).clamp(0.0, 1.0);

        // Resolve knob+CV parameters via ModulatedParam.
        self.amount.set_cv(amount_cv);
        self.threshold.set_cv(threshold_cv);
        let amount = self.amount.value().clamp(0.0, 1.0);
        let threshold = self.threshold.value().max(0.0);

        // Ballistics coefficients memoized on their CVs (bit-exact miss path).
        let sample_rate = self.sample_rate;
        let [attack_coef, release_coef] =
            self.coef_memo
                .get_or_compute([attack_cv, release_cv, sample_rate], || {
                    let attack_ms = 0.1 + attack_cv * 99.9;
                    let release_ms = 10.0 + release_cv * 990.0;
                    [
                        env_coef(attack_ms / 1000.0, sample_rate),
                        env_coef(release_ms / 1000.0, sample_rate),
                    ]
                });

        // Follow the key envelope with attack/release ballistics.
        let abs_key = Libm::<f64>::fabs(key);
        if abs_key > self.envelope {
            self.envelope = attack_coef * self.envelope + (1.0 - attack_coef) * abs_key;
        } else {
            self.envelope = release_coef * self.envelope + (1.0 - release_coef) * abs_key;
        }
        self.envelope = flush_denorm(self.envelope);

        // Gain reduction grows from 0 (key silent) to `amount` (key at/above
        // threshold), proportional in between.
        let ratio = if threshold > 1e-9 {
            (self.envelope / threshold).clamp(0.0, 1.0)
        } else {
            // A zero threshold means "duck on any key activity".
            if self.envelope > 1e-9 {
                1.0
            } else {
                0.0
            }
        };
        let gr = amount * ratio;
        let gain = 1.0 - gr;

        outputs.set(10, input * gain);
        outputs.set(11, gr * 10.0);
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    fn type_id(&self) -> &'static str {
        "ducker"
    }

    // Surface the depth/threshold knobs (ModuleIntrospection) through the boxed trait object
    // so a live `Patch` can discover, set, and serialize them. Without this the knobs are
    // dead code: `introspect()` defaults to `None` and only the CV input ports are visible.
    crate::impl_introspect!();
}

/// Envelope Follower
///
/// Extracts the amplitude envelope from an audio signal.
pub struct EnvelopeFollower {
    sample_rate: f64,
    envelope: f64,
    /// Memoized `[attack_coef, release_coef]` (two `exp` per sample while the
    /// ballistics CVs are static).
    coef_memo: Memo<3, [f64; 2]>,
    spec: PortSpec,
}

impl EnvelopeFollower {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            envelope: 0.0,
            coef_memo: Memo::new([0.0; 2]),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "attack", SignalKind::CvUnipolar)
                        .with_default(0.2)
                        .with_attenuverter(),
                    PortDef::new(2, "release", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(3, "gain", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::CvUnipolar),
                    PortDef::new(11, "inv", SignalKind::CvUnipolar),
                ],
            },
        }
    }
}

impl Default for EnvelopeFollower {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl GraphModule for EnvelopeFollower {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        // Q160: sanitize the audio input so a non-finite sample cannot latch the
        // envelope detector (a one-pole feedback state) to NaN permanently.
        let input = sanitize_audio(inputs.get_or(0, 0.0));
        let attack_cv = inputs.get_or(1, 0.2).clamp(0.0, 1.0);
        let release_cv = inputs.get_or(2, 0.3).clamp(0.0, 1.0);
        let gain = inputs.get_or(3, 0.5).clamp(0.0, 1.0) * 4.0;

        // Ballistics coefficients memoized on their CVs (bit-exact miss path).
        let sample_rate = self.sample_rate;
        let [attack_coef, release_coef] =
            self.coef_memo
                .get_or_compute([attack_cv, release_cv, sample_rate], || {
                    let attack_ms = 0.1 + attack_cv * 99.9;
                    let release_ms = 1.0 + release_cv * 999.0;
                    [
                        env_coef(attack_ms / 1000.0, sample_rate),
                        env_coef(release_ms / 1000.0, sample_rate),
                    ]
                });

        let abs_input = Libm::<f64>::fabs(input);
        if abs_input > self.envelope {
            self.envelope = attack_coef * self.envelope + (1.0 - attack_coef) * abs_input;
        } else {
            self.envelope = release_coef * self.envelope + (1.0 - release_coef) * abs_input;
        }
        // Q017: flush the detector so it settles to exactly 0 at silence.
        self.envelope = flush_denorm(self.envelope);

        let out = (self.envelope * gain).clamp(0.0, 10.0);
        outputs.set(10, out);
        outputs.set(11, 10.0 - out);
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn type_id(&self) -> &'static str {
        "envelope_follower"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analog::Saturator;
    use crate::modules::common::{measure_max_output, SAFE_AUDIO_LIMIT};

    #[test]
    fn test_adsr_envelope() {
        let mut adsr = Adsr::new(1000.0); // 1kHz for easy math
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Fast attack
        inputs.set(2, 0.1);

        // Gate on
        inputs.set(0, 5.0);

        // Run attack phase
        for _ in 0..100 {
            adsr.tick(&inputs, &mut outputs);
        }

        // Should have risen from 0
        let level = outputs.get(10).unwrap();
        assert!(level > 0.0);
    }
    #[test]
    fn test_vca() {
        let mut vca = Vca::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 5.0); // Input
        inputs.set(1, 5.0); // Half CV

        vca.tick(&inputs, &mut outputs);

        let out = outputs.get(10).unwrap();
        assert!((out - 2.5).abs() < 0.01);
    }
    #[test]
    fn test_limiter() {
        let mut limiter = Limiter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test with signal above threshold
        inputs.set(0, 10.0); // Way above threshold
        inputs.set(1, 0.5); // Threshold
        for _ in 0..100 {
            limiter.tick(&inputs, &mut outputs);
        }

        // Output should be limited
        let out = outputs.get(10).unwrap();
        assert!(out.abs() < 10.0);
        assert!(out.is_finite());
    }
    #[test]
    fn test_limiter_default() {
        let limiter = Limiter::default();
        assert_eq!(limiter.type_id(), "limiter");
    }
    #[test]
    fn test_noise_gate() {
        let mut gate = NoiseGate::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test with signal below threshold
        inputs.set(0, 0.01); // Very quiet
        inputs.set(1, 0.5); // Threshold
        for _ in 0..1000 {
            gate.tick(&inputs, &mut outputs);
        }

        // Gate should be closed, output attenuated
        let out = outputs.get(10).unwrap();
        assert!(out.abs() < 0.1);

        // Gate output should be closed
        let gate_out = outputs.get(11).unwrap();
        assert!(gate_out < 2.5);
    }
    #[test]
    fn test_noise_gate_default() {
        let gate = NoiseGate::default();
        assert_eq!(gate.type_id(), "noise_gate");
    }
    #[test]
    fn test_compressor() {
        let mut comp = Compressor::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Signal above threshold
        inputs.set(0, 5.0);
        inputs.set(1, 0.2); // Low threshold
        inputs.set(2, 0.8); // High ratio
        for _ in 0..100 {
            comp.tick(&inputs, &mut outputs);
        }

        let out = outputs.get(10).unwrap();
        assert!(out.is_finite());

        // Should have some gain reduction
        let gr = outputs.get(11).unwrap();
        assert!(gr >= 0.0);
    }
    #[test]
    fn test_compressor_default() {
        let comp = Compressor::default();
        assert_eq!(comp.type_id(), "compressor");
    }
    #[test]
    fn test_envelope_follower() {
        let mut ef = EnvelopeFollower::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Feed signal
        inputs.set(0, 5.0);
        for _ in 0..1000 {
            ef.tick(&inputs, &mut outputs);
        }

        let out = outputs.get(10).unwrap();
        assert!(out > 0.0);
        assert!(out.is_finite());

        // Inverted output
        let inv = outputs.get(11).unwrap();
        assert!(inv.is_finite());
    }
    #[test]
    fn test_envelope_follower_default() {
        let ef = EnvelopeFollower::default();
        assert_eq!(ef.type_id(), "envelope_follower");
    }
    #[test]
    fn test_adsr_default_reset_sample_rate() {
        let mut adsr = Adsr::default();
        assert!(adsr.sample_rate == 44100.0);

        adsr.set_sample_rate(48000.0);
        assert!(adsr.sample_rate == 48000.0);

        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 5.0); // Gate high
        for _ in 0..100 {
            adsr.tick(&inputs, &mut outputs);
        }

        adsr.reset();
        assert!(adsr.level == 0.0);
        assert!(adsr.stage == AdsrStage::Idle);

        assert_eq!(adsr.type_id(), "adsr");
    }
    #[test]
    fn test_vca_default_reset_sample_rate() {
        let mut vca = Vca::default();
        vca.reset();
        vca.set_sample_rate(48000.0);
        assert_eq!(vca.type_id(), "vca");
    }
    #[test]
    fn test_adsr_full_cycle() {
        let mut adsr = Adsr::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set fast envelope
        inputs.set(1, 10.0); // Fast attack
        inputs.set(2, 10.0); // Fast decay
        inputs.set(3, 5.0); // 50% sustain
        inputs.set(4, 10.0); // Fast release

        // Gate on - attack
        inputs.set(0, 5.0);
        for _ in 0..1000 {
            adsr.tick(&inputs, &mut outputs);
        }

        // Should have output during attack
        let peak = outputs.get(10).unwrap();
        assert!(peak > 0.0);

        // Continue through decay to sustain
        for _ in 0..1000 {
            adsr.tick(&inputs, &mut outputs);
        }

        // Gate off - release
        inputs.set(0, 0.0);
        for _ in 0..1000 {
            adsr.tick(&inputs, &mut outputs);
        }

        // Should be near zero after release
        let after_release = outputs.get(10).unwrap();
        assert!(after_release < 0.1);
    }
    #[test]
    fn test_adsr_output_bounded() {
        let mut adsr = Adsr::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Fast attack, instant release
        inputs.set(2, 0.0); // Attack
        inputs.set(3, 0.0); // Decay
        inputs.set(4, 1.0); // Sustain
        inputs.set(5, 0.0); // Release

        // Gate on
        inputs.set(0, 5.0);

        let max = measure_max_output(10000, || {
            adsr.tick(&inputs, &mut outputs);
            outputs.get(10).unwrap_or(0.0).abs()
        });

        assert!(
            max <= 10.5, // ADSR outputs 0-10V
            "ADSR output {} exceeds expected 0-10V range",
            max
        );
    }
    #[test]
    fn test_limiter_prevents_spikes() {
        let mut limiter = Limiter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set threshold to 3V
        inputs.set(1, 0.3); // Threshold CV (0-1 maps to 0-5V)

        // Feed in a 10V spike
        inputs.set(0, 10.0);

        limiter.tick(&inputs, &mut outputs);
        let out = outputs.get(10).unwrap_or(0.0);

        assert!(
            out.abs() <= 5.0,
            "Limiter failed to limit 10V input, got {}",
            out
        );
    }
    #[test]
    fn test_saturator_prevents_spikes() {
        let mut sat = Saturator::new(0.8); // High drive
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Large input
        inputs.set(0, 20.0);

        sat.tick(&inputs, &mut outputs);
        let out = outputs.get(10).unwrap_or(0.0);

        assert!(
            out.abs() <= SAFE_AUDIO_LIMIT,
            "Saturator failed to limit input, got {}",
            out
        );
    }

    // ---- Q014: Limiter is a true brick-wall in soft (default) mode ----

    #[test]
    fn test_limiter_brickwall_never_exceeds_threshold() {
        let fs = 44100.0;
        for &thr_cv in &[0.2_f64, 0.5, 0.8, 1.0] {
            let mut lim = Limiter::new(fs);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(1, thr_cv); // soft mode is on by default (port 3 default 5V)
            let threshold = thr_cv.clamp(0.01, 1.0) * 5.0;
            let mut max_out = 0.0f64;
            for i in 0..4000 {
                // Sweep amplitude up to +/-25V, far past any threshold.
                let x = 25.0 * (i as f64 * 0.05).sin();
                inputs.set(0, x);
                lim.tick(&inputs, &mut outputs);
                max_out = max_out.max(outputs.get(10).unwrap().abs());
            }
            assert!(
                max_out <= threshold + 1e-9,
                "soft limiter exceeded threshold {}: peak {}",
                threshold,
                max_out
            );
        }
    }

    #[test]
    fn test_limiter_passes_gentle_signals() {
        let mut lim = Limiter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 0.8); // threshold 4V
        for i in 0..1000 {
            let x = (i as f64 * 0.05).sin(); // +/-1V, well below threshold
            inputs.set(0, x);
            lim.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            assert!(
                (out - x).abs() < 1e-9,
                "gentle signal altered: in={} out={}",
                x,
                out
            );
        }
    }

    // ---- Soft-knee limiter is C0-continuous across the threshold ----
    //
    // The old soft curve was unity gain below threshold but jumped to
    // ~0.762*threshold just above it, a ~24% instant output drop. Sweep the
    // input amplitude across the knee in small steps and assert the steady-state
    // output never steps by more than the input amplitude step (the transfer
    // curve is 1-Lipschitz), which fails hard on that discontinuity.
    #[test]
    fn test_limiter_soft_knee_c0_continuous() {
        let mut lim = Limiter::new(48000.0);
        let threshold = 0.8 * 5.0; // default threshold knob 0.8 -> 4.0 V
        let step = 0.01_f64;

        let steady_output = |lim: &mut Limiter, a: f64| -> f64 {
            lim.reset();
            let mut inputs = PortValues::new();
            inputs.set(0, a); // in
            inputs.set(1, 0.8); // threshold knob -> 4.0 V
            inputs.set(2, 0.3); // release
            inputs.set(3, 5.0); // soft mode on
            let mut outputs = PortValues::new();
            // The detector jumps up to |input| on the first tick, so a constant
            // input reaches steady state immediately; tick a few times anyway.
            for _ in 0..8 {
                outputs = PortValues::new();
                lim.tick(&inputs, &mut outputs);
            }
            outputs.get(10).unwrap()
        };

        let mut prev: Option<(f64, f64)> = None;
        let mut a = 1.0_f64; // start below the knee (knee_start = 2.0 V)
        while a <= 6.0 {
            let out = steady_output(&mut lim, a);

            // Brick-wall guarantee is preserved.
            assert!(
                out <= threshold + 1e-9,
                "soft limiter output {out} exceeds threshold {threshold} at a={a}"
            );

            if let Some((pa, pout)) = prev {
                let jump = (out - pout).abs();
                assert!(
                    jump <= (a - pa) + 1e-6,
                    "soft-knee discontinuity: output stepped {jump} between \
                     a={pa} and a={a} (amplitude step {})",
                    a - pa
                );
            }
            prev = Some((a, out));
            a += step;
        }
    }

    // ---- Q015: ADSR decay/release times equal actual segment durations ----

    #[test]
    fn test_adsr_decay_release_durations() {
        let fs = 1000.0;
        let mut adsr = Adsr::new(fs);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(2, 0.0); // attack 1ms (rate 1.0 -> completes in 1 sample)
        inputs.set(3, 0.5); // decay cv 0.5 -> 0.1s -> 100 samples
        inputs.set(4, 0.5); // sustain 0.5
        inputs.set(5, 0.5); // release cv 0.5 -> 0.1s -> 100 samples
        inputs.set(0, 5.0); // gate on

        // Advance to the decay stage.
        loop {
            adsr.tick(&inputs, &mut outputs);
            if adsr.stage == AdsrStage::Decay {
                break;
            }
        }
        // Count decay samples until sustain is reached.
        let mut decay_samples = 0u32;
        while adsr.stage == AdsrStage::Decay {
            adsr.tick(&inputs, &mut outputs);
            decay_samples += 1;
        }
        assert!(
            (decay_samples as f64 - 100.0).abs() <= 5.0,
            "decay lasted {} samples, expected ~100",
            decay_samples
        );

        // Drop the gate and count release samples until idle.
        inputs.set(0, 0.0);
        let mut release_samples = 0u32;
        loop {
            adsr.tick(&inputs, &mut outputs);
            match adsr.stage {
                AdsrStage::Release => release_samples += 1,
                AdsrStage::Idle => {
                    release_samples += 1;
                    break;
                }
                _ => break,
            }
        }
        assert!(
            (release_samples as f64 - 100.0).abs() <= 5.0,
            "release lasted {} samples, expected ~100",
            release_samples
        );
    }

    // ---- Q016: NoiseGate hold + independent fade prevent chatter ----

    #[test]
    fn test_noise_gate_no_chatter_near_threshold() {
        let fs = 44100.0;
        let mut gate = NoiseGate::new(fs);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(1, 0.2); // open threshold 1.0V, close 0.7V
        let mut transitions = 0;
        let mut last_hi = false;
        for i in 0..(fs as usize) {
            // Dither straddling the open threshold; troughs stay above close.
            let amp = if i % 2 == 0 { 1.3 } else { 0.9 };
            inputs.set(0, amp);
            gate.tick(&inputs, &mut outputs);
            let hi = outputs.get(11).unwrap() > GATE_THRESHOLD_V;
            if hi != last_hi {
                transitions += 1;
                last_hi = hi;
            }
        }
        assert!(
            transitions <= 2,
            "gate chattered near threshold: {} transitions",
            transitions
        );
    }

    #[test]
    fn test_noise_gate_fade_rate_independent_of_detector() {
        // Count the gate-open fade (0 -> one time constant) from the moment the
        // gate opens, for a fast and a slow detector attack. The fade must take
        // the same time because it uses an independent fade coefficient.
        fn measure_open_fade(attack_cv: f64) -> usize {
            let fs = 44100.0;
            let mut gate = NoiseGate::new(fs);
            let mut inputs = PortValues::new();
            let mut outputs = PortValues::new();
            inputs.set(1, 0.2); // open threshold 1.0V
            inputs.set(2, attack_cv); // detector attack
            inputs.set(0, 5.0); // strong constant signal
            let mut started = false;
            let mut count = 0usize;
            for _ in 0..200_000 {
                gate.tick(&inputs, &mut outputs);
                if started {
                    count += 1;
                    if gate.gate_state >= 0.632 {
                        return count;
                    }
                } else if gate.gate_state > 0.0 {
                    started = true;
                    count = 1;
                    if gate.gate_state >= 0.632 {
                        return count;
                    }
                }
            }
            count
        }
        let fast = measure_open_fade(0.0); // detector attack ~0.1ms
        let slow = measure_open_fade(1.0); // detector attack 50ms
        assert!(
            (fast as i64 - slow as i64).abs() <= 2,
            "fade rate varied with detector attack: fast={} slow={}",
            fast,
            slow
        );
        // The fade time constant tracks FADE_MS (5ms -> ~220 samples at 44.1k).
        let expected = (NoiseGate::FADE_MS * 44100.0 / 1000.0) as i64;
        assert!(
            (fast as i64 - expected).abs() <= 3,
            "fade tc {} samples != expected {}",
            fast,
            expected
        );
    }

    // ---- Q017: detector one-poles flush to exactly 0 at silence ----

    #[test]
    fn test_dynamics_detectors_flush_to_zero() {
        let fs = 44100.0;
        const BUDGET: usize = 500_000;

        // EnvelopeFollower (release on port 2).
        {
            let mut m = EnvelopeFollower::new(fs);
            let mut i = PortValues::new();
            let mut o = PortValues::new();
            i.set(2, 0.0); // fast release
            i.set(0, 5.0);
            for _ in 0..2000 {
                m.tick(&i, &mut o);
            }
            i.set(0, 0.0);
            let mut n = 0;
            while m.envelope != 0.0 && n < BUDGET {
                m.tick(&i, &mut o);
                n += 1;
            }
            assert!(
                m.envelope == 0.0,
                "EnvelopeFollower left tail {}",
                m.envelope
            );
        }

        // Limiter (release on port 2).
        {
            let mut m = Limiter::new(fs);
            let mut i = PortValues::new();
            let mut o = PortValues::new();
            i.set(2, 0.0);
            i.set(0, 5.0);
            for _ in 0..2000 {
                m.tick(&i, &mut o);
            }
            i.set(0, 0.0);
            let mut n = 0;
            while m.envelope != 0.0 && n < BUDGET {
                m.tick(&i, &mut o);
                n += 1;
            }
            assert!(m.envelope == 0.0, "Limiter left tail {}", m.envelope);
        }

        // Compressor (release on port 4). Sidechain defaults to the silent in.
        {
            let mut m = Compressor::new(fs);
            let mut i = PortValues::new();
            let mut o = PortValues::new();
            i.set(4, 0.0);
            i.set(0, 5.0);
            for _ in 0..2000 {
                m.tick(&i, &mut o);
            }
            i.set(0, 0.0);
            let mut n = 0;
            while m.envelope != 0.0 && n < BUDGET {
                m.tick(&i, &mut o);
                n += 1;
            }
            assert!(m.envelope == 0.0, "Compressor left tail {}", m.envelope);
        }

        // NoiseGate: both the detector envelope and the gate fade must flush.
        {
            let mut m = NoiseGate::new(fs);
            let mut i = PortValues::new();
            let mut o = PortValues::new();
            i.set(3, 0.0); // fast release
            i.set(0, 5.0);
            for _ in 0..2000 {
                m.tick(&i, &mut o);
            }
            i.set(0, 0.0);
            let mut n = 0;
            while (m.envelope != 0.0 || m.gate_state != 0.0) && n < BUDGET {
                m.tick(&i, &mut o);
                n += 1;
            }
            assert!(
                m.envelope == 0.0 && m.gate_state == 0.0,
                "NoiseGate left tail: env {} gate {}",
                m.envelope,
                m.gate_state
            );
        }
    }

    // ---- Q018: ADSR exponential mode + linear stays unchanged ----

    #[test]
    fn test_adsr_exp_mode_reaches_sustain() {
        let fs = 1000.0;
        let mut adsr = Adsr::new(fs);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(2, 0.3); // attack ~15.8ms
        inputs.set(3, 0.5); // decay 0.1s
        inputs.set(4, 0.6); // sustain 0.6
        inputs.set(5, 0.5); // release 0.1s
        inputs.set(6, 5.0); // shape = exponential
        inputs.set(0, 5.0); // gate on

        let mut reached = None;
        for i in 0..5000 {
            adsr.tick(&inputs, &mut outputs);
            if adsr.stage == AdsrStage::Sustain {
                reached = Some(i);
                break;
            }
        }
        let reached = reached.expect("exponential envelope should reach sustain");
        assert!(
            (adsr.level - 0.6).abs() < 1e-6,
            "exp sustain level {} != 0.6",
            adsr.level
        );
        // Attack + decay complete within a handful of time constants.
        assert!(
            reached < 2000,
            "exp env took {} samples to reach sustain",
            reached
        );
    }

    #[test]
    fn test_adsr_linear_mode_is_linear() {
        let fs = 1000.0;
        let mut adsr = Adsr::new(fs);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(2, 0.5); // attack 0.1s -> linear rate 0.01/sample
        inputs.set(0, 5.0); // gate on
                            // shape defaults to 0 (linear) -- do not touch port 6.
        let mut levels = [0.0f64; 10];
        for l in levels.iter_mut() {
            adsr.tick(&inputs, &mut outputs);
            *l = adsr.level;
        }
        for (i, &lvl) in levels.iter().enumerate() {
            let expected = 0.01 * (i as f64 + 1.0);
            assert!(
                (lvl - expected).abs() < 1e-9,
                "linear attack sample {} = {}, expected {}",
                i,
                lvl,
                expected
            );
        }
    }

    // ---- Q019: VCA response curve, boost, and default parity ----

    #[test]
    fn test_vca_default_golden() {
        let mut vca = Vca::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        // (input, cv, expected) at default response (linear) and gain scale 1.0.
        let cases = [
            (1.0_f64, 10.0_f64, 1.0_f64),
            (2.0, 5.0, 1.0),
            (-4.0, 2.0, -0.8),
            (3.0, 7.0, 2.1),
            (5.0, 0.0, 0.0),
            (0.5, 10.0, 0.5),
        ];
        for (inp, cv, expected) in cases {
            inputs.set(0, inp);
            inputs.set(1, cv);
            vca.tick(&inputs, &mut outputs);
            let out = outputs.get(10).unwrap();
            assert!(
                (out - expected).abs() < 1e-12,
                "in={} cv={} => {} (want {})",
                inp,
                cv,
                out,
                expected
            );
        }
    }

    #[test]
    fn test_vca_exponential_response() {
        let mut vca = Vca::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(2, 5.0); // exponential response
        inputs.set(0, 1.0); // unit input -> out == gain

        // Documented midpoint: cv=5V -> gain 0.25.
        inputs.set(1, 5.0);
        vca.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 0.25).abs() < 1e-9);

        // Monotonic across the cv range.
        let mut prev = -1.0;
        for k in 0..=20 {
            let cv = k as f64 * 0.5;
            inputs.set(1, cv);
            vca.tick(&inputs, &mut outputs);
            let g = outputs.get(10).unwrap();
            assert!(g >= prev - 1e-12, "not monotonic at cv={}", cv);
            prev = g;
        }

        // Matched endpoints: cv=10V -> 1.0, cv=0V -> 0.0.
        inputs.set(1, 10.0);
        vca.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 1.0).abs() < 1e-9);
        inputs.set(1, 0.0);
        vca.tick(&inputs, &mut outputs);
        assert!(outputs.get(10).unwrap().abs() < 1e-12);
    }

    #[test]
    fn test_vca_boost() {
        let mut vca = Vca::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 1.0);
        inputs.set(1, 10.0); // linear gain 1.0
        inputs.set(3, 2.0); // 2x boost
        vca.tick(&inputs, &mut outputs);
        assert!(
            (outputs.get(10).unwrap() - 2.0).abs() < 1e-9,
            "boost failed: {}",
            outputs.get(10).unwrap()
        );
        // Gain scale is clamped to <= 2.0.
        inputs.set(3, 5.0);
        vca.tick(&inputs, &mut outputs);
        assert!((outputs.get(10).unwrap() - 2.0).abs() < 1e-9);
    }

    // ================================================================
    // Q148: sidechain keys + Ducker
    // ================================================================

    #[test]
    fn test_noise_gate_opens_from_sidechain() {
        // Main input is quiet (would keep the gate shut), but a loud sidechain key
        // opens the gate.
        let mut gate = NoiseGate::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 0.01); // quiet main
        inputs.set(1, 0.3); // threshold
        inputs.set(5, 5.0); // loud sidechain key

        for _ in 0..2000 {
            gate.tick(&inputs, &mut outputs);
        }
        // Gate output (port 11) should be open (high).
        assert!(
            outputs.get(11).unwrap() > GATE_THRESHOLD_V,
            "sidechain key should open the gate"
        );
    }

    #[test]
    fn test_noise_gate_sidechain_unpatched_matches_input() {
        // With no sidechain patched, the detector mirrors the main input, so a
        // quiet input keeps the gate closed (unchanged legacy behavior).
        let mut gate = NoiseGate::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.01);
        inputs.set(1, 0.5);
        for _ in 0..2000 {
            gate.tick(&inputs, &mut outputs);
        }
        assert!(outputs.get(11).unwrap() < GATE_THRESHOLD_V);
    }

    #[test]
    fn test_limiter_sidechain_drives_gain_reduction() {
        // Quiet main input, loud sidechain -> gain reduction driven by the key.
        let mut limiter = Limiter::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 0.5); // quiet main (below threshold on its own)
        inputs.set(1, 0.5); // threshold 2.5V
        inputs.set(4, 10.0); // loud sidechain key
        for _ in 0..200 {
            limiter.tick(&inputs, &mut outputs);
        }
        // gain reduction output should be positive (limiting active from key).
        assert!(
            outputs.get(11).unwrap() > 0.0,
            "sidechain key should drive limiting"
        );
    }

    #[test]
    fn test_ducker_attenuates_on_key_and_recovers() {
        let sr = 44100.0;
        let mut ducker = Ducker::new(sr);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        inputs.set(0, 4.0); // steady main signal
        inputs.set(4, 0.0); // fast attack
        inputs.set(5, 0.0); // fast release

        // Key present (loud) -> output ducked below the main level.
        inputs.set(1, 5.0);
        for _ in 0..2000 {
            ducker.tick(&inputs, &mut outputs);
        }
        let ducked = outputs.get(10).unwrap();
        assert!(
            ducked.abs() < 3.5,
            "output should be attenuated while key active, got {ducked}"
        );
        assert!(
            outputs.get(11).unwrap() > 0.0,
            "gain-reduction CV should be positive while ducking"
        );

        // Key removed -> output recovers toward the full main level.
        inputs.set(1, 0.0);
        for _ in 0..4000 {
            ducker.tick(&inputs, &mut outputs);
        }
        let recovered = outputs.get(10).unwrap();
        assert!(
            (recovered - 4.0).abs() < 0.2,
            "output should recover after key release, got {recovered}"
        );
    }

    #[test]
    fn test_ducker_default_type_id() {
        let ducker = Ducker::default();
        assert_eq!(ducker.type_id(), "ducker");
    }

    #[test]
    fn test_ducker_no_key_passes_through() {
        // Silent key -> no ducking, signal passes through unchanged.
        let mut ducker = Ducker::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 3.0);
        inputs.set(1, 0.0);
        for _ in 0..500 {
            ducker.tick(&inputs, &mut outputs);
        }
        assert!((outputs.get(10).unwrap() - 3.0).abs() < 1e-9);
        assert!(outputs.get(11).unwrap().abs() < 1e-9);
    }

    // ---- Q160: dynamics envelope detectors recover from non-finite input ----

    /// Poison a module's detector ports with NaN/±Inf, then feed a clean signal
    /// and confirm both the envelope state and the port-10 output recover to
    /// finite values (a NaN must not latch the one-pole detector permanently).
    fn assert_detector_recovers<M: GraphModule>(
        module: &mut M,
        poison_ports: &[u32],
        clean: &[(u32, f64)],
        envelope: impl Fn(&M) -> f64,
    ) {
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        for &bad in &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for &p in poison_ports {
                inputs.set(p, bad);
            }
            module.tick(&inputs, &mut outputs);
        }
        // Feed a clean signal and let the detector settle.
        let mut inputs = PortValues::new();
        for &(port, value) in clean {
            inputs.set(port, value);
        }
        for _ in 0..2000 {
            module.tick(&inputs, &mut outputs);
        }
        assert!(
            envelope(module).is_finite(),
            "envelope stayed non-finite after a NaN input"
        );
        assert!(
            outputs.get(10).unwrap().is_finite(),
            "output stayed non-finite after a NaN input"
        );
    }

    #[test]
    fn test_limiter_nan_recovery() {
        let mut m = Limiter::new(44100.0);
        assert_detector_recovers(&mut m, &[0, 4], &[(0, 0.5), (4, 0.5)], |m| m.envelope);
    }

    #[test]
    fn test_noise_gate_nan_recovery() {
        let mut m = NoiseGate::new(44100.0);
        assert_detector_recovers(&mut m, &[0, 5], &[(0, 0.5), (5, 0.5)], |m| m.envelope);
    }

    #[test]
    fn test_compressor_nan_recovery() {
        let mut m = Compressor::new(44100.0);
        assert_detector_recovers(&mut m, &[0, 6], &[(0, 0.5), (6, 0.5)], |m| m.envelope);
    }

    #[test]
    fn test_ducker_nan_recovery() {
        let mut m = Ducker::new(44100.0);
        assert_detector_recovers(&mut m, &[0, 1], &[(0, 0.5), (1, 0.5)], |m| m.envelope);
    }

    #[test]
    fn test_envelope_follower_nan_recovery() {
        let mut m = EnvelopeFollower::new(44100.0);
        assert_detector_recovers(&mut m, &[0], &[(0, 0.5)], |m| m.envelope);
    }

    // ---- Coefficient memoization (perf) ------------------------------------

    /// Memoization must be observationally invisible: an ADSR whose memo is
    /// invalidated before every tick executes the pre-memoization computation
    /// (three `pow` + three `exp` per sample) and must agree bit-for-bit with
    /// the memoized ADSR across gate cycles, both curve shapes, and both
    /// constant and per-sample-modulated time CVs.
    #[test]
    fn test_adsr_memo_bit_identical() {
        let mut memoized = Adsr::new(44100.0);
        let mut forced = Adsr::new(44100.0);
        let mut inputs = PortValues::new();
        let mut out_m = PortValues::new();
        let mut out_f = PortValues::new();

        for n in 0..30_000u32 {
            let t = n as f64;
            // Two gate cycles so attack/decay/sustain/release all run.
            let gate = if (n % 10_000) < 6_000 { 5.0 } else { 0.0 };
            inputs.set(0, gate);
            // Linear curves for the first half, exponential for the second.
            inputs.set(6, if n < 15_000 { 0.0 } else { 5.0 });
            if n < 20_000 {
                // Constant time CVs (memo hits after the first sample).
                inputs.set(2, 0.15);
                inputs.set(3, 0.25);
                inputs.set(5, 0.35);
            } else {
                // Per-sample-modulated attack CV (memo misses every sample).
                inputs.set(2, 0.15 + 0.1 * Libm::<f64>::sin(t * 0.002));
            }
            inputs.set(4, 0.6);

            memoized.tick(&inputs, &mut out_m);
            forced.time_memo.invalidate();
            forced.tick(&inputs, &mut out_f);

            for &id in &[10u32, 11, 12] {
                assert_eq!(
                    out_m.get(id).unwrap().to_bits(),
                    out_f.get(id).unwrap().to_bits(),
                    "ADSR output {id} diverged at sample {n}"
                );
            }
        }
        assert!(memoized.time_memo.recompute_count() <= 10_001);
        assert_eq!(forced.time_memo.recompute_count(), 30_000);
    }

    /// With constant time CVs the ADSR time/coefficient block is computed
    /// exactly once, and a sample-rate change (part of the key) recomputes.
    #[test]
    fn test_adsr_memo_recompute_count() {
        let mut adsr = Adsr::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, 5.0);
        for _ in 0..1000 {
            adsr.tick(&inputs, &mut outputs);
        }
        assert_eq!(adsr.time_memo.recompute_count(), 1);

        adsr.set_sample_rate(48000.0);
        adsr.tick(&inputs, &mut outputs);
        assert_eq!(adsr.time_memo.recompute_count(), 2);
    }

    /// Same equivalence for the dynamics detectors' memoized ballistics
    /// coefficients (Compressor shown; Limiter/NoiseGate/Ducker/Follower share
    /// the identical memo structure).
    #[test]
    fn test_compressor_memo_bit_identical() {
        let mut memoized = Compressor::new(44100.0);
        let mut forced = Compressor::new(44100.0);
        let mut inputs = PortValues::new();
        let mut out_m = PortValues::new();
        let mut out_f = PortValues::new();

        for n in 0..10_000u32 {
            let t = n as f64;
            inputs.set(0, Libm::<f64>::sin(t * 0.053) * 4.5);
            inputs.set(1, 0.3);
            inputs.set(2, 0.7);
            if n >= 5_000 {
                // Per-sample-modulated release CV.
                inputs.set(4, 0.3 + 0.2 * Libm::<f64>::sin(t * 0.004));
            }

            memoized.tick(&inputs, &mut out_m);
            forced.coef_memo.invalidate();
            forced.tick(&inputs, &mut out_f);

            for &id in &[10u32, 11] {
                assert_eq!(
                    out_m.get(id).unwrap().to_bits(),
                    out_f.get(id).unwrap().to_bits(),
                    "Compressor output {id} diverged at sample {n}"
                );
            }
        }
        assert!(memoized.coef_memo.recompute_count() <= 5_001);
    }
}
