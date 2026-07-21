# The Shape of a Wave

Every classic synthesizer waveform is two pictures of the same object. In
time, it is a <span class="qv-c-audio">shape</span> — a curve the speaker cone
traces, five volts up, five volts down. In frequency, it is a **recipe** — a
stack of pure sine harmonics, each with its own strength. Neither picture is
more real than the other; a trained ear hears the recipe, an oscilloscope
shows the shape. Quiver's `Vco` serves the four classics — sine, triangle,
saw, square — and below, both pictures are computed from the *same rendered
buffer*, so whatever you do to the shape you do to the recipe, instantly.

The oscillator is also where digital synthesis meets its oldest enemy. A
mathematically perfect saw edge contains harmonics beyond any sample rate,
and everything the samples cannot hold reflects back into the audio band as
**aliasing**. Quiver bandlimits its edges with PolyBLEP (and its corners with
PolyBLAMP). The toggle below turns that protection off — look at what floods
the spectrum floor, then press **▶ hear it**.

<div class="quiver-explorable" data-viz="oscillators"></div>

The small dial above the wave is the oscillator's entire inner life. A `Vco`
stores exactly one number — a phase \\( \varphi \\) that runs around a circle
once per cycle — and each waveform is just a different function of where on
the circle the phase currently is. The dial spins in slow motion (about one
turn every two seconds, nowhere near audio rate) while the
<span class="qv-c-mod">violet dot</span> traces the matching point on the
waveform: phase runs around the circle, and the shape is what the wave does
along the way.

Pitch here is not a number in hertz — it is a
<span class="qv-c-voct">voltage</span> on the `voct` input. Drag it:
<span class="qv-scrub" id="qv-oscillators-pitch" data-min="-2" data-max="4" data-step="0.05" data-value="0">0.00 V</span>.
Every volt doubles the frequency — 1 V per octave, with 0 V = C4
(261.63 Hz) — so a five-octave keyboard is just a 5 V span of
<span class="qv-c-voct">CV</span>. The <span class="qv-c-audio">blue
trace</span> is the waveform in volts; the
<span class="qv-c-audio">blue spectrum</span> underneath is that exact buffer
passed through an FFT, and the <span class="qv-c-voct">dashed yellow
lines</span> mark the integer harmonics \\( n \cdot f_0 \\) — with
bandlimiting on, every peak sits on one. The <span class="qv-c-voct">hollow
yellow rings</span> are the prediction, not the measurement: the closed-form
Fourier amplitude of each harmonic for the current shape, anchored to the
measured fundamental so theory and FFT share a reference. Watching the stems
land exactly on the rings is watching a 200-year-old theorem pass a live
test. The square wave has one more knob
the others lack: its pulse width, `pw` =
<span class="qv-scrub" id="qv-oscillators-pw" data-min="0.05" data-max="0.95" data-step="0.01" data-value="0.5">0.50</span>,
the fraction of each cycle spent high.

## Things to try

1. Pick **sin**. The spectrum is a single spike at \\( f_0 \\) — a sine *is*
   one harmonic and nothing else. Every other waveform on this page is built
   out of copies of it.
2. Cycle **saw → sqr → tri** and read the recipes: the saw has *every*
   harmonic falling off as \\( 1/n \\) (a straight −6 dB/octave staircase);
   the square keeps only the *odd* harmonics, also at \\( 1/n \\); the
   triangle keeps the odd ones but at \\( 1/n^2 \\), which is why its
   spectrum plunges and it sounds so much mellower than the square whose
   harmonics sit at the same frequencies.
3. Pick **sqr** and drag the pulse width off 0.50: the even harmonics fade
   back in — at exactly 0.5 they cancel perfectly. Park it near 0.10 for the
   thin, nasal pulse timbre every string-machine patch is built on. Hear it
   while you drag.
4. Pick **saw**, drag the pitch up to +4.00 V, and switch **bandlimited**
   off. The floor fills with hash that lands *between* the yellow lines —
   harmonics above Nyquist reflected to frequencies that are not multiples
   of \\( f_0 \\). Press **▶ hear it** and flip the toggle back and forth:
   the naive saw rings with inharmonic, metallic junk.
5. Still naive, scrub the pitch slowly upward while listening: the true
   harmonics rise, but the aliases sweep *downward*. Partials that bend the
   wrong way under a pitch change are the unmistakable fingerprint of
   aliasing.
6. Drop to −2.00 V, still naive. The damage nearly vanishes — the offending
   harmonics are weak and few down here. This is why naive oscillators
   almost get away with bass lines and fall apart the moment you play a lead.
7. Pick **sqr** and watch the phasor: the square's value is just *which part
   of the circle the phase is in* — inside the shaded slice the wave sits at
   +5 V, outside at −5 V. Drag `pw` and the slice and the wave's duty cycle
   move together. Now picture the saw the same way: its value is simply *how
   far around the circle am I*, climbing from −5 V back to −5 V once per lap.
8. With **bandlimited** on, every measured stem lands on a hollow ring — the
   FFT agreeing with Fourier's closed-form recipe, live. Switch it off at
   +4.00 V and the stems miss and smear off their targets: the theory did
   not fail, the sampling did. (Drag `pw` on the square and the rings
   themselves migrate — the recipe below is the pw = 0.5 special case of the
   general pulse wave.)

## What you just saw

The spectrum panel is not a decoration next to the waveform — it *is* the
waveform, written in a different basis. Fourier's theorem says any repeating
wave is a sum of sines at integer multiples of the fundamental, and the
classic shapes have closed-form recipes. The sawtooth uses every harmonic:

\\[ \mathrm{saw}(t) = \frac{2}{\pi} \sum_{n=1}^{\infty} \frac{(-1)^{n+1}}{n}   \sin(2\pi n f_0 t) \\]

The square and triangle use only the odd ones, at \\( 1/n \\) and
\\( 1/n^2 \\) respectively:

\\[ \mathrm{sqr}(t) = \frac{4}{\pi} \sum_{n\ \mathrm{odd}} \frac{1}{n}   \sin(2\pi n f_0 t) \qquad \mathrm{tri}(t) = \frac{8}{\pi^2} \sum_{n\ \mathrm{odd}} \frac{(-1)^{(n-1)/2}}{n^2}   \sin(2\pi n f_0 t) \\]

Those coefficients are exactly the stem heights you saw: \\( 1/n \\) is a
−6 dB/octave slope, \\( 1/n^2 \\) is −12 dB/octave. The pitch voltage feeds
the exponential V/Oct law, \\( f_0 = 261.63 \times 2^{V} \\) — one volt, one
octave — which is why equal drags of the scrub felt like equal musical
intervals.

The sums above are infinite, and there lies the problem: a sampled system at
rate \\( f_s \\) can only represent content below the Nyquist frequency
\\( f_s / 2 \\). A harmonic at \\( f > f_s/2 \\) does not disappear — it
folds back to \\( f_s - f \\), which is almost never a multiple of
\\( f_0 \\): inharmonic hash. PolyBLEP exists precisely for this. Instead of
a perfect instantaneous edge (an infinite series), it splices in a two-sample
polynomial rounding whose spectrum falls off steeply before Nyquist — the
wave you see with the toggle on is microscopically "wrong" in time and
audibly right in frequency.

## The Quiver code

The widget's bandlimited math is `Vco::tick`, line for line. In a real patch
the same oscillator is four lines of graph setup — its inputs are `voct`,
`fm`, `pw`, `sync`, and `fm_lin`; its outputs are `sin`, `tri`, `saw`, and
`sqr`, all ±5 V audio:

```rust,ignore
use quiver::prelude::*;

let sample_rate = 44100.0;
let mut patch = Patch::new(sample_rate);

// Add the oscillator and an output module.
let vco = patch.add("vco", Vco::new(sample_rate));
let output = patch.add("output", StereoOutput::new());

// Patch the sawtooth straight to the left channel. The unpatched `voct`
// input sits at 0 V, so the VCO free-runs at C4 (261.63 Hz) — exactly the
// widget's default. (StereoOutput normals the right input to the left.)
patch.connect(vco.out("saw"), output.in_("left")).unwrap();

// Compile once, then tick: each call advances the graph one sample.
patch.set_output(output.id());
patch.compile().unwrap();
let (left, _right) = patch.tick();
```

Swap `"saw"` for `"sin"`, `"tri"`, or `"sqr"` to pick a different recipe, and
patch a CV source into `"pw"` to sweep the square's pulse width the way you
just scrubbed it.

## Go deeper

- **Tutorial:** [Subtractive Synthesis](../tutorials/subtractive-synthesis.md)
  — patch this VCO into a filter and envelope and make the recipe move.
- **Reference:** [Oscillators](../reference/oscillators.md) — every port of
  `Vco`, `AnalogVco`, `Supersaw`, `Wavetable`, and the rest of the sources.
- **Concepts:** [Signals](../concepts/signals.md) — the voltage conventions
  this page leaned on: ±5 V audio, 1 V/octave pitch, 0 V = C4.
- **Next explorable:** [Sculpting the Spectrum](./filters.md) — you just met
  a wave as a stack of harmonics; next, carve that stack with a filter.

---

Next: [Sculpting the Spectrum](./filters.md)
