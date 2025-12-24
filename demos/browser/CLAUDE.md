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

The dev server runs at `http://localhost:5173` by default.

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

- Uses `@quiver/wasm` package from `packages/@quiver/wasm/`
- Vite for bundling and dev server
- TypeScript for type safety

## Key Concepts

### Audio Worklet Integration
The demo uses a Web Audio API AudioWorklet for real-time audio processing. The WASM module runs in the worklet thread, ensuring glitch-free audio.

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
