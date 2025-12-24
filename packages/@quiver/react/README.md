# @quiver/react

React hooks and components for integrating Quiver audio synthesis into React applications.

## Installation

```bash
npm install @quiver/react @quiver/wasm @quiver/types
# or
pnpm add @quiver/react @quiver/wasm @quiver/types
```

## Quick Start

```tsx
import { useQuiver, useModuleCatalog, useConnection } from '@quiver/react';

function Synth() {
  const { engine, isReady, error } = useQuiver({ sampleRate: 44100 });
  const catalog = useModuleCatalog(engine);

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

### useQuiver

Main hook for initializing and managing the Quiver engine.

```typescript
const {
  engine,      // QuiverEngine instance (null until ready)
  isReady,     // Whether WASM is loaded and engine is ready
  error,       // Any initialization error
  audioContext // AudioContext (if audio is started)
} = useQuiver({
  sampleRate: 44100,  // Optional, defaults to 44100
  autoInit: true      // Optional, auto-initialize on mount
});
```

### useModuleCatalog

Get the catalog of available module types.

```typescript
const catalog = useModuleCatalog(engine);
// catalog.modules: ModuleMetadata[]
// catalog.categories: string[]
```

### usePortSpec

Get port specification for a module type.

```typescript
const portSpec = usePortSpec(engine, 'vco');
// portSpec.inputs: PortDef[]
// portSpec.outputs: PortDef[]
```

### useConnection

Hook for managing connections between modules.

```typescript
const { connect, disconnect, cables } = useConnection(engine);

// Connect two ports
connect('osc.saw', 'filter.in');

// Disconnect
disconnect('osc.saw', 'filter.in');
```

### useParameter

Hook for module parameter control with optional debouncing.

```typescript
const [value, setValue] = useParameter(engine, 'filter', 0, {
  min: 0,
  max: 1,
  debounce: 16 // ms
});
```

### useObserver

Subscribe to real-time value updates from the engine.

```typescript
const updates = useObserver(engine, [
  { type: 'level', node_id: 'output', port_id: 0 },
  { type: 'scope', node_id: 'osc', port_id: 0, buffer_size: 512 }
]);

// updates is an array of ObservableValue
```

## Components

### QuiverProvider

Context provider for sharing engine state across components.

```tsx
import { QuiverProvider, useQuiverContext } from '@quiver/react';

function App() {
  return (
    <QuiverProvider sampleRate={44100}>
      <Synth />
    </QuiverProvider>
  );
}

function Synth() {
  const { engine, isReady } = useQuiverContext();
  // ...
}
```

## Types

This package exports types compatible with React Flow for building visual patching interfaces:

```typescript
import type {
  QuiverNodeData,
  QuiverEdgeData,
  ModuleTypeId
} from '@quiver/react';
```

### QuiverNodeData

Data structure for React Flow nodes representing modules.

```typescript
interface QuiverNodeData extends Record<string, unknown> {
  moduleType: ModuleTypeId;
  state?: Record<string, unknown>;
  label?: string;
}
```

### QuiverEdgeData

Data structure for React Flow edges representing cables.

```typescript
interface QuiverEdgeData extends Record<string, unknown> {
  signalKind?: string;
  attenuation?: number;
  offset?: number;
}
```

## Example: Simple Synth

```tsx
import { useQuiver, useParameter } from '@quiver/react';

function SimpleSynth() {
  const { engine, isReady } = useQuiver();
  const [cutoff, setCutoff] = useParameter(engine, 'filter', 0, {
    min: 0, max: 1, initial: 0.5
  });

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

## License

MIT
