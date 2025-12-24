# @quiver/react

React hooks and utilities for integrating Quiver audio synthesis into React applications.

## Installation

```bash
npm install @quiver/react @quiver/wasm
# or
pnpm add @quiver/react @quiver/wasm
```

## Quick Start

```tsx
import { useQuiverEngine, useQuiverCatalog } from '@quiver/react';

function Synth() {
  const { engine, isReady, error } = useQuiverEngine(44100);
  const catalog = useQuiverCatalog(engine);

  if (!isReady) return <div>Loading WASM...</div>;
  if (error) return <div>Error: {error.message}</div>;

  return (
    <div>
      <h2>Available Modules: {catalog?.modules.length}</h2>
      {/* Your synth UI here */}
    </div>
  );
}
```

## Hooks

### useQuiverEngine

Main hook for initializing and managing the Quiver engine.

```typescript
const {
  engine,      // QuiverEngine instance (null until ready)
  isReady,     // Whether WASM is loaded and engine is ready
  error,       // Any initialization error
} = useQuiverEngine(44100); // sampleRate, defaults to 44100
```

### useQuiverCatalog

Get the catalog of available module types.

```typescript
const catalog = useQuiverCatalog(engine);
// catalog.modules: ModuleCatalogEntry[]
// catalog.categories: string[]
```

### useQuiverSearch

Search for modules by name or description.

```typescript
const results = useQuiverSearch(engine, 'filter');
// results: ModuleCatalogEntry[]
```

### useQuiverParam

Hook for module parameter control with local state.

```typescript
const [value, setValue] = useQuiverParam(engine, 'filter', 0);

// value: current parameter value
// setValue: function to update the parameter
```

### useQuiverLevel

Subscribe to real-time level meter updates.

```typescript
const { rmsDb, peakDb } = useQuiverLevel(engine, 'output', 0);
// rmsDb: RMS level in dB
// peakDb: Peak level in dB
```

### useQuiverGate

Subscribe to gate state updates.

```typescript
const isActive = useQuiverGate(engine, 'env', 0);
// isActive: boolean gate state
```

### useQuiverUpdates

Low-level hook for subscribing to real-time value updates.

```typescript
const updates = useQuiverUpdates(engine, [
  { type: 'level', node_id: 'output', port_id: 0 },
  { type: 'gate', node_id: 'env', port_id: 0 },
]);

// updates: Map<string, ObservableValue>
```

### useQuiverPatch

Hook for loading and managing patches.

```typescript
const { isLoaded, error, loadPatch, savePatch, clearPatch } = useQuiverPatch(engine);

// Load a patch
await loadPatch(patchDef);

// Save current patch
const savedPatch = savePatch('My Patch');

// Clear the patch
clearPatch();
```

## React Flow Integration

This package provides utilities for integrating Quiver with React Flow for visual patching interfaces.

### Type Definitions

```typescript
import type {
  QuiverNodeData,
  QuiverEdgeData,
  QuiverNode,
  QuiverEdge,
} from '@quiver/react';
```

### Conversion Functions

```typescript
import {
  patchToReactFlow,
  reactFlowToPatch,
  createQuiverNode,
  createQuiverEdge,
} from '@quiver/react';

// Convert a Quiver patch to React Flow format
const { nodes, edges } = patchToReactFlow(patchDef, {
  defaultPosition: { x: 0, y: 0 },
  moduleSpacing: 250,
});

// Convert React Flow back to a Quiver patch
const patch = reactFlowToPatch(nodes, edges, {
  name: 'My Patch',
  author: 'User',
});

// Create new nodes and edges
const newNode = createQuiverNode('vco', { x: 100, y: 100 }, existingNames);
const newEdge = createQuiverEdge('osc', 'saw', 'filter', 'in');
```

### Utility Functions

```typescript
import {
  generateModuleName,
  updatePatchPositions,
  getCablesForModule,
  removeModuleFromPatch,
} from '@quiver/react';
```

## Example: Simple Synth

```tsx
import { useEffect } from 'react';
import { useQuiverEngine, useQuiverParam } from '@quiver/react';

function SimpleSynth() {
  const { engine, isReady } = useQuiverEngine();
  const [cutoff, setCutoff] = useQuiverParam(engine, 'filter', 0);

  useEffect(() => {
    if (!engine) return;

    // Build patch
    engine.add_module('vco', 'osc');
    engine.add_module('svf', 'filter');
    engine.add_module('stereo_output', 'out');

    engine.connect('osc.saw', 'filter.in');
    engine.connect('filter.lp', 'out.left');
    engine.connect('filter.lp', 'out.right');

    engine.set_output('out');
    engine.compile();
  }, [engine]);

  if (!isReady) return <div>Loading...</div>;

  return (
    <div>
      <label>
        Filter Cutoff
        <input
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={cutoff}
          onChange={(e) => setCutoff(parseFloat(e.target.value))}
        />
      </label>
    </div>
  );
}
```

## Re-exports

This package re-exports common types and functions from `@quiver/wasm`:

```typescript
import {
  // Types
  type PatchDef,
  type ModuleDef,
  type CableDef,
  type SignalKind,
  // Constants
  DEFAULT_SIGNAL_COLORS,
  // Functions
  parsePortReference,
  createPortReference,
  getSignalColor,
} from '@quiver/react';
```

## License

MIT
