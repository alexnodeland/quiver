# GUI Integration Framework Plan

This document outlines Quiver's approach to GUI integration. Rather than reimplementing graph editor functionality, we focus on providing the **audio-domain primitives** that graph libraries (React Flow, xyflow, etc.) need to build a modular synth UI.

## Philosophy

**What React Flow / xyflow already handles well:**
- Node positioning and dragging
- Edge/cable rendering (bezier curves)
- Hit testing and selection
- Pan/zoom
- Undo/redo
- Copy/paste
- Layout algorithms (dagre integration)
- Keyboard shortcuts

**What Quiver must provide:**
- Module introspection (parameters, ports, metadata)
- Signal semantics (port types, compatibility, colors)
- Real-time state bridge (parameter values, meters, scopes)
- Serialization contract (JSON schema for patches)

---

## Deployment Architectures

Quiver supports two deployment modes. The **same core types** are used in both—only the transport layer differs.

### Architecture A: WASM In-Browser (Recommended for Web Apps)

Audio processing runs in the browser via WebAssembly + AudioWorklet. No server required.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              Browser                                    │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                     React Frontend                               │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌───────────────────────┐   │   │
│  │  │ React Flow  │  │  Knobs/UI   │  │  Meters/Scopes        │   │   │
│  │  │  (graph)    │  │  (params)   │  │  (real-time)          │   │   │
│  │  └──────┬──────┘  └──────┬──────┘  └───────────┬───────────┘   │   │
│  │         │                │                     │               │   │
│  │         └────────────────┼─────────────────────┘               │   │
│  │                          │                                     │   │
│  │  ┌───────────────────────┴──────────────────────────────────┐  │   │
│  │  │              @quiver/wasm bindings                        │  │   │
│  │  │  • Direct function calls (no serialization overhead)     │  │   │
│  │  │  • Patch state lives in WASM memory                      │  │   │
│  │  │  • requestAnimationFrame for UI updates                  │  │   │
│  │  └───────────────────────┬──────────────────────────────────┘  │   │
│  └──────────────────────────┼──────────────────────────────────────┘   │
│                             │ wasm-bindgen                             │
│  ┌──────────────────────────┴──────────────────────────────────────┐   │
│  │                    Quiver WASM Module                            │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │   │
│  │  │   Patch     │  │  Module     │  │  AudioWorkletProcessor  │  │   │
│  │  │   Graph     │  │  Registry   │  │  (real-time DSP)        │  │   │
│  │  └─────────────┘  └─────────────┘  └─────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

**Pros:**
- Zero latency UI ↔ engine communication
- Works offline, no server costs
- Simpler deployment (static hosting)

**Cons:**
- Limited to browser audio capabilities
- WASM binary size (~500KB-2MB)

### Architecture B: Rust Backend (For Desktop/DAW Plugins)

Full Rust backend with HTTP/WebSocket API. UI can be web-based or native.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          React Frontend                                 │
│  ┌─────────────┐  ┌─────────────┐  ┌───────────────────────────────┐   │
│  │ React Flow  │  │  Knobs/UI   │  │  Meters/Scopes                │   │
│  │  (graph)    │  │  (params)   │  │  (real-time via WebSocket)    │   │
│  └──────┬──────┘  └──────┬──────┘  └───────────────┬───────────────┘   │
│         └────────────────┼─────────────────────────┘                   │
│                          │                                             │
│  ┌───────────────────────┴──────────────────────────────────────────┐  │
│  │                    HTTP/WebSocket Client                          │  │
│  └───────────────────────┬──────────────────────────────────────────┘  │
└──────────────────────────┼──────────────────────────────────────────────┘
                           │ HTTP / WebSocket
                           ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         Rust Backend                                    │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────────────┐ │
│  │   Patch     │  │  Module     │  │  Audio Thread                   │ │
│  │   Graph     │  │  Registry   │  │  (JACK/ALSA/CoreAudio/WASAPI)   │ │
│  └─────────────┘  └─────────────┘  └─────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

**Pros:**
- Full system audio access (JACK, ASIO, etc.)
- Lower CPU usage for complex patches
- Can integrate with DAWs as plugin

**Cons:**
- Requires running server
- Network latency for UI updates

---

## Unified Bridge API

The bridge API is designed to work identically in both architectures. Types are defined once in Rust with `#[wasm_bindgen]` + serde for dual-target support.

### Transport Abstraction

```typescript
// The React app uses this interface regardless of backend
interface QuiverBridge {
  // Catalog
  getCatalog(): Promise<CatalogResponse>;
  searchModules(query: string): Promise<ModuleCatalogEntry[]>;

  // Patch operations
  loadPatch(patch: PatchDef): Promise<void>;
  savePatch(): Promise<PatchDef>;

  // Module operations
  addModule(typeId: string, name: string, position: [number, number]): Promise<string>;
  removeModule(nodeId: string): Promise<void>;
  setModulePosition(nodeId: string, position: [number, number]): Promise<void>;

  // Cables
  connect(from: string, to: string): Promise<void>;
  disconnect(from: string, to: string): Promise<void>;

  // Parameters
  getParams(nodeId: string): Promise<ParamInfo[]>;
  setParam(nodeId: string, paramId: string, value: number): Promise<void>;

  // Real-time subscriptions
  subscribe(targets: SubscriptionTarget[]): Unsubscribe;
  onUpdate(callback: (updates: ObservableValue[]) => void): Unsubscribe;

  // Signal info
  getSignalColors(): SignalColors;
  checkCompatibility(from: SignalKind, to: SignalKind): Compatibility;
}

// WASM implementation (direct calls)
class WasmBridge implements QuiverBridge {
  private engine: QuiverEngine; // wasm-bindgen generated

  async getCatalog() {
    return this.engine.get_catalog(); // Direct WASM call
  }
  // ...
}

// HTTP implementation (fetch + WebSocket)
class HttpBridge implements QuiverBridge {
  private baseUrl: string;
  private ws: WebSocket;

  async getCatalog() {
    const res = await fetch(`${this.baseUrl}/api/catalog`);
    return res.json();
  }
  // ...
}
```

### Feature Detection

```typescript
// Auto-detect best available backend
async function createBridge(): Promise<QuiverBridge> {
  // Try WASM first (preferred for web)
  if (typeof WebAssembly !== 'undefined') {
    try {
      const wasm = await import('@quiver/wasm');
      await wasm.default(); // Initialize WASM
      return new WasmBridge(wasm);
    } catch (e) {
      console.warn('WASM unavailable, falling back to HTTP');
    }
  }

  // Fall back to HTTP backend
  const serverUrl = process.env.QUIVER_SERVER_URL || 'http://localhost:3000';
  return new HttpBridge(serverUrl);
}
```

---

## Phase 1: Introspection API

Expose module metadata so the UI can render appropriate controls.

### 1.1 Parameter Information

```rust
/// Complete parameter descriptor for UI generation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct ParamInfo {
    /// Unique identifier within module
    pub id: String,
    /// Display name
    pub name: String,
    /// Current value (normalized 0.0-1.0 or actual)
    pub value: f64,
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// Default value
    pub default: f64,
    /// Value scaling
    pub curve: ParamCurve,
    /// Suggested control type
    pub control: ControlType,
    /// Unit for display (Hz, ms, dB, %, etc.)
    pub unit: Option<String>,
    /// Value formatting hint
    pub format: ValueFormat,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum ParamCurve {
    Linear,
    Exponential,
    Logarithmic,
    Stepped { steps: u32 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum ControlType {
    Knob,
    Slider,
    Toggle,
    Select,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case")]
pub enum ValueFormat {
    Decimal { places: u8 },
    Frequency,
    Time,
    Decibels,
    Percent,
    NoteName,
    Ratio,
}
```

### 1.2 Module Introspection Trait

```rust
/// Trait for modules to expose their parameters to UIs
pub trait ModuleIntrospection: GraphModule {
    /// Get parameter descriptors
    fn parameters(&self) -> Vec<ParamInfo> {
        Vec::new()
    }

    /// Get parameter by id
    fn get_param_info(&self, id: &str) -> Option<ParamInfo> {
        self.parameters().into_iter().find(|p| p.id == id)
    }
}
```

### 1.3 WASM Bindings

```rust
#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl QuiverEngine {
    /// Get parameters for a module instance
    pub fn get_params(&self, node_id: &str) -> Result<JsValue, JsError> {
        let params = self.patch.get_module_params(node_id)?;
        Ok(serde_wasm_bindgen::to_value(&params)?)
    }

    /// Set a parameter value
    pub fn set_param(&mut self, node_id: &str, param_id: &str, value: f64) -> Result<(), JsError> {
        self.patch.set_param(node_id, param_id, value)?;
        Ok(())
    }
}
```

**Deliverables:**
- [ ] `ParamInfo` struct with serde + tsify derives
- [ ] `ParamCurve`, `ControlType`, `ValueFormat` enums
- [ ] `ModuleIntrospection` trait
- [ ] Implement for all built-in modules (VCO, VCF, ADSR, etc.)
- [ ] WASM bindings via `wasm-bindgen`
- [ ] Unit tests

---

## Phase 2: Signal Semantics

Provide port type information for cable coloring and compatibility validation.

### 2.1 Enhanced Port Information

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct PortInfo {
    pub id: u32,
    pub name: String,
    pub kind: SignalKind,
    pub normalled_to: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct SignalColors {
    pub audio: String,
    pub cv_bipolar: String,
    pub cv_unipolar: String,
    pub volt_per_octave: String,
    pub gate: String,
    pub trigger: String,
    pub clock: String,
}

impl Default for SignalColors {
    fn default() -> Self {
        Self {
            audio: "#e94560".into(),
            cv_bipolar: "#0f3460".into(),
            cv_unipolar: "#00b4d8".into(),
            volt_per_octave: "#90be6d".into(),
            gate: "#f9c74f".into(),
            trigger: "#f8961e".into(),
            clock: "#9d4edd".into(),
        }
    }
}
```

### 2.2 Port Compatibility

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum Compatibility {
    Exact,
    Allowed,
    Warning { message: String },
}

pub fn ports_compatible(from: SignalKind, to: SignalKind) -> Compatibility {
    use SignalKind::*;
    match (from, to) {
        (a, b) if a == b => Compatibility::Exact,
        (Audio, _) => Compatibility::Allowed,
        (CvBipolar, CvUnipolar) | (CvUnipolar, CvBipolar) => Compatibility::Allowed,
        (VoltPerOctave, CvBipolar) | (VoltPerOctave, CvUnipolar) => Compatibility::Allowed,
        (Gate, Trigger) | (Trigger, Gate) => Compatibility::Allowed,
        (Clock, Gate) | (Clock, Trigger) => Compatibility::Allowed,
        (Gate, Audio) | (Trigger, Audio) => {
            Compatibility::Warning { message: "Gate→Audio may cause clicks".into() }
        }
        (CvBipolar, VoltPerOctave) => {
            Compatibility::Warning { message: "CV→V/Oct may cause tuning issues".into() }
        }
        _ => Compatibility::Allowed,
    }
}
```

**Deliverables:**
- [ ] `PortInfo` with serde + tsify derives
- [ ] `SignalColors` with CSS hex defaults
- [ ] `ports_compatible()` function (available in both WASM and server)
- [ ] `Compatibility` enum

---

## Phase 3: Module Catalog

Provide searchable module list for the "add module" UI.

### 3.1 Catalog Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct ModuleCatalogEntry {
    pub type_id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub ports: PortSummary,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct PortSummary {
    pub inputs: u8,
    pub outputs: u8,
    pub has_audio_in: bool,
    pub has_audio_out: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct CatalogResponse {
    pub modules: Vec<ModuleCatalogEntry>,
    pub categories: Vec<String>,
}
```

### 3.2 Registry Extensions

```rust
impl ModuleRegistry {
    pub fn catalog(&self) -> CatalogResponse;
    pub fn search(&self, query: &str) -> Vec<ModuleCatalogEntry>;
    pub fn by_category(&self, category: &str) -> Vec<ModuleCatalogEntry>;
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl QuiverEngine {
    pub fn get_catalog(&self) -> Result<JsValue, JsError> {
        Ok(serde_wasm_bindgen::to_value(&self.registry.catalog())?)
    }

    pub fn search_modules(&self, query: &str) -> Result<JsValue, JsError> {
        Ok(serde_wasm_bindgen::to_value(&self.registry.search(query))?)
    }
}
```

**Deliverables:**
- [ ] `ModuleCatalogEntry` struct
- [ ] `PortSummary` struct
- [ ] `ModuleRegistry::catalog()`, `search()`, `by_category()`
- [ ] WASM bindings
- [ ] Populate keywords for all built-in modules

---

## Phase 4: Real-Time State Bridge

Stream live values from the audio processing to the UI.

### 4.1 Observable Values

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ObservableValue {
    Param { node_id: String, param_id: String, value: f64 },
    Level { node_id: String, port_id: u32, rms_db: f64, peak_db: f64 },
    Gate { node_id: String, port_id: u32, active: bool },
    Scope { node_id: String, port_id: u32, samples: Vec<f32> },
    Spectrum { node_id: String, port_id: u32, bins: Vec<f32>, freq_range: (f32, f32) },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum SubscriptionTarget {
    Param { node_id: String, param_id: String },
    Level { node_id: String, port_id: u32 },
    Gate { node_id: String, port_id: u32 },
    Scope { node_id: String, port_id: u32, buffer_size: usize },
    Spectrum { node_id: String, port_id: u32, fft_size: usize },
}
```

### 4.2 Architecture-Specific Implementation

**WASM (requestAnimationFrame polling):**

```rust
#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl QuiverEngine {
    /// Called from requestAnimationFrame loop
    /// Returns pending updates since last call
    pub fn poll_updates(&mut self) -> Result<JsValue, JsError> {
        let updates = self.observer.drain_updates();
        Ok(serde_wasm_bindgen::to_value(&updates)?)
    }

    /// Subscribe to real-time values
    pub fn subscribe(&mut self, targets: JsValue) -> Result<(), JsError> {
        let targets: Vec<SubscriptionTarget> = serde_wasm_bindgen::from_value(targets)?;
        self.observer.add_subscriptions(targets);
        Ok(())
    }

    pub fn unsubscribe(&mut self, target_ids: JsValue) -> Result<(), JsError> {
        let ids: Vec<String> = serde_wasm_bindgen::from_value(target_ids)?;
        self.observer.remove_subscriptions(&ids);
        Ok(())
    }
}
```

```typescript
// React hook for WASM polling
function useQuiverUpdates(bridge: WasmBridge, targets: SubscriptionTarget[]) {
  const [values, setValues] = useState<Map<string, ObservableValue>>(new Map());

  useEffect(() => {
    bridge.subscribe(targets);

    let animationId: number;
    const poll = () => {
      const updates = bridge.pollUpdates();
      if (updates.length > 0) {
        setValues(prev => {
          const next = new Map(prev);
          for (const update of updates) {
            next.set(getUpdateKey(update), update);
          }
          return next;
        });
      }
      animationId = requestAnimationFrame(poll);
    };
    animationId = requestAnimationFrame(poll);

    return () => {
      cancelAnimationFrame(animationId);
      bridge.unsubscribe(targets.map(t => getTargetId(t)));
    };
  }, [bridge, targets]);

  return values;
}
```

**HTTP Backend (WebSocket push):**

```typescript
// React hook for WebSocket streaming
function useQuiverUpdates(bridge: HttpBridge, targets: SubscriptionTarget[]) {
  const [values, setValues] = useState<Map<string, ObservableValue>>(new Map());

  useEffect(() => {
    const unsubscribe = bridge.subscribe(targets);

    bridge.onUpdate((updates) => {
      setValues(prev => {
        const next = new Map(prev);
        for (const update of updates) {
          next.set(getUpdateKey(update), update);
        }
        return next;
      });
    });

    return unsubscribe;
  }, [bridge, targets]);

  return values;
}
```

**Deliverables:**
- [ ] `ObservableValue` enum with all observable types
- [ ] `SubscriptionTarget` enum
- [ ] WASM: `poll_updates()`, `subscribe()`, `unsubscribe()`
- [ ] HTTP: WebSocket message types
- [ ] React hooks for both backends (unified interface)
- [ ] Rate limiting (max 60 Hz updates)

---

## Phase 5: Serialization Contract

Document the JSON schema for frontend interop.

### 5.1 Patch Format (Already Implemented)

```typescript
interface PatchDef {
  version: number;
  name: string;
  author?: string;
  description?: string;
  tags: string[];
  modules: ModuleDef[];
  cables: CableDef[];
  parameters: Record<string, number>;
}

interface ModuleDef {
  name: string;
  module_type: string;
  position?: [number, number];
  state?: object;
}

interface CableDef {
  from: string;
  to: string;
  attenuation?: number;
  offset?: number;
}
```

### 5.2 React Flow Mapping

```typescript
function patchToReactFlow(patch: PatchDef): { nodes: Node[], edges: Edge[] } {
  const nodes = patch.modules.map(m => ({
    id: m.name,
    type: 'quiverModule',
    position: { x: m.position?.[0] ?? 0, y: m.position?.[1] ?? 0 },
    data: { moduleType: m.module_type, state: m.state }
  }));

  const edges = patch.cables.map((c, i) => {
    const [fromModule, fromPort] = c.from.split('.');
    const [toModule, toPort] = c.to.split('.');
    return {
      id: `cable-${i}`,
      source: fromModule,
      sourceHandle: fromPort,
      target: toModule,
      targetHandle: toPort,
      data: { attenuation: c.attenuation, offset: c.offset }
    };
  });

  return { nodes, edges };
}

function reactFlowToPatch(
  nodes: Node[],
  edges: Edge[],
  metadata: { name: string, author?: string }
): PatchDef {
  return {
    version: 1,
    name: metadata.name,
    author: metadata.author,
    tags: [],
    modules: nodes.map(n => ({
      name: n.id,
      module_type: n.data.moduleType,
      position: [n.position.x, n.position.y],
      state: n.data.state
    })),
    cables: edges.map(e => ({
      from: `${e.source}.${e.sourceHandle}`,
      to: `${e.target}.${e.targetHandle}`,
      attenuation: e.data?.attenuation,
      offset: e.data?.offset
    })),
    parameters: {}
  };
}
```

**Deliverables:**
- [ ] JSON Schema file (`schemas/patch.schema.json`)
- [ ] TypeScript type definitions (`@quiver/types` package)
- [ ] React Flow mapping utilities
- [ ] Validation function for patch JSON

---

## Implementation Order

| Phase | Name | Priority | Effort | WASM | HTTP |
|-------|------|----------|--------|------|------|
| 5 | Serialization Contract | High | Low | ✓ | ✓ |
| 2 | Signal Semantics | High | Low | ✓ | ✓ |
| 1 | Introspection API | High | Medium | ✓ | ✓ |
| 3 | Module Catalog | Medium | Low | ✓ | ✓ |
| 4 | Real-Time Bridge | Medium | High | polling | WebSocket |

**Recommended order:** 5 → 2 → 1 → 3 → 4

---

## Integration Layer (Part Two)

The core types from Phases 1-5 are complete. The following integration work connects these types to the actual modules and exposes them to JavaScript.

### Module Introspection Implementations

Each built-in module must implement `ModuleIntrospection` to expose its parameters to the UI.

**Modules requiring implementation (36 total):**

| Category | Modules |
|----------|---------|
| Oscillators | `Vco`, `AnalogVco`, `Lfo` |
| Filters | `StateVariableFilter`, `DiodeLadder` |
| Envelopes | `Adsr` |
| Utilities | `Vca`, `Mixer`, `Offset`, `UnitDelay`, `Multiple`, `Attenuverter`, `SlewLimiter`, `SampleAndHold`, `PrecisionAdder`, `VcSwitch`, `Min`, `Max` |
| Sources | `Noise` |
| Sequencing | `StepSequencer`, `Clock` |
| Effects | `Saturator`, `Wavefolder`, `RingMod`, `Crossfader`, `Rectifier` |
| Analog Modeling | `Crosstalk`, `GroundLoop` |
| Logic | `LogicAnd`, `LogicOr`, `LogicXor`, `LogicNot`, `Comparator` |
| Random | `BernoulliGate` |
| I/O | `StereoOutput`, `Quantizer` |

**Implementation pattern for each module:**

```rust
impl ModuleIntrospection for Vco {
    fn param_infos(&self) -> Vec<ParamInfo> {
        vec![
            ParamInfo::frequency("frequency", "Frequency")
                .with_range(0.1, 20000.0)
                .with_default(440.0)
                .with_value(self.frequency),
            ParamInfo::select("waveform", "Waveform", 4)
                .with_value(self.waveform as f64),
            ParamInfo::percent("pulse_width", "Pulse Width")
                .with_default(0.5)
                .with_value(self.pulse_width),
        ]
    }

    fn set_param_by_id(&mut self, id: &str, value: f64) -> bool {
        match id {
            "frequency" => { self.frequency = value; true }
            "waveform" => { self.waveform = value as u8; true }
            "pulse_width" => { self.pulse_width = value; true }
            _ => false
        }
    }
}
```

**Effort estimate:** ~2-4 hours (mostly mechanical, with some judgment on parameter ranges/curves)

### WASM Bindings

The WASM bindings expose the Rust engine to JavaScript via `wasm-bindgen`. This requires:

1. **Feature flag setup** in `Cargo.toml`:

```toml
[features]
wasm = ["wasm-bindgen", "tsify", "serde-wasm-bindgen", "alloc"]

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
tsify = { version = "0.4", features = ["js"] }
serde-wasm-bindgen = "0.6"
```

2. **QuiverEngine wrapper** (`src/wasm/engine.rs`):

```rust
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub struct QuiverEngine {
    patch: Patch,
    registry: ModuleRegistry,
    observer: StateObserver,
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl QuiverEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f64) -> Self {
        let registry = ModuleRegistry::with_builtins();
        Self {
            patch: Patch::new(sample_rate),
            registry,
            observer: StateObserver::new(),
        }
    }

    // Catalog API (Phase 3)
    pub fn get_catalog(&self) -> Result<JsValue, JsError> {
        Ok(serde_wasm_bindgen::to_value(&self.registry.catalog())?)
    }

    pub fn search_modules(&self, query: &str) -> Result<JsValue, JsError> {
        Ok(serde_wasm_bindgen::to_value(&self.registry.search(query))?)
    }

    // Introspection API (Phase 1)
    pub fn get_params(&self, node_id: &str) -> Result<JsValue, JsError> {
        let module = self.patch.get_module(node_id)?;
        let params = module.param_infos();
        Ok(serde_wasm_bindgen::to_value(&params)?)
    }

    pub fn set_param(&mut self, node_id: &str, param_id: &str, value: f64) -> Result<(), JsError> {
        let module = self.patch.get_module_mut(node_id)?;
        module.set_param_by_id(param_id, value);
        Ok(())
    }

    // Real-Time Bridge (Phase 4)
    pub fn subscribe(&mut self, targets: JsValue) -> Result<(), JsError> {
        let targets: Vec<SubscriptionTarget> = serde_wasm_bindgen::from_value(targets)?;
        self.observer.add_subscriptions(targets);
        Ok(())
    }

    pub fn unsubscribe(&mut self, target_keys: JsValue) -> Result<(), JsError> {
        let keys: Vec<String> = serde_wasm_bindgen::from_value(target_keys)?;
        self.observer.remove_subscriptions(&keys);
        Ok(())
    }

    pub fn poll_updates(&mut self) -> Result<JsValue, JsError> {
        let updates = self.observer.drain_updates();
        Ok(serde_wasm_bindgen::to_value(&updates)?)
    }

    // Patch operations
    pub fn load_patch(&mut self, patch_json: JsValue) -> Result<(), JsError> {
        let patch_def: PatchDef = serde_wasm_bindgen::from_value(patch_json)?;
        self.patch = self.registry.instantiate_patch(&patch_def)?;
        Ok(())
    }

    pub fn save_patch(&self) -> Result<JsValue, JsError> {
        let patch_def = self.patch.to_def();
        Ok(serde_wasm_bindgen::to_value(&patch_def)?)
    }

    // Audio processing
    pub fn process(&mut self, buffer_size: usize) -> Result<JsValue, JsError> {
        let output = self.patch.process(buffer_size);
        self.observer.collect_updates(&self.patch);
        Ok(serde_wasm_bindgen::to_value(&output)?)
    }
}
```

3. **Build with wasm-pack:**

```bash
wasm-pack build --target web --features wasm
```

4. **React hooks** (`packages/@quiver/react/src/hooks.ts`):

```typescript
import { useEffect, useState, useRef, useCallback } from 'react';
import type { ObservableValue, SubscriptionTarget } from '@quiver/types';
import { getObservableValueKey } from '@quiver/types';

export function useQuiverUpdates(
  engine: QuiverEngine,
  targets: SubscriptionTarget[]
): Map<string, ObservableValue> {
  const [values, setValues] = useState<Map<string, ObservableValue>>(new Map());
  const targetsRef = useRef(targets);

  useEffect(() => {
    engine.subscribe(targets);
    targetsRef.current = targets;

    let animationId: number;
    const poll = () => {
      const updates = engine.poll_updates();
      if (updates.length > 0) {
        setValues(prev => {
          const next = new Map(prev);
          for (const update of updates) {
            next.set(getObservableValueKey(update), update);
          }
          return next;
        });
      }
      animationId = requestAnimationFrame(poll);
    };
    animationId = requestAnimationFrame(poll);

    return () => {
      cancelAnimationFrame(animationId);
      engine.unsubscribe(targets.map(t => getSubscriptionTargetKey(t)));
    };
  }, [engine, JSON.stringify(targets)]);

  return values;
}

export function useQuiverParam(
  engine: QuiverEngine,
  nodeId: string,
  paramId: string
): [number, (value: number) => void] {
  const [value, setValue] = useState(0);

  const targets = useMemo(
    () => [{ type: 'param' as const, node_id: nodeId, param_id: paramId }],
    [nodeId, paramId]
  );

  const updates = useQuiverUpdates(engine, targets);

  useEffect(() => {
    const key = `param:${nodeId}:${paramId}`;
    const update = updates.get(key);
    if (update?.type === 'param') {
      setValue(update.value);
    }
  }, [updates, nodeId, paramId]);

  const setParam = useCallback(
    (newValue: number) => {
      engine.set_param(nodeId, paramId, newValue);
      setValue(newValue);
    },
    [engine, nodeId, paramId]
  );

  return [value, setParam];
}
```

**Effort estimate:** ~4-8 hours (depends on AudioWorklet integration complexity)

---

## File Structure

```
src/
├── introspection.rs     # ParamInfo, ModuleIntrospection trait
├── bridge.rs            # QuiverBridge types, shared between WASM/HTTP
├── serialize.rs         # (existing) + JSON schema docs
└── lib.rs               # Re-exports

src/wasm/                # feature = "wasm"
├── mod.rs
├── engine.rs            # QuiverEngine wasm_bindgen wrapper
└── observer.rs          # Real-time value polling

src/server/              # feature = "server"
├── mod.rs
├── routes.rs            # HTTP endpoints
└── websocket.rs         # WebSocket handler

schemas/
└── patch.schema.json    # JSON Schema for validation

packages/
├── @quiver/types/       # Generated TypeScript types
├── @quiver/wasm/        # WASM bindings (wasm-pack)
└── @quiver/react/       # React hooks + bridge abstraction
```

---

## Feature Flags

```toml
[features]
default = ["std"]
std = []
alloc = []

# WASM target (browser)
wasm = ["wasm-bindgen", "tsify", "serde-wasm-bindgen", "alloc"]

# HTTP server target
server = ["std", "tokio", "axum", "tower-http"]
```

---

## What We're NOT Building

The following are explicitly **out of scope** (React Flow handles them):

| Feature | Use Instead |
|---------|-------------|
| Node dragging | React Flow `onNodeDrag` |
| Cable bezier rendering | React Flow edges |
| Hit testing | React Flow built-in |
| Pan/zoom | React Flow viewport |
| Undo/redo | React state + `use-undoable` |
| Keyboard shortcuts | React event handlers |
| Selection | React Flow selection |
| Copy/paste | React Flow + clipboard API |
| Layout algorithms | React Flow + dagre |

---

## Future Considerations

- **MIDI learn mode** - Map MIDI CC to parameters
- **Preset thumbnails** - Render patch preview images
- **Module grouping** - Visual subpatches
- **Collaborative editing** - CRDT-based sync
- **Native UI** - egui/iced bindings using same bridge types

---

## References

- [React Flow](https://reactflow.dev/) - React library for node-based UIs
- [xyflow](https://github.com/xyflow/xyflow) - Framework-agnostic core
- [wasm-bindgen](https://rustwasm.github.io/docs/wasm-bindgen/) - Rust↔JS interop
- [tsify](https://github.com/madonoharu/tsify) - Generate TypeScript types from Rust
- [Quiver Serialization](./src/serialize.rs) - Existing implementation
