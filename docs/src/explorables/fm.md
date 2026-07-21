# Sidebands from Nothing

Vibrato is a slow wobble in pitch — a modulator bending a carrier a few times a
second. Speed the wobble up a thousandfold, past the point where the ear can
follow the pitch moving, and it stops being vibrato and becomes **timbre**. Two
pure sine waves, no filter anywhere in sight, and a forest of partials appears —
not at random, but at positions you can predict to the hertz. That is FM
synthesis, and the whole trick is below. The yellow stems are the *prediction*;
the blue curve is the *measurement* of the exact buffer you can hear.

<div class="quiver-explorable" data-viz="fm"></div>

The <span class="qv-c-audio">carrier</span> is the sine you hear, pitched at
<span class="qv-scrub" id="qv-fm-pitch" data-min="-1" data-max="2" data-step="0.05" data-value="0">0.00 V</span>
on the V/Oct scale (0 V = C4 ≈ 261.6 Hz). The
<span class="qv-c-mod">modulator</span> — the ghosted violet sine in the top
plot — runs at
<span class="qv-scrub" id="qv-fm-ratio" data-min="0.25" data-max="8" data-step="0.25" data-value="2">2×</span>
the carrier's frequency and never reaches your ears directly: its only job is
to bend the carrier's frequency up and down. How *hard* it bends is the
**modulation index** I =
<span class="qv-scrub" id="qv-fm-index" data-min="0" data-max="12" data-step="0.1" data-value="3">3.0</span>,
the peak frequency deviation divided by the modulator's frequency,
\\( I = \Delta F / f_m \\). Scrub all three and watch the
<span class="qv-c-voct">predicted stems</span> and the
<span class="qv-c-audio">measured spectrum</span> move in lockstep.

Two smaller views close the loop. The **Bessel panel** graphs the first five
weights \\( J_0 \\) through \\( J_4 \\) against the index, with a yellow cursor
parked at the current I — the stem heights in the spectrum are *literally*
these curves sampled at the cursor, and the dots on the zero line mark
\\( J_0 \\)'s first two zeros, 2.405 and 5.520, where the carrier's own stem
vanishes. The **index waterfall** underneath records a sweep: during *bloom*
(or instantly, via the *waterfall* button) each column paints the predicted
spectrum at one value of I, low frequencies at the bottom — so you can watch
the sidebands fan outward in Bessel order as the index rises, frozen into one
picture.

One honest disclosure: the widget computes the classic *phase-modulation* form,
\\( y(t) = 5\sin(2\pi f_c t + I \sin(2\pi f_m t)) \\) volts, because that is the
parameterization the sideband math is written in. It has exactly the spectrum
of sinusoidal linear FM with peak deviation \\( \Delta F = I f_m \\), which is
what Quiver's `Vco` does on its `fm_lin` jack: the module computes
`freq += (fm_lin / 5) · base`, so a modulator swinging ±depth volts produces
\\( \Delta F = (\text{depth}/5)   f_c \\), and therefore
\\( I = \frac{\text{depth}}{5} \cdot \frac{f_c}{f_m} \\). To dial an index of 3
at a 2× ratio you need a ±30 V modulator swing — which is why FM patches put a
gain stage between the operators.

## Things to try

1. Set the ratio to **1×**. Every sideband lands exactly on a harmonic of the
   carrier — fc, 2fc, 3fc… — and the tone turns sawlike. The readout says
   *harmonic*.
2. Set the ratio to **2×**. Sidebands land at fc ± 2k·fc: only the *odd*
   harmonics, and the reflected ones fold back onto the same grid. Hollow,
   squarelike — the same recipe a clarinet uses.
3. Set the ratio to **3.75×**. Now the sidebands miss the harmonic grid and the
   readout flips to *inharmonic*: this is where bells, gongs, and metal live.
4. Scrub the index down to **0** — a lone stem, a pure sine. Now raise it slowly
   and watch the spectrum's width track the **Carson bandwidth** readout,
   ≈ 2(I+1)fm: each unit of index wakes roughly one more sideband pair.
5. Find **I ≈ 2.4**. The center stem — the carrier itself — drops into the
   noise floor, because \\( J_0(2.4048) = 0 \\): the carrier vanishes from its
   own spectrum while everything around it keeps ringing. The next
   disappearance is at I ≈ 5.5.
6. Press **bloom**: the index sweeps from 0 to its target and the sidebands
   grow *outward in order*, each k-th pair waking only once I catches up to k —
   Bessel functions, animated. The waterfall strip freezes the whole sweep
   into one picture: sidebands fanning outward as the columns march right.
7. Find **I ≈ 2.4 on the Bessel panel** — the blue \\( J_0 \\) curve crosses
   zero exactly where the carrier disappears from its own spectrum. Park the
   yellow cursor on the dot and watch the center stem die; the next blackout
   waits at 5.520.
8. Set the index to **12**, press **waterfall**, then flip the ratio between
   **1×** and **3.75×** and press it again: harmonic ratios print clean
   horizontal stripes, inharmonic ones weave a dense unaligned fabric.

## What you just saw

The entire widget is one identity. A sine whose phase is wiggled by another
sine is *exactly* a sum of sines at evenly spaced frequencies:

\\[ \sin\bigl(2\pi f_c t + I \sin(2\pi f_m t)\bigr) = \sum_{k=-\infty}^{\infty} J_k(I)  \sin\bigl(2\pi (f_c + k f_m)  t\bigr) \\]

The weights \\( J_k(I) \\) are the **Bessel functions of the first kind** — the
yellow stem heights. Each one answers "how much energy lands k steps away from
the carrier at this index?" At \\( I = 0 \\), \\( J_0 = 1 \\) and every other
\\( J_k = 0 \\): a bare carrier. As I grows, \\( J_k(I) \\) stays near zero
until \\( I \approx k \\), then swells — which is why sidebands appear in
order, and why the audible bandwidth obeys **Carson's rule**,
\\( B \approx 2(I+1)f_m \\). Each \\( J_k \\) also *oscillates* in I, passing
through zeros — at \\( I \approx 2.405 \\) it is \\( J_0 \\)'s turn, and the
carrier itself blinks out. Energy is never created, only redistributed:
\\( \sum_k J_k(I)^2 = 1 \\) for every I. When \\( f_c + k f_m \\) goes
negative, \\( \sin(-\omega t) = -\sin(\omega t) \\): the component reflects
back to \\( |f_c + k f_m| \\), which is why low carriers with big ratios grow
extra stems near the bottom — that fold *is* through-zero FM, the oscillator's
phase briefly running backwards.

This math belongs to **linear** FM only, and the `Vco` gives you both flavors
for a reason. The linear input adds deviation symmetrically —
\\( f_c(1 + m\sin\omega t) \\) averages to exactly \\( f_c \\), so the note
stays in tune at any depth. The exponential `fm` input multiplies instead:
\\( f_c \cdot 2^{a\sin\omega t} \\), and because \\( 2^x \\) is convex, the
*average* of \\( 2^{a\sin\omega t} \\) is greater than 1 — the upward swings
outweigh the downward ones and the perceived pitch climbs as depth grows.
Lovely for vibrato at small depths, hopeless for tuned FM timbres. That is why
classic Chowning FM is patched into `fm_lin`.

## The Quiver code

The two-operator patch behind everything above — modulator sine, a gain stage
to set the depth (and thus the index), into the carrier's *linear*
through-zero FM input:

```rust,ignore
use quiver::prelude::*;

let sample_rate = 44100.0;
let mut patch = Patch::new(sample_rate);

let carrier = patch.add("carrier", Vco::new(sample_rate));
let modulator = patch.add("modulator", Vco::new(sample_rate));
let depth = patch.add("depth", Attenuverter::new());
// fm = 2 x fc. V/Oct is logarithmic, so a frequency RATIO becomes a
// voltage OFFSET into the modulator's pitch input: log2(2.0) = 1 V.
let ratio_cv = patch.add("ratio_cv", Offset::new(2.0_f64.log2()));
let output = patch.add("output", StereoOutput::new());

patch.connect(ratio_cv.out("out"), modulator.in_("voct")).unwrap();
// Modulator sine -> depth -> the carrier's LINEAR (through-zero) FM input.
// `fm_lin` is the sideband math above; the exponential `fm` input is not.
patch.connect(modulator.out("sin"), depth.in_("in")).unwrap();
patch.connect(depth.out("out"), carrier.in_("fm_lin")).unwrap();
patch.connect(carrier.out("sin"), output.in_("left")).unwrap();
patch.connect(carrier.out("sin"), output.in_("right")).unwrap();

patch.set_output(output.id());
patch.compile().unwrap();
```

The `Attenuverter`'s gain is `level / 5 V` (unity by default), so drive its
`level` input above 5 V — an `Offset` works — for the >1 gains that big indices
demand: index I at ratio r needs a modulator swing of `5 · I · r` volts. To
make the index *move* — the bright-attack electric-piano trick — patch an
`Adsr` into that `level` input instead. The runnable version, with a sweep of
ratios and depths, is
[`examples/tutorial_fm.rs`](https://github.com/alexnodeland/quiver/blob/main/examples/tutorial_fm.rs).

## Go deeper

- **Tutorial:** [FM Synthesis Basics](../tutorials/fm-synthesis.md) — operator
  algorithms, index envelopes, and the classic DX7 recipes.
- **Reference:** [Oscillators](../reference/oscillators.md) — the `Vco`'s full
  port map, including both FM inputs.
- **Next explorable:** [Patch Flow](./patch-flow.md) — how signals actually
  move through a compiled patch, one tick at a time.

---

Next: [Patch Flow](./patch-flow.md)
