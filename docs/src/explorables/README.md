# About the Explorables

Reading about a filter tells you *that* resonance boosts the cutoff frequency.
Dragging a resonance knob while the frequency response reshapes under your
pointer tells you *what resonance is*. This section is a set of *explorable
explanations* — in the tradition of Bret Victor's reactive documents and
3Blue1Brown's animated mathematics — where every plot is live, every number in
the prose can be scrubbed, and everything you see can also be heard.

Two promises hold on every page:

1. **The math is the library's math.** The widgets don't sketch textbook
   approximations — they run the same formulas as Quiver's Rust source: the
   PolyBLEP oscillator from `oscillators.rs`, the exact TPT filter response
   from `filters.rs`, the envelope stage machine from `dynamics.rs`. What you
   drag is what ships.
2. **What you see is what you hear.** The ▶ buttons play the very buffer being
   plotted, at the voltage conventions of the library (±5 V audio, scaled down
   to your speakers). Sound never plays until you ask.

## The color language

Signals are colored by *kind*, consistently across prose, plots, and patch
diagrams:

- <span class="qv-c-audio">**audio**</span> — the sound itself, ±5 V
- <span class="qv-c-cv">**CV**</span> — control voltages that shape it, like envelopes
- <span class="qv-c-gate">**gate / trigger / clock**</span> — 0 or +5 V timing signals
- <span class="qv-c-voct">**V/Oct**</span> — pitch, one volt per octave, 0 V = C4
- <span class="qv-c-mod">**modulation**</span> — LFOs and secondary movement

When you see a <span class="qv-scrub" style="cursor:default">dashed number</span>
in the text, drag it sideways (or focus it and use arrow keys) — the figures
respond immediately.

## The pages

- **[The Shape of a Wave](./oscillators.md)** — waveforms and their harmonic
  recipes, and why the oscillator must be bandlimited.
- **[Sculpting the Spectrum](./filters.md)** — the state-variable filter's
  exact frequency response, from gentle rolloff to self-oscillation.
- **[Envelopes Shape Time](./envelopes.md)** — drag an ADSR by its corners and
  hear loudness become a contour.
- **[The Geometry of Pitch](./voct.md)** — one volt per octave, and why
  exponential pitch looks like a line on a log axis.
- **[Sidebands from Nothing](./fm.md)** — FM synthesis: two sines, a forest of
  partials, exactly where Bessel said they'd be.
- **[Follow the Signal](./patch-flow.md)** — the full subtractive voice as a
  clickable circuit; scope any cable.

These pages need JavaScript (and a reasonably modern browser). Animations
respect your reduced-motion preference; audio always waits for a click.
