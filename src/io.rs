//! External I/O Integration
//!
//! This module provides components for bridging the patch graph with
//! external systems: MIDI controllers, audio interfaces, etc.
//!
//! - [`ExternalInput`] / [`ExternalOutput`] carry one control value across threads
//!   through an [`AtomicF64`], read or written once per tick.
//! - [`AudioInput`] brings a host's **audio** in: the host writes a block of samples per
//!   channel into an [`AudioInputStream`] before each process call, and every
//!   `AudioInput` reading that stream plays it back one frame per tick.
//! - [`MidiState`] turns raw MIDI bytes into the atomics `ExternalInput` reads.

use crate::introspection::{ModuleIntrospection, ParamInfo};
use crate::port::{
    BlockInputs, BlockOutputs, GraphModule, PortDef, PortSpec, PortValues, SignalKind,
};
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{fence, AtomicU32, AtomicUsize, Ordering};
// `AtomicU64` from `portable-atomic`, not `core`: the latter is absent on
// targets with `max_atomic_width < 64` (e.g. `thumbv7em-none-eabihf`).
use portable_atomic::AtomicU64;

/// Atomic f64 for lock-free communication between threads
///
/// Uses AtomicU64 internally since there's no native AtomicF64.
/// Suitable for real-time audio thread communication.
#[derive(Debug)]
pub struct AtomicF64(AtomicU64);

impl AtomicF64 {
    /// Create a new atomic f64 with the given initial value
    pub fn new(value: f64) -> Self {
        Self(AtomicU64::new(value.to_bits()))
    }

    /// Get the current value
    pub fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }

    /// Set a new value
    pub fn set(&self, value: f64) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }

    /// Load with specified ordering
    pub fn load(&self, ordering: Ordering) -> f64 {
        f64::from_bits(self.0.load(ordering))
    }

    /// Store with specified ordering
    pub fn store(&self, value: f64, ordering: Ordering) {
        self.0.store(value.to_bits(), ordering);
    }
}

impl Default for AtomicF64 {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl Clone for AtomicF64 {
    fn clone(&self) -> Self {
        Self::new(self.get())
    }
}

/// A note event (pitch + gate) published atomically in a single 64-bit word.
///
/// MIDI note-on must hand a *coherent* (pitch, gate) pair from the MIDI thread
/// to the audio thread. Writing pitch and gate as two independent atomics with
/// `Relaxed` ordering lets the audio thread observe a new gate paired with a
/// stale pitch (a wrong-pitch transient on note changes). Packing both into one
/// `AtomicU64` and reading it in a single load removes the tear for consumers of
/// this type (i.e. [`snapshot`](Self::snapshot) / [`MidiState::note_snapshot`]):
///
/// - the high 32 bits hold `pitch` (V/Oct) as `f32` bits,
/// - the low 32 bits hold `gate` (volts) as `f32` bits.
///
/// Writers use [`publish`](Self::publish) (`Ordering::Release`); readers use
/// [`snapshot`](Self::snapshot) (`Ordering::Acquire`). The release/acquire pair
/// establishes a happens-before edge so a reader that observes a gate value also
/// observes the matching pitch published in the same word.
#[derive(Debug)]
pub struct AtomicNote(AtomicU64);

impl AtomicNote {
    /// Create a new packed note event.
    pub fn new(pitch: f64, gate: f64) -> Self {
        Self(AtomicU64::new(Self::pack(pitch, gate)))
    }

    #[inline]
    fn pack(pitch: f64, gate: f64) -> u64 {
        (((pitch as f32).to_bits() as u64) << 32) | ((gate as f32).to_bits() as u64)
    }

    #[inline]
    fn unpack(bits: u64) -> (f64, f64) {
        let pitch = f32::from_bits((bits >> 32) as u32) as f64;
        let gate = f32::from_bits(bits as u32) as f64;
        (pitch, gate)
    }

    /// Publish a coherent `(pitch, gate)` pair with `Release` ordering.
    #[inline]
    pub fn publish(&self, pitch: f64, gate: f64) {
        self.0.store(Self::pack(pitch, gate), Ordering::Release);
    }

    /// Load a coherent `(pitch, gate)` snapshot with `Acquire` ordering.
    ///
    /// The returned pair is never torn: pitch and gate always come from the same
    /// [`publish`](Self::publish) call.
    #[inline]
    pub fn snapshot(&self) -> (f64, f64) {
        Self::unpack(self.0.load(Ordering::Acquire))
    }
}

impl Default for AtomicNote {
    fn default() -> Self {
        Self::new(0.0, 0.0)
    }
}

impl Clone for AtomicNote {
    fn clone(&self) -> Self {
        let (pitch, gate) = self.snapshot();
        Self::new(pitch, gate)
    }
}

/// External input source - reads from an atomic value set by another thread
///
/// This module allows values from external sources (MIDI, OSC, GUI, etc.)
/// to be brought into the patch graph in a lock-free manner.
pub struct ExternalInput {
    value: Arc<AtomicF64>,
    spec: PortSpec,
}

impl ExternalInput {
    /// Create a new external input with the specified signal kind
    pub fn new(value: Arc<AtomicF64>, kind: SignalKind) -> Self {
        Self {
            value,
            spec: PortSpec {
                inputs: vec![],
                outputs: vec![PortDef::new(0, "out", kind)],
            },
        }
    }

    /// Create for pitch CV (V/Oct)
    pub fn voct(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::VoltPerOctave)
    }

    /// Create for gate signals
    pub fn gate(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::Gate)
    }

    /// Create for unipolar CV
    pub fn cv(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::CvUnipolar)
    }

    /// Create for bipolar CV
    pub fn cv_bipolar(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::CvBipolar)
    }

    /// Create for trigger signals
    pub fn trigger(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::Trigger)
    }

    /// Create for audio input
    pub fn audio(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::Audio)
    }

    /// Get a reference to the underlying atomic value
    pub fn value_ref(&self) -> &Arc<AtomicF64> {
        &self.value
    }
}

impl GraphModule for ExternalInput {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, _inputs: &PortValues, outputs: &mut PortValues) {
        outputs.set(0, self.value.get());
    }

    /// Reads the value once per frame, as `tick` does.
    fn tick_frames(
        &mut self,
        _inputs: &BlockInputs<'_>,
        outputs: &mut BlockOutputs<'_>,
        _wanted: u32,
    ) -> bool {
        if outputs.len() != 1 {
            return false;
        }
        for value in outputs.port(0) {
            *value = self.value.get();
        }
        true
    }

    /// Reads a cell the host, or an `ExternalOutput` of the same patch, may write.
    fn shares_state(&self) -> Option<crate::port::SharedState> {
        Some(crate::port::SharedState::reads(Arc::as_ptr(&self.value)))
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "external_input"
    }
}

// =============================================================================
// Block-fed audio input
// =============================================================================

/// Block slots in an [`AudioInputStream`]: the block readers are on, and the one the
/// writer fills next.
const AUDIO_INPUT_SLOTS: usize = 2;

/// A block of host audio, written by the host and read by any number of [`AudioInput`]s.
///
/// The host holds an `Arc<AudioInputStream>` and, **before each process call**, writes the
/// block of input it has for that call with [`write`](Self::write) (planar, like
/// [`PluginProcessor::process`](crate::extended_io::PluginProcessor::process)) or
/// [`write_interleaved`](Self::write_interleaved) (like Web Audio and most native drivers).
/// Every [`AudioInput`] built on the stream then plays that block back, one frame per tick.
///
/// ```
/// use std::sync::Arc;
/// use quiver::prelude::*;
///
/// let input = Arc::new(AudioInputStream::new(2, 512)); // stereo, up to 512 frames a block
/// let mut patch = Patch::new(48_000.0);
/// let mic = patch.add("mic", AudioInput::new(Arc::clone(&input)));
/// let out = patch.add("out", StereoOutput::new());
/// patch.connect(mic.out("left"), out.in_("left")).unwrap();
/// patch.connect(mic.out("right"), out.in_("right")).unwrap();
/// patch.set_output(out.id());
/// patch.compile().unwrap();
///
/// // In the audio callback: write this call's input, then render the same frames.
/// let (in_l, in_r) = ([0.5f32; 128], [-0.25f32; 128]);
/// let (mut out_l, mut out_r) = ([0.0; 128], [0.0; 128]);
/// input.write(&[&in_l[..], &in_r[..]]);
/// patch.tick_block(&mut out_l, &mut out_r);
/// assert_eq!(out_l[0], 0.5 * AudioInput::FULL_SCALE_VOLTS);
/// assert_eq!(out_r[127], -0.25 * AudioInput::FULL_SCALE_VOLTS);
/// ```
///
/// # Why a shared block, not a ring buffer
///
/// One capture fans out: the same input may feed several `AudioInput` nodes, in one patch
/// or in several (one per polyphonic voice), and a host may render those patches one after
/// another over the same block. A single-consumer FIFO hands each frame to one reader, so
/// the first voice to render would consume the frames the next one needs. Here the block
/// stays put and every reader reads it.
///
/// # Two ways to read
///
/// How a reader finds its place in the block depends on how the host renders:
///
/// - **Voice-major hosts: [`new`](Self::new).** Each reader keeps its own cursor and starts
///   every new block at its first frame. Readers stay in step as long as each one starts
///   or resumes at a **block boundary**: one patch ticked frame by frame (all its readers
///   move together), several patches each rendering the whole block in turn, a voice that
///   sits out whole blocks (quiver's WASM engine; Auracle's live voice bank). A reader
///   that resumes *mid-block* starts the block over (late) and drops its end, and one
///   built mid-block is silent until the next block.
/// - **Frame-by-frame hosts: [`with_host_clock`](Self::with_host_clock).** The host names
///   the current frame, with [`advance`](Self::advance) after each rendered frame (or
///   [`set_frame`](Self::set_frame)), and every reader reads that frame, however late it
///   joined. Use it when voices are ticked sample by sample and may start or resume
///   mid-block ([`PolyPatch`](crate::polyphony::PolyPatch) skips free voices; an offline
///   render that compiles voices at note onsets), and for clips: write a whole clip as one
///   block (capacity = clip length) and `advance` once per rendered sample.
///
/// # Timing rules
///
/// - **Underrun gives silence.** A reader that has used up the block (the engine rendered
///   more frames than the host wrote), or whose host frame is past its end, or that has
///   had no block yet, outputs `0.0`. It never repeats a block.
/// - **Overrun drops the unread frames.** When the host writes a new block before a reader
///   has finished the current one, the reader moves to the new block at its first frame,
///   so input latency never grows past one block. A write longer than
///   [`capacity`](Self::capacity) keeps the first `capacity` frames.
/// - **Cursor readers hear the blocks written after they exist.** A newly built or
///   [`reset`](GraphModule::reset) `AudioInput` on a [`new`](Self::new) stream treats the
///   current block as consumed and starts with the next one, so it cannot replay stale
///   input. Build the patch, then write each block just before the frames that should
///   hear it. (Host-clock readers have no position of their own: they read the host's
///   frame of the newest block from their first tick.)
/// - Frames pass through one per tick at whatever rate the host delivers; resampling to
///   the engine's rate, if they differ, is the host's job.
///
/// # Real-time safety
///
/// All storage is allocated in [`new`](Self::new). Writing, reading and the host clock are
/// lock-free and allocation-free. The usual host writes and renders on the same thread (an
/// audio callback, an `AudioWorklet`'s `process()`, an offline render loop), where every
/// reader sees exactly the frames written. Writing from another thread is also sound:
/// blocks are double-buffered and published with release/acquire ordering, and a reader
/// that the writer laps mid-read discards the sample (outputs `0.0`) rather than return a
/// torn one. Cursor readers on different threads can be a frame apart around the moment
/// the stream runs dry; readers on the rendering thread are sample-exact.
///
/// There must be **one writer**. Writes from two threads at once are memory-safe but can
/// mix samples of both blocks within one block, and interleave their blocks.
pub struct AudioInputStream {
    channels: usize,
    capacity: usize,
    /// `AUDIO_INPUT_SLOTS` blocks of `capacity` frames, each frame `channels` interleaved
    /// `f32` bit patterns. Block `seq` lives in slot `seq % AUDIO_INPUT_SLOTS`.
    samples: Box<[AtomicU32]>,
    /// Frame count of the block in each slot.
    lens: [AtomicUsize; AUDIO_INPUT_SLOTS],
    /// Sequence number of the newest complete block (`0` before the first write).
    published: AtomicU32,
    /// Sequence number of the block the writer is filling, announced before its first
    /// sample is stored; equal to `published` between writes. A reader of block `b` that
    /// sees `writing - b >= AUDIO_INPUT_SLOTS` after its read may have read a sample of the
    /// block that reuses `b`'s slot, and drops it.
    writing: AtomicU32,
    /// Readers read the frame the host sets ([`with_host_clock`](Self::with_host_clock))
    /// rather than keeping their own cursors.
    host_clock: bool,
    /// The host's current frame within the newest block (host-clock streams only).
    frame: AtomicUsize,
}

impl AudioInputStream {
    /// Create a stream of `channels` channels holding blocks of up to `capacity` frames.
    ///
    /// `channels` is clamped to at least 1. Choose `capacity` to cover the largest block the
    /// host will write in one call (Web Audio's render quantum is 128 frames; native hosts
    /// commonly use 64–2048). This is the only allocation the stream makes.
    pub fn new(channels: usize, capacity: usize) -> Self {
        let channels = channels.max(1);
        let len = AUDIO_INPUT_SLOTS * capacity * channels;
        Self {
            channels,
            capacity,
            samples: (0..len).map(|_| AtomicU32::new(0)).collect(),
            lens: [AtomicUsize::new(0), AtomicUsize::new(0)],
            published: AtomicU32::new(0),
            writing: AtomicU32::new(0),
            host_clock: false,
            frame: AtomicUsize::new(0),
        }
    }

    /// Like [`new`](Self::new), for a **frame-by-frame** host: every reader reads the frame
    /// the host names with [`advance`](Self::advance) or [`set_frame`](Self::set_frame),
    /// whenever it joined. See [Two ways to read](Self#two-ways-to-read).
    ///
    /// Two rules come with the clock:
    ///
    /// - **Render frame by frame**, with `Patch::tick` / `PolyPatch::tick`, and call
    ///   `advance` between frames. `Patch::tick_block` renders a whole block without
    ///   returning to the host, so the clock stays put and every sample of it reads the
    ///   same frame.
    /// - **Keep `write`, `advance` and `set_frame` on the rendering thread.** The clock is
    ///   a plain position, not a queue; moved from another thread it would race the
    ///   readers (memory-safe, but they would read whichever frame they happened to see).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use quiver::prelude::*;
    ///
    /// // An offline render: the whole clip is one block, read at the host's frame.
    /// let clip: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
    /// let input = Arc::new(AudioInputStream::with_host_clock(1, clip.len()));
    /// input.write(&[&clip[..]]);
    ///
    /// let (inputs, mut outputs) = (PortValues::new(), PortValues::new());
    /// let mut early = AudioInput::new(Arc::clone(&input));
    /// for _ in 0..500 {
    ///     early.tick(&inputs, &mut outputs);
    ///     input.advance();
    /// }
    /// // A voice that starts at frame 500 is in step with the one that started at 0.
    /// let mut late = AudioInput::new(Arc::clone(&input));
    /// early.tick(&inputs, &mut outputs);
    /// let a = outputs.get(11).unwrap();
    /// late.tick(&inputs, &mut outputs);
    /// assert_eq!(outputs.get(11).unwrap(), a);
    /// assert_eq!(a, 0.5 * AudioInput::FULL_SCALE_VOLTS);
    /// ```
    pub fn with_host_clock(channels: usize, capacity: usize) -> Self {
        Self {
            host_clock: true,
            ..Self::new(channels, capacity)
        }
    }

    /// Whether readers follow the host's frame ([`with_host_clock`](Self::with_host_clock))
    /// rather than their own cursors.
    pub fn is_host_clocked(&self) -> bool {
        self.host_clock
    }

    /// Host clock: make `frame` (within the block last written) the frame every reader
    /// reads next. Frames at or past the block's end read silence. No effect on a stream
    /// built with [`new`](Self::new).
    #[inline]
    pub fn set_frame(&self, frame: usize) {
        if self.host_clock {
            self.frame.store(frame, Ordering::Relaxed);
        }
    }

    /// Host clock: move every reader to the next frame. Call it once after each frame is
    /// rendered (each [`write`](Self::write) starts again at frame 0). No effect on a
    /// stream built with [`new`](Self::new).
    #[inline]
    pub fn advance(&self) {
        if self.host_clock {
            // One writer, so a load and a store; no read-modify-write (which targets
            // without compare-and-swap lack).
            let next = self.frame.load(Ordering::Relaxed).saturating_add(1);
            self.frame.store(next, Ordering::Relaxed);
        }
    }

    /// Host clock: the frame readers read next (always 0 on a stream built with
    /// [`new`](Self::new)).
    pub fn frame(&self) -> usize {
        self.frame.load(Ordering::Relaxed)
    }

    /// Number of channels per frame.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Most frames one block can hold; longer writes are truncated to this.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Write a block of planar input: one slice per channel. Returns the frames written.
    ///
    /// The block length is the shortest slice, capped at [`capacity`](Self::capacity).
    /// A single slice is mono and fills every channel. Otherwise slice `c` fills channel
    /// `c`; channels without a slice are silent and slices past the stream's channel count
    /// are ignored. No slices publishes an empty block, like [`clear`](Self::clear).
    /// Non-finite samples are written as silence.
    pub fn write<S: AsRef<[f32]>>(&self, channels: &[S]) -> usize {
        let frames = channels
            .iter()
            .take(self.channels)
            .map(|c| c.as_ref().len())
            .min()
            .unwrap_or(0);
        let mono = channels.len() == 1;
        self.publish(frames, |frame, ch| {
            let source = if mono { 0 } else { ch };
            channels.get(source).map_or(0.0, |c| c.as_ref()[frame])
        })
    }

    /// Write a block of interleaved input with `source_channels` samples per frame.
    /// Returns the frames written.
    ///
    /// Channels map as in [`write`](Self::write): a mono source fills every channel, extra
    /// source channels are ignored, missing ones are silent. A trailing partial frame is
    /// dropped; `source_channels == 0` publishes an empty block.
    pub fn write_interleaved(&self, samples: &[f32], source_channels: usize) -> usize {
        if source_channels == 0 {
            return self.publish(0, |_, _| 0.0);
        }
        let frames = samples.len() / source_channels;
        self.publish(frames, |frame, ch| {
            let source = if source_channels == 1 { 0 } else { ch };
            if source < source_channels {
                samples[frame * source_channels + source]
            } else {
                0.0
            }
        })
    }

    /// Publish an empty block: every reader falls silent at once.
    ///
    /// Use it when the input goes away (a device is unplugged, a capture stops), so no
    /// reader finishes the last block late.
    pub fn clear(&self) {
        self.publish(0, |_, _| 0.0);
    }

    /// Store `frames` frames from `sample(frame, channel)` as the next block and publish it.
    fn publish(&self, frames: usize, mut sample: impl FnMut(usize, usize) -> f32) -> usize {
        let frames = frames.min(self.capacity);
        // Single writer: `published` is our own last store.
        let next = self.published.load(Ordering::Relaxed).wrapping_add(1);
        // Announce the block before touching its slot. The release fence orders this store
        // before every sample store below, so a reader that observes any of those samples
        // and then (after its acquire fence) loads `writing` sees `next` or later.
        self.writing.store(next, Ordering::Relaxed);
        fence(Ordering::Release);

        let slot = next as usize % AUDIO_INPUT_SLOTS;
        let base = slot * self.capacity * self.channels;
        for frame in 0..frames {
            for ch in 0..self.channels {
                let value = sample(frame, ch);
                let value = if value.is_finite() { value } else { 0.0 };
                self.samples[base + frame * self.channels + ch]
                    .store(value.to_bits(), Ordering::Relaxed);
            }
        }
        self.lens[slot].store(frames, Ordering::Relaxed);
        if self.host_clock {
            self.frame.store(0, Ordering::Relaxed);
        }
        // Release: a reader that acquires `next` sees every sample, the length and the
        // frame reset above.
        self.published.store(next, Ordering::Release);
        frames
    }

    /// Sequence number of the newest complete block.
    #[inline]
    fn newest(&self) -> u32 {
        self.published.load(Ordering::Acquire)
    }

    /// Frame count of block `seq`, which the caller acquired through [`newest`](Self::newest).
    #[inline]
    fn block_len(&self, seq: u32) -> usize {
        self.lens[seq as usize % AUDIO_INPUT_SLOTS]
            .load(Ordering::Relaxed)
            .min(self.capacity)
    }

    /// Read frame `frame` of block `seq` as `[left, right, mean of all channels]`, or `None`
    /// if the writer began overwriting the block's slot while it was being read.
    #[inline]
    fn read_frame(&self, seq: u32, frame: usize) -> Option<[f64; 3]> {
        let slot = seq as usize % AUDIO_INPUT_SLOTS;
        let at = (slot * self.capacity + frame) * self.channels;
        let load = |ch: usize| f32::from_bits(self.samples[at + ch].load(Ordering::Relaxed)) as f64;

        let left = load(0);
        let (right, mean) = match self.channels {
            1 => (left, left),
            2 => {
                let right = load(1);
                (right, (left + right) * 0.5)
            }
            n => {
                let mut sum = left;
                for ch in 1..n {
                    sum += load(ch);
                }
                (load(1), sum / n as f64)
            }
        };

        // Seqlock validation: had the writer started the block that reuses this slot, the
        // values above may mix two blocks.
        fence(Ordering::Acquire);
        let writing = self.writing.load(Ordering::Relaxed);
        if writing.wrapping_sub(seq) >= AUDIO_INPUT_SLOTS as u32 {
            return None;
        }
        Some([left, right, mean])
    }
}

impl core::fmt::Debug for AudioInputStream {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AudioInputStream")
            .field("channels", &self.channels)
            .field("capacity", &self.capacity)
            .field("published", &self.published.load(Ordering::Relaxed))
            .field("host_clock", &self.host_clock)
            .field("frame", &self.frame())
            .finish()
    }
}

/// Which input channel an [`AudioInput`]'s `out` port carries.
///
/// Non-exhaustive: more channel choices may come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum InputChannel {
    /// Channel 0.
    Left,
    /// Channel 1 (channel 0 on a mono stream).
    Right,
    /// Every channel mixed to mono at equal gain: `(left + right) / 2` on a stereo stream,
    /// so a centred or mono-duplicated source keeps its level.
    #[default]
    Both,
}

impl InputChannel {
    /// The `channel` parameter value: `0` left, `1` right, `2` both.
    pub fn index(self) -> usize {
        match self {
            InputChannel::Left => 0,
            InputChannel::Right => 1,
            InputChannel::Both => 2,
        }
    }

    /// Inverse of [`index`](Self::index); the value is rounded, and anything out of range
    /// or not finite is `None`.
    pub fn from_index(value: f64) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        match libm::round(value) as i64 {
            0 => Some(InputChannel::Left),
            1 => Some(InputChannel::Right),
            2 => Some(InputChannel::Both),
            _ => None,
        }
    }
}

/// Audio input: plays the host's audio, written block by block into an
/// [`AudioInputStream`], into the patch.
///
/// Each tick reads a frame of the block the host last wrote (see the stream's
/// [timing rules](AudioInputStream#timing-rules): silence on underrun, unread frames
/// dropped on overrun). Any number of `AudioInput`s may share one stream, in one patch or
/// in many. Which frame a tick reads depends on the stream: on a
/// [`new`](AudioInputStream::new) stream each input keeps its own cursor, which keeps
/// voice-major hosts (each voice renders whole blocks) in step; on a
/// [`with_host_clock`](AudioInputStream::with_host_clock) stream every input reads the
/// frame the host names, which keeps frame-by-frame hosts in step when voices start or
/// resume mid-block. See [Two ways to read](AudioInputStream#two-ways-to-read).
///
/// # Ports
///
/// | Port | Kind | |
/// |---|---|---|
/// | `gain` (in) | CV unipolar | Linear gain, default `1.0`, clamped to `0..=`[`MAX_GAIN`](Self::MAX_GAIN) |
/// | `out` | Audio | The channel picked by the `channel` parameter |
/// | `left` | Audio | Channel 0 |
/// | `right` | Audio | Channel 1, or channel 0 on a mono stream |
///
/// The `channel` parameter (an introspection `select`: `0` left, `1` right, `2` both,
/// default both) chooses what `out` carries; see [`InputChannel`].
///
/// # Level
///
/// Host audio is full-scale `±1.0`; quiver's audio is `±5 V`. An `AudioInput` scales by
/// [`FULL_SCALE_VOLTS`](Self::FULL_SCALE_VOLTS), so a full-scale input drives filters,
/// followers and gates exactly as a full-scale oscillator does. (The WASM engine's older
/// `audio_in` [`ExternalInput`] passes samples through unscaled.)
///
/// # Serialization
///
/// `channel` and the `gain` knob serialize like any other parameter; the stream cannot,
/// so a loaded `audio_input` reads whatever stream its [`ModuleRegistry`] was given
/// ([`ModuleRegistry::register_audio_input`]), or nothing (silence) by default.
///
/// [`ModuleRegistry`]: crate::serialize::ModuleRegistry
/// [`ModuleRegistry::register_audio_input`]: crate::serialize::ModuleRegistry::register_audio_input
pub struct AudioInput {
    stream: Arc<AudioInputStream>,
    channel: InputChannel,
    /// Sequence number of the block this reader is on.
    seq: u32,
    /// Next frame to read in that block.
    pos: usize,
    /// Frames in that block; `pos >= len` means it is used up.
    len: usize,
    spec: PortSpec,
}

impl AudioInput {
    /// Volts that a full-scale (`±1.0`) host sample becomes: quiver's audio peak.
    pub const FULL_SCALE_VOLTS: f64 = 5.0;

    /// Upper bound of the `gain` input (+12 dB).
    pub const MAX_GAIN: f64 = 4.0;

    /// Read `stream`, starting with the next block the host writes. `out` carries
    /// [`InputChannel::Both`] until [`set_channel`](Self::set_channel) says otherwise.
    pub fn new(stream: Arc<AudioInputStream>) -> Self {
        let seq = stream.newest();
        Self {
            stream,
            channel: InputChannel::default(),
            seq,
            pos: 0,
            len: 0,
            spec: PortSpec {
                inputs: vec![PortDef::new(0, "gain", SignalKind::CvUnipolar).with_default(1.0)],
                outputs: vec![
                    PortDef::new(10, "out", SignalKind::Audio),
                    PortDef::new(11, "left", SignalKind::Audio),
                    PortDef::new(12, "right", SignalKind::Audio),
                ],
            },
        }
    }

    /// An input on a stream nothing can write to: silent until replaced. This is what a
    /// [`ModuleRegistry`](crate::serialize::ModuleRegistry) builds for `audio_input` when
    /// no host stream was registered.
    pub fn unbound() -> Self {
        Self::new(Arc::new(AudioInputStream::new(2, 0)))
    }

    /// Builder form of [`set_channel`](Self::set_channel).
    pub fn with_channel(mut self, channel: InputChannel) -> Self {
        self.channel = channel;
        self
    }

    /// Choose what the `out` port carries.
    pub fn set_channel(&mut self, channel: InputChannel) {
        self.channel = channel;
    }

    /// What the `out` port carries.
    pub fn channel(&self) -> InputChannel {
        self.channel
    }

    /// The stream this input reads.
    pub fn stream(&self) -> &Arc<AudioInputStream> {
        &self.stream
    }

    /// Advance one frame: `[left, right, mean]` of the next frame, or `None` (silence).
    #[inline]
    fn next_frame(&mut self) -> Option<[f64; 3]> {
        let newest = self.stream.newest();
        if self.stream.host_clock {
            // The host names the frame; this reader keeps no position of its own.
            let frame = self.stream.frame();
            if frame >= self.stream.block_len(newest) {
                return None;
            }
            return self.stream.read_frame(newest, frame);
        }
        if newest != self.seq {
            // A new block: start it at its first frame, abandoning any unread rest of the
            // previous one (overrun).
            self.seq = newest;
            self.pos = 0;
            self.len = self.stream.block_len(newest);
        }
        if self.pos >= self.len {
            return None; // underrun
        }
        let frame = self.pos;
        self.pos += 1;
        let read = self.stream.read_frame(self.seq, frame);
        if read.is_none() {
            // Lapped by a writer on another thread: block `seq + 1` is published and
            // `seq + 2` is being written into this one's slot. The next tick moves to the
            // newest published block.
            self.pos = self.len;
        }
        read
    }
}

impl GraphModule for AudioInput {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let gain = inputs.get_or(0, 1.0);
        let gain = if gain.is_finite() {
            gain.clamp(0.0, Self::MAX_GAIN)
        } else {
            0.0
        };
        let scale = gain * Self::FULL_SCALE_VOLTS;
        let [left, right, mean] = self.next_frame().unwrap_or([0.0; 3]);
        let out = match self.channel {
            InputChannel::Left => left,
            InputChannel::Right => right,
            InputChannel::Both => mean,
        };
        outputs.set(10, out * scale);
        outputs.set(11, left * scale);
        outputs.set(12, right * scale);
    }

    /// Discard the rest of the current block: the input resumes with the next block the
    /// host writes.
    fn reset(&mut self) {
        self.seq = self.stream.newest();
        self.pos = 0;
        self.len = 0;
    }

    /// Frames pass through at the host's rate; nothing here depends on the sample rate.
    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "audio_input"
    }

    crate::impl_introspect!();
}

impl ModuleIntrospection for AudioInput {
    fn param_infos(&self) -> Vec<ParamInfo> {
        vec![ParamInfo::select("channel", "Channel", 3)
            .with_default(InputChannel::default().index() as f64)
            .with_value(self.channel.index() as f64)]
    }

    fn set_param_by_id(&mut self, id: &str, value: f64) -> bool {
        match (id, InputChannel::from_index(value)) {
            ("channel", Some(channel)) => {
                self.channel = channel;
                true
            }
            _ => false,
        }
    }
}

/// MIDI state that can be updated from a MIDI thread
///
/// This structure holds atomic values for common MIDI controllers.
/// Update from a MIDI callback thread, read from the audio thread.
#[derive(Debug)]
pub struct MidiState {
    /// Pitch in V/Oct (0V = C4, MIDI note 60).
    ///
    /// Read per-field with `Relaxed` ([`AtomicF64::get`]). This is a convenience
    /// mirror: reading `pitch` and [`gate`](Self::gate) as two separate atomics
    /// is **torn-capable** — across a note change a reader can observe a new gate
    /// paired with the previous pitch. For a coherent `(pitch, gate)` pair use
    /// [`note_snapshot`](Self::note_snapshot), which reads the packed
    /// [`AtomicNote`] and never tears.
    pub pitch: Arc<AtomicF64>,

    /// Gate signal (0 or 5V).
    ///
    /// A `Relaxed` convenience mirror; see [`pitch`](Self::pitch) for why a
    /// `(pitch, gate)` pair read from these two fields is torn-capable and why
    /// [`note_snapshot`](Self::note_snapshot) is the coherent alternative.
    pub gate: Arc<AtomicF64>,

    /// Velocity (0-10V)
    pub velocity: Arc<AtomicF64>,

    /// Mod wheel (0-10V)
    pub mod_wheel: Arc<AtomicF64>,

    /// Pitch bend (±semitones as V/Oct)
    pub pitch_bend: Arc<AtomicF64>,

    /// Channel aftertouch (0-10V)
    pub aftertouch: Arc<AtomicF64>,

    /// Sustain pedal (0 or 5V)
    pub sustain: Arc<AtomicF64>,

    /// Expression pedal (0-10V)
    pub expression: Arc<AtomicF64>,

    /// Coherent, torn-free (pitch, gate) note event.
    ///
    /// Published atomically on every note-on/off so the audio thread never sees
    /// a new gate paired with a stale pitch. Prefer [`MidiState::note_snapshot`]
    /// over reading `pitch`/`gate` separately when a coherent pair matters.
    pub note: Arc<AtomicNote>,

    // Internal state for note handling
    held_notes: Vec<u8>,
}

impl MidiState {
    /// Create a new MIDI state with all values at zero
    pub fn new() -> Self {
        Self {
            pitch: Arc::new(AtomicF64::new(0.0)),
            gate: Arc::new(AtomicF64::new(0.0)),
            velocity: Arc::new(AtomicF64::new(0.0)),
            mod_wheel: Arc::new(AtomicF64::new(0.0)),
            pitch_bend: Arc::new(AtomicF64::new(0.0)),
            aftertouch: Arc::new(AtomicF64::new(0.0)),
            sustain: Arc::new(AtomicF64::new(0.0)),
            expression: Arc::new(AtomicF64::new(10.0)),
            note: Arc::new(AtomicNote::new(0.0, 0.0)),
            held_notes: Vec::new(),
        }
    }

    /// Process a MIDI message (3-byte format)
    ///
    /// Call this from your MIDI callback to update the state.
    pub fn handle_message(&mut self, msg: &[u8]) {
        if msg.is_empty() {
            return;
        }

        let status = msg[0] & 0xF0;
        let _channel = msg[0] & 0x0F;

        match (status, msg.len()) {
            // Note On (with velocity > 0)
            (0x90, 3) if msg[2] > 0 => {
                let note = msg[1];
                let vel = msg[2];
                let voct = Self::note_to_voct(note);

                self.held_notes.push(note);
                // The separate `pitch`/`gate` atomics are convenience mirrors read
                // per-field with `Relaxed` (see `AtomicF64::get`); across two words
                // they are inherently torn-capable, so the ordering here cannot make
                // a `(pitch, gate)` pair read from them coherent. Callers needing a
                // torn-free pair must use `note_snapshot()` (the packed `note` word
                // published below), which is the sole coherence guarantee.
                self.pitch.set(voct);
                self.velocity.set(vel as f64 / 127.0 * 10.0);
                self.gate.set(5.0);
                // Publish the coherent (pitch, gate) pair in a single word.
                self.note.publish(voct, 5.0);
            }

            // Note Off (or Note On with velocity 0)
            (0x80, 3) | (0x90, 3) => {
                let note = msg[1];
                self.held_notes.retain(|&n| n != note);

                if self.held_notes.is_empty() {
                    self.gate.set(0.0);
                    // Gate closes; keep the last pitch in the coherent word.
                    self.note.publish(self.pitch.get(), 0.0);
                } else {
                    // Legato: switch to last held note (gate stays high).
                    let last = *self.held_notes.last().unwrap();
                    let voct = Self::note_to_voct(last);
                    self.pitch.set(voct);
                    self.note.publish(voct, 5.0);
                }
            }

            // Control Change
            (0xB0, 3) => {
                let cc = msg[1];
                let value = msg[2];
                let v = value as f64 / 127.0 * 10.0;

                match cc {
                    1 => self.mod_wheel.set(v),                                  // Mod wheel
                    11 => self.expression.set(v),                                // Expression
                    64 => self.sustain.set(if value >= 64 { 5.0 } else { 0.0 }), // Sustain
                    _ => {}
                }
            }

            // Pitch Bend
            (0xE0, 3) => {
                let lsb = msg[1] as u16;
                let msb = msg[2] as u16;
                let bend_raw = lsb | (msb << 7);
                // ±2 semitones = ±2/12 V
                let bend = (bend_raw as f64 - 8192.0) / 8192.0 * (2.0 / 12.0);
                self.pitch_bend.set(bend);
            }

            // Channel Aftertouch
            (0xD0, 2) => {
                let pressure = msg[1];
                self.aftertouch.set(pressure as f64 / 127.0 * 10.0);
            }

            // Polyphonic Aftertouch (we'll treat it as channel AT for mono)
            (0xA0, 3) => {
                let pressure = msg[2];
                self.aftertouch.set(pressure as f64 / 127.0 * 10.0);
            }

            _ => {}
        }
    }

    /// Convert MIDI note number to V/Oct
    ///
    /// 0V = C4 = MIDI note 60
    fn note_to_voct(note: u8) -> f64 {
        (note as f64 - 60.0) / 12.0
    }

    /// Get a coherent, torn-free `(pitch_voct, gate)` snapshot of the last note
    /// event.
    ///
    /// Reads the packed [`AtomicNote`] with `Acquire` ordering, so the pitch and
    /// gate always originate from the same note-on/off — never a new gate paired
    /// with a stale pitch. Prefer this over reading `pitch`/`gate` separately on
    /// the audio thread.
    pub fn note_snapshot(&self) -> (f64, f64) {
        self.note.snapshot()
    }

    /// Get all held notes
    pub fn held_notes(&self) -> &[u8] {
        &self.held_notes
    }

    /// Check if any notes are currently held
    pub fn notes_active(&self) -> bool {
        !self.held_notes.is_empty()
    }

    /// Reset all state
    pub fn reset(&mut self) {
        self.pitch.set(0.0);
        self.gate.set(0.0);
        self.velocity.set(0.0);
        self.mod_wheel.set(0.0);
        self.pitch_bend.set(0.0);
        self.aftertouch.set(0.0);
        self.sustain.set(0.0);
        self.expression.set(10.0);
        self.note.publish(0.0, 0.0);
        self.held_notes.clear();
    }

    /// All notes off
    pub fn all_notes_off(&mut self) {
        self.held_notes.clear();
        self.gate.set(0.0);
        self.note.publish(self.pitch.get(), 0.0);
    }
}

impl Default for MidiState {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MidiState {
    fn clone(&self) -> Self {
        Self {
            pitch: Arc::new(AtomicF64::new(self.pitch.get())),
            gate: Arc::new(AtomicF64::new(self.gate.get())),
            velocity: Arc::new(AtomicF64::new(self.velocity.get())),
            mod_wheel: Arc::new(AtomicF64::new(self.mod_wheel.get())),
            pitch_bend: Arc::new(AtomicF64::new(self.pitch_bend.get())),
            aftertouch: Arc::new(AtomicF64::new(self.aftertouch.get())),
            sustain: Arc::new(AtomicF64::new(self.sustain.get())),
            expression: Arc::new(AtomicF64::new(self.expression.get())),
            note: Arc::new((*self.note).clone()),
            held_notes: self.held_notes.clone(),
        }
    }
}

/// External output - writes to an atomic value for reading by another thread
///
/// Useful for sending CV values out to external systems.
pub struct ExternalOutput {
    value: Arc<AtomicF64>,
    spec: PortSpec,
}

impl ExternalOutput {
    pub fn new(value: Arc<AtomicF64>, kind: SignalKind) -> Self {
        Self {
            value,
            spec: PortSpec {
                inputs: vec![PortDef::new(0, "in", kind)],
                outputs: vec![],
            },
        }
    }

    pub fn value_ref(&self) -> &Arc<AtomicF64> {
        &self.value
    }
}

impl GraphModule for ExternalOutput {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, _outputs: &mut PortValues) {
        let value = inputs.get_or(0, 0.0);
        self.value.set(value);
    }

    /// Writes a cell an `ExternalInput` of the same patch may read.
    fn shares_state(&self) -> Option<crate::port::SharedState> {
        Some(crate::port::SharedState::writes(Arc::as_ptr(&self.value)))
    }

    fn reset(&mut self) {
        self.value.set(0.0);
    }

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "external_output"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_f64() {
        let a = AtomicF64::new(3.5);
        assert!((a.get() - 3.5).abs() < 0.001);

        a.set(2.5);
        assert!((a.get() - 2.5).abs() < 0.001);
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_atomic_f64_thread_safe() {
        let a = Arc::new(AtomicF64::new(0.0));
        let a2 = Arc::clone(&a);

        std::thread::spawn(move || {
            a2.set(42.0);
        })
        .join()
        .unwrap();

        assert!((a.get() - 42.0).abs() < 0.001);
    }

    #[test]
    fn test_external_input() {
        let value = Arc::new(AtomicF64::new(5.0));
        let mut input = ExternalInput::voct(value.clone());

        let inputs = PortValues::new();
        let mut outputs = PortValues::new();

        input.tick(&inputs, &mut outputs);
        assert!((outputs.get(0).unwrap() - 5.0).abs() < 0.001);

        // Update from "external thread"
        value.set(10.0);
        input.tick(&inputs, &mut outputs);
        assert!((outputs.get(0).unwrap() - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_midi_state_note_on_off() {
        let mut midi = MidiState::new();

        // Note on: C4 (note 60) with velocity 100
        midi.handle_message(&[0x90, 60, 100]);
        assert!((midi.pitch.get() - 0.0).abs() < 0.001); // C4 = 0V
        assert!((midi.gate.get() - 5.0).abs() < 0.001);
        assert!(midi.velocity.get() > 0.0);

        // Note on: C5 (note 72)
        midi.handle_message(&[0x90, 72, 100]);
        assert!((midi.pitch.get() - 1.0).abs() < 0.001); // C5 = 1V

        // Note off: C5
        midi.handle_message(&[0x80, 72, 0]);
        // Should return to C4 (legato)
        assert!((midi.pitch.get() - 0.0).abs() < 0.001);
        assert!((midi.gate.get() - 5.0).abs() < 0.001); // Still held

        // Note off: C4
        midi.handle_message(&[0x80, 60, 0]);
        assert!((midi.gate.get() - 0.0).abs() < 0.001); // Gate off
    }

    #[test]
    fn test_midi_state_pitch_bend() {
        let mut midi = MidiState::new();

        // Center (no bend)
        midi.handle_message(&[0xE0, 0, 64]);
        assert!(midi.pitch_bend.get().abs() < 0.01);

        // Full up (should be ~+2 semitones = +1/6 V)
        midi.handle_message(&[0xE0, 127, 127]);
        assert!(midi.pitch_bend.get() > 0.1);

        // Full down (should be ~-2 semitones = -1/6 V)
        midi.handle_message(&[0xE0, 0, 0]);
        assert!(midi.pitch_bend.get() < -0.1);
    }

    #[test]
    fn test_midi_state_cc() {
        let mut midi = MidiState::new();

        // Mod wheel
        midi.handle_message(&[0xB0, 1, 127]);
        assert!((midi.mod_wheel.get() - 10.0).abs() < 0.01);

        // Sustain pedal on
        midi.handle_message(&[0xB0, 64, 127]);
        assert!((midi.sustain.get() - 5.0).abs() < 0.01);

        // Sustain pedal off
        midi.handle_message(&[0xB0, 64, 0]);
        assert!((midi.sustain.get() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_external_output() {
        let value = Arc::new(AtomicF64::new(0.0));
        let mut output = ExternalOutput::new(value.clone(), SignalKind::CvUnipolar);

        let mut inputs = PortValues::new();
        inputs.set(0, 7.5);

        output.tick(&inputs, &mut PortValues::new());
        assert!((value.get() - 7.5).abs() < 0.001);
    }

    #[test]
    fn test_atomic_f64_load_store() {
        use core::sync::atomic::Ordering;
        let a = AtomicF64::new(1.0);
        assert!((a.load(Ordering::SeqCst) - 1.0).abs() < 0.001);

        a.store(99.0, Ordering::SeqCst);
        assert!((a.load(Ordering::SeqCst) - 99.0).abs() < 0.001);
    }

    #[test]
    fn test_external_input_constructors() {
        let value = Arc::new(AtomicF64::new(0.0));

        let gate = ExternalInput::gate(value.clone());
        assert!(gate.spec.outputs[0].kind == SignalKind::Gate);

        let cv = ExternalInput::cv(value.clone());
        assert!(cv.spec.outputs[0].kind == SignalKind::CvUnipolar);

        let cv_bi = ExternalInput::cv_bipolar(value.clone());
        assert!(cv_bi.spec.outputs[0].kind == SignalKind::CvBipolar);

        let trigger = ExternalInput::trigger(value.clone());
        assert!(trigger.spec.outputs[0].kind == SignalKind::Trigger);

        let audio = ExternalInput::audio(value.clone());
        assert!(audio.spec.outputs[0].kind == SignalKind::Audio);
    }

    #[test]
    fn test_external_input_value_ref() {
        let value = Arc::new(AtomicF64::new(42.0));
        let input = ExternalInput::voct(value.clone());
        assert!((input.value_ref().get() - 42.0).abs() < 0.001);
    }

    #[test]
    fn test_external_input_reset_set_sample_rate() {
        let value = Arc::new(AtomicF64::new(5.0));
        let mut input = ExternalInput::voct(value.clone());

        input.reset();
        input.set_sample_rate(48000.0);
        assert_eq!(input.type_id(), "external_input");
    }

    #[test]
    fn test_external_output_reset_type_id() {
        let value = Arc::new(AtomicF64::new(5.0));
        let mut output = ExternalOutput::new(value.clone(), SignalKind::Audio);

        output.reset();
        assert!((value.get() - 0.0).abs() < 0.001);

        output.set_sample_rate(48000.0);
        assert_eq!(output.type_id(), "external_output");
        assert!(output.value_ref().get().abs() < 0.001);
    }

    #[test]
    fn test_midi_state_default() {
        let midi = MidiState::default();
        assert!(midi.pitch.get().abs() < 0.001);
    }

    #[test]
    fn test_midi_state_clone() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0x90, 60, 100]);

        let cloned = midi.clone();
        assert!((cloned.pitch.get() - midi.pitch.get()).abs() < 0.001);
    }

    #[test]
    fn test_midi_state_reset() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0x90, 60, 100]);
        midi.handle_message(&[0xB0, 1, 127]);

        midi.reset();
        assert!(midi.pitch.get().abs() < 0.001);
        assert!(midi.gate.get().abs() < 0.001);
        assert!(midi.held_notes.is_empty());
    }

    #[test]
    fn test_midi_state_all_notes_off() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0x90, 60, 100]);
        midi.handle_message(&[0x90, 62, 100]);

        assert!(midi.notes_active());

        midi.all_notes_off();
        assert!(!midi.notes_active());
        assert!(midi.gate.get().abs() < 0.001);
    }

    #[test]
    fn test_midi_state_held_notes() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0x90, 60, 100]);
        midi.handle_message(&[0x90, 62, 100]);

        assert_eq!(midi.held_notes(), &[60, 62]);
    }

    #[test]
    fn test_midi_state_channel_aftertouch() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0xD0, 100]);
        assert!(midi.aftertouch.get() > 0.0);
    }

    #[test]
    fn test_midi_state_poly_aftertouch() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0xA0, 60, 100]);
        assert!(midi.aftertouch.get() > 0.0);
    }

    #[test]
    fn test_midi_state_expression() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0xB0, 11, 100]);
        assert!(midi.expression.get() > 0.0);
    }

    #[test]
    fn test_midi_state_note_on_with_zero_velocity() {
        let mut midi = MidiState::new();
        midi.handle_message(&[0x90, 60, 100]);
        assert!(midi.gate.get() > 0.0);

        // Note on with velocity 0 = note off
        midi.handle_message(&[0x90, 60, 0]);
        assert!(midi.gate.get().abs() < 0.001);
    }

    // ---- Q102: coherent, torn-free note publication ----
    #[test]
    fn test_atomic_note_pack_roundtrip() {
        let note = AtomicNote::new(0.0, 0.0);

        note.publish(0.75, 5.0);
        let (p, g) = note.snapshot();
        assert!((p - 0.75).abs() < 1e-6);
        assert!((g - 5.0).abs() < 1e-6);

        note.publish(-1.25, 0.0);
        let (p, g) = note.snapshot();
        assert!((p + 1.25).abs() < 1e-6);
        assert_eq!(g, 0.0);

        // Default and Clone preserve the packed pair.
        assert_eq!(AtomicNote::default().snapshot(), (0.0, 0.0));
        assert_eq!(note.clone().snapshot(), note.snapshot());
    }

    #[test]
    fn test_midi_state_note_snapshot_coherent() {
        let mut midi = MidiState::new();

        // Note on: C5 (note 72) -> 1V, gate 5V, published together.
        midi.handle_message(&[0x90, 72, 100]);
        let (pitch, gate) = midi.note_snapshot();
        assert!((pitch - 1.0).abs() < 1e-6);
        assert!((gate - 5.0).abs() < 1e-6);

        // Note off -> gate closes, pitch retained coherently.
        midi.handle_message(&[0x80, 72, 0]);
        let (_pitch, gate) = midi.note_snapshot();
        assert!(gate.abs() < 1e-6);
    }

    // Regression: `note_snapshot` is the coherent `(pitch, gate)` read across a
    // note change (legato). The separate `pitch`/`gate` fields are Relaxed
    // convenience mirrors and are torn-capable across two words by design; the
    // packed `note` word is the only guaranteed-coherent pair. This asserts the
    // snapshot always pairs the *current* pitch with the *current* gate, never a
    // mixed pair, through a held-note switch.
    #[test]
    fn test_midi_state_legato_snapshot_stays_coherent() {
        let mut midi = MidiState::new();

        // Hold C4 (0V), then legato to C5 (1V) without releasing C4.
        midi.handle_message(&[0x90, 60, 100]);
        let (p, g) = midi.note_snapshot();
        assert!((p - 0.0).abs() < 1e-6 && (g - 5.0).abs() < 1e-6);

        midi.handle_message(&[0x90, 72, 100]);
        let (p, g) = midi.note_snapshot();
        assert!(
            (p - 1.0).abs() < 1e-6 && (g - 5.0).abs() < 1e-6,
            "legato pitch change must pair the new pitch with a held gate, got ({p}, {g})"
        );

        // Release the newer note: gate stays high, pitch falls back to C4 (still
        // held) as a coherent pair.
        midi.handle_message(&[0x80, 72, 0]);
        let (p, g) = midi.note_snapshot();
        assert!(
            (p - 0.0).abs() < 1e-6 && (g - 5.0).abs() < 1e-6,
            "after releasing the top note the held note's pitch pairs with gate 5V, got ({p}, {g})"
        );

        // Release the last held note: gate closes coherently.
        midi.handle_message(&[0x80, 60, 0]);
        let (_p, g) = midi.note_snapshot();
        assert!(g.abs() < 1e-6, "last note off closes the gate");
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_atomic_note_no_tearing_across_threads() {
        let note = Arc::new(AtomicNote::new(0.0, 0.0));
        let writer_note = Arc::clone(&note);

        // Writer alternates between two coherent (pitch, gate) states.
        let writer = std::thread::spawn(move || {
            for i in 0..200_000u32 {
                if i % 2 == 0 {
                    writer_note.publish(1.0, 5.0);
                } else {
                    writer_note.publish(0.0, 0.0);
                }
            }
        });

        // A single AtomicU64 can never expose a mixed pair.
        for _ in 0..200_000 {
            let (p, g) = note.snapshot();
            let coherent = ((p - 1.0).abs() < 1e-6 && (g - 5.0).abs() < 1e-6)
                || (p.abs() < 1e-6 && g.abs() < 1e-6);
            assert!(coherent, "torn note snapshot observed: ({p}, {g})");
        }

        writer.join().unwrap();
    }

    // ---- AudioInput / AudioInputStream ----

    const V: f64 = AudioInput::FULL_SCALE_VOLTS;

    /// Tick `node` `n` times and return each frame's `[out, left, right]`.
    fn run(node: &mut AudioInput, n: usize) -> Vec<[f64; 3]> {
        let inputs = PortValues::new();
        let mut outputs = PortValues::new();
        (0..n)
            .map(|_| {
                node.tick(&inputs, &mut outputs);
                [
                    outputs.get(10).unwrap(),
                    outputs.get(11).unwrap(),
                    outputs.get(12).unwrap(),
                ]
            })
            .collect()
    }

    /// The `left` output of `n` ticks, back in host units (`±1.0`).
    fn left(node: &mut AudioInput, n: usize) -> Vec<f64> {
        run(node, n).iter().map(|f| f[1] / V).collect()
    }

    /// A ramp `start, start + 1, ..` as host samples.
    fn ramp(start: usize, len: usize) -> Vec<f32> {
        (start..start + len).map(|i| i as f32).collect()
    }

    fn as_f64(samples: &[f32]) -> Vec<f64> {
        samples.iter().map(|&s| s as f64).collect()
    }

    #[test]
    fn audio_input_samples_arrive_in_order_across_block_boundaries() {
        let stream = Arc::new(AudioInputStream::new(1, 16));
        let mut node = AudioInput::new(Arc::clone(&stream));
        let mut heard = Vec::new();
        for block in 0..5 {
            stream.write(&[ramp(block * 16, 16)]);
            heard.extend(left(&mut node, 16));
        }
        assert_eq!(heard, as_f64(&ramp(0, 80)));
    }

    #[test]
    fn audio_input_engine_blocks_smaller_than_the_host_block_continue_through_it() {
        // Host writes 128 frames per call; the engine renders them as 64 + 64, or 32 at a
        // time, or one tick at a time: the frames come out continuous either way.
        let stream = Arc::new(AudioInputStream::new(1, 128));
        let mut node = AudioInput::new(Arc::clone(&stream));
        let mut heard = Vec::new();
        for call in 0..3 {
            stream.write(&[ramp(call * 128, 128)]);
            for chunk in [64, 32, 31, 1] {
                heard.extend(left(&mut node, chunk));
            }
        }
        assert_eq!(heard, as_f64(&ramp(0, 384)));
    }

    #[test]
    fn audio_input_engine_blocks_larger_than_the_host_block_pad_with_silence() {
        // Host writes 64 frames, engine renders 128: 64 frames of input, then silence —
        // never the block again, never stale data — and the next block starts clean.
        let stream = Arc::new(AudioInputStream::new(1, 128));
        let mut node = AudioInput::new(Arc::clone(&stream));
        for call in 0..3 {
            let block: Vec<f32> = ramp(call * 64 + 1, 64);
            stream.write(&[&block]);
            let heard = left(&mut node, 128);
            assert_eq!(&heard[..64], &as_f64(&block)[..], "call {call}");
            assert!(heard[64..].iter().all(|&s| s == 0.0), "call {call}");
        }
    }

    #[test]
    fn audio_input_underrun_gives_silence() {
        let stream = Arc::new(AudioInputStream::new(2, 32));
        let mut node = AudioInput::new(Arc::clone(&stream));
        // Nothing written yet.
        assert!(run(&mut node, 50).iter().all(|f| *f == [0.0; 3]));
        // One block, then the host stops: the block plays once, then silence for good.
        stream.write(&[[0.5f32; 8], [0.25; 8]]);
        let frames = run(&mut node, 200);
        assert!(frames[..8]
            .iter()
            .all(|f| f[1] == 0.5 * V && f[2] == 0.25 * V));
        assert!(frames[8..].iter().all(|f| *f == [0.0; 3]));
        // `clear` silences a reader mid-block.
        stream.write(&[[0.5f32; 8], [0.25; 8]]);
        run(&mut node, 3);
        stream.clear();
        assert!(run(&mut node, 10).iter().all(|f| *f == [0.0; 3]));
    }

    #[test]
    fn audio_input_overrun_drops_the_unread_frames() {
        let stream = Arc::new(AudioInputStream::new(1, 16));
        let mut node = AudioInput::new(Arc::clone(&stream));
        // A new block lands mid-block: the reader abandons the rest and starts it at 0.
        stream.write(&[ramp(100, 8)]);
        assert_eq!(left(&mut node, 3), [100.0, 101.0, 102.0]);
        stream.write(&[ramp(200, 8)]);
        assert_eq!(
            left(&mut node, 9),
            [200., 201., 202., 203., 204., 205., 206., 207., 0.]
        );
        // Two writes before any tick: only the second is heard.
        stream.write(&[ramp(300, 4)]);
        stream.write(&[ramp(400, 4)]);
        assert_eq!(left(&mut node, 5), [400.0, 401.0, 402.0, 403.0, 0.0]);
    }

    #[test]
    fn audio_input_write_truncates_to_capacity() {
        let stream = Arc::new(AudioInputStream::new(1, 4));
        assert_eq!(stream.capacity(), 4);
        assert_eq!(stream.write(&[ramp(1, 10)]), 4);
        let mut node = AudioInput::new(Arc::clone(&stream));
        stream.write(&[ramp(1, 10)]);
        assert_eq!(left(&mut node, 6), [1.0, 2.0, 3.0, 4.0, 0.0, 0.0]);
        // A capacity-0 stream (an unbound input's) accepts nothing.
        let unbound = AudioInput::unbound();
        assert_eq!(unbound.stream().write(&[[1.0f32; 4]]), 0);
    }

    #[test]
    fn audio_input_hears_blocks_written_after_it_exists_or_resets() {
        let stream = Arc::new(AudioInputStream::new(1, 8));
        // A block written before the node existed is not replayed to it.
        stream.write(&[ramp(1, 8)]);
        let mut node = AudioInput::new(Arc::clone(&stream));
        assert!(left(&mut node, 8).iter().all(|&s| s == 0.0));
        stream.write(&[ramp(11, 8)]);
        assert_eq!(left(&mut node, 2), [11.0, 12.0]);
        // reset() discards the rest of the current block; the next block plays from 0.
        node.reset();
        assert!(left(&mut node, 6).iter().all(|&s| s == 0.0));
        stream.write(&[ramp(21, 8)]);
        assert_eq!(left(&mut node, 2), [21.0, 22.0]);
    }

    /// RFC-008: one capture fans out. This is the case a single-consumer ring buffer cannot
    /// pass: several readers of one stream, including separate patches rendered one after
    /// another over the same block (voice-major, as a polyphonic host does) and a reader
    /// that sits out a block, all hear identical frames.
    #[test]
    fn audio_input_one_stream_fans_out_to_many_readers() {
        use crate::graph::Patch;
        use crate::modules::StereoOutput;

        let sr = 48_000.0;
        let stream = Arc::new(AudioInputStream::new(2, 64));
        let voice = |channel: InputChannel| {
            let mut patch = Patch::new(sr);
            let input = patch.add(
                "in",
                AudioInput::new(Arc::clone(&stream)).with_channel(channel),
            );
            let out = patch.add("out", StereoOutput::new());
            patch.connect(input.out("out"), out.in_("left")).unwrap();
            patch.connect(input.out("left"), out.in_("right")).unwrap();
            patch.set_output(out.id());
            patch.compile().unwrap();
            patch
        };
        let mut a = voice(InputChannel::Left);
        let mut b = voice(InputChannel::Left);
        let mut idle = voice(InputChannel::Left);
        let mut right = voice(InputChannel::Right);

        // Two nodes inside one patch read the same frames as well.
        let mut both = Patch::new(sr);
        let n1 = both.add("n1", AudioInput::new(Arc::clone(&stream)));
        let n2 = both.add("n2", AudioInput::new(Arc::clone(&stream)));
        let out = both.add("out", StereoOutput::new());
        both.connect(n1.out("left"), out.in_("left")).unwrap();
        both.connect(n2.out("left"), out.in_("right")).unwrap();
        both.set_output(out.id());
        both.compile().unwrap();

        let render = |patch: &mut Patch| {
            let (mut l, mut r) = ([0.0; 64], [0.0; 64]);
            patch.tick_block(&mut l, &mut r);
            (l, r)
        };

        for block in 0..4 {
            let lefts = ramp(block * 64 + 1, 64);
            let rights: Vec<f32> = lefts.iter().map(|s| -s).collect();
            stream.write(&[&lefts, &rights]);
            let expect_l: Vec<f64> = lefts.iter().map(|&s| s as f64 * V).collect();
            let expect_r: Vec<f64> = rights.iter().map(|&s| s as f64 * V).collect();

            // Voice-major: each patch renders the whole block before the next starts.
            let (al, ar) = render(&mut a);
            let (bl, br) = render(&mut b);
            assert_eq!(al.to_vec(), expect_l, "block {block}: first voice");
            assert_eq!((bl, br), (al, ar), "block {block}: second voice");
            assert_eq!(render(&mut right).0.to_vec(), expect_r, "block {block}");
            let (l, r) = render(&mut both);
            assert_eq!(
                (l.to_vec(), r.to_vec()),
                (expect_l.clone(), expect_l.clone())
            );

            // An idle voice skips blocks 1 and 2 entirely, then rejoins in step.
            if block == 0 || block == 3 {
                assert_eq!(
                    render(&mut idle).0.to_vec(),
                    expect_l,
                    "block {block}: idle"
                );
            }
        }
    }

    /// A patch around one `AudioInput` on `stream`, `out` on both output channels.
    fn input_patch(stream: &Arc<AudioInputStream>) -> crate::graph::Patch {
        use crate::modules::StereoOutput;
        let mut patch = crate::graph::Patch::new(48_000.0);
        let input = patch.add("in", AudioInput::new(Arc::clone(stream)));
        let out = patch.add("out", StereoOutput::new());
        patch.connect(input.out("left"), out.in_("left")).unwrap();
        patch.set_output(out.id());
        patch.compile().unwrap();
        patch
    }

    /// The frame-by-frame case a cursor cannot serve: patch A ticks every frame of a
    /// 128-frame block, voice B is built at frame 64, and voice C pauses for frames
    /// 32..96. On a host-clock stream all three read the same frame at every tick.
    #[test]
    fn audio_input_host_clock_keeps_frame_by_frame_readers_in_step() {
        let stream = Arc::new(AudioInputStream::with_host_clock(1, 128));
        assert!(stream.is_host_clocked());
        let mut a = input_patch(&stream);
        let mut c = input_patch(&stream);
        let mut b = None;
        for block in 0..3 {
            let samples = ramp(block * 128 + 1, 128);
            stream.write(&[&samples]);
            assert_eq!(stream.frame(), 0, "each write starts at frame 0");
            for (f, &sample) in samples.iter().enumerate() {
                let expect = sample as f64 * V;
                assert_eq!(a.tick().0, expect, "block {block} frame {f}: A");
                if block == 0 && f == 64 {
                    b = Some(input_patch(&stream));
                }
                if let Some(b) = b.as_mut() {
                    assert_eq!(
                        b.tick().0,
                        expect,
                        "block {block} frame {f}: B joined at 64"
                    );
                }
                if block != 1 || !(32..96).contains(&f) {
                    assert_eq!(c.tick().0, expect, "block {block} frame {f}: C resumed");
                }
                stream.advance();
            }
            // Past the block's end: silence for everyone.
            assert_eq!(a.tick().0, 0.0);
        }
        // set_frame jumps every reader.
        stream.write(&[ramp(1, 128)]);
        stream.set_frame(100);
        assert_eq!(a.tick().0, 101.0 * V);
        assert_eq!(c.tick().0, 101.0 * V);
    }

    /// A whole clip written once, read at the host's frame: a reader built after the
    /// write, or half-way through the render, hears the clip at the host's position.
    #[test]
    fn audio_input_host_clock_reads_a_whole_clip() {
        let clip = ramp(1, 4_800);
        let stream = Arc::new(AudioInputStream::with_host_clock(1, clip.len()));
        stream.write(&[&clip]);
        let mut first = AudioInput::new(Arc::clone(&stream));
        let mut heard = Vec::new();
        let mut late = None;
        for f in 0..clip.len() {
            if f == 2_400 {
                late = Some(AudioInput::new(Arc::clone(&stream)));
            }
            let [_, l, _] = run(&mut first, 1)[0];
            heard.push(l / V);
            if let Some(late) = late.as_mut() {
                assert_eq!(run(late, 1)[0][1], l, "frame {f}");
            }
            stream.advance();
        }
        assert_eq!(heard, as_f64(&clip));
        assert_eq!(run(&mut first, 1)[0], [0.0; 3], "past the clip: silence");
        // reset() does not move a host-clock reader.
        stream.set_frame(10);
        first.reset();
        assert_eq!(run(&mut first, 1)[0][1], 11.0 * V);
    }

    /// The documented limit of cursor streams (and why host-clock streams exist): a
    /// reader resuming mid-block starts the block over, late; one built mid-block is
    /// silent until the next block. At block boundaries every cursor reader is in step.
    #[test]
    fn audio_input_cursor_readers_rejoin_at_block_boundaries_only() {
        let stream = Arc::new(AudioInputStream::new(1, 128));
        // The clock calls are ignored on a cursor stream.
        stream.set_frame(5);
        stream.advance();
        assert_eq!(stream.frame(), 0);
        assert!(!stream.is_host_clocked());

        let mut a = AudioInput::new(Arc::clone(&stream));
        let mut paused = AudioInput::new(Arc::clone(&stream));
        stream.write(&[ramp(1, 128)]);
        run(&mut a, 64);
        run(&mut paused, 32);
        let built_mid_block = &mut AudioInput::new(Arc::clone(&stream));
        // Frame 64 for A; the paused reader resumes where it left off (frame 32).
        assert_eq!(left(&mut a, 1), [65.0]);
        assert_eq!(left(&mut paused, 1), [33.0]);
        assert_eq!(left(built_mid_block, 1), [0.0]);
        // From the next block on, all three are in step.
        stream.write(&[ramp(201, 128)]);
        for r in [&mut a, &mut paused, built_mid_block] {
            assert_eq!(left(r, 2), [201.0, 202.0]);
        }
        // The reviewer's case: a voice that sat out resumes at frame 64 of a new block
        // and plays its frame 0 there, 64 frames late.
        stream.write(&[ramp(301, 128)]);
        run(&mut a, 64);
        assert_eq!(left(&mut a, 1), [365.0]);
        assert_eq!(left(&mut paused, 1), [301.0]);
    }

    #[test]
    fn audio_input_channel_selection_and_mono_sources() {
        let stream = Arc::new(AudioInputStream::new(2, 4));
        let mut node = AudioInput::new(Arc::clone(&stream));
        assert_eq!(node.channel(), InputChannel::Both);

        let check = |node: &mut AudioInput, channel, expect_out: f64| {
            node.set_channel(channel);
            stream.write(&[[0.5f32], [-0.25]]);
            let [out, l, r] = run(node, 1)[0];
            assert_eq!(
                (out, l, r),
                (expect_out * V, 0.5 * V, -0.25 * V),
                "{channel:?}"
            );
        };
        check(&mut node, InputChannel::Left, 0.5);
        check(&mut node, InputChannel::Right, -0.25);
        check(&mut node, InputChannel::Both, 0.125);

        // A mono block fills both channels, so `both` keeps the source's level.
        stream.write(&[[0.5f32]]);
        assert_eq!(run(&mut node, 1)[0], [0.5 * V; 3]);
        // Interleaved stereo, and an interleaved mono source.
        stream.write_interleaved(&[0.5, -0.5, 0.25, -0.25], 2);
        assert_eq!(
            run(&mut node, 2),
            [[0.0, 0.5 * V, -0.5 * V], [0.0, 0.25 * V, -0.25 * V]]
        );
        stream.write_interleaved(&[0.5, 0.25], 1);
        assert_eq!(run(&mut node, 2), [[0.5 * V; 3], [0.25 * V; 3]]);
        // Missing channels are silent; extra source channels are ignored.
        stream.write_interleaved(&[0.5, -0.5, 0.9, 0.1, -0.1, 0.9], 3);
        assert_eq!(run(&mut node, 1)[0], [0.0, 0.5 * V, -0.5 * V]);
        stream.write(&[&[0.5f32][..], &[][..]]);
        assert!(
            run(&mut node, 1)[0] == [0.0; 3],
            "an empty slice makes an empty block"
        );
        assert_eq!(stream.write_interleaved(&[0.5, 0.5], 0), 0);

        // A mono stream: `right` and `both` read channel 0.
        let mono = Arc::new(AudioInputStream::new(1, 4));
        let mut node = AudioInput::new(Arc::clone(&mono)).with_channel(InputChannel::Right);
        mono.write(&[[0.5f32], [0.9]]);
        assert_eq!(run(&mut node, 1)[0], [0.5 * V; 3]);

        // More than two channels: `both` is the mean of all of them.
        let quad = Arc::new(AudioInputStream::new(4, 4));
        let mut node = AudioInput::new(Arc::clone(&quad));
        quad.write(&[[0.1f32], [0.2], [0.3], [0.4]]);
        let [out, l, r] = run(&mut node, 1)[0];
        assert!((out - 0.25 * V).abs() < 1e-6);
        assert_eq!((l, r), (0.1f32 as f64 * V, 0.2f32 as f64 * V));
    }

    #[test]
    fn audio_input_level_gain_and_non_finite_samples() {
        let stream = Arc::new(AudioInputStream::new(1, 8));
        let mut node = AudioInput::new(Arc::clone(&stream));
        let mut outputs = PortValues::new();
        let mut inputs = PortValues::new();
        let mut tick = |gain: Option<f64>| {
            if let Some(g) = gain {
                inputs.set(0, g);
            }
            node.tick(&inputs, &mut outputs);
            outputs.get(11).unwrap()
        };
        stream.write(&[[1.0f32, 1.0, 1.0, 1.0, 1.0, f32::NAN, f32::INFINITY, -1.0]]);
        assert_eq!(tick(None), 5.0, "full scale is 5 V at the default gain");
        assert_eq!(tick(Some(2.0)), 10.0);
        assert_eq!(
            tick(Some(100.0)),
            AudioInput::MAX_GAIN * 5.0,
            "gain is bounded"
        );
        assert_eq!(tick(Some(-3.0)), 0.0);
        assert_eq!(tick(Some(f64::NAN)), 0.0, "a non-finite gain is silence");
        assert_eq!(tick(Some(1.0)), 0.0, "NaN is written as silence");
        assert_eq!(tick(None), 0.0, "so is infinity");
        assert_eq!(tick(None), -5.0);
    }

    #[test]
    fn audio_input_module_contract() {
        let mut node = AudioInput::unbound();
        assert_eq!(node.type_id(), "audio_input");
        let spec = node.port_spec();
        assert_eq!(spec.input_by_name("gain").unwrap().default, 1.0);
        for name in ["out", "left", "right"] {
            assert_eq!(spec.output_by_name(name).unwrap().kind, SignalKind::Audio);
        }
        node.set_sample_rate(96_000.0);
        assert!(run(&mut node, 4).iter().all(|f| *f == [0.0; 3]));

        // `channel` is an introspection select, defaulting to both.
        let info = &node.param_infos()[0];
        assert_eq!(
            (info.id.as_str(), info.value, info.default),
            ("channel", 2.0, 2.0)
        );
        assert!(node.set_param_by_id("channel", 0.0));
        assert_eq!(node.channel(), InputChannel::Left);
        assert!(node.set_param_by_id("channel", 1.2));
        assert_eq!(node.channel(), InputChannel::Right);
        assert!(!node.set_param_by_id("channel", 3.0));
        assert!(
            !node.set_param_by_id("gain", 1.0),
            "gain is a port, not an internal param"
        );
        assert_eq!(node.channel(), InputChannel::Right);
        assert!(node.introspect().is_some());
        for channel in [InputChannel::Left, InputChannel::Right, InputChannel::Both] {
            assert_eq!(
                InputChannel::from_index(channel.index() as f64),
                Some(channel)
            );
        }
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 3.0] {
            assert_eq!(InputChannel::from_index(bad), None, "{bad}");
        }
        assert!(
            !node.set_param_by_id("channel", f64::NAN),
            "NaN does not pick left"
        );
        assert_eq!(node.channel(), InputChannel::Right);
        assert!(format!("{:?}", node.stream()).contains("capacity: 0"));
    }

    /// A writer on another thread never hands a reader a torn frame: every sample a reader
    /// outputs is either silence or exactly what was written at that frame of one block.
    #[test]
    #[cfg(feature = "std")]
    fn audio_input_never_tears_across_threads() {
        const FRAMES: usize = 16;
        const BLOCKS: usize = 20_000;
        let stream = Arc::new(AudioInputStream::new(2, FRAMES));
        // Built before the writer starts, so it hears the blocks it publishes. Built
        // after a writer that had already finished, it would treat the last block as
        // consumed and never hear anything, and the loop below would never end.
        let mut node = AudioInput::new(Arc::clone(&stream));
        let writer_stream = Arc::clone(&stream);
        let writer = std::thread::spawn(move || {
            let mut block = [0.0f32; FRAMES];
            for k in 1..=BLOCKS {
                for (f, s) in block.iter_mut().enumerate() {
                    *s = (k * FRAMES + f) as f32;
                }
                writer_stream.write(&[&block, &block]);
            }
        });

        let mut previous = 0.0;
        let mut heard = 0usize;
        let mut rounds = 0usize;
        while !writer.is_finished() || heard == 0 {
            rounds += 1;
            assert!(rounds < 10_000_000, "the reader never heard the writer");
            for [_, l, r] in run(&mut node, 64) {
                assert_eq!(l, r, "torn frame: channels from different blocks");
                if l != 0.0 {
                    let value = l / V;
                    // Within a block the value climbs by exactly 1 per tick; a jump must land
                    // on the first frame of a newer block.
                    let in_step = value == previous + 1.0;
                    let block_start = value as usize % FRAMES == 0 && value > previous;
                    assert!(in_step || block_start, "torn read: {previous} then {value}");
                    heard += 1;
                }
                previous = l / V;
            }
        }
        writer.join().unwrap();
        assert!(heard > 0);
    }
}
