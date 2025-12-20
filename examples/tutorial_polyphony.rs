//! Tutorial: Polyphonic Patches
//!
//! Demonstrates voice allocation for playing multiple simultaneous notes.
//! This is essential for keyboard-style synthesizers.
//!
//! Run with: cargo run --example tutorial_polyphony

use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;
    let num_voices = 4;

    println!("=== Polyphony Demo ===\n");
    println!("Simulating a {}-voice polyphonic synthesizer\n", num_voices);

    // Create a voice allocator
    let mut allocator = VoiceAllocator::new(num_voices, AllocationMode::RoundRobin);

    // Helper to convert MIDI note to V/Oct
    fn midi_to_voct(note: u8) -> f64 {
        (note as f64 - 60.0) / 12.0
    }

    fn note_name(note: u8) -> String {
        let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
        let octave = (note / 12) as i32 - 1;
        format!("{}{}", names[(note % 12) as usize], octave)
    }

    // Simulate playing a chord: C4, E4, G4, B4 (Cmaj7)
    let chord = [60u8, 64, 67, 71]; // C4, E4, G4, B4

    println!("Playing Cmaj7 chord:");
    for &note in &chord {
        let voice_idx = allocator.note_on(note, 0.8);
        println!("  {} (MIDI {}) → Voice {}, V/Oct = {:.3}V",
                 note_name(note), note, voice_idx, midi_to_voct(note));
    }

    // Show voice states
    println!("\nVoice states after chord:");
    for i in 0..num_voices {
        let state = allocator.voice(i);
        match state.state {
            VoiceState::Active => {
                if let Some(note) = state.note {
                    println!("  Voice {}: Active, playing {} (V/Oct: {:.3}V)",
                             i, note_name(note), state.voct);
                }
            }
            VoiceState::Free => println!("  Voice {}: Free", i),
            VoiceState::Releasing => println!("  Voice {}: Releasing", i),
        }
    }

    // Now try to play another note - will steal!
    println!("\nPlaying D5 (MIDI 74) - all voices busy, must steal:");
    let stolen_voice = allocator.note_on(74, 0.9);
    println!("  D5 assigned to Voice {} (stolen from previous note)", stolen_voice);

    // Show updated states
    println!("\nVoice states after steal:");
    for i in 0..num_voices {
        let state = allocator.voice(i);
        match state.state {
            VoiceState::Active => {
                if let Some(note) = state.note {
                    println!("  Voice {}: Active, playing {} (V/Oct: {:.3}V)",
                             i, note_name(note), state.voct);
                }
            }
            VoiceState::Free => println!("  Voice {}: Free", i),
            VoiceState::Releasing => println!("  Voice {}: Releasing", i),
        }
    }

    // Release some notes
    println!("\nReleasing E4 and G4:");
    allocator.note_off(64); // E4
    allocator.note_off(67); // G4

    println!("\nVoice states after release:");
    for i in 0..num_voices {
        let state = allocator.voice(i);
        match state.state {
            VoiceState::Active => {
                if let Some(note) = state.note {
                    println!("  Voice {}: Active, {}", i, note_name(note));
                }
            }
            VoiceState::Free => println!("  Voice {}: Free", i),
            VoiceState::Releasing => {
                if let Some(note) = state.note {
                    println!("  Voice {}: Releasing (was {})", i, note_name(note));
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

    println!("\nPolyphony enables expressive keyboard playing and chord voicings.");
}
