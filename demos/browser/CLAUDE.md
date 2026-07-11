# Browser Synth Demo

This directory contains a browser-based synthesizer demo showcasing Quiver's WASM capabilities.

## Overview

The browser synth is a fully functional web synthesizer that demonstrates:
- Real-time audio processing via Web Audio API
- WASM-based audio worklet for low-latency processing
- Interactive UI with keyboard and MIDI input
- Visualizations (oscilloscope, spectrum analyzer)
- Preset management and patch editing

## Structure

```
browser/
├── src/
│   └── main.ts         # Main TypeScript entry point
├── tests/              # Playwright E2E tests
│   ├── tests/          # Test specifications
│   ├── fixtures/       # Test fixtures
│   └── playwright.config.ts
├── dist/               # Built assets
├── index.html          # Main HTML entry point
├── package.json        # npm dependencies
├── vite.config.ts      # Vite bundler config
└── tsconfig.json       # TypeScript config
```

## Development

```bash
# From repository root
make browser-synth      # Build WASM and start dev server

# Or manually
make wasm               # Build WASM package first
cd demos/browser
npm install
npm run dev             # Start Vite dev server
```

The dev server runs at `http://localhost:3000` (set in `vite.config.ts`). Build the
`@quiver/wasm` package first (`make wasm` then `npm run build:wasm:ts` at the repo
root) so Vite can resolve the worklet/wasm assets.

> Note: the Playwright E2E tests under `tests/` run against a **separate** static
> fixture (`tests/fixtures/index.html`, served by `vite fixtures` on port 5173) that
> exercises the raw `QuiverEngine` API directly on the main thread. They do not load
> this demo's worklet path.

## Testing

Browser tests use Playwright for E2E testing:

```bash
# From repository root
make test-browser       # Run browser tests (Chromium only)
make test-browser-all   # Run on all browsers (Chromium, Firefox, WebKit)

# Or manually from tests/ directory
cd demos/browser/tests
npm install
npx playwright test
```

## Dependencies

- Depends on the `@quiver/wasm` npm workspace package (declared in `package.json`
  as `"@quiver/wasm": "^0.1.0"`; resolved via the root npm workspace symlink). The
  demo is itself a workspace (`demos/browser` in the root `package.json`
  `workspaces`), so `npm install` at the repo root links everything.
- Vite for bundling and dev server
- TypeScript for type safety

## Key Concepts

### Audio Worklet Integration (the real audio path)
Audio runs through the package's AudioWorklet helper, `createQuiverAudioNode` from
`@quiver/wasm`. The Quiver engine lives **inside** the worklet render thread; the
demo drives it with message-based control calls (`addModule`, `connect`, `setParam`,
`setOutput`, `loadPatch`, `savePatch`, MIDI). The worklet script and the `.wasm`
binary are pulled in as Vite `?url` assets:

```ts
import { createQuiverAudioNode } from '@quiver/wasm';
import workletUrl from '@quiver/wasm/worklet?url';
import wasmUrl from '@quiver/wasm/quiver_bg.wasm?url';

const quiver = await createQuiverAudioNode(ctx, { workletUrl, wasmUrl });
```

There is **no** main-thread audio engine and **no** `ScriptProcessorNode` — the old
demo used both, which bypassed the package entirely (fixed). A second, separate
main-thread `QuiverEngine` (via `createEngine`) is used only for the static module
catalog browser and patch validation; it never produces audio.

### Visualization
Because audio never returns to the main thread, the scope / lissajous / bars /
spectrum / VU visualizations read from Web Audio `AnalyserNode`s tapped off the
worklet output (a `ChannelSplitter` feeds per-channel time-domain analysers; a mono
analyser feeds the spectrum).

### MIDI Support
The demo supports Web MIDI API for external MIDI controllers:
- Note on/off messages
- CC messages for parameter control
- Pitch bend

### UI Components
- Virtual keyboard
- Knobs and sliders for module parameters
- Oscilloscope and spectrum analyzer visualizations
- Module patching interface

## Building for Production

```bash
cd demos/browser
npm run build           # Creates optimized build in dist/
npm run preview         # Preview production build
```
