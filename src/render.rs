//! Offline rendering and WAV export (Q145).
//!
//! Renders a [`Patch`] to interleaved audio and writes a standard 16-bit PCM
//! stereo `.wav` file with a hand-rolled 44-byte RIFF/WAVE header (no external
//! dependency such as `hound`).
//!
//! Requires the `std` feature (file and stream I/O).
//!
//! # Sample scale
//!
//! [`render`] returns the patch's raw `f64` output samples. [`render_to_wav`]
//! converts them to 16-bit PCM assuming a normalized `[-1.0, 1.0]` full-scale
//! range and clamps anything outside it. Patches that follow the library's ±5 V
//! `Audio` convention should scale their output down (e.g. via a `Vca` or
//! `Attenuverter`) before export, otherwise the signal clips at full scale.

use crate::graph::Patch;
use std::io::{self, Write};
use std::path::Path;

/// Bits per sample for the exported PCM data.
const BITS_PER_SAMPLE: u16 = 16;
/// Channel count (stereo).
const NUM_CHANNELS: u16 = 2;
/// Size in bytes of the canonical PCM WAV header.
pub const WAV_HEADER_LEN: usize = 44;

/// Render `seconds` of audio from `patch`, returning `(left, right)` sample
/// buffers.
///
/// The number of frames is `round(seconds * patch.sample_rate())`. The patch is
/// advanced with [`Patch::tick_block`], which lazily compiles it if needed (so a
/// freshly-built patch renders without an explicit `compile()` call) and drives
/// the same per-sample engine as [`Patch::tick`] with no per-frame allocation.
/// The rendered samples are therefore identical to ticking one sample at a time.
pub fn render(patch: &mut Patch, seconds: f64) -> (Vec<f64>, Vec<f64>) {
    let sr = patch.sample_rate();
    let frames = if seconds > 0.0 && sr > 0.0 {
        (seconds * sr).round() as usize
    } else {
        0
    };
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    patch.tick_block(&mut left, &mut right);
    (left, right)
}

/// Render `seconds` of audio from `patch` and write it to `path` as a 16-bit PCM
/// stereo WAV file.
pub fn render_to_wav(patch: &mut Patch, seconds: f64, path: &Path) -> io::Result<()> {
    let sample_rate = patch.sample_rate() as u32;
    let (left, right) = render(patch, seconds);
    write_wav(path, sample_rate, &left, &right)
}

/// Write interleaved `left`/`right` samples to `path` as 16-bit PCM stereo WAV.
///
/// `left` and `right` are treated as full-scale `[-1.0, 1.0]`; out-of-range values
/// are clamped. If the channel lengths differ, the shorter length is used.
pub fn write_wav(path: &Path, sample_rate: u32, left: &[f64], right: &[f64]) -> io::Result<()> {
    let frames = left.len().min(right.len());
    let bytes = build_wav(sample_rate, &left[..frames], &right[..frames]);
    let mut file = std::fs::File::create(path)?;
    file.write_all(&bytes)?;
    file.flush()
}

/// Build the full WAV byte stream (header + PCM data) in memory.
fn build_wav(sample_rate: u32, left: &[f64], right: &[f64]) -> Vec<u8> {
    let frames = left.len().min(right.len());
    let bytes_per_sample = (BITS_PER_SAMPLE / 8) as u32;
    let data_size = frames as u32 * NUM_CHANNELS as u32 * bytes_per_sample;
    let byte_rate = sample_rate * NUM_CHANNELS as u32 * bytes_per_sample;
    let block_align = NUM_CHANNELS * BITS_PER_SAMPLE / 8;

    let mut out = Vec::with_capacity(WAV_HEADER_LEN + data_size as usize);

    // ---- RIFF chunk descriptor ----
    out.extend_from_slice(b"RIFF");
    // ChunkSize = 36 + Subchunk2Size (everything after this field).
    out.extend_from_slice(&(36 + data_size).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    // ---- "fmt " subchunk ----
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size (16 for PCM)
    out.extend_from_slice(&1u16.to_le_bytes()); // AudioFormat = 1 (PCM)
    out.extend_from_slice(&NUM_CHANNELS.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

    // ---- "data" subchunk ----
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());

    // Interleaved little-endian i16 samples.
    for i in 0..frames {
        out.extend_from_slice(&to_i16(left[i]).to_le_bytes());
        out.extend_from_slice(&to_i16(right[i]).to_le_bytes());
    }

    out
}

/// Convert a full-scale `[-1.0, 1.0]` sample to 16-bit PCM, clamping out-of-range.
#[inline]
fn to_i16(sample: f64) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::{Offset, StereoOutput, Vca};

    /// Minimal parser of the fields we write, for round-trip verification.
    struct WavHeader {
        sample_rate: u32,
        channels: u16,
        bits: u16,
        data_size: u32,
    }

    fn parse_header(bytes: &[u8]) -> WavHeader {
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");
        let u16at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
        let u32at =
            |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        WavHeader {
            channels: u16at(22),
            sample_rate: u32at(24),
            bits: u16at(34),
            data_size: u32at(40),
        }
    }

    /// A patch that outputs a constant `value` on both channels.
    fn constant_patch(sr: f64, value: f64) -> Patch {
        let mut patch = Patch::new(sr);
        let off = patch.add("dc", Offset::new(value));
        let out = patch.add("out", StereoOutput::new());
        // Offset "out" -> StereoOutput "left" (right normals to left).
        patch.connect(off.out("out"), out.in_("left")).unwrap();
        patch.set_output(out.id());
        patch.compile().unwrap();
        patch
    }

    #[test]
    fn test_render_sample_count_matches() {
        let sr = 48000.0;
        let mut patch = constant_patch(sr, 0.0);
        let (left, right) = render(&mut patch, 0.5);
        assert_eq!(left.len(), 24000);
        assert_eq!(right.len(), 24000);
    }

    #[test]
    fn test_render_zero_seconds_is_empty() {
        let mut patch = constant_patch(44100.0, 0.0);
        let (left, right) = render(&mut patch, 0.0);
        assert!(left.is_empty());
        assert!(right.is_empty());
    }

    #[test]
    fn test_wav_header_bytes_correct() {
        let sr = 44100;
        // 100 frames of silence.
        let left = vec![0.0; 100];
        let right = vec![0.0; 100];
        let bytes = build_wav(sr, &left, &right);

        assert_eq!(bytes.len(), WAV_HEADER_LEN + 100 * 2 * 2);
        let h = parse_header(&bytes);
        assert_eq!(h.sample_rate, 44100);
        assert_eq!(h.channels, 2);
        assert_eq!(h.bits, 16);
        assert_eq!(h.data_size, 100 * 2 * 2);

        // RIFF ChunkSize = 36 + data.
        let chunk_size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(chunk_size, 36 + h.data_size);
    }

    #[test]
    fn test_wav_roundtrip_own_header() {
        let sr = 22050;
        let left = vec![0.25, -0.5, 1.0, -1.0];
        let right = vec![0.0, 0.1, -0.1, 0.5];
        let bytes = build_wav(sr, &left, &right);
        let h = parse_header(&bytes);
        assert_eq!(h.sample_rate, sr);
        assert_eq!(h.data_size as usize, left.len() * 2 * 2);

        // First interleaved sample pair decodes to expected i16 values.
        let s0 = i16::from_le_bytes([bytes[44], bytes[45]]);
        assert_eq!(s0, (0.25_f64 * 32767.0).round() as i16);
        // Full-scale +1.0 clamps to +32767, not overflow.
        let s2_left = i16::from_le_bytes([bytes[44 + 8], bytes[44 + 9]]);
        assert_eq!(s2_left, 32767);
    }

    #[test]
    fn test_silent_patch_zero_samples() {
        let sr = 8000.0;
        let mut patch = constant_patch(sr, 0.0);
        let (left, right) = render(&mut patch, 0.1);
        assert!(left.iter().all(|&s| s == 0.0));
        assert!(right.iter().all(|&s| s == 0.0));

        let bytes = build_wav(sr as u32, &left, &right);
        // All PCM data bytes are zero.
        assert!(bytes[WAV_HEADER_LEN..].iter().all(|&b| b == 0));
    }

    #[test]
    fn test_render_to_wav_file() {
        let sr = 16000.0;
        // A patch outputting a small constant so the file has non-zero content.
        let mut patch = Patch::new(sr);
        let dc = patch.add("dc", Offset::new(1.0));
        let vca = patch.add("vca", Vca::new());
        let out = patch.add("out", StereoOutput::new());
        // Scale the 1 V DC down to 0.5 full-scale via the VCA gain CV.
        patch.connect(dc.out("out"), vca.in_("in")).unwrap();
        // Vca gain CV (unipolar 0..10V) at 5V => 0.5.
        let g = patch.add("gain", Offset::new(5.0));
        patch.connect(g.out("out"), vca.in_("cv")).unwrap();
        patch.connect(vca.out("out"), out.in_("left")).unwrap();
        patch.set_output(out.id());
        patch.compile().unwrap();

        let dir = std::env::temp_dir();
        let path = dir.join("quiver_render_test.wav");
        render_to_wav(&mut patch, 0.05, &path).unwrap();

        let data = std::fs::read(&path).unwrap();
        let h = parse_header(&data);
        assert_eq!(h.sample_rate, 16000);
        assert_eq!(h.channels, 2);
        let frames = (0.05 * sr).round() as usize;
        assert_eq!(h.data_size as usize, frames * 2 * 2);

        let _ = std::fs::remove_file(&path);
    }
}
