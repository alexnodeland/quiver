# Envelopes Shape Time

A raw oscillator is a voltage that never changes its mind — press a key and it
drones at full amplitude forever. Real notes have a *shape*: a hammer strike
that blooms and dies, a bowed swell, a pad that breathes in slowly. In a
modular synth that shape is itself a voltage. The ADSR envelope listens to a
<span class="qv-c-gate">gate</span> — the key going down — and answers with a
<span class="qv-c-mod">contour</span> from 0 to 10 V that other modules obey.
Loudness is a contour, not a constant. Grab the curve below and bend it.

<div class="quiver-explorable" data-viz="envelopes"></div>

The primary interaction here is **dragging the handles on the curve itself** —
this is the envelope you would otherwise dial in with four knobs. The
<span class="qv-c-mod">violet curve</span> is the module's `env` output (its
internal 0–1 level times 10 V); the <span class="qv-c-gate">green bar</span> is
the gate, high from the moment the key goes down until you let go —
<span class="qv-scrub" id="qv-envelopes-gate" data-min="0.05" data-max="4" data-step="0.05" data-value="0.8">0.80</span> s
later (scrub that number, or drag the marker on the bar). Drag the attack peak
and the release end left and right to set their times; drag the decay corner
sideways for decay time and up or down for the sustain level. Watch the
readouts: each stage shows both its time *and* the 0–1 knob CV that produces
it — the module's time inputs sweep 1 ms to 10 s exponentially, so the first
few pixels of a drag move milliseconds and the last few move whole seconds.
The <span class="qv-c-audio">blue band</span> underneath is the same envelope
multiplied into a saw wave at C3 — press **▶ hear it** (or **gate**, which is
just pressing the key again) and that exact buffer plays. Below that, the
**knob law** strip plots all three time knobs on the module's actual law,
\\( T(\text{cv}) = 0.001 \cdot 10000^{\text{cv}} \\), drawn on a log-time
axis where an exponential is a straight line — drag a time handle and watch
its marker slide along the very curve your finger is feeling. Finally, the
**retrigger mid-release** toggle simulates a second key press arriving halfway
through the release: the <span class="qv-c-cv">coral second pass</span> shows
the module's real behavior — the new attack climbs *from the current level*,
never resetting to zero.

## Things to try

1. **Make a pluck.** Drag the attack peak hard left (about 1 ms), the decay
   corner left to ~100 ms and *all the way down* so sustain is zero. The gate
   length now barely matters — the note is over before the key comes up. Hear
   it: that's a mallet, a pizzicato, a bass stab.
2. **Make a pad.** Drag the attack peak right until attack is 1–2 s and raise
   sustain near the top. The note now *arrives* instead of starting. Notice the
   time axis rescaling to keep the whole story on screen.
3. **Sustain is a level, not a time.** With sustain at 0.6, scrub the gate
   length from 0.1 s to 4 s. Attack, decay, and release never change — only the
   flat sustain shelf stretches. The player owns that segment, not the module.
4. **Flip on exponential stages** and compare releases. The linear ramp ends
   with an audible corner; the one-pole curve loses a constant *fraction* of
   its level per unit time — like a real string, a real RC circuit — and simply
   fades below hearing. That's why exponential releases sound more natural.
5. **The release time is honest.** Set sustain low (0.2) and release to ~1 s,
   and watch the release segment: it still takes the full labeled time to reach
   zero. The module scales the release *rate* by the level it captured at
   gate-off, so "release = 1 s" means one second from wherever the envelope
   was, not from an imaginary full-scale peak.
6. **Cut a stage short.** Make the attack longer than the gate. The envelope
   never reaches the peak — the gate falls mid-climb and the release begins
   from the current level. Envelopes follow the key, not the plan.
7. **Retrigger mid-release.** Flip on **retrigger mid-release** and hear it:
   the key comes up, the note starts dying, and a second key press lands
   halfway through the release. The coral pass climbs from wherever the level
   *is* — this is legato playing, and it is why envelopes continue from the
   current level. A zero-reset would snap the voltage to 0 in one sample and
   click on every fast repeated note.
8. **Watch the knob law.** Drag the attack peak slowly from hard left to hard
   right while watching the A marker in the strip below. It rides a straight
   line on log-time paper — equal pixels of drag multiply the time by equal
   factors. That single law governs every time knob in the module.

## What you just saw

The widget runs the same stage machine as `Adsr::tick`. In linear mode (the
`shape` input at 0 V) each segment is a constant per-sample rate, scaled by the
span it actually has to traverse:

\\[
\text{attack\\_rate} = \frac{1}{A \cdot f_s},
\qquad
\text{decay\\_rate} = \frac{1 - S}{D \cdot f_s},
\qquad
\text{release\\_rate} = \frac{\ell_{\text{gate-off}}}{R \cdot f_s}
\\]

where \\( A, D, R \\) are the stage times in seconds, \\( S \\) is the sustain
level, and \\( \ell_{\text{gate-off}} \\) is the level captured the instant the
gate falls — the scaling from "things to try" #5. In exponential mode each
stage becomes a one-pole approach toward its target (1 for attack, \\( S \\)
for decay, 0 for release), with the stage time as the time constant:

\\[
c = e^{-1/(T f_s)},
\qquad
\ell \leftarrow \ell + (\text{target} - \ell)(1 - c)
\\]

Every step closes a fixed fraction \\( 1 - c \\) of the remaining distance,
which is exactly why the curve is steep at first and asymptotic at the end.
And the knob law you felt while dragging — fine near the left, coarse near the
right — is the exponential map from a 0–1 CV to seconds:

\\[
T(\text{cv}) = 0.001 \cdot 10000^{\text{cv}}
\\]

so cv = 0 is 1 ms, cv = 0.5 is 100 ms, and cv = 1 is 10 s: each quarter turn
of the knob multiplies the time by ten.

## The Quiver code

The classic patch: a gate presses the envelope's key, and the envelope's
0–10 V contour drives a VCA sitting on the audio path — voltage shaping
voltage, exactly what the widget draws.

```rust,ignore
use quiver::prelude::*;
use std::sync::Arc;

let sample_rate = 44100.0;
let mut patch = Patch::new(sample_rate);

// A key press, as voltage: this line goes 0 V -> +5 V -> 0 V.
let gate_cv = Arc::new(AtomicF64::new(0.0));
let gate = patch.add("gate", ExternalInput::gate(Arc::clone(&gate_cv)));

let vco = patch.add("vco", Vco::new(sample_rate));
let env = patch.add("env", Adsr::new(sample_rate));
let vca = patch.add("vca", Vca::new());
let out = patch.add("out", StereoOutput::new());

// The gate presses the envelope's key.
patch.connect(gate.out("out"), env.in_("gate")).unwrap();

// Audio path: raw saw through the VCA.
patch.connect(vco.out("saw"), vca.in_("in")).unwrap();
patch.connect(vca.out("out"), out.in_("left")).unwrap();
patch.connect(vca.out("out"), out.in_("right")).unwrap();

// The envelope's 0-10 V `env` output drives the VCA's `cv`:
// loudness IS the contour you just dragged.
patch.connect(env.out("env"), vca.in_("cv")).unwrap();

patch.set_output(out.id());
patch.compile().unwrap();

gate_cv.set(5.0); // key down: attack -> decay -> sustain...
// ... tick() for the length of the note ...
gate_cv.set(0.0); // key up: release, from the current level
```

The `Adsr`'s `attack`, `decay`, `sustain`, and `release` are themselves CV
*inputs* (0–1 through the knob law above), so anything — an LFO, a sequencer
row, another envelope — can reshape the shape. It also offers a `retrig`
trigger input (restart the attack from the current level without dropping the
gate — the same never-reset-to-zero behavior the retrigger toggle above
demonstrates), an `inv` output (the contour upside down, for ducking), and an `eoc`
end-of-cycle trigger for chaining. Set the `shape` input high (+5 V) for the
exponential stages you toggled above.

## Go deeper

- **Tutorial:** [Envelope Shaping](../tutorials/envelope-shaping.md) builds
  this patch step by step and modulates the filter with the same contour.
- **Reference:** [Modulators](../reference/modulators.md) — the full `Adsr`
  port list, plus the LFO and the other contour generators.
- **Next explorable:** [One Volt per Octave](./voct.md) — the other CV
  convention every module agrees on: pitch as voltage.

---

Next: [One Volt per Octave](./voct.md)
