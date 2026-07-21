# The Geometry of Pitch

In a modular synthesizer, pitch is not a note name, a MIDI number, or a
frequency dial — it is a **voltage on a wire**. The whole standard fits in one
sentence: *one volt is one octave, and 0 V is middle C.* Everything else about
musical pitch — semitones, cents, why an octave "sounds the same," why detuning
by a few millivolts makes a chorus — falls out of a single exponential,
\\( f(V) = 261.63 \cdot 2^{V} \\). Drag the marker below and watch the geometry
happen.

<div class="quiver-explorable" data-viz="voct"></div>

The <span class="qv-c-voct">yellow ruler</span> is the pitch CV — the voltage a
sequencer or keyboard actually sends down a <span class="qv-c-voct">V/Oct</span>
cable. The <span class="qv-c-audio">blue curve</span> above it is the frequency
a VCO turns that voltage into, plotted on a *linear* Hz axis so you can feel the
exponential: each volt to the right doesn't add frequency, it **doubles** it.
Set the pitch CV to
<span class="qv-scrub" id="qv-voct-volts" data-min="-2" data-max="3" data-step="0.001" data-value="0.75">+0.750 V</span>
and read the marker — at exactly +0.750 V you get A4 = 440 Hz, nine semitones
above middle C. Now switch **quantize to semitones** on: the marker snaps to the
nearest multiple of 1/12 V while a faint ghost keeps tracking the raw voltage.
That snap *is* Quiver's `Quantizer` module — a pitch quantizer is nothing more
than rounding a voltage to the semitone grid. The piano strip riding on the
ruler makes the volts↔notes bijection tactile: every key sits on an exact
multiple of 1/12 V, so tapping **E4** *is* setting the CV to +0.333 V.
And switch on **detune demo** to hear the raw pitch and its quantized ghost
sounding together: the slow swelling and fading you hear — and see in the
envelope strip that appears — is *beating*, the audible form of the cents
readout, undulating at exactly \\( |f_{raw} - f_{quantized}| \\) times per
second. The lower strip replots the same
curve on a **log**-frequency axis, where it becomes a perfectly straight line.
This is why engineers love log axes for pitch: equal musical intervals are equal
frequency *ratios*, and ratios only become equal *distances* on a log axis.

## Things to try

1. Drag the marker up by exactly **+1 V** from anywhere — from 0 V to +1 V, or
   from −1.317 V to −0.317 V — and watch the frequency readout **exactly
   double**. The starting point never matters; only the voltage *distance* does.
2. Each semitone is **1/12 V ≈ 83.3 mV**. Turn **quantize** on and drag slowly:
   the raw ghost glides continuously while the marker holds, then jumps a whole
   83.3 mV step at once — a staircase built from a ramp.
3. Scrub the voltage by just a few millivolts (arrow keys on the scrub work
   too). The **cents** readout shows how far off-pitch you are: 1 cent is a mere
   0.83 mV, which is why analog oscillators need precise, temperature-stable
   volt-per-octave circuits.
4. Compare the two plots at the top of the range. From +2 V to +3 V the linear
   plot rockets from about 1046 Hz to 2093 Hz — the same 1 V that only moved you
   65 Hz down at −2 V. On the log strip below, those two steps are *identical*
   distances. Same geometry, different lens.
5. Press **play interval**: you hear the marker's note, then the note exactly
   +1.0 V above it — twice the frequency — then both together. That fused,
   "same-note-but-higher" quality is what a 2:1 ratio sounds like.
6. Park the marker at +0.750 V and press **▶ hear it**: the orchestra's tuning
   A, produced by nothing but \\( 261.63 \cdot 2^{0.75} \\).
7. Tap **C4**, then **C#4**, then **D4** on the piano strip and watch the raw
   CV climb by exactly 83.3 mV per key — the keyboard is a voltage ladder, one
   rung per semitone.
8. Turn on **detune demo** and tap **E4**: the envelope strip is flat — raw and
   quantized agree, so there is nothing to beat. Now drag a hair sharp and
   count the slow swells: at +5 ¢ they come about once per second, and the
   **beat rate** readout predicts exactly the rate you count,
   \\( |f_{raw} - f_{quantized}| \approx 0.95\ \text{Hz} \\).

## What you just saw

The V/Oct standard is one function. With \\( f_{C4} = 261.6255653\ \text{Hz} \\)
(the 0 V reference), a pitch voltage \\( V \\) maps to frequency as

\\[ f(V) = f_{C4} \cdot 2^{V}. \\]

Because the volt lives in the *exponent*, adding voltage multiplies frequency:
\\( f(V + 1) = 2 f(V) \\) is the octave, and dividing the volt into twelve equal
slices gives the equal-tempered semitone as a **ratio**,

\\[ \text{one semitone} = \tfrac{1}{12}\ \text{V} \ \Longrightarrow\  f\left(V + \tfrac{1}{12}\right) = 2^{1/12} f(V) \approx 1.0595  f(V). \\]

Cents subdivide the semitone a hundred times further. The widget's cents readout
is the distance from the nearest note on the semitone grid:

\\[ \text{cents}(V) = 100 \left( 12V - \operatorname{round}(12V) \right), \\]

so 1 cent is 1/1200 V ≈ 0.83 mV. Finally, taking \\( \log_2 \\) of the pitch law
explains the straight line in the lower strip:

\\[ \log_2 f = \log_2 f_{C4} + V. \\]

On a log-frequency axis, frequency is a *linear* function of voltage — slope
exactly one octave per volt. Pitch is geometry: intervals are ratios, and the
log axis is the space in which those ratios become plain distances. The
quantizer is just \\( V \mapsto \operatorname{round}(12V)/12 \\), rounding in
that space.

## The Quiver code

The same three ideas as the widget — a pitch voltage, a semitone quantizer, and
the exponential VCO — as a real patch. `Offset` with nothing patched into its
input is the idiomatic constant-CV source; `Quantizer` snaps it to the chromatic
grid (the real module also adds hysteresis so a CV parked on a boundary doesn't
chatter between notes); the `Vco` applies \\( f = 261.63 \cdot 2^{V} \\)
internally via `voct_to_hz`.

```rust,ignore
use quiver::prelude::*;

fn main() {
    let sample_rate = 44100.0;
    let mut patch = Patch::new(sample_rate);

    // A constant pitch CV: +0.762 V — a few cents sharp of A4, on purpose.
    let pitch = patch.add("pitch", Offset::new(0.762));

    // Snap to the semitone grid: round(12·V)/12. The quantizer commits
    // +0.750 V — exactly A4.
    let quant = patch.add("quant", Quantizer::new(Scale::Chromatic));

    // The VCO turns volts into Hz: f = 261.63 · 2^V, so +0.750 V -> 440 Hz.
    let vco = patch.add("vco", Vco::new(sample_rate));
    let output = patch.add("output", StereoOutput::new());

    patch.connect(pitch.out("out"), quant.in_("in")).unwrap();
    patch.connect(quant.out("out"), vco.in_("voct")).unwrap();
    patch.connect(vco.out("sin"), output.in_("left")).unwrap();

    patch.set_output(output.id());
    patch.compile().unwrap();

    // Render one second of the tuning A.
    for _ in 0..sample_rate as usize {
        let (_left, _right) = patch.tick();
    }
}
```

Swap `Scale::Chromatic` for `Scale::Major` or `Scale::PentatonicMinor` and the
same rounding idea snaps to a musical scale instead of all twelve semitones —
that's the entire difference between `Quantizer`'s modes.

## Go deeper

- **Reference:** [V/Oct Reference](../appendix/voct-reference.md) — the complete
  note/voltage/frequency table this page is built on.
- **Concept:** [Signals](../concepts/signals.md) — where `VoltPerOctave` sits
  among Quiver's signal kinds.
- **Tutorial:** [Sequenced Bass](../tutorials/sequenced-bass.md) — a step
  sequencer emitting these very voltages into a VCO.
- **Next explorable:** [Two Oscillators, One Wire](./fm.md) — what happens when
  the thing modulating pitch is itself an oscillator.

---

Next: [Two Oscillators, One Wire](./fm.md)
