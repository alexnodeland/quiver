# Browser & App Integration

This guide explains how to integrate Quiver into your browser application using the WASM bindings and npm packages.

## Overview

Quiver provides three npm packages for browser integration:

| Package | Purpose |
|---------|---------|
| `@quiver-dsp/wasm` | Core WASM engine and AudioWorklet utilities |
| `@quiver-dsp/react` | React hooks for UI integration |
| `@quiver-dsp/types` | TypeScript type definitions |

## Installation

```bash
npm install @quiver-dsp/wasm @quiver-dsp/react
```

## Initializing the Engine

The WASM module must be initialized before use:

```typescript
import { initWasm, createEngine } from '@quiver-dsp/wasm';

// Initialize once at app startup
await initWasm();

// Create an engine instance (44100 Hz sample rate)
const engine = await createEngine(44100);
```

## Building a Patch

### Add Modules

`add_module(typeId, name)` takes the snake_case `type_id` first and a unique instance
name second:

```typescript
engine.add_module('vco', 'osc');
engine.add_module('vca', 'amp');
engine.add_module('stereo_output', 'out');
```

### Connect Modules

Connections use `"module.port"` strings and return a stable `CableId` (a number):

```typescript
const c1 = engine.connect('osc.saw', 'amp.in');
engine.connect('amp.out', 'out.left');
engine.connect('amp.out', 'out.right');

// Attenuated / modulated variants:
engine.connect_attenuated('lfo.sin', 'vcf.cutoff', 0.5);
engine.connect_modulated('lfo.sin', 'vcf.cutoff', 0.3, 5.0);

// Choose the output module, then compile.
engine.set_output('out');
```

Remove a cable by the `CableId` you kept:

```typescript
engine.disconnect_cable(c1);
```

### Compile the Graph

After adding or connecting modules, compile the graph:

```typescript
engine.compile();
```

## Processing Audio

### Sample-by-Sample

```typescript
// Process one sample; returns a Float64Array [left, right].
const [left, right] = engine.tick();
```

### Block Processing (Recommended)

More efficient for real-time audio:

```typescript
// Process 128 samples at once.
const samples = engine.process_block(128);
// Returns Float32Array, interleaved: [L0, R0, L1, R1, ...]
```

> In practice you rarely call `process_block` yourself — the AudioWorklet helper below
> runs the engine on the audio thread for you.

## AudioWorklet Setup (Real-Time Audio)

Real-time audio runs the Quiver engine **inside** an AudioWorklet. `createQuiverAudioNode`
takes an `AudioContext` and the worklet/wasm URLs, and returns a handle you drive with
`loadPatch`, `setParam`, `connect`, MIDI methods, and so on. (The older
`createAudioContext` helper has been removed — this worklet path is the only supported way
to get audio out.)

```typescript
import { createQuiverAudioNode } from '@quiver-dsp/wasm';
import workletUrl from '@quiver-dsp/wasm/dist/worklet.js?url';
import wasmUrl from '@quiver-dsp/wasm/quiver_bg.wasm?url';

async function startAudio(myPatch) {
  const ctx = new AudioContext();

  // The engine lives in the worklet render thread.
  const quiver = await createQuiverAudioNode(ctx, { workletUrl, wasmUrl });

  // Load a patch (a PatchDef object), then connect to the speakers.
  await quiver.loadPatch(myPatch);
  quiver.node.connect(ctx.destination);

  // Resume requires a user gesture.
  await ctx.resume();

  return { ctx, quiver };
}
```

`createQuiverAudio({ workletUrl, wasmUrl })` is a convenience wrapper that creates the
`AudioContext` and connects the node to the destination for you.

The returned handle exposes `node`, `context`, `loadPatch`, `savePatch`, `setParam`,
`addModule`, `removeModule`, `connect`, `disconnect`, `setOutput`, `addMidiInputs`,
`midiNoteOn`, `midiNoteOff`, `midiCc`, `midiPitchBend`, `compile`, `reset`, and `dispose`.

### MIDI

Inject the engine-owned MIDI CV source modules, then drive them:

```typescript
quiver.addMidiInputs();      // adds midi_voct, midi_gate, midi_velocity, midi_mod, midi_bend
quiver.midiNoteOn(60, 100);
quiver.midiCc(1, 64);        // CC1 drives midi_mod
quiver.midiPitchBend(0.5);
```

Cable those `midi_*` module outputs into your patch to play it from MIDI.

### Architecture

```
Main Thread                      Audio Thread
┌─────────────┐                 ┌─────────────────┐
│  React UI   │ ──postMessage──▶│  AudioWorklet   │
│  (params)   │ ◀──────────────│  (process)      │──▶ Speakers
└─────────────┘                 └─────────────────┘
```

## React Integration

### useQuiverEngine

Initialize the engine in a component:

```tsx
import { useQuiverEngine } from '@quiver-dsp/react';

function Synth() {
  const { engine, isReady, error } = useQuiverEngine(44100);

  if (error) return <div>Error: {error.message}</div>;
  if (!isReady || !engine) return <div>Loading...</div>;

  return <PatchEditor engine={engine} />;
}
```

### useQuiverParam

Bind a parameter to UI. Returns a `[value, setValue]` tuple:

```tsx
import { useQuiverParam } from '@quiver-dsp/react';

function FrequencyKnob({ engine, nodeId }) {
  const [value, setValue] = useQuiverParam(engine, nodeId, 0);

  return <Knob value={value} onChange={setValue} />;
}
```

### useQuiverLevel

Display level meters. Returns `{ rmsDb, peakDb }`:

```tsx
import { useQuiverLevel } from '@quiver-dsp/react';

function Meter({ engine, nodeId, portId }) {
  const { rmsDb, peakDb } = useQuiverLevel(engine, nodeId, portId);

  return <LevelMeter rms={rmsDb} peak={peakDb} />;
}
```

## Next Steps

- [Module Catalog](./module-catalog.md) - Browse and search available modules
- [Observable Streaming](./observable-streaming.md) - Real-time visualization data
- [Serialization](./serialization.md) - Save and load patches
