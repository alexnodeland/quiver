# Browser & App Integration

This guide explains how to integrate Quiver into your browser application using the WASM bindings and npm packages.

## Overview

Quiver provides two npm packages for browser integration:

| Package | Purpose |
|---------|---------|
| `@quiver/wasm` | Core WASM engine, TypeScript types, and AudioWorklet utilities |
| `@quiver/react` | React hooks for UI integration |

## Installation

```bash
npm install @quiver/wasm @quiver/react
```

## Initializing the Engine

The WASM module must be initialized before use:

```typescript
import { initWasm, createEngine } from '@quiver/wasm';

// Initialize once at app startup
await initWasm();

// Create an engine instance (44100 Hz sample rate)
const engine = await createEngine(44100);
```

## Building a Patch

### Add Modules

```typescript
// Add modules by name and type
engine.add_module('vco', 'Vco');
engine.add_module('vca', 'Vca');
engine.add_module('output', 'StereoOutput');
```

### Connect Modules

```typescript
// Connect output port 0 of 'vco' to input port 0 of 'vca'
engine.connect('vco', 0, 'vca', 0);

// Connect VCA to stereo output (both channels)
engine.connect('vca', 0, 'output', 0);  // Left
engine.connect('vca', 0, 'output', 1);  // Right
```

### Compile the Graph

After adding or connecting modules, compile the graph:

```typescript
engine.compile();
```

## Processing Audio

### Sample-by-Sample

```typescript
// Process one sample, returns [left, right]
const [left, right] = engine.tick();
```

### Block Processing (Recommended)

More efficient for real-time audio:

```typescript
// Process 128 samples at once
const samples = engine.process_block(128);
// Returns Float64Array: [L0, R0, L1, R1, ...]
```

## AudioWorklet Setup

For real-time audio output in the browser:

```typescript
import { createQuiverAudioNode } from '@quiver/wasm';

async function startAudio(engine) {
  const audioContext = new AudioContext();

  // Create worklet node connected to engine
  const quiverNode = await createQuiverAudioNode(audioContext, engine);

  // Connect to speakers
  quiverNode.connect(audioContext.destination);

  // Start (requires user gesture)
  await audioContext.resume();

  return { audioContext, quiverNode };
}
```

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
import { useQuiverEngine } from '@quiver/react';

function Synth() {
  const { engine, loading, error } = useQuiverEngine(44100);

  if (loading) return <div>Loading...</div>;
  if (error) return <div>Error: {error.message}</div>;

  return <PatchEditor engine={engine} />;
}
```

### useQuiverParam

Bind a parameter to UI:

```tsx
import { useQuiverParam } from '@quiver/react';

function FrequencyKnob({ engine, nodeId }) {
  const [value, setValue, info] = useQuiverParam(engine, nodeId, 0);

  return (
    <Knob
      value={value}
      min={info.min}
      max={info.max}
      onChange={setValue}
    />
  );
}
```

### useQuiverLevel

Display level meters:

```tsx
import { useQuiverLevel } from '@quiver/react';

function Meter({ engine, nodeId, portId }) {
  const { rms_db, peak_db } = useQuiverLevel(engine, nodeId, portId);

  return <LevelMeter rms={rms_db} peak={peak_db} />;
}
```

## Next Steps

- [Module Catalog](./module-catalog.md) - Browse and search available modules
- [Observable Streaming](./observable-streaming.md) - Real-time visualization data
- [Serialization](./serialization.md) - Save and load patches
