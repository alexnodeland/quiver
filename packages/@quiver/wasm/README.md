# @quiver/wasm

WebAssembly bindings for the Quiver modular audio synthesis library.

## Installation

```bash
npm install @quiver/wasm
# or
pnpm add @quiver/wasm
```

## Quick Start

```typescript
import init, { QuiverEngine } from '@quiver/wasm/quiver';

// Initialize WASM module
await init();

// Create engine at 44.1kHz
const engine = new QuiverEngine(44100.0);

// Build a simple synth patch
engine.add_module('vco', 'osc');
engine.add_module('svf', 'filter');
engine.add_module('adsr', 'env');
engine.add_module('vca', 'amp');
engine.add_module('stereo_output', 'out');

// Connect modules
engine.connect('osc.saw', 'filter.in');
engine.connect('filter.lp', 'amp.in');
engine.connect('env.env', 'amp.cv');
engine.connect('amp.out', 'out.left');
engine.connect('amp.out', 'out.right');

// Set output and compile
engine.set_output('out');
engine.compile();

// Process audio (returns Float32Array)
const samples = engine.process_block(128);
```

## API Reference

### QuiverEngine

Main interface for the audio engine.

#### Constructor

```typescript
new QuiverEngine(sampleRate: number)
```

#### Module Management

- `add_module(typeId: string, name: string)` - Add a module to the patch
- `remove_module(name: string)` - Remove a module
- `module_count()` - Get number of modules
- `get_catalog()` - Get all available module types
- `get_categories()` - Get module categories
- `get_port_spec(typeId: string)` - Get input/output ports for a module type

#### Connections

- `connect(from: string, to: string)` - Connect two ports (format: "module.port")
- `disconnect(from: string, to: string)` - Disconnect two ports
- `cable_count()` - Get number of cables

#### Parameters

- `set_param(module: string, paramIndex: number, value: number)` - Set a module parameter

#### Audio Processing

- `compile()` - Compile the patch (required before processing)
- `reset()` - Reset all module state
- `tick()` - Process single sample, returns `[left, right]`
- `process_block(numSamples: number)` - Process block, returns interleaved Float32Array

#### MIDI

- `create_midi_input()` - Create MIDI CV modules (midi_voct, midi_gate, midi_velocity, midi_pitch_bend, midi_mod_wheel)
- `create_midi_cc_input(cc: number)` - Create CC input module
- `midi_note_on(note: number, velocity: number)` - Handle note on
- `midi_note_off(note: number, velocity: number)` - Handle note off
- `midi_cc(cc: number, value: number)` - Handle CC message
- `midi_pitch_bend(value: number)` - Handle pitch bend (-1 to 1)

#### Serialization

- `save_patch(name: string)` - Serialize patch to JSON
- `load_patch(json: string)` - Load patch from JSON

#### Observer

- `subscribe(type: string, nodeId: string, portId: string)` - Subscribe to updates
- `unsubscribe(subscriptionId: string)` - Unsubscribe
- `drain_updates()` - Get pending observable updates

### AudioManager

High-level manager for SharedArrayBuffer-based AudioWorklet integration.

```typescript
import { AudioManager } from '@quiver/wasm';

const manager = new AudioManager();
await manager.init(audioContext);
await manager.start();

// MIDI and parameter control
manager.noteOn(60, 100);
manager.noteOff(60, 0);
manager.setParameter('filter', 0, 0.5);
```

## Module Types

Available module types include:

**Oscillators**: `vco`, `lfo`, `noise_generator`, `supersaw`, `wavetable`

**Filters**: `svf`, `diode_ladder_filter`

**Envelopes**: `adsr`, `envelope_follower`

**Amplifiers**: `vca`, `mixer`, `attenuverter`

**Effects**: `chorus`, `reverb`, `delay_line`, `distortion`, `bitcrusher`

**Utilities**: `slew_limiter`, `quantizer`, `sample_and_hold`, `offset`

**Output**: `stereo_output`

See the full catalog with `engine.get_catalog()`.

## License

MIT
