# Follow the Signal

A patch is not a program that runs once — it is a circuit where every cable
carries a living voltage, all the time. The diagram below is the classic
subtractive voice from [Your First Patch](../getting-started/first-patch.md):
a <span class="qv-c-gate">gate</span> triggers an
<span class="qv-c-cv">envelope</span>, while the oscillator's
<span class="qv-c-audio">audio</span> runs through a filter and an amplifier.
**Click any module** to put a scope on its output and watch the same note from
six different points in the circuit. The strip of mini-scopes under the main
scope shows all six stations at once — tap one to jump the big scope there —
and while the note plays, a playhead sweeps the full-note view so you can
match what you hear to where you are.

<div class="quiver-explorable" data-viz="patchflow"></div>

The voice plays a note at
<span id="qv-patchflow-pitch" class="qv-scrub" data-min="-3" data-max="2" data-step="0.05" data-value="-1">-1</span> V
(1 V/octave, so −1 V is C3), with the gate held for
<span id="qv-patchflow-gate" class="qv-scrub" data-min="0.1" data-max="2" data-step="0.05" data-value="0.6">0.6</span> s.
The filter sits at a base cutoff of
<span id="qv-patchflow-cutoff" class="qv-scrub" data-min="0" data-max="1" data-step="0.01" data-value="0.15">0.15</span>
(knob CV, 0–1) with resonance
<span id="qv-patchflow-res" class="qv-scrub" data-min="0" data-max="1" data-step="0.01" data-value="0.25">0.25</span>,
and the <span class="qv-c-cv">envelope</span> pushes the cutoff up by
<span id="qv-patchflow-depth" class="qv-scrub" data-min="0" data-max="1" data-step="0.01" data-value="0.55">0.55</span>
before falling back — the brightness of the note is a *shape in time*, not a
setting.

## Things to try

1. Read the mini-scope strip left to right: rectangle → contour → tone →
   sculpted tone → note. That row *is* the patch — every station of the
   transformation in one glance. Tap any mini-scope to inspect it up close.
2. Scope the **VCO**, then the **SVF**, then the **VCA** — the same 30 ms of
   waveform at three stations. The saw's sharp corners melt at the filter, and
   the VCA finally gives the sound a beginning and an end.
3. Turn *zoom to waveform* off while scoping the **VCA**, then press
   *▶ hear this point*: the playhead sweeps the note's whole amplitude
   contour — the <span class="qv-c-cv">ADSR</span> curve, worn by the audio
   like a glove — while you listen.
4. Scope the **GATE** and then the **ADSR**: a rectangle goes in, a contour
   comes out. Every knob on an envelope module is a statement about how to
   round off a rectangle.
5. Drag the envelope→cutoff depth to `0`: the note becomes static and dull —
   subtractive synthesis with nobody moving the tone control. Now drag it to
   `1` and listen to the attack spit.
6. Raise resonance toward `1` with a low base cutoff: the filter starts to
   sing its own note on top of the saw (the SVF self-oscillates; its
   integrator states are soft-clipped in the Rust so this stays bounded).
7. Shorten the gate to `0.1` s: the envelope never reaches sustain — you can
   *see* it turn around mid-decay when the gate falls.

## What you just saw

Each cable color is a signal type, and each type is a voltage convention:

- <span class="qv-c-audio">**audio**</span> — the sound itself, ±5 V
- <span class="qv-c-cv">**CV**</span> — control voltage, here the envelope's 0–10 V
- <span class="qv-c-gate">**gate**</span> — 0 V or +5 V, "the key is down"
- <span class="qv-c-voct">**V/Oct**</span> — pitch, one volt per octave, 0 V = C4

The VCA is nothing but a multiplication,

\\[ \text{out}(t) = \text{in}(t) \cdot \frac{\text{cv}(t)}{10~\text{V}}, \\]

and the filter's cutoff knob follows an exponential law so that equal knob
motion covers equal *musical* distance,

\\[ f_c(\text{cv}) = 20 \cdot 1000^{\text{cv}}~\text{Hz}, \qquad \text{cv} \in [0, 1]. \\]

The envelope output is 10 V at full level, while the cutoff CV wants 0–1 — in
hardware you would patch through an attenuverter, and Quiver's `cutoff` port
carries one for exactly this reason. The scrub for *depth* above **is** that
attenuverter.

## The Quiver code

This is the real example the diagram mirrors — module for module, cable for
cable ([`examples/first_patch.rs`](https://github.com/alexnodeland/quiver/blob/main/examples/first_patch.rs)):

```rust,ignore
{{#include ../../../examples/first_patch.rs}}
```

Run it with `cargo run --example first_patch`.

## Go deeper

- **Tutorial:** [Basic Subtractive Synthesis](../tutorials/subtractive-synthesis.md)
  builds this voice step by step.
- **Concepts:** [Understanding Signal Flow](../getting-started/signal-flow.md)
  and [Signal Conventions](../concepts/signals.md) for the voltage rules the
  colors encode.
- **Other explorables:** each module in this circuit has its own page —
  [the oscillator](./oscillators.md), [the filter](./filters.md),
  [the envelope](./envelopes.md), [pitch](./voct.md).
