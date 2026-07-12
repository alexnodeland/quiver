//! Render a Musical Phrase to WAV
//!
//! This is the "hear Quiver make sound" flagship example: a short sequenced
//! arpeggio through a resonant filter, shaped by an envelope, rendered
//! offline to a real `.wav` file you can play in any audio player.
//!
//! # Why this patch sounds the way it does
//!
//! - **V/Oct pitch**: the VCO's `voct` input follows the 1-volt-per-octave
//!   convention used by real modular synths — each additional volt doubles
//!   the oscillator frequency, and each `1/12`V step is one semitone. That's
//!   why converting a MIDI note to a control voltage is just
//!   `(note - 60) / 12.0` (MIDI note 60 = middle C = 0V here).
//! - **Envelope-to-filter modulation**: the same ADSR signal that shapes the
//!   VCA's amplitude also drives the filter's cutoff. This is the classic
//!   "plucked" synth-bass trick: the filter snaps open on the attack (bright
//!   pluck) and closes again as the envelope decays (a duller sustain/tail),
//!   all from one modulation source instead of two.
//! - **Gate vs. trigger timing**: each note holds its gate high for only
//!   80% of its slot before releasing, leaving an audible gap before the
//!   next note's attack — otherwise back-to-back notes at full sustain would
//!   blur together with no perceptible attack transient.
//!
//! Run with: cargo run --example render_wav

use quiver::prelude::*;
use quiver::render::write_wav;
use std::path::Path;
use std::sync::Arc;

/// Convert a MIDI note number to a V/Oct control voltage (0V = MIDI 60 / C4).
fn midi_to_voct(note: u8) -> f64 {
    (note as f64 - 60.0) / 12.0
}

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    // External inputs stand in for a sequencer: we drive pitch and gate by
    // hand, one note at a time, in the loop below.
    let pitch_cv = Arc::new(AtomicF64::new(0.0));
    let gate_cv = Arc::new(AtomicF64::new(0.0));
    let pitch = patch.add("pitch", ExternalInput::voct(Arc::clone(&pitch_cv)));
    let gate = patch.add("gate", ExternalInput::gate(Arc::clone(&gate_cv)));

    // Voice: VCO -> VCF -> VCA, the same subtractive-synthesis chain as
    // first_patch.rs, but with the envelope also opening/closing the filter.
    let vco = patch.add("vco", Vco::new(sample_rate));
    let vcf = patch.add("vcf", Svf::new(sample_rate));
    let vca = patch.add("vca", Vca::new());
    let env = patch.add("env", Adsr::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    patch.connect(pitch.out("out"), vco.in_("voct")).unwrap();
    patch.connect(gate.out("out"), env.in_("gate")).unwrap();
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
    patch.connect(vca.out("out"), output.in_("left")).unwrap();
    patch.connect(vca.out("out"), output.in_("right")).unwrap();
    // One envelope, two destinations: amplitude AND filter brightness.
    patch.connect(env.out("env"), vca.in_("cv")).unwrap();
    patch.connect(env.out("env"), vcf.in_("cutoff")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    // A one-bar arpeggio: C3 - E3 - G3 - C4, played twice.
    let phrase = [48u8, 52, 55, 60, 48, 52, 55, 60];
    let note_seconds = 0.25;
    // Hold the gate for 80% of each slot so the release tail is audible
    // before the next note's attack (see the module doc above).
    let gate_high_seconds = note_seconds * 0.8;
    let gate_low_seconds = note_seconds - gate_high_seconds;

    println!("=== Render to WAV: Arpeggio Phrase ===\n");

    let mut left_all = Vec::new();
    let mut right_all = Vec::new();

    for (i, &note) in phrase.iter().enumerate() {
        let voct = midi_to_voct(note);
        println!("Step {}: MIDI {note} ({voct:.3}V)", i + 1);

        // Gate on: set the pitch and open the gate, then render the "on"
        // portion of this note's slot.
        pitch_cv.set(voct);
        gate_cv.set(5.0);
        let (l, r) = render(&mut patch, gate_high_seconds);
        left_all.extend(l);
        right_all.extend(r);

        // Gate off: release the envelope for the remainder of the slot.
        gate_cv.set(0.0);
        let (l, r) = render(&mut patch, gate_low_seconds);
        left_all.extend(l);
        right_all.extend(r);
    }

    let peak = left_all.iter().map(|s| s.abs()).fold(0.0_f64, f64::max);
    println!(
        "\nGenerated {} samples ({:.2}s)",
        left_all.len(),
        left_all.len() as f64 / sample_rate
    );
    println!("Peak amplitude: {peak:.2}V");

    // Quiver's Audio ports use a +-5V modular convention, but a .wav file's
    // samples are full-scale +-1.0, so scale down before writing (see the
    // `# Sample scale` note on `quiver::render`). We divide by 6 rather than
    // 5 to leave a little headroom for the resonant filter's brief overshoot
    // above 5V on sharp attack transients, so the WAV doesn't clip.
    let to_full_scale = |buf: &[f64]| -> Vec<f64> { buf.iter().map(|s| s / 6.0).collect() };
    let path = Path::new("target/render_wav.wav");
    write_wav(
        path,
        sample_rate as u32,
        &to_full_scale(&left_all),
        &to_full_scale(&right_all),
    )
    .expect("failed to write WAV file");

    println!(
        "\nWrote {} - play it in any audio player to hear Quiver make sound!",
        path.display()
    );
}
