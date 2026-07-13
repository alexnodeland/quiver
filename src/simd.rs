//! SIMD Vectorization and Block Processing
//!
//! This module provides SIMD-accelerated DSP operations and block-oriented
//! processing utilities for improved performance.
//!
//! # Features
//!
//! - [`AudioBlock`] - audio buffer for block/vectorized operations
//! - Block processing utilities
//! - Lazy evaluation framework
//! - SIMD-accelerated block operations (when the `simd` feature is enabled),
//!   backed by the no_std-compatible `wide` crate (`f64x4`)
//!
//! # Alignment
//!
//! Buffers are backed by a plain `Vec<f64>`, which the global allocator aligns
//! to `align_of::<f64>()` = 8 bytes. The `simd` path uses unaligned lane loads
//! (`f64x4::new`) over `chunks_exact(4)` with a scalar remainder, so no
//! over-alignment of the backing storage is required or guaranteed.

use crate::port::{BlockPortValues, GraphModule, PortValues};
use alloc::vec;
use alloc::vec::Vec;
use core::f64::consts::PI;
use libm::Libm;

#[cfg(feature = "simd")]
use wide::f64x4;

/// Block size for SIMD operations (typically 4 or 8 for SSE/AVX)
pub const SIMD_BLOCK_SIZE: usize = 4;

/// Default processing block size
pub const DEFAULT_BLOCK_SIZE: usize = 64;

/// Audio buffer for block/vectorized operations.
///
/// Provides efficient storage and operations for audio blocks. The backing
/// store is a plain `Vec<f64>` (8-byte aligned); the `simd` feature accelerates
/// the block ops with `wide::f64x4` unaligned loads and therefore imposes no
/// over-alignment requirement on the storage.
#[derive(Clone)]
pub struct AudioBlock {
    /// Sample data
    samples: Vec<f64>,
    /// Block size (number of samples)
    size: usize,
}

impl AudioBlock {
    /// Create a new audio block with the given size
    pub fn new(size: usize) -> Self {
        Self {
            samples: vec![0.0; size],
            size,
        }
    }

    /// Create a block filled with a constant value
    pub fn constant(size: usize, value: f64) -> Self {
        Self {
            samples: vec![value; size],
            size,
        }
    }

    /// Create a block from existing samples
    pub fn from_samples(samples: Vec<f64>) -> Self {
        let size = samples.len();
        Self { samples, size }
    }

    /// Get the block size
    #[inline]
    pub fn len(&self) -> usize {
        self.size
    }

    /// Check if empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Get a sample at the given index
    #[inline]
    pub fn get(&self, index: usize) -> f64 {
        self.samples.get(index).copied().unwrap_or(0.0)
    }

    /// Set a sample at the given index
    #[inline]
    pub fn set(&mut self, index: usize, value: f64) {
        if index < self.size {
            self.samples[index] = value;
        }
    }

    /// Get a slice of all samples
    #[inline]
    pub fn as_slice(&self) -> &[f64] {
        &self.samples
    }

    /// Get a mutable slice of all samples
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [f64] {
        &mut self.samples
    }

    /// Fill the block with a constant value
    pub fn fill(&mut self, value: f64) {
        self.samples.fill(value);
    }

    /// Clear the block (fill with zeros)
    pub fn clear(&mut self) {
        self.fill(0.0);
    }

    /// Add a constant to all samples
    #[cfg(not(feature = "simd"))]
    pub fn add_scalar(&mut self, value: f64) {
        for sample in &mut self.samples {
            *sample += value;
        }
    }

    /// Multiply all samples by a constant
    #[cfg(not(feature = "simd"))]
    pub fn mul_scalar(&mut self, value: f64) {
        for sample in &mut self.samples {
            *sample *= value;
        }
    }

    /// Add another block element-wise
    #[cfg(not(feature = "simd"))]
    pub fn add_block(&mut self, other: &AudioBlock) {
        let len = self.size.min(other.size);
        for i in 0..len {
            self.samples[i] += other.samples[i];
        }
    }

    /// Multiply by another block element-wise
    #[cfg(not(feature = "simd"))]
    pub fn mul_block(&mut self, other: &AudioBlock) {
        let len = self.size.min(other.size);
        for i in 0..len {
            self.samples[i] *= other.samples[i];
        }
    }

    /// SIMD-accelerated scalar addition (when the `simd` feature is enabled).
    ///
    /// Processes four lanes at a time with `wide::f64x4` over
    /// `chunks_exact(4)`, with a scalar remainder. Because IEEE-754 addition is
    /// correctly rounded per lane, the result is bit-for-bit identical to the
    /// scalar path.
    #[cfg(feature = "simd")]
    pub fn add_scalar(&mut self, value: f64) {
        let mut chunks = self.samples.chunks_exact_mut(SIMD_BLOCK_SIZE);
        for c in &mut chunks {
            let r = (f64x4::new([c[0], c[1], c[2], c[3]]) + value).to_array();
            c.copy_from_slice(&r);
        }
        for x in chunks.into_remainder() {
            *x += value;
        }
    }

    /// SIMD-accelerated scalar multiplication (when the `simd` feature is
    /// enabled). Bit-for-bit identical to the scalar path.
    #[cfg(feature = "simd")]
    pub fn mul_scalar(&mut self, value: f64) {
        let mut chunks = self.samples.chunks_exact_mut(SIMD_BLOCK_SIZE);
        for c in &mut chunks {
            let r = (f64x4::new([c[0], c[1], c[2], c[3]]) * value).to_array();
            c.copy_from_slice(&r);
        }
        for x in chunks.into_remainder() {
            *x *= value;
        }
    }

    /// SIMD-accelerated element-wise block addition (when the `simd` feature is
    /// enabled). Bit-for-bit identical to the scalar path.
    #[cfg(feature = "simd")]
    pub fn add_block(&mut self, other: &AudioBlock) {
        let len = self.size.min(other.size);
        let a = &mut self.samples[..len];
        let b = &other.samples[..len];
        let mut a_chunks = a.chunks_exact_mut(SIMD_BLOCK_SIZE);
        let mut b_chunks = b.chunks_exact(SIMD_BLOCK_SIZE);
        for (ca, cb) in a_chunks.by_ref().zip(b_chunks.by_ref()) {
            let va = f64x4::new([ca[0], ca[1], ca[2], ca[3]]);
            let vb = f64x4::new([cb[0], cb[1], cb[2], cb[3]]);
            ca.copy_from_slice(&(va + vb).to_array());
        }
        for (x, y) in a_chunks
            .into_remainder()
            .iter_mut()
            .zip(b_chunks.remainder())
        {
            *x += *y;
        }
    }

    /// SIMD-accelerated element-wise block multiplication (when the `simd`
    /// feature is enabled). Bit-for-bit identical to the scalar path.
    #[cfg(feature = "simd")]
    pub fn mul_block(&mut self, other: &AudioBlock) {
        let len = self.size.min(other.size);
        let a = &mut self.samples[..len];
        let b = &other.samples[..len];
        let mut a_chunks = a.chunks_exact_mut(SIMD_BLOCK_SIZE);
        let mut b_chunks = b.chunks_exact(SIMD_BLOCK_SIZE);
        for (ca, cb) in a_chunks.by_ref().zip(b_chunks.by_ref()) {
            let va = f64x4::new([ca[0], ca[1], ca[2], ca[3]]);
            let vb = f64x4::new([cb[0], cb[1], cb[2], cb[3]]);
            ca.copy_from_slice(&(va * vb).to_array());
        }
        for (x, y) in a_chunks
            .into_remainder()
            .iter_mut()
            .zip(b_chunks.remainder())
        {
            *x *= *y;
        }
    }

    /// Apply a function to all samples
    pub fn map<F: Fn(f64) -> f64>(&mut self, f: F) {
        for sample in &mut self.samples {
            *sample = f(*sample);
        }
    }

    /// Apply soft clipping (tanh saturation)
    pub fn soft_clip(&mut self, drive: f64) {
        for sample in &mut self.samples {
            *sample = Libm::<f64>::tanh(*sample * drive) / Libm::<f64>::tanh(drive).max(0.001);
        }
    }

    /// Apply hard clipping
    pub fn hard_clip(&mut self, threshold: f64) {
        for sample in &mut self.samples {
            *sample = sample.clamp(-threshold, threshold);
        }
    }

    /// Get the peak (maximum absolute value)
    pub fn peak(&self) -> f64 {
        self.samples.iter().map(|s| s.abs()).fold(0.0, f64::max)
    }

    /// Get the RMS (root mean square) value
    pub fn rms(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f64 = self.samples.iter().map(|s| s * s).sum();
        Libm::<f64>::sqrt(sum_sq / self.size as f64)
    }

    /// Copy from another block
    pub fn copy_from(&mut self, other: &AudioBlock) {
        let len = self.size.min(other.size);
        self.samples[..len].copy_from_slice(&other.samples[..len]);
    }
}

impl Default for AudioBlock {
    fn default() -> Self {
        Self::new(DEFAULT_BLOCK_SIZE)
    }
}

/// Block processor for efficient batch processing
pub struct BlockProcessor {
    /// Processing block size
    block_size: usize,
    /// Sample rate
    sample_rate: f64,
}

impl BlockProcessor {
    /// Create a new block processor
    pub fn new(block_size: usize, sample_rate: f64) -> Self {
        Self {
            block_size,
            sample_rate,
        }
    }

    /// Get the block size
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Get the sample rate
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Process a module for one block
    pub fn process_block(
        &self,
        module: &mut dyn GraphModule,
        inputs: &BlockPortValues,
        outputs: &mut BlockPortValues,
    ) {
        module.process_block(inputs, outputs, self.block_size);
    }

    /// Process a module sample-by-sample fallback
    pub fn process_samples(
        &self,
        module: &mut dyn GraphModule,
        inputs: &BlockPortValues,
        outputs: &mut BlockPortValues,
    ) {
        for i in 0..self.block_size {
            let in_frame = inputs.frame(i);
            let mut out_frame = PortValues::new();
            module.tick(&in_frame, &mut out_frame);
            outputs.set_frame(i, out_frame);
        }
    }
}

/// Lazy signal node for deferred evaluation
///
/// Signals are only computed when their value is actually needed,
/// avoiding unnecessary computation.
pub struct LazySignal<F: Fn() -> f64> {
    /// Computation function
    compute: F,
    /// Cached value (if computed)
    cached: Option<f64>,
    /// Whether the cache is valid
    valid: bool,
}

impl<F: Fn() -> f64> LazySignal<F> {
    /// Create a new lazy signal
    pub fn new(compute: F) -> Self {
        Self {
            compute,
            cached: None,
            valid: false,
        }
    }

    /// Get the signal value (computing if necessary)
    pub fn get(&mut self) -> f64 {
        if !self.valid {
            self.cached = Some((self.compute)());
            self.valid = true;
        }
        self.cached.unwrap_or(0.0)
    }

    /// Invalidate the cache (force recomputation on next get)
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// Check if the value has been computed
    pub fn is_computed(&self) -> bool {
        self.valid
    }
}

/// Lazy block signal for deferred block evaluation
pub struct LazyBlock {
    /// Cached block
    block: AudioBlock,
    /// Whether the cache is valid
    valid: bool,
}

impl LazyBlock {
    /// Create a new lazy block with the given size
    pub fn new(size: usize) -> Self {
        Self {
            block: AudioBlock::new(size),
            valid: false,
        }
    }

    /// Get the block, computing if necessary
    pub fn get<F: FnOnce(&mut AudioBlock)>(&mut self, compute: F) -> &AudioBlock {
        if !self.valid {
            compute(&mut self.block);
            self.valid = true;
        }
        &self.block
    }

    /// Get mutable access to the block (marks as valid)
    pub fn get_mut(&mut self) -> &mut AudioBlock {
        self.valid = true;
        &mut self.block
    }

    /// Invalidate the cache
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// Check if computed
    pub fn is_computed(&self) -> bool {
        self.valid
    }
}

/// Stereo audio block pair
#[derive(Clone)]
pub struct StereoBlock {
    /// Left channel
    pub left: AudioBlock,
    /// Right channel
    pub right: AudioBlock,
}

impl StereoBlock {
    /// Create a new stereo block with the given size
    pub fn new(size: usize) -> Self {
        Self {
            left: AudioBlock::new(size),
            right: AudioBlock::new(size),
        }
    }

    /// Get the block size
    pub fn len(&self) -> usize {
        self.left.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }

    /// Clear both channels
    pub fn clear(&mut self) {
        self.left.clear();
        self.right.clear();
    }

    /// Apply gain to both channels
    pub fn apply_gain(&mut self, gain: f64) {
        self.left.mul_scalar(gain);
        self.right.mul_scalar(gain);
    }

    /// Apply stereo panning
    /// pan: -1.0 (full left) to 1.0 (full right)
    pub fn apply_pan(&mut self, pan: f64) {
        let pan_angle = (pan + 1.0) * PI / 4.0;
        let left_gain = Libm::<f64>::cos(pan_angle);
        let right_gain = Libm::<f64>::sin(pan_angle);

        self.left.mul_scalar(left_gain);
        self.right.mul_scalar(right_gain);
    }

    /// Mix another stereo block into this one
    pub fn mix(&mut self, other: &StereoBlock) {
        self.left.add_block(&other.left);
        self.right.add_block(&other.right);
    }

    /// Get the peak level (max of both channels)
    pub fn peak(&self) -> f64 {
        self.left.peak().max(self.right.peak())
    }

    /// Get a stereo sample at the given index
    pub fn get_sample(&self, index: usize) -> (f64, f64) {
        (self.left.get(index), self.right.get(index))
    }

    /// Set a stereo sample at the given index
    pub fn set_sample(&mut self, index: usize, left: f64, right: f64) {
        self.left.set(index, left);
        self.right.set(index, right);
    }
}

impl Default for StereoBlock {
    fn default() -> Self {
        Self::new(DEFAULT_BLOCK_SIZE)
    }
}

/// Ring buffer for delay lines and lookahead.
///
/// # Capacity semantics
///
/// `capacity` is the requested capacity and the maximum valid delay tap:
/// [`read`](Self::read) returns `0.0` for any `delay >= capacity`, so taps
/// beyond the requested capacity stay bounded even though the internal storage
/// may be larger. Internally the backing storage is rounded up to a power of
/// two so the per-sample wrap is a bitmask (`& (len - 1)`) rather than a modulo
/// — the slowest integer op — in the real-time audio path. A requested
/// capacity of `0` is clamped to a single-slot internal buffer so
/// [`write`](Self::write)/[`read`](Self::read) never index an empty `Vec` or
/// divide by zero; `len`/`is_empty` still report the requested capacity.
pub struct RingBuffer {
    buffer: Vec<f64>,
    write_pos: usize,
    /// Requested capacity (public delay-tap bound).
    capacity: usize,
    /// `buffer.len() - 1`; `buffer.len()` is a power of two, so this is a mask.
    mask: usize,
}

impl RingBuffer {
    /// Create a new ring buffer with the given capacity.
    ///
    /// The internal storage is rounded up to a power of two (and to at least
    /// one slot when `capacity == 0`) so wrapping uses a bitmask; the public
    /// capacity reported by [`len`](Self::len) is the value requested here.
    pub fn new(capacity: usize) -> Self {
        // Clamp internal storage to at least one slot so a zero-capacity buffer
        // (nonsensical but constructible via the public API) cannot panic on
        // write()/read(); round up to a power of two for masked wrapping.
        let internal = capacity.max(1).next_power_of_two();
        Self {
            buffer: vec![0.0; internal],
            write_pos: 0,
            capacity,
            mask: internal - 1,
        }
    }

    /// Get the requested buffer capacity (maximum valid delay tap).
    pub fn len(&self) -> usize {
        self.capacity
    }

    /// Check if empty (requested capacity is zero).
    pub fn is_empty(&self) -> bool {
        self.capacity == 0
    }

    /// Write a sample and return the oldest sample.
    pub fn write(&mut self, sample: f64) -> f64 {
        let old = self.buffer[self.write_pos];
        self.buffer[self.write_pos] = sample;
        // Power-of-two mask wrap instead of `% size`.
        self.write_pos = (self.write_pos + 1) & self.mask;
        old
    }

    /// Read a sample with the given delay (in samples).
    ///
    /// Returns `0.0` for `delay >= capacity` (the requested capacity), so taps
    /// beyond the requested capacity are bounded.
    pub fn read(&self, delay: usize) -> f64 {
        if delay >= self.capacity {
            return 0.0;
        }
        // buffer.len() == mask + 1 is a power of two, so masking equals modulo.
        let read_pos = (self.write_pos + self.buffer.len() - delay - 1) & self.mask;
        self.buffer[read_pos]
    }

    /// Read with fractional delay using linear interpolation.
    ///
    /// The interpolated result is passed through
    /// `flush_denorm`: this is the
    /// read side of feedback delay lines, where a decaying loop asymptotes to
    /// subnormals that incur large CPU penalties. Flushing sub-threshold values
    /// to exactly `0.0` keeps the feedback path real-time-safe.
    pub fn read_interp(&self, delay: f64) -> f64 {
        let delay_floor = Libm::<f64>::floor(delay);
        let delay_int = delay_floor as usize;
        let frac = delay - delay_floor;

        let s1 = self.read(delay_int);
        let s2 = self.read(delay_int + 1);

        crate::modules::common::flush_denorm(s1 + frac * (s2 - s1))
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}

/// Processing context for block-oriented operations
pub struct ProcessContext {
    /// Sample rate
    pub sample_rate: f64,
    /// Block size
    pub block_size: usize,
    /// Current sample position (absolute)
    pub sample_position: u64,
    /// Tempo (BPM) if known
    pub tempo: Option<f64>,
    /// Time signature (numerator, denominator) if known
    pub time_signature: Option<(u32, u32)>,
}

impl ProcessContext {
    /// Create a new processing context
    pub fn new(sample_rate: f64, block_size: usize) -> Self {
        Self {
            sample_rate,
            block_size,
            sample_position: 0,
            tempo: None,
            time_signature: None,
        }
    }

    /// Get the current time in seconds
    pub fn time_seconds(&self) -> f64 {
        self.sample_position as f64 / self.sample_rate
    }

    /// Advance the position by one block
    pub fn advance(&mut self) {
        self.sample_position += self.block_size as u64;
    }

    /// Reset to the beginning
    pub fn reset(&mut self) {
        self.sample_position = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_block_basic() {
        let mut block = AudioBlock::new(64);
        assert_eq!(block.len(), 64);

        block.set(0, 1.0);
        block.set(63, -1.0);
        assert_eq!(block.get(0), 1.0);
        assert_eq!(block.get(63), -1.0);
        assert_eq!(block.get(100), 0.0); // Out of bounds
    }

    #[test]
    fn test_audio_block_operations() {
        let mut block = AudioBlock::constant(4, 2.0);

        block.add_scalar(1.0);
        assert_eq!(block.get(0), 3.0);

        block.mul_scalar(2.0);
        assert_eq!(block.get(0), 6.0);
    }

    #[test]
    fn test_audio_block_block_ops() {
        let mut a = AudioBlock::constant(4, 2.0);
        let b = AudioBlock::constant(4, 3.0);

        a.add_block(&b);
        assert_eq!(a.get(0), 5.0);

        a.mul_block(&b);
        assert_eq!(a.get(0), 15.0);
    }

    #[test]
    fn test_audio_block_stats() {
        let block = AudioBlock::from_samples(vec![1.0, -2.0, 1.5, -1.5]);

        assert_eq!(block.peak(), 2.0);
        assert!((block.rms() - 1.541).abs() < 0.01);
    }

    #[test]
    fn test_stereo_block() {
        let mut stereo = StereoBlock::new(4);

        stereo.set_sample(0, 1.0, 0.5);
        let (l, r) = stereo.get_sample(0);
        assert_eq!(l, 1.0);
        assert_eq!(r, 0.5);

        stereo.apply_gain(2.0);
        let (l, r) = stereo.get_sample(0);
        assert_eq!(l, 2.0);
        assert_eq!(r, 1.0);
    }

    #[test]
    fn test_ring_buffer() {
        let mut ring = RingBuffer::new(4);

        // Write and read
        ring.write(1.0);
        ring.write(2.0);
        ring.write(3.0);

        assert_eq!(ring.read(0), 3.0); // Most recent
        assert_eq!(ring.read(1), 2.0);
        assert_eq!(ring.read(2), 1.0);
    }

    #[test]
    fn test_ring_buffer_interp() {
        let mut ring = RingBuffer::new(4);

        ring.write(0.0);
        ring.write(1.0);
        ring.write(2.0);

        // Interpolated read
        let interp = ring.read_interp(0.5);
        assert!((interp - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_lazy_signal() {
        let mut lazy = LazySignal::new(|| 42.0);

        // First get should compute
        assert_eq!(lazy.get(), 42.0);
        assert!(lazy.is_computed());

        // Second get should use cache
        assert_eq!(lazy.get(), 42.0);

        // Invalidate and recompute
        lazy.invalidate();
        assert!(!lazy.is_computed());
    }

    #[test]
    fn test_lazy_block() {
        let mut lazy = LazyBlock::new(4);

        let block = lazy.get(|b| {
            b.fill(5.0);
        });
        assert_eq!(block.get(0), 5.0);
        assert!(lazy.is_computed());

        lazy.invalidate();
        assert!(!lazy.is_computed());
    }

    #[test]
    fn test_process_context() {
        let mut ctx = ProcessContext::new(44100.0, 64);

        assert_eq!(ctx.sample_position, 0);
        assert_eq!(ctx.time_seconds(), 0.0);

        ctx.advance();
        assert_eq!(ctx.sample_position, 64);
        assert!((ctx.time_seconds() - 64.0 / 44100.0).abs() < 0.0001);
    }

    #[test]
    fn test_audio_block_constant() {
        let block = AudioBlock::constant(8, 5.0);
        assert_eq!(block.get(0), 5.0);
        assert_eq!(block.get(7), 5.0);
    }

    #[test]
    fn test_audio_block_from_samples() {
        let samples = vec![1.0, 2.0, 3.0, 4.0];
        let block = AudioBlock::from_samples(samples);
        assert_eq!(block.len(), 4);
        assert_eq!(block.get(0), 1.0);
        assert_eq!(block.get(3), 4.0);
    }

    #[test]
    fn test_audio_block_is_empty() {
        let empty = AudioBlock::new(0);
        assert!(empty.is_empty());

        let non_empty = AudioBlock::new(4);
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_audio_block_as_slice() {
        let mut block = AudioBlock::new(4);
        block.fill(2.0);

        let slice = block.as_slice();
        assert_eq!(slice.len(), 4);
        assert_eq!(slice[0], 2.0);

        let mut_slice = block.as_mut_slice();
        mut_slice[0] = 99.0;
        assert_eq!(block.get(0), 99.0);
    }

    #[test]
    fn test_audio_block_add_scalar() {
        let mut block = AudioBlock::from_samples(vec![1.0, 2.0, 3.0, 4.0]);
        block.add_scalar(10.0);
        assert_eq!(block.get(0), 11.0);
        assert_eq!(block.get(3), 14.0);
    }

    #[test]
    fn test_audio_block_add_block() {
        let mut block1 = AudioBlock::from_samples(vec![1.0, 2.0, 3.0, 4.0]);
        let block2 = AudioBlock::from_samples(vec![10.0, 20.0, 30.0, 40.0]);
        block1.add_block(&block2);
        assert_eq!(block1.get(0), 11.0);
        assert_eq!(block1.get(3), 44.0);
    }

    #[test]
    fn test_audio_block_mul_block() {
        let mut block1 = AudioBlock::from_samples(vec![1.0, 2.0, 3.0, 4.0]);
        let block2 = AudioBlock::from_samples(vec![2.0, 2.0, 2.0, 2.0]);
        block1.mul_block(&block2);
        assert_eq!(block1.get(0), 2.0);
        assert_eq!(block1.get(3), 8.0);
    }

    #[test]
    fn test_audio_block_map() {
        let mut block = AudioBlock::from_samples(vec![1.0, 2.0, 3.0, 4.0]);
        block.map(|x| x * 2.0);
        assert_eq!(block.get(0), 2.0);
        assert_eq!(block.get(3), 8.0);
    }

    #[test]
    fn test_audio_block_hard_clip() {
        let mut block = AudioBlock::from_samples(vec![-10.0, -1.0, 0.0, 1.0, 10.0]);
        block.hard_clip(5.0);
        assert_eq!(block.get(0), -5.0);
        assert_eq!(block.get(4), 5.0);
    }

    #[test]
    fn test_audio_block_copy_from() {
        let source = AudioBlock::from_samples(vec![1.0, 2.0, 3.0, 4.0]);
        let mut dest = AudioBlock::new(4);
        dest.copy_from(&source);
        assert_eq!(dest.get(0), 1.0);
        assert_eq!(dest.get(3), 4.0);
    }

    #[test]
    fn test_stereo_block_default() {
        let stereo = StereoBlock::default();
        assert_eq!(stereo.len(), DEFAULT_BLOCK_SIZE);
    }

    #[test]
    fn test_stereo_block_get_set_sample() {
        let mut stereo = StereoBlock::new(4);
        stereo.set_sample(0, 1.0, 2.0);
        let (l, r) = stereo.get_sample(0);
        assert_eq!(l, 1.0);
        assert_eq!(r, 2.0);
    }

    #[test]
    fn test_stereo_block_apply_pan() {
        let mut stereo = StereoBlock::new(4);
        stereo.left.fill(1.0);
        stereo.right.fill(1.0);

        stereo.apply_pan(-1.0); // Full left
        assert!(stereo.left.peak() > stereo.right.peak());
    }

    #[test]
    fn test_stereo_block_mix() {
        let mut stereo1 = StereoBlock::new(4);
        stereo1.left.fill(1.0);
        stereo1.right.fill(1.0);

        let mut stereo2 = StereoBlock::new(4);
        stereo2.left.fill(2.0);
        stereo2.right.fill(2.0);

        stereo1.mix(&stereo2);
        assert_eq!(stereo1.left.get(0), 3.0);
        assert_eq!(stereo1.right.get(0), 3.0);
    }

    #[test]
    fn test_ring_buffer_is_empty() {
        let buf = RingBuffer::new(4);
        assert!(!buf.is_empty());

        let empty_buf = RingBuffer::new(0);
        assert!(empty_buf.is_empty());
    }

    #[test]
    fn test_ring_buffer_read_interp() {
        let mut buf = RingBuffer::new(4);
        buf.write(1.0);
        buf.write(3.0);

        // Interpolated read at 0.5 should be between 1.0 and 3.0
        let interp = buf.read_interp(0.5);
        assert!(interp > 1.0 && interp < 3.0);
    }

    #[test]
    fn test_ring_buffer_clear() {
        let mut buf = RingBuffer::new(4);
        buf.write(10.0);
        buf.write(20.0);

        buf.clear();
        assert_eq!(buf.read(0), 0.0);
        assert_eq!(buf.read(1), 0.0);
    }

    #[test]
    fn test_block_processor_process_samples() {
        use crate::modules::Vco;
        use crate::port::BlockPortValues;

        let processor = BlockProcessor::new(64, 44100.0);

        let mut vco = Vco::new(44100.0);
        let inputs = BlockPortValues::new(64);
        let mut outputs = BlockPortValues::new(64);

        processor.process_samples(&mut vco, &inputs, &mut outputs);
        assert_eq!(processor.block_size(), 64);
    }

    #[test]
    fn test_lazy_block_get_mut() {
        let mut lazy = LazyBlock::new(4);
        let block = lazy.get_mut();
        block.fill(42.0);
        assert!(lazy.is_computed());
        assert_eq!(lazy.get(|_| {}).get(0), 42.0);
    }

    #[test]
    fn test_process_context_reset() {
        let mut ctx = ProcessContext::new(44100.0, 64);
        ctx.advance();
        ctx.advance();

        ctx.reset();
        assert_eq!(ctx.sample_position, 0);
    }

    // Q061: a zero-capacity ring buffer must not panic on write/read.
    #[test]
    fn test_ring_buffer_zero_capacity_no_panic() {
        let mut ring = RingBuffer::new(0);
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);
        // Previously: OOB index into an empty Vec + modulo-by-zero.
        let old = ring.write(1.0);
        assert_eq!(old, 0.0);
        assert_eq!(ring.read(0), 0.0); // delay >= capacity(0) -> bounded to 0.0
        assert_eq!(ring.read_interp(0.0), 0.0);
    }

    // Q060: power-of-two internal storage preserves read/write index semantics
    // and bounds delay taps to the requested capacity.
    #[test]
    fn test_ring_buffer_power_of_two_wrap_semantics() {
        // Requested capacity 3 rounds internal storage up to 4.
        let mut ring = RingBuffer::new(3);
        assert_eq!(ring.len(), 3);
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            ring.write(v);
        }
        // Most-recent-first within the requested capacity.
        assert_eq!(ring.read(0), 5.0);
        assert_eq!(ring.read(1), 4.0);
        assert_eq!(ring.read(2), 3.0);
        // Delay at or beyond the requested capacity stays bounded to 0.0.
        assert_eq!(ring.read(3), 0.0);
        assert_eq!(ring.read(100), 0.0);
    }

    // Q057: the feedback read path flushes sub-threshold values to exactly 0.0.
    #[test]
    fn test_ring_buffer_read_interp_flushes_denormals() {
        let mut ring = RingBuffer::new(8);

        // A single sub-threshold value read back must flush to exactly 0.0.
        ring.write(1e-300); // far below the 1e-20 flush threshold
        ring.write(1e-300);
        assert_eq!(ring.read_interp(0.5), 0.0);

        // A decaying feedback loop must settle at exactly 0.0 rather than
        // lingering at a tiny (denormal-prone) value. Without flushing, `out`
        // would be ~0.5^200 (a nonzero normal number), not 0.0.
        ring.clear();
        ring.write(1.0);
        let feedback = 0.5;
        let mut out = 0.0;
        for _ in 0..200 {
            let delayed = ring.read_interp(0.0);
            out = delayed * feedback;
            ring.write(out);
        }
        assert_eq!(out, 0.0, "decaying feedback loop must reach exactly 0.0");
    }

    // Q062: the SIMD block ops must agree bit-for-bit with a scalar reference
    // over random-length, non-multiple-of-4 blocks (exercising the vectorized
    // chunks and the scalar remainder). Gated on `simd` so the AudioBlock
    // methods here are the vectorized `wide::f64x4` path.
    #[cfg(feature = "simd")]
    #[test]
    fn test_simd_scalar_equivalence_nonmultiple_of_four() {
        use crate::rng::Rng;

        let mut rng = Rng::from_seed(0xC0FFEE);
        for &len in &[1usize, 2, 3, 5, 6, 7, 9, 13, 17, 31, 63, 100, 127] {
            let data: Vec<f64> = (0..len).map(|_| rng.next_f64_bipolar()).collect();
            let other: Vec<f64> = (0..len).map(|_| rng.next_f64_bipolar()).collect();
            let s = rng.next_f64_bipolar();

            // add_scalar
            let mut blk = AudioBlock::from_samples(data.clone());
            blk.add_scalar(s);
            let expect: Vec<f64> = data.iter().map(|x| x + s).collect();
            assert_eq!(blk.as_slice(), expect.as_slice(), "add_scalar len={len}");

            // mul_scalar
            let mut blk = AudioBlock::from_samples(data.clone());
            blk.mul_scalar(s);
            let expect: Vec<f64> = data.iter().map(|x| x * s).collect();
            assert_eq!(blk.as_slice(), expect.as_slice(), "mul_scalar len={len}");

            // add_block
            let mut blk = AudioBlock::from_samples(data.clone());
            blk.add_block(&AudioBlock::from_samples(other.clone()));
            let expect: Vec<f64> = data.iter().zip(&other).map(|(a, b)| a + b).collect();
            assert_eq!(blk.as_slice(), expect.as_slice(), "add_block len={len}");

            // mul_block
            let mut blk = AudioBlock::from_samples(data.clone());
            blk.mul_block(&AudioBlock::from_samples(other.clone()));
            let expect: Vec<f64> = data.iter().zip(&other).map(|(a, b)| a * b).collect();
            assert_eq!(blk.as_slice(), expect.as_slice(), "mul_block len={len}");
        }
    }
}
