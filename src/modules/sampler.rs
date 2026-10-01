//! Sample playback (Q142) and capture.
//!
//! [`SamplePlayer`] plays a mono sample buffer with V/Oct pitch control, a
//! selectable start position, one-shot / looping modes, trigger and gated
//! playback, and an end-of-sample trigger output. Reads are cubic-interpolated
//! (Catmull-Rom) and the audio-path `tick` is allocation-free; only the non-RT
//! [`SamplePlayer::set_buffer`] setter allocates.
//!
//! [`Capture`] records its input into a fixed-size buffer and plays the recording
//! back the same way, so a patch can resample a signal (an audio input, itself) and
//! play it as a source. Its recording serializes with the patch.
//!
//! Buffers are held as `alloc::vec::Vec<f64>`. Like the rest of `modules/`, this
//! relies on the crate's unconditional `extern crate alloc`, so it compiles in
//! pure `no_std` as well as `alloc`/`std`.

use super::common::{sanitize_audio, EdgeDetector, Memo, GATE_HIGH_V, GATE_THRESHOLD_V};
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
    /// Memoized playback-rate multiplier `2^voct` (one `pow` per sample while
    /// the pitch is static). Keyed on every varying field feeding
    /// `pitch.value()` (`base`, `cv`, `attenuverter`; the range mapping is
    /// fixed at construction), so any pitch change misses correctly.
    rate_memo: Memo<3, f64>,
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
            rate_memo: Memo::new(0.0),
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
        read_cubic(&self.buffer, pos)
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

/// Cubic (Catmull-Rom) interpolated read of `buffer` at fractional `pos` (in samples),
/// with edge indices clamped into range. Shared by [`SamplePlayer`] and [`Capture`].
fn read_cubic<T: Copy + Into<f64>>(buffer: &[T], pos: f64) -> f64 {
    let len = buffer.len();
    if len == 0 {
        return 0.0;
    }
    if len == 1 {
        return buffer[0].into();
    }
    let i = Libm::<f64>::floor(pos) as isize;
    let frac = pos - i as f64;
    let last = (len - 1) as isize;
    let sample = |k: isize| -> f64 {
        let idx = (i + k).clamp(0, last) as usize;
        buffer[idx].into()
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
        // is the playback-rate multiplier 2^voct, memoized on the pitch inputs
        // (bit-exact miss path).
        self.pitch.base = voct;
        let pitch = &self.pitch;
        let rate_mult = self
            .rate_memo
            .get_or_compute([pitch.base, pitch.cv, pitch.attenuverter], || pitch.value());

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
                // Wrap back into the loop region [start, end) with a single
                // bounded modulo instead of a data-dependent `while` loop: at a
                // high playback rate over a short loop span the loop could
                // otherwise iterate O(rate/span) times per tick (a variable-time
                // algorithm in the RT path). `fmod(phase - start, span)` lands in
                // [0, span) since span > 0, so `start + ..` is always in
                // [start, end).
                let start = self.start_sample();
                let span = (end - start).max(1.0);
                self.phase = start + Libm::<f64>::fmod(self.phase - start, span);
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

/// Records its input into a fixed-size buffer and plays the recording back.
///
/// A recorder and a sample player in one module: raise `record` to capture `in`
/// from the start of the buffer, and play the recording with `trig` (one-shot),
/// `gate` (held) and `loop`, pitched by `voct` — exactly as [`SamplePlayer`]
/// plays its buffer. A patch can so resample a signal (an [`AudioInput`], or
/// itself) and play it as a source.
///
/// # Ports
///
/// | Port | Kind | |
/// |---|---|---|
/// | `in` | Audio | The signal to record |
/// | `record` | Gate | Rising edge starts a new recording from the start of the buffer (replacing the old one); it stops when the gate falls or the buffer is full |
/// | `trig` | Trigger | Rising edge plays the recording to its end (or loops) |
/// | `gate` | Gate | Rising edge plays; falling edge stops |
/// | `voct` | V/Oct | Playback pitch: 0 V plays at the recorded speed, +1 V an octave up |
/// | `loop` | Gate | While high, playback wraps at the end of the recording |
/// | `out` | Audio | Playback (silent while nothing plays) |
/// | `eos` | Trigger | Fires when one-shot playback reaches the end, or a loop wraps |
///
/// # Buffer and real-time safety
///
/// The buffer is allocated once, in [`new`](Self::new) / [`with_seconds`](Self::with_seconds)
/// ([`DEFAULT_SECONDS`](Self::DEFAULT_SECONDS) of the graph's sample rate), and stores
/// `f32` samples; recording and playback never allocate. The recording keeps the
/// sample rate it was made at, and plays back at the right speed if the graph's rate
/// changes. `reset()` stops recording and playback but keeps the recording: it is the
/// sound's content, as a [`SamplePlayer`]'s buffer is.
///
/// # Serialization
///
/// The recording is saved with the patch (`ModuleDef.state`): its samples as
/// little-endian `f32` bytes in base64 — lossless, so a reloaded patch renders
/// bit-identically — with its sample rate, length and the buffer's capacity. Every state
/// a `Capture` can save loads back; the saved capacity is clamped so a file cannot make
/// the loader allocate more than its data backs (see `deserialize_state`). An empty
/// capture saves no state.
///
/// [`AudioInput`]: crate::io::AudioInput
pub struct Capture {
    /// Recording storage; its length is the capacity.
    buffer: Vec<f32>,
    /// Samples recorded.
    len: usize,
    /// Sample rate the recording was made at.
    recorded_rate: f64,
    /// Graph sample rate.
    sample_rate: f64,
    recording: bool,
    /// Whether `record` was high on the previous tick.
    record_held: bool,
    /// Fractional playback position, in recorded samples.
    phase: f64,
    playing: bool,
    started_by_gate: bool,
    trig_edge: EdgeDetector,
    gate_edge: EdgeDetector,
    /// Memoized `2^voct`.
    rate_memo: Memo<1, f64>,
    spec: PortSpec,
}

impl Capture {
    /// Buffer length [`new`](Self::new) allocates, in seconds.
    pub const DEFAULT_SECONDS: f64 = 4.0;

    /// Longest buffer [`with_seconds`](Self::with_seconds) allocates, in seconds at the
    /// rate it is given. A take can still be longer (recorded into a buffer built for a
    /// higher rate than the graph now runs at, or set by the host); the state loader
    /// accepts any take its data backs.
    pub const MAX_SECONDS: f64 = 60.0;

    /// A capture holding up to [`DEFAULT_SECONDS`](Self::DEFAULT_SECONDS) at `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        Self::with_seconds(sample_rate, Self::DEFAULT_SECONDS)
    }

    /// A capture holding up to `seconds` (clamped to `MAX_SECONDS`) at `sample_rate`.
    pub fn with_seconds(sample_rate: f64, seconds: f64) -> Self {
        let sample_rate = if sample_rate > 0.0 && sample_rate.is_finite() {
            sample_rate
        } else {
            44_100.0
        };
        let seconds = if seconds.is_finite() {
            seconds.clamp(0.0, Self::MAX_SECONDS)
        } else {
            Self::DEFAULT_SECONDS
        };
        Self {
            buffer: vec![0.0; (seconds * sample_rate) as usize],
            len: 0,
            recorded_rate: sample_rate,
            sample_rate,
            recording: false,
            record_held: false,
            phase: 0.0,
            playing: false,
            started_by_gate: false,
            trig_edge: EdgeDetector::new(),
            gate_edge: EdgeDetector::new(),
            rate_memo: Memo::new(1.0),
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "record", SignalKind::Gate),
                    PortDef::new(2, "trig", SignalKind::Trigger),
                    PortDef::new(3, "gate", SignalKind::Gate),
                    PortDef::new(4, "voct", SignalKind::VoltPerOctave),
                    PortDef::new(5, "loop", SignalKind::Gate),
                ],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "eos", SignalKind::Trigger),
                ],
            },
        }
    }

    /// Most samples one recording can hold.
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }

    /// Samples recorded.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing is recorded.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The recording.
    pub fn recording(&self) -> &[f32] {
        &self.buffer[..self.len]
    }

    /// Sample rate the recording was made at.
    pub fn recorded_sample_rate(&self) -> f64 {
        self.recorded_rate
    }

    /// Whether `record` is capturing right now.
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Replace the recording with `samples` made at `sample_rate` (not real-time: grows
    /// the buffer if they do not fit). Stops recording and playback; non-finite samples
    /// become silence; a rate that is not positive and finite keeps the previous one.
    pub fn set_recording(&mut self, samples: &[f32], sample_rate: f64) {
        if sample_rate > 0.0 && sample_rate.is_finite() {
            self.recorded_rate = sample_rate;
        }
        if samples.len() > self.buffer.len() {
            self.buffer.resize(samples.len(), 0.0);
        }
        for (dst, &s) in self.buffer.iter_mut().zip(samples) {
            *dst = if s.is_finite() { s } else { 0.0 };
        }
        self.len = samples.len();
        self.stop();
    }

    /// Forget the recording.
    pub fn clear(&mut self) {
        self.len = 0;
        self.stop();
    }

    fn stop(&mut self) {
        self.recording = false;
        self.phase = 0.0;
        self.playing = false;
        self.started_by_gate = false;
    }
}

impl Default for Capture {
    fn default() -> Self {
        Self::new(44_100.0)
    }
}

impl GraphModule for Capture {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let x = sanitize_audio(inputs.get_or(0, 0.0));
        let record = inputs.get_or(1, 0.0) > GATE_THRESHOLD_V;
        let trig = inputs.get_or(2, 0.0);
        let gate = inputs.get_or(3, 0.0);
        let voct = sanitize_audio(inputs.get_or(4, 0.0)).clamp(-10.0, 10.0);
        let looping = inputs.get_or(5, 0.0) > GATE_THRESHOLD_V;

        // Record: a rising edge starts over at the top of the buffer.
        if record && !self.record_held {
            self.recording = true;
            self.len = 0;
            self.recorded_rate = self.sample_rate;
        } else if !record {
            self.recording = false;
        }
        self.record_held = record;
        if self.recording {
            if self.len < self.buffer.len() {
                self.buffer[self.len] = x as f32;
                self.len += 1;
            } else {
                self.recording = false; // full
            }
        }

        // Play, as SamplePlayer does.
        let trig_edge = self.trig_edge.rising(trig);
        let gate_edge = self.gate_edge.rising(gate);
        if trig_edge || gate_edge {
            self.phase = 0.0;
            self.playing = self.len > 0;
            self.started_by_gate = !trig_edge;
        }
        if self.started_by_gate && gate <= GATE_THRESHOLD_V {
            self.playing = false;
        }

        let len = self.len;
        let mut eos = 0.0;
        if len == 0 || !self.playing {
            outputs.set(10, 0.0);
            outputs.set(11, eos);
            return;
        }

        let out = read_cubic(&self.buffer[..len], self.phase);
        let rate_mult = self
            .rate_memo
            .get_or_compute([voct], || Libm::<f64>::pow(2.0, voct));
        self.phase += rate_mult * (self.recorded_rate / self.sample_rate);

        let end = len as f64;
        if self.phase >= end {
            eos = GATE_HIGH_V;
            if looping {
                // Bounded wrap (see SamplePlayer): one fmod, never a loop.
                self.phase = Libm::<f64>::fmod(self.phase, end);
            } else {
                self.playing = false;
                self.phase = end;
            }
        }

        outputs.set(10, out);
        outputs.set(11, eos);
    }

    /// Stop recording and playback. The recording itself is content and is kept.
    fn reset(&mut self) {
        self.recording = false;
        self.record_held = false;
        self.phase = 0.0;
        self.playing = false;
        self.started_by_gate = false;
        self.trig_edge.reset();
        self.gate_edge.reset();
    }

    /// Keeps the buffer and the recording (which remembers its own rate).
    fn set_sample_rate(&mut self, sample_rate: f64) {
        if sample_rate > 0.0 && sample_rate.is_finite() {
            self.sample_rate = sample_rate;
        }
    }

    fn type_id(&self) -> &'static str {
        "capture"
    }

    /// Save the recording: `{format, sample_rate, capacity, length, data}`, with `data`
    /// the recorded samples as little-endian `f32` bytes in base64. `None` when empty.
    #[cfg(feature = "alloc")]
    fn serialize_state(&self) -> Option<serde_json::Value> {
        if self.len == 0 {
            return None;
        }
        let mut bytes = Vec::with_capacity(self.len * 4);
        for s in self.recording() {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        Some(serde_json::json!({
            "format": CAPTURE_FORMAT,
            "sample_rate": self.recorded_rate,
            "capacity": self.buffer.len(),
            "length": self.len,
            "data": base64::encode(&bytes),
        }))
    }

    /// Restore a recording saved by [`serialize_state`](Self::serialize_state).
    ///
    /// Accepts every state a `Capture` can save: any positive, finite `sample_rate` (the
    /// rates `set_recording` and `set_sample_rate` accept), any `length` the data backs,
    /// and any `capacity`. Patch JSON is untrusted, so `capacity` is a request, clamped
    /// to `[length, max(length, the buffer the module already has)]`: beyond the buffer
    /// the module was built with, allocation is proportional to the decoded samples. A
    /// capture built with more headroom than the default keeps only the default after a
    /// reload through the registry; its take is never cut. Rejected: another format, a
    /// rate that is not positive and finite, `data` that is not base64, a `length` that
    /// disagrees with the data, and non-finite samples.
    #[cfg(feature = "alloc")]
    fn deserialize_state(
        &mut self,
        state: &serde_json::Value,
    ) -> Result<(), alloc::string::String> {
        use alloc::format;
        let err = |what: &str| format!("Capture state: {what}");
        if state.get("format").and_then(|f| f.as_str()) != Some(CAPTURE_FORMAT) {
            return Err(err("format must be \"f32le-base64\""));
        }
        let rate = state
            .get("sample_rate")
            .and_then(|v| v.as_f64())
            .filter(|r| *r > 0.0 && r.is_finite())
            .ok_or_else(|| err("sample_rate must be positive and finite"))?;
        let count = |name: &str| {
            state
                .get(name)
                .and_then(|v| v.as_u64())
                .ok_or_else(|| err(&format!("{name} must be a non-negative integer")))
        };
        let saved_length = count("length")?;
        let saved_capacity = count("capacity")?;
        let data = state
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| err("data must be a base64 string"))?;
        let bytes = base64::decode(data).ok_or_else(|| err("data is not valid base64"))?;
        // The data decides the length; `length` must agree with it.
        let length = bytes.len() / 4;
        if bytes.len() % 4 != 0 || saved_length != length as u64 {
            return Err(err(&format!(
                "length {saved_length} disagrees with data of {} bytes",
                bytes.len()
            )));
        }
        let mut samples = Vec::with_capacity(length);
        for chunk in bytes.chunks_exact(4) {
            let s = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            if !s.is_finite() {
                return Err(err("data holds a non-finite sample"));
            }
            samples.push(s);
        }
        // `capacity` may exceed what this platform can address; it is a request anyway.
        let requested = usize::try_from(saved_capacity).unwrap_or(usize::MAX);
        let capacity = requested.clamp(length, length.max(self.buffer.len()));
        if self.buffer.len() != capacity {
            self.buffer = vec![0.0; capacity];
        }
        self.set_recording(&samples, rate);
        Ok(())
    }
}

/// `format` tag of a serialized [`Capture`] recording.
#[cfg(feature = "alloc")]
const CAPTURE_FORMAT: &str = "f32le-base64";

/// Standard base64 (RFC 4648, with padding) for [`Capture`]'s saved recordings.
#[cfg(feature = "alloc")]
mod base64 {
    use alloc::string::String;
    use alloc::vec::Vec;

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub(super) fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
                if i <= chunk.len() {
                    out.push(ALPHABET[((n >> shift) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    pub(super) fn decode(text: &str) -> Option<Vec<u8>> {
        let text = text.as_bytes();
        if text.len() % 4 != 0 {
            return None;
        }
        let value = |c: u8| -> Option<u32> {
            Some(match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return None,
            } as u32)
        };
        let mut out = Vec::with_capacity(text.len() / 4 * 3);
        let quads = text.len() / 4;
        for (q, quad) in text.chunks(4).enumerate() {
            let pad = quad.iter().rev().take_while(|&&c| c == b'=').count();
            if pad > 2 || (pad > 0 && q + 1 != quads) {
                return None;
            }
            let mut n = 0u32;
            for &c in &quad[..4 - pad] {
                n = (n << 6) | value(c)?;
            }
            n <<= 6 * pad as u32;
            let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            out.extend_from_slice(&bytes[..3 - pad]);
        }
        Some(out)
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
    fn test_loop_wrap_bounded_at_pathological_rate() {
        // Regression: the loop wrap must be bounded modular arithmetic, not a
        // data-dependent `while` that iterates O(rate/span) times per tick. With
        // a huge playback rate over a 1-sample loop span the old loop would hang
        // (at f64 magnitudes where `phase -= span` is a no-op it never
        // terminates). The fix keeps every tick O(1) and phase inside the loop.
        let sr = 48000.0;
        let mut player = SamplePlayer::new(impulse_buffer(1, 8), sr, sr); // len 8
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        // Start knob at the very end so the loop span collapses to 1 sample.
        player.set_start(1.0);
        inputs.set(4, 5.0); // loop on
        inputs.set(2, 60.0); // +60 V/oct: rate = 2^60, wildly overshoots each tick

        trigger_once(&mut player, &mut inputs, &mut outputs);
        // Each tick must terminate quickly and keep phase within [0, len).
        for _ in 0..100 {
            player.tick(&inputs, &mut outputs);
            assert!(
                player.phase.is_finite()
                    && player.phase >= 0.0
                    && player.phase < player.len() as f64,
                "phase escaped the loop region: {}",
                player.phase
            );
            assert!(outputs.get(10).unwrap().is_finite());
        }
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

    // ---- Capture ----

    const IN: u32 = 0;
    const RECORD: u32 = 1;
    const TRIG: u32 = 2;
    const GATE: u32 = 3;
    const VOCT: u32 = 4;
    const LOOP: u32 = 5;

    /// Tick once with `set` applied to the inputs; returns `(out, eos)`.
    fn step(capture: &mut Capture, inputs: &mut PortValues, set: &[(u32, f64)]) -> (f64, f64) {
        for &(port, value) in set {
            inputs.set(port, value);
        }
        let mut outputs = PortValues::new();
        capture.tick(inputs, &mut outputs);
        (outputs.get(10).unwrap(), outputs.get(11).unwrap())
    }

    /// Record `samples` with one `record` gate.
    fn record(capture: &mut Capture, inputs: &mut PortValues, samples: &[f64]) {
        for &x in samples {
            step(capture, inputs, &[(IN, x), (RECORD, 5.0)]);
        }
        step(capture, inputs, &[(IN, 0.0), (RECORD, 0.0)]);
    }

    /// Fire `trig` and collect `n` outputs, starting with the triggering tick.
    fn play(capture: &mut Capture, inputs: &mut PortValues, n: usize) -> Vec<(f64, f64)> {
        step(capture, inputs, &[(TRIG, 0.0)]);
        let mut out = vec![step(capture, inputs, &[(TRIG, 5.0)])];
        out.extend((1..n).map(|_| step(capture, inputs, &[(TRIG, 0.0)])));
        out
    }

    fn ramp(n: usize) -> Vec<f64> {
        (0..n).map(|i| (i as f64 + 1.0) / 8.0).collect()
    }

    #[test]
    fn capture_records_then_plays_back_exactly() {
        let mut capture = Capture::new(48_000.0);
        let mut inputs = PortValues::new();
        assert!(capture.is_empty());
        assert_eq!(capture.capacity(), 4 * 48_000);

        // Nothing plays before anything is recorded.
        assert!(play(&mut capture, &mut inputs, 10)
            .iter()
            .all(|&(o, e)| o == 0.0 && e == 0.0));

        let take = ramp(32);
        record(&mut capture, &mut inputs, &take);
        assert_eq!(capture.len(), 32);
        assert!(!capture.is_recording());
        let as_f32: Vec<f32> = take.iter().map(|&x| x as f32).collect();
        assert_eq!(capture.recording(), &as_f32[..]);

        // One-shot: the take, once, then silence, with one end-of-sample trigger.
        let heard = play(&mut capture, &mut inputs, 40);
        let outs: Vec<f64> = heard.iter().map(|h| h.0).collect();
        let expected: Vec<f64> = as_f32.iter().map(|&s| s as f64).collect();
        assert_eq!(&outs[..32], &expected[..]);
        assert!(outs[32..].iter().all(|&o| o == 0.0));
        assert_eq!(heard.iter().filter(|h| h.1 > 0.0).count(), 1);
    }

    #[test]
    fn capture_loops_gates_and_pitches() {
        let mut capture = Capture::new(48_000.0);
        let mut inputs = PortValues::new();
        let take = ramp(8);
        record(&mut capture, &mut inputs, &take);

        // Loop: the take repeats, with an eos at each wrap.
        inputs.set(LOOP, 5.0);
        let heard = play(&mut capture, &mut inputs, 24);
        let outs: Vec<f64> = heard.iter().map(|h| h.0).collect();
        assert_eq!(&outs[..8], &outs[8..16]);
        assert_eq!(&outs[8..16], &outs[16..24]);
        assert_eq!(heard.iter().filter(|h| h.1 > 0.0).count(), 3);
        inputs.set(LOOP, 0.0);

        // Gate: plays while held, stops on release.
        step(&mut capture, &mut inputs, &[(GATE, 5.0)]);
        step(&mut capture, &mut inputs, &[]);
        let (out, _) = step(&mut capture, &mut inputs, &[(GATE, 0.0)]);
        assert_eq!(out, 0.0);

        // +1 V plays an octave up: every other sample.
        inputs.set(VOCT, 1.0);
        let outs: Vec<f64> = play(&mut capture, &mut inputs, 4)
            .iter()
            .map(|h| h.0)
            .collect();
        let every_other: Vec<f64> = [0, 2, 4, 6]
            .iter()
            .map(|&i| take[i] as f32 as f64)
            .collect();
        assert_eq!(outs, every_other);
    }

    #[test]
    fn capture_buffer_fills_and_rerecords_from_the_start() {
        let sr = 1_000.0;
        let mut capture = Capture::with_seconds(sr, 0.016); // 16 samples
        let mut inputs = PortValues::new();
        assert_eq!(capture.capacity(), 16);
        // Recording past the capacity stops at it.
        record(&mut capture, &mut inputs, &ramp(40));
        assert_eq!(capture.len(), 16);
        // A new recording replaces the old one.
        record(&mut capture, &mut inputs, &[0.5, -0.5, 0.25]);
        assert_eq!(capture.recording(), &[0.5, -0.5, 0.25]);
        // Non-finite input is recorded as silence.
        record(&mut capture, &mut inputs, &[f64::NAN, 1.0, f64::INFINITY]);
        assert_eq!(capture.recording(), &[0.0, 1.0, 0.0]);
        capture.clear();
        assert!(capture.is_empty());
        assert_eq!(
            Capture::with_seconds(sr, 1e9).capacity(),
            (Capture::MAX_SECONDS * sr) as usize
        );
    }

    #[test]
    fn capture_keeps_its_recording_rate_and_survives_reset() {
        let mut capture = Capture::new(48_000.0);
        let mut inputs = PortValues::new();
        record(&mut capture, &mut inputs, &ramp(8));
        // Played in a graph at twice the rate, the take runs at half the step.
        capture.set_sample_rate(96_000.0);
        assert_eq!(capture.recorded_sample_rate(), 48_000.0);
        let outs: Vec<f64> = play(&mut capture, &mut inputs, 3)
            .iter()
            .map(|h| h.0)
            .collect();
        assert_eq!(outs[0], 0.125);
        assert!(outs[1] > 0.125 && outs[1] < 0.25, "half-way: {}", outs[1]);
        assert_eq!(outs[2], 0.25);

        // reset() stops playback but keeps the take.
        capture.reset();
        assert_eq!(capture.len(), 8);
        assert_eq!(step(&mut capture, &mut inputs, &[]).0, 0.0);

        // A take loaded by the host (e.g. an audition clip) plays like a recorded one.
        capture.set_recording(&[0.5, f32::NAN, -0.5], 96_000.0);
        assert_eq!(capture.recording(), &[0.5, 0.0, -0.5]);
        let outs: Vec<f64> = play(&mut capture, &mut inputs, 4)
            .iter()
            .map(|h| h.0)
            .collect();
        assert_eq!(outs, [0.5, 0.0, -0.5, 0.0]);
        assert_eq!(capture.type_id(), "capture");
        assert_eq!(Capture::default().capacity(), (4.0 * 44_100.0) as usize);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn capture_state_round_trips_losslessly_and_rejects_bad_state() {
        let mut capture = Capture::with_seconds(48_000.0, 0.5);
        assert!(
            capture.serialize_state().is_none(),
            "an empty capture saves nothing"
        );
        let take: Vec<f32> = (0..1000).map(|i| ((i as f32) * 0.37).sin() * 3.3).collect();
        capture.set_recording(&take, 44_100.0);
        let state = capture.serialize_state().unwrap();
        assert_eq!(state["format"], "f32le-base64");
        assert_eq!(state["length"], 1000);

        let mut loaded = Capture::new(48_000.0);
        loaded.deserialize_state(&state).unwrap();
        assert_eq!(loaded.recording(), &take[..], "bit-exact");
        assert_eq!(loaded.recorded_sample_rate(), 44_100.0);
        assert_eq!(loaded.capacity(), capture.capacity());

        let broken = |edit: &dyn Fn(&mut serde_json::Value)| {
            let mut s = state.clone();
            edit(&mut s);
            Capture::new(48_000.0).deserialize_state(&s).unwrap_err()
        };
        assert!(broken(&|s| s["format"] = "wav".into()).contains("format"));
        assert!(broken(&|s| s["sample_rate"] = (-1.0).into()).contains("sample_rate"));
        assert!(broken(&|s| s["length"] = 1001.into()).contains("disagrees"));
        assert!(broken(&|s| s["length"] = u64::MAX.into()).contains("disagrees"));
        assert!(broken(&|s| s["capacity"] = (-3).into()).contains("capacity"));
        assert!(broken(&|s| s["sample_rate"] = 0.0.into()).contains("sample_rate"));
        assert!(broken(&|s| s["data"] = "not base64!".into()).contains("base64"));
        let nan = base64::encode(&f32::NAN.to_le_bytes());
        assert!(broken(&|s| {
            s["length"] = 1.into();
            s["data"] = nan.clone().into();
        })
        .contains("non-finite"));
    }

    /// Patch JSON is untrusted: a saved capacity is a request the loader clamps, so it
    /// cannot allocate memory the file's data does not back.
    #[test]
    #[cfg(feature = "alloc")]
    fn capture_state_cannot_allocate_beyond_its_data() {
        let mut small = Capture::with_seconds(44_100.0, 0.01); // 441 samples
        small.set_recording(&[0.25; 300], 44_100.0);
        let state = small.serialize_state().unwrap();
        let with = |edit: &dyn Fn(&mut serde_json::Value)| {
            let mut s = state.clone();
            edit(&mut s);
            s
        };

        // A huge capacity with a tiny take: the buffer stays at what the module had.
        for capacity in [(Capture::MAX_SECONDS * 44_100.0) as u64 + 1, u64::MAX] {
            let huge = with(&|s| s["capacity"] = capacity.into());
            let mut loaded = Capture::new(48_000.0);
            loaded.deserialize_state(&huge).unwrap();
            assert_eq!(
                loaded.capacity(),
                4 * 48_000,
                "no growth past the default buffer"
            );
            assert_eq!(loaded.len(), 300);
            // ...and a module built smaller grows only to the data.
            let mut tiny = Capture::with_seconds(48_000.0, 0.001); // 48 samples
            tiny.deserialize_state(&huge).unwrap();
            assert_eq!(tiny.capacity(), 300);
        }
        // A capacity below the take is raised to it.
        let mut loaded = Capture::new(48_000.0);
        loaded
            .deserialize_state(&with(&|s| s["capacity"] = 10.into()))
            .unwrap();
        assert_eq!((loaded.capacity(), loaded.len()), (300, 300));

        // A huge rate does not unlock a bigger buffer either.
        let fast = with(&|s| {
            s["sample_rate"] = 1e12.into();
            s["capacity"] = u64::MAX.into();
        });
        let mut loaded = Capture::new(48_000.0);
        loaded.deserialize_state(&fast).unwrap();
        assert_eq!(loaded.capacity(), 4 * 48_000);
        assert_eq!(loaded.recorded_sample_rate(), 1e12);
    }

    /// Every state a Capture can save loads back, bit-exact: the reviewer's three cases.
    #[test]
    #[cfg(feature = "alloc")]
    fn capture_loads_every_state_it_can_save() {
        let round_trip = |capture: &Capture| {
            let state = capture.serialize_state().unwrap();
            // Through JSON text, as a saved patch is.
            let text = serde_json::to_string(&state).unwrap();
            let state: serde_json::Value = serde_json::from_str(&text).unwrap();
            let mut loaded = Capture::new(48_000.0);
            loaded.deserialize_state(&state).unwrap();
            assert_eq!(loaded.recording(), capture.recording(), "bit-exact");
            assert_eq!(
                loaded.recorded_sample_rate(),
                capture.recorded_sample_rate()
            );
            loaded
        };

        // A: 60 s built at 96 kHz (5.76 M samples), run in a 48 kHz graph, a 0.1 s take.
        // Its saved capacity is 120 s at the take's rate.
        let mut a = Capture::with_seconds(96_000.0, 60.0);
        a.set_sample_rate(48_000.0);
        let mut inputs = PortValues::new();
        let take: Vec<f64> = (0..4_800).map(|i| (i as f64 * 0.01).sin()).collect();
        record(&mut a, &mut inputs, &take);
        assert_eq!((a.len(), a.recorded_sample_rate()), (4_800, 48_000.0));
        assert_eq!(a.serialize_state().unwrap()["capacity"], 5_760_000);
        let loaded = round_trip(&a);
        assert_eq!(
            loaded.capacity(),
            4 * 48_000,
            "the default buffer, not 5.76 M"
        );

        // B: a 70 s take at 48 kHz, longer than MAX_SECONDS.
        let mut b = Capture::new(48_000.0);
        let long: Vec<f32> = (0..70 * 48_000)
            .map(|i| ((i % 997) as f32) / 997.0)
            .collect();
        b.set_recording(&long, 48_000.0);
        assert_eq!(b.len(), 70 * 48_000);
        assert_eq!(round_trip(&b).len(), 70 * 48_000);

        // C: 768 kHz, from the host and from recording in a 768 kHz graph.
        let mut c = Capture::new(48_000.0);
        c.set_recording(&[0.5, -0.25, 0.125], 768_000.0);
        round_trip(&c);
        let mut c = Capture::with_seconds(768_000.0, 0.01);
        record(&mut c, &mut inputs, &[0.75, -0.75]);
        assert_eq!(round_trip(&c).recorded_sample_rate(), 768_000.0);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn capture_base64_matches_rfc_4648() {
        for (raw, text) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64::encode(raw), text);
            assert_eq!(base64::decode(text).unwrap(), raw);
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(base64::decode(&base64::encode(&all)).unwrap(), all);
        for bad in ["Zg=", "Z===", "Zg==Zg==", "Zm9*", "===="] {
            assert!(base64::decode(bad).is_none(), "{bad}");
        }
    }
}
