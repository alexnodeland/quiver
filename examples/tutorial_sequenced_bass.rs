//! Tutorial: Building a Sequenced Bass
//!
//! A step sequencer driving a classic subtractive bass voice.
//! This pattern is the foundation of house, techno, and many other genres.
//!
//! # Why a step sequencer instead of manual gate/pitch control
//!
//! Earlier tutorials (`tutorial_envelope.rs`, `first_patch.rs`) drove pitch
//! and gate signals from Rust code directly. A `StepSequencer` moves that
//! job *into the graph*: it holds up to 8 (V/Oct pitch, gate-on/off) pairs
//! and advances one step every time its `clock` input receives a rising
//! edge, outputting that step's stored CV and gate. This is exactly how a
//! hardware step sequencer module works — the tempo comes from a separate
//! `Clock` module, decoupled from the pattern itself, so the same 8-step
//! bassline can run at any tempo just by changing the clock's rate.
//!
//! Two things worth understanding about the signal chain below:
//! - **V/Oct pitch**: each step stores a control voltage where every extra
//!   1V is one octave up (`(midi_note - 60) / 12` converts a MIDI note to
//!   this scale, since 12 semitones = 1 octave = 1V).
//! - **Gate-gated triggering**: the sequencer only asserts its gate output
//!   while the clock pulse itself is high *and* that step is marked "on" —
//!   a "rest" step in the pattern lets the ADSR's release tail finish
//!   naturally instead of re-triggering, which is what keeps a rest sounding
//!   like silence rather than a stuck note.
//!
//! Run with: cargo run --example tutorial_sequenced_bass

use quiver::prelude::*;

/// Convert a MIDI note number to a V/Oct control voltage (0V = MIDI 60 / C4).
fn midi_to_voct(note: u8) -> f64 {
    (note as f64 - 60.0) / 12.0
}

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    // Our bassline: C3, D3, rest, G2, C3, rest, E3, D3
    let pattern = [
        (48, true), // C3
        (50, true), // D3
        (0, false), // rest
        (43, true), // G2
        (48, true), // C3
        (0, false), // rest
        (52, true), // E3
        (50, true), // D3
    ];

    // Step sequencer - stores our bassline pattern. Steps must be programmed
    // with `set_step(index, voct, gate)` *before* the module is handed to
    // `patch.add`: once a module is inside the graph it's only reachable by
    // port name, not by its concrete Rust type, so this is the only chance
    // to configure it directly.
    let mut seq_module = StepSequencer::new();
    for (i, (note, active)) in pattern.iter().enumerate() {
        seq_module.set_step(i, midi_to_voct(*note), *active);
    }
    let seq = patch.add("seq", seq_module);

    // Master clock - sets the tempo. `out` is the main pulse (2 Hz / 120 BPM
    // by default); `div2`/`div4` divide it further for slower sub-patterns.
    // We use the un-divided `out` so the 8-step pattern advances once per
    // pulse.
    let clock = patch.add("clock", Clock::new(sample_rate));

    // Bass voice: VCO → VCF → VCA
    let vco = patch.add("vco", Vco::new(sample_rate));
    let vcf = patch.add("vcf", Svf::new(sample_rate));
    let vca = patch.add("vca", Vca::new());
    let env = patch.add("env", Adsr::new(sample_rate));

    // Output
    let output = patch.add("output", StereoOutput::new());

    // Clock → Sequencer
    patch.connect(clock.out("out"), seq.in_("clock")).unwrap();

    // Sequencer → Voice
    patch.connect(seq.out("cv"), vco.in_("voct")).unwrap();
    patch.connect(seq.out("gate"), env.in_("gate")).unwrap();

    // Audio path
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
    patch.connect(vca.out("out"), output.in_("left")).unwrap();
    patch.connect(vca.out("out"), output.in_("right")).unwrap();

    // Envelope → Filter & VCA
    patch.connect(env.out("env"), vcf.in_("cutoff")).unwrap();
    patch.connect(env.out("env"), vca.in_("cv")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    println!("=== Sequenced Bass Demo ===\n");

    fn note_name(note: u8) -> &'static str {
        match note % 12 {
            0 => "C",
            1 => "C#",
            2 => "D",
            3 => "D#",
            4 => "E",
            5 => "F",
            6 => "F#",
            7 => "G",
            8 => "G#",
            9 => "A",
            10 => "A#",
            11 => "B",
            _ => "?",
        }
    }

    println!("Bassline pattern (now actually programmed into the sequencer):");
    for (i, (note, active)) in pattern.iter().enumerate() {
        if *active {
            let voct = midi_to_voct(*note);
            let octave = (note / 12) - 1;
            println!(
                "  Step {}: {}{} ({:.3}V)",
                i + 1,
                note_name(*note),
                octave,
                voct
            );
        } else {
            println!("  Step {}: rest", i + 1);
        }
    }

    // The clock's main "out" pulses at 2 Hz by default (120 BPM), so each of
    // the 8 steps lasts half a second — one full pass through the pattern
    // takes 4 seconds.
    //
    // One quirk worth knowing: `Clock`'s phase starts at 0, which is already
    // inside its pulse window, so the very first sample tick delivers a
    // rising edge before we've heard anything — the sequencer advances past
    // step 0 in zero time. Since the pattern loops forever in a real patch,
    // this just means playback effectively starts one step ahead; we offset
    // our step index by one below so the printed labels match what's
    // actually sounding.
    let step_samples = (sample_rate * 0.5) as usize;
    println!("\nRunning one pass through the pattern (4.0s)...\n");

    for i in 0..pattern.len() {
        let (note, active) = pattern[(i + 1) % pattern.len()];
        let mut peak = 0.0_f64;
        for _ in 0..step_samples {
            let (left, _) = patch.tick();
            peak = peak.max(left.abs());
        }

        let label = if active {
            format!("{}{}", note_name(note), (note / 12) as i32 - 1)
        } else {
            "rest".to_string()
        };
        let bar = "█".repeat((peak * 2.0) as usize);
        println!(
            "Step {} ({:>4}): {:5.2}V |{}",
            (i + 1) % pattern.len() + 1,
            label,
            peak,
            bar
        );
    }

    println!("\nThe sequencer cycles through the pattern,");
    println!("triggering the envelope on each gated step and resting on the others.");
}
