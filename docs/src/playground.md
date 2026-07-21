# Live Playground

<!-- Relative href: the site serves from / on quiver-dsp.com but /quiver/ on
     github.io, so an absolute path would 404 on one of them. This page sits at
     the book root, so "playground/" resolves correctly on both. -->
<a class="qv-playground-link" href="playground/" target="_blank" rel="noopener">**Open the Playground &rarr;**</a>

The playground is a full polyphonic synthesizer running **Quiver's actual WASM engine**
— the same compiled Rust that ships in the [`@quiver-dsp/wasm`](https://www.npmjs.com/package/@quiver-dsp/wasm)
npm package. Nothing is emulated: the patch graph is compiled and ticked sample-by-sample
inside an `AudioWorklet` on the audio render thread.

What you can do there:

- **Play it** — click *Initialize Audio*, then use your computer keyboard
  (<kbd>A</kbd>–<kbd>K</kbd> for white keys, <kbd>W</kbd><kbd>E</kbd><kbd>T</kbd><kbd>Y</kbd><kbd>U</kbd>
  for black keys), the on-screen keys, or a connected **MIDI controller**.
- **Shape the voice** — a 4-voice subtractive patch (VCO &rarr; SVF &rarr; VCA with ADSR
  and chorus) with live controls for waveform, pulse width, detune, cutoff, resonance,
  envelope amount, ADSR times, and chorus.
- **Watch it** — oscilloscope, Lissajous (stereo phase), bar, and spectrum views, plus
  per-channel VU meters, all tapped from the worklet's output.
- **Save and load patches** — the same JSON [patch format](./how-to/serialization.md)
  the Rust library reads and writes (`schemas/patch.schema.json`).
- **Browse the module catalog** — every module the WASM build exposes, with its ports
  and signal types, queried live from the engine's registry.

## How it relates to the Explorables

The [Explorables](./explorables/README.md) are small, instant-loading JavaScript
mirrors of individual DSP algorithms — built for *understanding one idea at a time*.
The playground is the opposite end of the spectrum: the complete engine, compiled from
the Rust source, patched and played in real time. When you want to check that what you
learned holds up in the real thing, this is where you go.

## Running it locally

The playground is the browser demo in [`demos/browser`](https://github.com/alexnodeland/quiver/tree/main/demos/browser):

```bash
make browser-synth   # builds the WASM package and starts a dev server on :3000
```

See [Browser & App Integration](./how-to/browser-integration.md) for using the same
engine in your own app.
