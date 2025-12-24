# @quiver/types

TypeScript type definitions for the Quiver modular audio synthesis library.

## Installation

```bash
npm install @quiver/types
# or
pnpm add @quiver/types
```

## Usage

```typescript
import type {
  ModuleCatalog,
  ModuleMetadata,
  PortSpec,
  PortDef,
  SignalKind,
  PatchDef,
  ConnectionDef,
} from '@quiver/types';
```

## Types

### Module Types

#### ModuleCatalog

Catalog of all available module types.

```typescript
interface ModuleCatalog {
  modules: ModuleMetadata[];
  categories: string[];
}
```

#### ModuleMetadata

Metadata for a single module type.

```typescript
interface ModuleMetadata {
  type_id: string;
  name: string;
  category?: string;
  description?: string;
  port_spec: PortSpec;
}
```

#### PortSpec

Input and output port definitions for a module.

```typescript
interface PortSpec {
  inputs: PortDef[];
  outputs: PortDef[];
}
```

#### PortDef

Definition of a single port.

```typescript
interface PortDef {
  index: number;
  name: string;
  signal_kind?: SignalKind;
  default_value?: number;
  has_attenuverter?: boolean;
}
```

### Signal Types

#### SignalKind

Type of signal a port carries.

```typescript
type SignalKind =
  | 'audio'          // -1.0 to 1.0 audio signal
  | 'cv_bipolar'     // -5V to +5V CV
  | 'cv_unipolar'    // 0V to +10V CV
  | 'volt_per_octave' // 1V/octave pitch (0V = C4)
  | 'gate'           // 0V or +5V gate
  | 'trigger'        // Short pulse
  | 'clock';         // Clock pulses
```

#### SignalColors

Display colors for each signal type.

```typescript
interface SignalColors {
  audio: string;
  cv_bipolar: string;
  cv_unipolar: string;
  volt_per_octave: string;
  gate: string;
  trigger: string;
  clock: string;
}
```

### Patch Serialization

#### PatchDef

Serialized patch definition.

```typescript
interface PatchDef {
  name: string;
  modules: ModuleDef[];
  cables: ConnectionDef[];
  output_module?: string;
}
```

#### ModuleDef

Serialized module instance.

```typescript
interface ModuleDef {
  type_id: string;
  name: string;
  params?: Record<string, number>;
}
```

#### ConnectionDef

Serialized cable connection.

```typescript
interface ConnectionDef {
  from_module: string;
  from_port: string;
  to_module: string;
  to_port: string;
  attenuation?: number;
  offset?: number;
}
```

### Observer Types

#### ObservableValue

Values emitted by the observer system.

```typescript
type ObservableValue =
  | { type: 'param'; node_id: string; param_id: string; value: number }
  | { type: 'level'; node_id: string; port_id: number; rms_db: number; peak_db: number }
  | { type: 'gate'; node_id: string; port_id: number; active: boolean }
  | { type: 'scope'; node_id: string; port_id: number; samples: Float32Array }
  | { type: 'spectrum'; node_id: string; port_id: number; bins: Float32Array; freq_range: [number, number] };
```

#### SubscriptionTarget

Target for observer subscriptions.

```typescript
type SubscriptionTarget =
  | { type: 'param'; node_id: string; param_id: string }
  | { type: 'level'; node_id: string; port_id: number }
  | { type: 'gate'; node_id: string; port_id: number }
  | { type: 'scope'; node_id: string; port_id: number; buffer_size: number }
  | { type: 'spectrum'; node_id: string; port_id: number; fft_size: number };
```

## Type Generation

These types are derived from Rust source types via `tsify` and maintained in sync with the WASM bindings. See `scripts/check-types.ts` for compile-time type validation.

## License

MIT
