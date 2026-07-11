//! Sample playback module (Q142).
//!
//! [`SamplePlayer`] plays a mono sample buffer with V/Oct pitch control, a
//! selectable start position, one-shot / looping modes, trigger and gated
//! playback, and an end-of-sample trigger output. Reads are cubic-interpolated
//! (Catmull-Rom) and the audio-path `tick` is allocation-free; only the non-RT
//! [`SamplePlayer::set_buffer`] setter allocates.
//!
//! Buffers are held as `alloc::vec::Vec<f64>`. Like the rest of `modules/`, this
//! relies on the crate's unconditional `extern crate alloc`, so it compiles in
//! pure `no_std` as well as `alloc`/`std`.

use super::common::{EdgeDetector, GATE_HIGH_V, GATE_THRESHOLD_V};
use crate::port::{
    GraphModule, ModulatedParam, ParamRange, PortDef, PortSpec, PortValues, SignalKind,
};
use alloc::vec;
use alloc::vec::Vec;
use libm::Libm;

/// Mono sample player with V/Oct pitch, start position, and looping.
///
/// # Parameter reads via [`ModulatedParam`] (Q147)
///
/// Pitch and start position are read through [`ModulatedParam`], making that type
/// a live part of a real DSP path rather than an unused export:
/// - `pitch` uses a [`ParamRange::VoltPerOctave`] mapping. Its `base` field carries
///   the coarse V/Oct pitch from the `voct` input, and its value is `2^voct`, so
///   0 V plays at unity rate and +1 V doubles the playback speed.
/// - `start` uses a [`ParamRange::Linear`] `0..1` mapping. Its `base` is the panel
///   start-position knob and its CV comes from the `start` input (normalized on the
///   `ModulatedParam` ±5 V scale), combined into a normalized `0..1` position.
pub struct SamplePlayer {
    /// Mono sample data.
    buffer: Vec<f64>,
    /// Sample rate the buffer was recorded at.
    buffer_sample_rate: f64,
    /// Engine (graph) sample rate.
    sample_rate: f64,
    /// Current fractional read position, in buffer samples.
    phase: f64,
    /// Whether playback is currently active.
    playing: bool,
    /// True when the current playback was started by the gate input (so a gate
    /// release stops it); false when started by the trigger input (gate ignored).
    started_by_gate: bool,
    /// Rising-edge detector for the trigger input.
    trig_edge: EdgeDetector,
    /// Rising-edge detector for the gate input.
    gate_edge: EdgeDetector,
    /// Pitch read path (V/Oct -> playback-rate multiplier).
    pitch: ModulatedParam,
    /// Start-position read path (normalized 0..1).
    start: ModulatedParam,
    spec: PortSpec,
}

impl SamplePlayer {
    /// Create a player over `buffer` recorded at `buffer_sample_rate`, running in a
    /// graph at `engine_sample_rate`.
    pub fn new(buffer: Vec<f64>, buffer_sample_rate: f64, engine_sample_rate: f64) -> Self {
        Self {
            buffer,
            buffer_sample_rate: if buffer_sample_rate > 0.0 {
                buffer_sample_rate
            } else {
                44100.0
            },
            sample_rate: if engine_sample_rate > 0.0 {
                engine_sample_rate
            } else {
                44100.0
            },
            phase: 0.0,
            playing: false,
            started_by_gate: false,
            trig_edge: EdgeDetector::new(),
            gate_edge: EdgeDetector::new(),
            pitch: ModulatedParam::new(ParamRange::VoltPerOctave { base_freq: 1.0 }),
            start: ModulatedParam::new(ParamRange::Linear { min: 0.0, max: 1.0 }).with_base(0.0),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "trig", SignalKind::Trigger),
                    PortDef::new(1, "gate", SignalKind::Gate),
                    PortDef::new(2, "voct", SignalKind::VoltPerOctave),
                    PortDef::new(3, "start", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(4, "loop", SignalKind::Gate).with_default(0.0),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "eos", SignalKind::Trigger),
                ],
            },
        }
    }

    /// Create an empty player (silent until a buffer is assigned).
    pub fn empty(engine_sample_rate: f64) -> Self {
        Self::new(Vec::new(), engine_sample_rate, engine_sample_rate)
    }

    /// Replace the sample buffer (non-real-time; allocates/moves the `Vec`).
    ///
    /// Resets playback state so a stale read position cannot index past a shorter
    /// new buffer.
    pub fn set_buffer(&mut self, buffer: Vec<f64>, buffer_sample_rate: f64) {
        self.buffer = buffer;
        if buffer_sample_rate > 0.0 {
            self.buffer_sample_rate = buffer_sample_rate;
        }
        self.phase = 0.0;
        self.playing = false;
        self.started_by_gate = false;
    }

    /// Number of samples in the loaded buffer.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Whether the loaded buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Set the panel start-position knob (0..1), the `base` of the start
    /// [`ModulatedParam`].
    pub fn set_start(&mut self, start: f64) {
        self.start.base = start.clamp(0.0, 1.0);
    }

    /// Current start-position knob (0..1).
    pub fn start_position(&self) -> f64 {
        self.start.base
    }

    /// Cubic (Catmull-Rom) interpolated read at fractional `pos` (buffer samples),
    /// with edge indices clamped into range.
    fn read_cubic(&self, pos: f64) -> f64 {
        let len = self.buffer.len();
        if len == 0 {
            return 0.0;
        }
        if len == 1 {
            return self.buffer[0];
        }
        let i = Libm::<f64>::floor(pos) as isize;
        let frac = pos - i as f64;
        let last = (len - 1) as isize;
        let sample = |k: isize| -> f64 {
            let idx = (i + k).clamp(0, last) as usize;
            self.buffer[idx]
        };
        let y0 = sample(-1);
        let y1 = sample(0);
        let y2 = sample(1);
        let y3 = sample(2);
        let a = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
        let b = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c = -0.5 * y0 + 0.5 * y2;
        let d = y1;
        ((a * frac + b) * frac + c) * frac + d
    }

    /// Start-position in buffer samples, resolved from the start `ModulatedParam`.
    fn start_sample(&self) -> f64 {
        let len = self.buffer.len();
        if len == 0 {
            0.0
        } else {
            self.start.value().clamp(0.0, 1.0) * (len - 1) as f64
        }
    }
}

impl Default for SamplePlayer {
    fn default() -> Self {
        Self::empty(44100.0)
    }
}

impl GraphModule for SamplePlayer {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let trig = inputs.get_or(0, 0.0);
        let gate = inputs.get_or(1, 0.0);
        let voct = inputs.get_or(2, 0.0);
        let start_cv = inputs.get_or(3, 0.0);
        let looping = inputs.get_or(4, 0.0) > GATE_THRESHOLD_V;

        // Feed the start CV into its ModulatedParam so the resolved start position
        // combines the panel knob (base) with incoming CV.
        self.start.set_cv(start_cv);

        // Coarse V/Oct pitch drives the base of the pitch ModulatedParam; its value
        // is the playback-rate multiplier 2^voct.
        self.pitch.base = voct;
        let rate_mult = self.pitch.value();

        let len = self.buffer.len();
        let mut eos = 0.0;

        // Retrigger handling: trigger and gate both (re)start from the start
        // position; a trigger-started voice ignores the gate, a gate-started voice
        // stops when the gate falls (gated one-shot / looper).
        let trig_edge = self.trig_edge.rising(trig);
        let gate_edge = self.gate_edge.rising(gate);
        if trig_edge {
            self.phase = self.start_sample();
            self.playing = len > 0;
            self.started_by_gate = false;
        } else if gate_edge {
            self.phase = self.start_sample();
            self.playing = len > 0;
            self.started_by_gate = true;
        }

        // Gated release: a voice started by the gate stops when the gate goes low.
        if self.started_by_gate && gate <= GATE_THRESHOLD_V {
            self.playing = false;
        }

        if len == 0 || !self.playing {
            outputs.set(10, 0.0);
            outputs.set(11, eos);
            return;
        }

        // Read at the current position, then advance.
        let out = self.read_cubic(self.phase);

        // Playback rate in buffer-samples per engine-sample.
        let rate = rate_mult * (self.buffer_sample_rate / self.sample_rate);
        self.phase += rate;

        let end = len as f64;
        if self.phase >= end {
            eos = GATE_HIGH_V;
            if looping {
                // Wrap back into the loop region [start, end).
                let start = self.start_sample();
                let span = (end - start).max(1.0);
                while self.phase >= end {
                    self.phase -= span;
                }
                if self.phase < start {
                    self.phase = start;
                }
            } else {
                self.playing = false;
                self.phase = end;
            }
        }

        outputs.set(10, out);
        outputs.set(11, eos);
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.playing = false;
        self.started_by_gate = false;
        self.trig_edge.reset();
        self.gate_edge.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        if sample_rate > 0.0 {
            self.sample_rate = sample_rate;
        }
    }

    fn type_id(&self) -> &'static str {
        "sample_player"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A buffer with an impulse every `spacing` samples, `count` impulses long.
    fn impulse_buffer(spacing: usize, count: usize) -> Vec<f64> {
        let mut buf = vec![0.0; spacing * count];
        for k in 0..count {
            buf[k * spacing] = 1.0;
        }
        buf
    }

    fn trigger_once(player: &mut SamplePlayer, inputs: &mut PortValues, outputs: &mut PortValues) {
        // Rising edge on the trigger port.
        inputs.set(0, 0.0);
        player.tick(inputs, outputs);
        inputs.set(0, 5.0);
        player.tick(inputs, outputs);
    }

    #[test]
    fn test_unity_rate_impulse_spacing() {
        // buffer_sr == engine_sr and 0 V => rate 1.0 => output spacing == buffer spacing.
        let sr = 48000.0;
        let mut player = SamplePlayer::new(impulse_buffer(4, 6), sr, sr);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(2, 0.0); // 0 V

        trigger_once(&mut player, &mut inputs, &mut outputs);
        // First tick after the trigger already produced buffer[0] (an impulse).
        let mut impulse_positions = Vec::new();
        // The trigger's second tick is output index 0.
        let first = outputs.get(10).unwrap();
        if first > 0.5 {
            impulse_positions.push(0);
        }
        for i in 1..20 {
            player.tick(&inputs, &mut outputs);
            if outputs.get(10).unwrap() > 0.5 {
                impulse_positions.push(i);
            }
        }
        // Impulses at 0, 4, 8, ...
        assert!(impulse_positions.len() >= 3);
        assert_eq!(impulse_positions[0], 0);
        assert_eq!(impulse_positions[1], 4);
        assert_eq!(impulse_positions[2], 8);
    }

    #[test]
    fn test_plus_one_volt_doubles_speed() {
        let sr = 48000.0;
        let mut player = SamplePlayer::new(impulse_buffer(4, 6), sr, sr);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(2, 1.0); // +1 V => 2x rate

        trigger_once(&mut player, &mut inputs, &mut outputs);
        let mut impulse_positions = Vec::new();
        if outputs.get(10).unwrap() > 0.5 {
            impulse_positions.push(0);
        }
        for i in 1..20 {
            player.tick(&inputs, &mut outputs);
            if outputs.get(10).unwrap() > 0.5 {
                impulse_positions.push(i);
            }
        }
        // At 2x rate impulses come out at half the spacing: 0, 2, 4, ...
        assert!(impulse_positions.len() >= 3);
        assert_eq!(impulse_positions[0], 0);
        assert_eq!(impulse_positions[1], 2);
        assert_eq!(impulse_positions[2], 4);
    }

    #[test]
    fn test_loop_wraps() {
        let sr = 48000.0;
        // Short buffer, looping on.
        let mut player = SamplePlayer::new(impulse_buffer(2, 3), sr, sr); // len 6
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(2, 0.0);
        inputs.set(4, 5.0); // loop on

        trigger_once(&mut player, &mut inputs, &mut outputs);
        let mut impulses = 0;
        for _ in 0..60 {
            player.tick(&inputs, &mut outputs);
            if outputs.get(10).unwrap() > 0.5 {
                impulses += 1;
            }
        }
        // Without looping there are only 3 impulses total; wrapping produces many more.
        assert!(impulses > 6, "loop did not wrap: {impulses} impulses");
    }

    #[test]
    fn test_eos_fires_once_at_end() {
        let sr = 48000.0;
        let mut player = SamplePlayer::new(impulse_buffer(1, 8), sr, sr); // len 8, loop off
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(2, 0.0);

        trigger_once(&mut player, &mut inputs, &mut outputs);
        let mut eos_count = 0;
        for _ in 0..40 {
            player.tick(&inputs, &mut outputs);
            if outputs.get(11).unwrap() > GATE_THRESHOLD_V {
                eos_count += 1;
            }
        }
        assert_eq!(eos_count, 1, "eos should fire exactly once at end");
    }

    #[test]
    fn test_empty_buffer_silent() {
        let mut player = SamplePlayer::empty(48000.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        assert!(player.is_empty());
        assert_eq!(player.len(), 0);

        trigger_once(&mut player, &mut inputs, &mut outputs);
        for _ in 0..50 {
            player.tick(&inputs, &mut outputs);
            assert_eq!(outputs.get(10).unwrap(), 0.0);
        }
    }

    #[test]
    fn test_gated_playback_stops_on_release() {
        let sr = 48000.0;
        let mut player = SamplePlayer::new(impulse_buffer(1, 64), sr, sr);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(2, 0.0);

        // Gate on -> starts.
        inputs.set(1, 0.0);
        player.tick(&inputs, &mut outputs);
        inputs.set(1, 5.0);
        player.tick(&inputs, &mut outputs);
        assert!(player.playing);

        // Gate off -> gated voice stops.
        inputs.set(1, 0.0);
        player.tick(&inputs, &mut outputs);
        assert!(!player.playing);
        assert_eq!(outputs.get(10).unwrap(), 0.0);
    }

    #[test]
    fn test_type_id_and_default() {
        let player = SamplePlayer::default();
        assert_eq!(player.type_id(), "sample_player");
        assert!(player.is_empty());
    }

    #[test]
    fn test_set_buffer_swaps() {
        let mut player = SamplePlayer::empty(48000.0);
        assert!(player.is_empty());
        player.set_buffer(vec![0.5; 100], 48000.0);
        assert_eq!(player.len(), 100);
    }
}
