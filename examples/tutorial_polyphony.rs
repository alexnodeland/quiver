//! Tutorial: Polyphonic Patches
//!
//! Demonstrates voice allocation for playing multiple simultaneous notes.
//! This is essential for keyboard-style synthesizers.
//!
//! # Why voice allocation is its own problem
//!
//! A single VCO/VCF/VCA chain can only play one note at a time. Polyphony
//! means running several of those chains ("voices") in parallel and
//! deciding, for each incoming note, *which* voice plays it:
//! - With fewer voices than notes played at once, something has to give —
//!   that's what `AllocationMode` governs (steal the oldest note? the
//!   quietest one? refuse the new note entirely?). Real keyboards make this
//!   same tradeoff; even 16-32 voices can be exhausted by a sustain pedal
//!   held through a fast run.
//! - Mixing N simultaneous voices multiplies their combined peak amplitude
//!   roughly by N if they're in phase, so naively summing voices can clip.
//!   `PolyPatch` applies gain compensation (dividing by roughly `sqrt(N)`,
//!   matching how uncorrelated signals combine in power rather than
//!   amplitude) so a 4-note chord doesn't come out 4x louder than one note.
//! - `PolyPatch::with_voice_fn` builds one identical copy of your voice
//!   graph per voice and wires a per-voice controller into it exposing
//!   `voct`/`gate`/`trigger`/`velocity` — the same four signals a
//!   monophonic patch would drive by hand (see `tutorial_envelope.rs`),
//!   just supplied automatically by the allocator instead of external inputs
//!   you manage yourself.
//!
//! Run with: cargo run --example tutorial_polyphony

use quiver::prelude::*;

fn main() {
    let num_voices = 4;

    println!("=== Polyphony Demo ===\n");
    println!("Simulating a {}-voice polyphonic synthesizer\n", num_voices);

    // Create a voice allocator
    let mut allocator = VoiceAllocator::new(num_voices);

    // Helper to convert MIDI note to V/Oct
    fn midi_to_voct(note: u8) -> f64 {
        (note as f64 - 60.0) / 12.0
    }

    fn note_name(note: u8) -> String {
        let names = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];
        let octave = (note / 12) as i32 - 1;
        format!("{}{}", names[(note % 12) as usize], octave)
    }

    // Simulate playing a chord: C4, E4, G4, B4 (Cmaj7)
    let chord = [60u8, 64, 67, 71]; // C4, E4, G4, B4

    println!("Playing Cmaj7 chord:");
    for &note in &chord {
        if let Some(voice_idx) = allocator.note_on(note, 0.8) {
            println!(
                "  {} (MIDI {}) → Voice {}, V/Oct = {:.3}V",
                note_name(note),
                note,
                voice_idx,
                midi_to_voct(note)
            );
        } else {
            println!(
                "  {} (MIDI {}) → No voice available!",
                note_name(note),
                note
            );
        }
    }

    // Show voice states
    println!("\nVoice states after chord:");
    for i in 0..num_voices {
        if let Some(voice) = allocator.voice(i) {
            match voice.state {
                VoiceState::Active => {
                    if let Some(note) = voice.note {
                        println!(
                            "  Voice {}: Active, playing {} (V/Oct: {:.3}V)",
                            i,
                            note_name(note),
                            voice.voct
                        );
                    }
                }
                VoiceState::Free => println!("  Voice {}: Free", i),
                VoiceState::Releasing => println!("  Voice {}: Releasing", i),
            }
        }
    }

    // Now try to play another note - will steal!
    println!("\nPlaying D5 (MIDI 74) - all voices busy, must steal:");
    if let Some(stolen_voice) = allocator.note_on(74, 0.9) {
        println!(
            "  D5 assigned to Voice {} (stolen from previous note)",
            stolen_voice
        );
    } else {
        println!("  D5 could not be allocated (NoSteal mode)");
    }

    // Show updated states
    println!("\nVoice states after steal:");
    for i in 0..num_voices {
        if let Some(voice) = allocator.voice(i) {
            match voice.state {
                VoiceState::Active => {
                    if let Some(note) = voice.note {
                        println!(
                            "  Voice {}: Active, playing {} (V/Oct: {:.3}V)",
                            i,
                            note_name(note),
                            voice.voct
                        );
                    }
                }
                VoiceState::Free => println!("  Voice {}: Free", i),
                VoiceState::Releasing => println!("  Voice {}: Releasing", i),
            }
        }
    }

    // Release some notes
    println!("\nReleasing E4 and G4:");
    allocator.note_off(64); // E4
    allocator.note_off(67); // G4

    println!("\nVoice states after release:");
    for i in 0..num_voices {
        if let Some(voice) = allocator.voice(i) {
            match voice.state {
                VoiceState::Active => {
                    if let Some(note) = voice.note {
                        println!("  Voice {}: Active, {}", i, note_name(note));
                    }
                }
                VoiceState::Free => println!("  Voice {}: Free", i),
                VoiceState::Releasing => {
                    if let Some(note) = voice.note {
                        println!("  Voice {}: Releasing (was {})", i, note_name(note));
                    }
                }
            }
        }
    }

    // Demonstrate different allocation modes
    println!("\n--- Allocation Modes ---\n");

    for mode in [
        AllocationMode::RoundRobin,
        AllocationMode::QuietestSteal,
        AllocationMode::OldestSteal,
        AllocationMode::NoSteal,
        AllocationMode::HighestPriority,
        AllocationMode::LowestPriority,
    ] {
        let mode_name = match mode {
            AllocationMode::RoundRobin => "RoundRobin",
            AllocationMode::QuietestSteal => "QuietestSteal",
            AllocationMode::OldestSteal => "OldestSteal",
            AllocationMode::NoSteal => "NoSteal",
            AllocationMode::HighestPriority => "HighestPriority",
            AllocationMode::LowestPriority => "LowestPriority",
        };

        let desc = match mode {
            AllocationMode::RoundRobin => "Cycles through voices in order",
            AllocationMode::QuietestSteal => "Steals the voice with lowest envelope",
            AllocationMode::OldestSteal => "Steals the note held longest",
            AllocationMode::NoSteal => "Ignores new notes when full",
            AllocationMode::HighestPriority => "Higher notes can steal lower",
            AllocationMode::LowestPriority => "Lower notes can steal higher",
        };

        println!("{}: {}", mode_name, desc);
    }

    // ------------------------------------------------------------------
    // Building an ACTUAL polyphonic synthesizer with PolyPatch
    // ------------------------------------------------------------------
    // PolyPatch inserts an in-graph voice controller into each voice, so the
    // allocator's per-voice pitch/gate signals actually reach real DSP. Here
    // each voice is: voice controller -> Vco -> Vca (shaped by an Adsr) -> out.
    println!("\n--- Audible PolyPatch (4 voices) ---\n");

    let sample_rate = 48_000.0;
    let mut synth = PolyPatch::with_voice_fn(num_voices, sample_rate, |patch, ctrl| {
        let sr = patch.sample_rate();
        let vco = patch.add("vco", Vco::new(sr));
        let adsr = patch.add("adsr", Adsr::new(sr));
        let vca = patch.add("vca", Vca::new());
        let out = patch.add("out", StereoOutput::new());
        // The controller exposes voct / gate / trigger / velocity outputs.
        patch.connect(ctrl.out("voct"), vco.in_("voct"))?;
        patch.connect(ctrl.out("gate"), adsr.in_("gate"))?;
        patch.connect(vco.out("saw"), vca.in_("in"))?;
        patch.connect(adsr.out("env"), vca.in_("cv"))?;
        patch.connect(vca.out("out"), out.in_("left"))?;
        patch.set_output(out.id());
        Ok(())
    })
    .expect("failed to build voice graph");

    // Play the Cmaj7 chord. Voices output modular-level (~±5 V) audio.
    for &note in &chord {
        synth.note_on(note, 100);
    }
    // Let the envelopes settle past the onset transient, then measure the
    // gain-compensated steady-state peak (1/sqrt(N) keeps chords from clipping).
    for _ in 0..(sample_rate as usize / 10) {
        synth.tick();
    }
    let mut peak = 0.0f64;
    for _ in 0..(sample_rate as usize / 10) {
        let (l, r) = synth.tick();
        peak = peak.max(l.abs()).max(r.abs());
    }
    println!(
        "  {} voices sounding; gain-compensated steady peak = {:.3} (audible, bounded)",
        synth.allocator().active_count(),
        peak
    );

    // Release the chord: the ADSR release tails complete before voices free,
    // and the summed output is gain-compensated so chords do not clip.
    synth.all_notes_off();
    for _ in 0..(sample_rate as usize / 4) {
        synth.tick();
    }
    println!(
        "  After release: {} voices still freeing (tails complete, not truncated).",
        synth.allocator().active_count()
    );

    println!("\nPolyphony enables expressive keyboard playing and chord voicings.");
}
