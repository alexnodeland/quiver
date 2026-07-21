# Sculpting the Spectrum

A sawtooth wave is the block of marble: every harmonic is in there, the
*n*-th at 1/*n* amplitude, a comb of partials stretching to the top of
hearing. A filter is the chisel. Subtractive synthesis is exactly what the
name says — you start with everything and carve away what you don't want.
Below, the carving happens in front of you: the ghost comb is the raw saw,
the bold curve is the filter, and the solid comb is what survives. Drag
directly on the plot and press **▶ hear it** — the sound you hear is the
literal buffer behind the solid curve.

<div class="quiver-explorable" data-viz="filters"></div>

The <span class="qv-c-audio">ghosted comb</span> is the input: a sawtooth at
<span class="qv-scrub" id="qv-filters-pitch" data-min="-24" data-max="12" data-step="1" data-value="-15">A2 · 110 Hz</span>,
straight from Quiver's bandlimited `Vco`. The
<span class="qv-c-cv">bold curve</span> is the filter's magnitude response —
not a textbook sketch but the *exact* discrete response of Quiver's `Svf`,
evaluated from the same math that runs in `Svf::tick`. The cutoff knob sits at
CV <span class="qv-scrub" id="qv-filters-cutoff" data-min="0" data-max="1" data-step="0.01" data-value="0.5">0.50</span>;
the module maps that knob exponentially, \\( f_c = 20 \cdot 1000^{\text{cv}} \\) Hz,
so CV 0 is 20 Hz, CV 0.5 is ~632 Hz, and CV 1 is 20 kHz — every hundredth of
knob travel is the same *musical* distance, about a ninth of an octave. That
is why the dot glides evenly across the log axis instead of spending 95% of
its travel above 1 kHz. Resonance is at
<span class="qv-scrub" id="qv-filters-res" data-min="0" data-max="1" data-step="0.01" data-value="0.2">0.20</span>,
which sets the damping \\( k = 2 - 2\cdot\text{res} \\) — the sharpness of the
peak at the cutoff. The <span class="qv-c-audio">solid comb</span> is the
output: the same saw pushed sample-by-sample through the filter. Watch it as
you drag — every harmonic lands exactly where the ghost comb meets the
response curve, because the output *is* the input times the response.

The slim strip under the spectrum replays the same story in *time*: three
cycles of the raw saw (ghosted) with the filtered output drawn over it. A
sawtooth's razor edge is nothing but high harmonics in phase, so as the
lowpass takes them away the corners visibly melt — the ramp survives, the
snap rounds off. And the **◠ sweep** button performs the most famous gesture
in synthesis for you: the cutoff rides from closed to open and back over a
couple of seconds, each position leaving a fading ghost of the response curve
behind — a long-exposure photograph of a filter sweep. Your own cutoff
setting is untouched and the plot returns to it when the sweep lands.

## Things to try

1. **Sweep the cutoff down** in LP mode, slowly, from 1.00 toward 0. The
   harmonics don't fade together — they disappear one at a time, from the top
   down, and the sound darkens while the pitch never moves. That ordered
   demolition *is* subtractive synthesis.
2. **Isolate a single harmonic**: switch to **BP**, push res to 0.95+, and
   drag the cutoff dot onto the third partial (330 Hz for A2). One tooth of
   the comb survives at full height; the saw becomes a near-sine. A filter
   this sharp is a harmonic *selector*.
3. **Find the self-oscillation**: set res to 1.00. The damping floors at
   \\( k \approx 0 \\) and the peak explodes off the top of the plot — the
   filter rings at the cutoff frequency even *between* harmonics, adding a
   pitch of its own. Press **▶ hear it** and drag the cutoff: you're playing
   the filter. (The Rust module soft-clips its two integrator states, so this
   near-lossless resonator sustains a bounded whistle instead of diverging.)
4. **Notch = LP + HP**: flip between LP, HP, and Notch at the same cutoff.
   The notch curve is the other two summed — and in `Svf::tick` it literally
   is: `notch = low + high`. Everything cancels only where the two responses
   overlap in antiphase, right at the cutoff.
5. **Feel the knob law**: scrub the cutoff CV 0.25 → 0.50 → 0.75. Each equal
   step slides the curve an equal *distance* on the log axis — equal octaves
   per knob-degree. A linear-in-Hz knob would cram all the musically useful
   range into its first few degrees.
6. **Remove the fundamental**: in HP mode, sweep the cutoff up past 110 Hz.
   The lowest partial vanishes first, yet the ear still hears the same pitch
   — the surviving harmonics imply the missing fundamental.
7. **Watch the corners melt**: in LP mode, drag the cutoff from 0.9 down to
   about 0.3 while watching the time strip. The saw's edges are made of high
   harmonics, so they soften first; by the time only a few partials survive,
   the razor has become a wave. Flip to HP and the opposite happens — the
   body drains away and only the snap of the edge remains.
8. **Take a long exposure**: press **◠ sweep** with res around 0.6. The trail
   of fading curves is the whole family of responses one knob can draw — the
   resonant bump gliding along the log axis at constant width, which is
   exactly why a filter sweep sounds like a vowel morph and not a volume
   change. (With reduced motion enabled, five static snapshots appear
   instead.)

## What you just saw

The state-variable filter computes all four responses at once from one
two-integrator core. With \\( g \\) the integrator gain and \\( k \\) the
damping, the transfer functions are

\\[
H_{lp}(s) = \frac{g^2}{D(s)}, \qquad
H_{bp}(s) = \frac{g s}{D(s)}, \qquad
H_{hp}(s) = \frac{s^2}{D(s)},
\\]

all sharing the denominator

\\[
D(s) = s^2 + k g s + g^2, \qquad k = \frac{1}{Q} = 2 - 2\cdot\text{res}.
\\]

The notch is the sum of the extremes, \\( H_{notch} = (g^2 + s^2)/D \\), and
the three primary outputs obey the identity
\\( H_{lp} + k H_{bp} + H_{hp} = 1 \\) — the core splits the input, it never
invents energy. At res = 0 the damping is \\( k = 2 \\) (Q = 0.5, no peak);
as res → 1, \\( k \to 0 \\) and Q → ∞: the poles slide onto the unit circle
and the filter becomes a sine oscillator at \\( f_c \\).

Quiver's `Svf` is a Zavalishin topology-preserving-transform (zero-delay
feedback) discretization of that analog prototype, with the **prewarped**
coefficient

\\[
g = \tan\left(\frac{\pi f_c}{f_s}\right).
\\]

The bilinear transform squeezes the analog frequency axis into the digital
one, and the tangent prewarp bends \\( f_c \\) in advance by exactly the
amount the squeeze will undo — so the cutoff lands where you asked all the
way toward Nyquist, where the older Chamberlin core (coefficient
\\( 2\sin(\pi f_c/f_s) \\)) froze above roughly \\( f_s/6 \\). It also makes
the bold curve honest: evaluating the prototype at
\\( s = j\tan(\pi f / f_s) \\) gives the *exact* response of the digital
filter, and that is precisely what the plot draws.

## The Quiver code

The widget is this patch. `Svf` takes the audio at `in`, the knob CVs at
`cutoff` and `res` (plus `fm`, `keytrack`, and `keytrack_amt` for modulation),
and produces `lp`, `bp`, `hp`, and `notch` simultaneously — patch whichever
mode you want:

```rust,ignore
use quiver::prelude::*;

let sample_rate = 44100.0;
let mut patch = Patch::new(sample_rate);

// Sound source and filter — the ghost comb and the bold curve.
let vco = patch.add("vco", Vco::new(sample_rate));
let vcf = patch.add("vcf", Svf::new(sample_rate));

// The two knobs, as constant CVs (0-1, exponential cutoff law inside).
let cutoff = patch.add("cutoff", Offset::new(0.5)); // 20·1000^0.5 ≈ 632 Hz
let res = patch.add("res", Offset::new(0.2))        // k = 2 - 2·0.2 = 1.6

let output = patch.add("output", StereoOutput::new());

// Saw → filter → lowpass tap → out. Swap "lp" for "bp"/"hp"/"notch".
patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
patch.connect(cutoff.out("out"), vcf.in_("cutoff")).unwrap();
patch.connect(res.out("out"), vcf.in_("res")).unwrap();
patch.connect(vcf.out("lp"), output.in_("left")).unwrap();
patch.connect(vcf.out("lp"), output.in_("right")).unwrap();

patch.set_output(output.id());
patch.compile().unwrap();

let (left, _right) = patch.tick(); // one filtered sample, in volts
```

## Go deeper

- **Tutorial:** [Filter Modulation](../tutorials/filter-modulation.md) — drive
  the cutoff with an LFO and the static curve above starts to *move*.
- **Reference:** [Filters](../reference/filters.md) — the full `Svf` and
  `DiodeLadderFilter` port maps.
- **Next explorable:** [Shaping Time](./envelopes.md) — the filter sculpts
  the spectrum; the envelope sculpts when you hear it.

---

Next: [Shaping Time](./envelopes.md)
