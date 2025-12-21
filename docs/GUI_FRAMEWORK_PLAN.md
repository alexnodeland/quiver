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

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                     React Frontend                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ React Flow  │  │  Knobs/UI   │  │  Meters/Scopes      │ │
│  │  (graph)    │  │  (params)   │  │  (real-time)        │ │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘ │
│         │                │                     │            │
│         └────────────────┼─────────────────────┘            │
│                          │                                  │
│  ┌───────────────────────┴───────────────────────────────┐ │
│  │                  quiver-web bridge                     │ │
│  │  • Patch JSON ←→ React Flow nodes/edges               │ │
│  │  • Module catalog endpoint                            │ │
│  │  • Parameter get/set                                  │ │
│  │  • WebSocket for real-time values                     │ │
│  └───────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ HTTP/WebSocket
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Rust Backend (Quiver)                    │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │   Patch     │  │  Module     │  │  Audio Thread       │ │
│  │   Graph     │  │  Registry   │  │  (real-time DSP)    │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Introspection API

Expose module metadata so the UI can render appropriate controls.

### 1.1 Parameter Information

```rust
/// Complete parameter descriptor for UI generation
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[serde(rename_all = "snake_case")]
pub enum ParamCurve {
    Linear,
    Exponential,
    Logarithmic,
    /// Stepped/quantized values
    Stepped { steps: u32 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlType {
    Knob,
    Slider,
    Toggle,
    /// Dropdown with named options
    Select { options: &'static [&'static str] },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueFormat {
    /// Show raw number with N decimal places
    Decimal { places: u8 },
    /// Format as frequency (20 Hz, 1.5 kHz, etc.)
    Frequency,
    /// Format as time (5 ms, 1.2 s, etc.)
    Time,
    /// Format as dB
    Decibels,
    /// Format as percentage
    Percent,
    /// Format as note name (C4, A#3, etc.)
    NoteName,
    /// Format as ratio (1:4, 2:1)
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

    /// Get parameter by id (for batch queries)
    fn get_param_info(&self, id: &str) -> Option<ParamInfo> {
        self.parameters().into_iter().find(|p| p.id == id)
    }
}
```

### 1.3 JSON Endpoint Schema

```typescript
// GET /api/modules/:node_id/params
interface ModuleParams {
  node_id: string;
  module_type: string;
  params: ParamInfo[];
}

// GET /api/modules/:node_id/params/:param_id
// PUT /api/modules/:node_id/params/:param_id  { value: number }
interface ParamValue {
  id: string;
  value: number;
}
```

**Deliverables:**
- [ ] `ParamInfo` struct with serde derives
- [ ] `ParamCurve`, `ControlType`, `ValueFormat` enums
- [ ] `ModuleIntrospection` trait
- [ ] Implement for all built-in modules (VCO, VCF, ADSR, etc.)
- [ ] Unit tests

---

## Phase 2: Signal Semantics

Provide port type information for cable coloring and compatibility validation.

### 2.1 Enhanced Port Information

```rust
/// Extended port info for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortInfo {
    pub id: u32,
    pub name: String,
    pub kind: SignalKind,
    /// Normalled connection (what this defaults to when unpatched)
    pub normalled_to: Option<String>,
    /// Port description for tooltips
    pub description: Option<String>,
}

/// Color scheme for signal types (CSS hex colors)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalColors {
    pub audio: String,         // "#e94560"
    pub cv_bipolar: String,    // "#0f3460"
    pub cv_unipolar: String,   // "#00b4d8"
    pub volt_per_octave: String, // "#90be6d"
    pub gate: String,          // "#f9c74f"
    pub trigger: String,       // "#f8961e"
    pub clock: String,         // "#9d4edd"
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

### 2.2 Port Compatibility Matrix

```rust
/// Check if connecting these port types makes sense
pub fn ports_compatible(from: SignalKind, to: SignalKind) -> Compatibility {
    use SignalKind::*;
    match (from, to) {
        // Exact matches
        (a, b) if a == b => Compatibility::Exact,

        // Audio can go anywhere (it's just numbers)
        (Audio, _) => Compatibility::Allowed,

        // CV is generally interchangeable
        (CvBipolar, CvUnipolar) | (CvUnipolar, CvBipolar) => Compatibility::Allowed,

        // V/Oct to CV is fine
        (VoltPerOctave, CvBipolar) | (VoltPerOctave, CvUnipolar) => Compatibility::Allowed,

        // Gate/Trigger are similar
        (Gate, Trigger) | (Trigger, Gate) => Compatibility::Allowed,
        (Clock, Gate) | (Clock, Trigger) => Compatibility::Allowed,

        // Mismatches that work but may be unintentional
        (Gate, Audio) | (Trigger, Audio) => Compatibility::Warning("Gate→Audio may cause clicks"),
        (CvBipolar, VoltPerOctave) => Compatibility::Warning("CV→V/Oct may cause tuning issues"),

        // Everything else is allowed (modular = no rules)
        _ => Compatibility::Allowed,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum Compatibility {
    Exact,
    Allowed,
    Warning { message: String },
}
```

### 2.3 JSON Endpoint Schema

```typescript
// GET /api/signal-colors
interface SignalColorsResponse {
  colors: Record<SignalKind, string>;
}

// GET /api/compatibility?from=audio&to=gate
interface CompatibilityResponse {
  status: "exact" | "allowed" | "warning";
  message?: string;
}

// GET /api/modules/:type_id/ports
interface ModulePorts {
  inputs: PortInfo[];
  outputs: PortInfo[];
}
```

**Deliverables:**
- [ ] `PortInfo` with serde derives
- [ ] `SignalColors` with CSS hex defaults
- [ ] `ports_compatible()` function
- [ ] `Compatibility` enum
- [ ] Endpoint schemas documented

---

## Phase 3: Module Catalog

Provide searchable module list for the "add module" UI.

### 3.1 Catalog Entry

```rust
/// Module catalog entry for browser/search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleCatalogEntry {
    /// Type identifier (for instantiation)
    pub type_id: String,
    /// Display name
    pub name: String,
    /// Category (Oscillators, Filters, etc.)
    pub category: String,
    /// Short description
    pub description: String,
    /// Search keywords
    pub keywords: Vec<String>,
    /// Port summary for quick preview
    pub ports: PortSummary,
    /// Tags for filtering
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortSummary {
    pub inputs: u8,
    pub outputs: u8,
    pub has_audio_in: bool,
    pub has_audio_out: bool,
}
```

### 3.2 Registry Extensions

```rust
impl ModuleRegistry {
    /// Get full catalog for UI
    pub fn catalog(&self) -> Vec<ModuleCatalogEntry>;

    /// Search by query (matches name, description, keywords)
    pub fn search(&self, query: &str) -> Vec<ModuleCatalogEntry>;

    /// Filter by category
    pub fn by_category(&self, category: &str) -> Vec<ModuleCatalogEntry>;

    /// Get all categories
    pub fn categories(&self) -> Vec<String>;
}
```

### 3.3 JSON Endpoint Schema

```typescript
// GET /api/catalog
interface CatalogResponse {
  modules: ModuleCatalogEntry[];
  categories: string[];
}

// GET /api/catalog/search?q=filter
interface SearchResponse {
  query: string;
  results: ModuleCatalogEntry[];
}
```

**Deliverables:**
- [ ] `ModuleCatalogEntry` struct
- [ ] `PortSummary` struct
- [ ] `ModuleRegistry::catalog()`, `search()`, `by_category()`
- [ ] Populate keywords for all built-in modules
- [ ] Unit tests for search

---

## Phase 4: Real-Time State Bridge

Stream live values from the audio thread to the UI.

### 4.1 Observable Values

```rust
/// Values that can be observed in real-time
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ObservableValue {
    /// Current parameter value
    Param { node_id: String, param_id: String, value: f64 },

    /// Output level (RMS dB)
    Level { node_id: String, port_id: u32, rms_db: f64, peak_db: f64 },

    /// Gate state
    Gate { node_id: String, port_id: u32, active: bool },

    /// Scope buffer snapshot
    Scope { node_id: String, port_id: u32, samples: Vec<f32> },

    /// Spectrum data
    Spectrum { node_id: String, port_id: u32, bins: Vec<f32>, freq_range: (f32, f32) },
}

/// Subscription request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub target: SubscriptionTarget,
    /// Update rate in Hz (capped at 60)
    pub rate_hz: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum SubscriptionTarget {
    Param { node_id: String, param_id: String },
    Level { node_id: String, port_id: u32 },
    Gate { node_id: String, port_id: u32 },
    Scope { node_id: String, port_id: u32, buffer_size: usize },
    Spectrum { node_id: String, port_id: u32, fft_size: usize },
}
```

### 4.2 WebSocket Protocol

```typescript
// Client → Server
interface SubscribeMessage {
  action: "subscribe";
  subscriptions: Subscription[];
}

interface UnsubscribeMessage {
  action: "unsubscribe";
  ids: string[];
}

// Server → Client (streaming)
interface ValueUpdate {
  subscription_id: string;
  timestamp_ms: number;
  value: ObservableValue;
}

// Batched updates (sent at subscription rate)
interface BatchUpdate {
  updates: ValueUpdate[];
}
```

### 4.3 Implementation Notes

```rust
/// Manages real-time value extraction from audio thread
pub struct StateObserver {
    /// Ring buffers for scope data (one per subscribed port)
    scope_buffers: HashMap<(NodeId, u32), RingBuffer<f32>>,

    /// Level meters
    meters: HashMap<(NodeId, u32), LevelMeter>,

    /// Active subscriptions
    subscriptions: Vec<Subscription>,
}

impl StateObserver {
    /// Called from audio thread (must be lock-free)
    pub fn process(&mut self, patch: &Patch);

    /// Called from network thread to get pending updates
    pub fn drain_updates(&mut self) -> Vec<ValueUpdate>;
}
```

**Deliverables:**
- [ ] `ObservableValue` enum
- [ ] `Subscription` and `SubscriptionTarget` types
- [ ] `StateObserver` with lock-free audio thread interface
- [ ] WebSocket message types
- [ ] Rate limiting (max 60 Hz updates)
- [ ] Example: scope visualization over WebSocket

---

## Phase 5: Serialization Contract

Document the JSON schema so frontends can reliably parse/generate patches.

### 5.1 JSON Schema Documentation

```typescript
// Patch format (already implemented, documenting here)
interface PatchDef {
  version: number;
  name: string;
  author?: string;
  description?: string;
  tags: string[];
  modules: ModuleDef[];
  cables: CableDef[];
  parameters: Record<string, number>; // "module.param" → value
}

interface ModuleDef {
  name: string;          // Instance name (unique within patch)
  module_type: string;   // Type ID from registry
  position?: [number, number]; // [x, y] for UI
  state?: object;        // Module-specific state
}

interface CableDef {
  from: string;          // "module_name.port_name"
  to: string;            // "module_name.port_name"
  attenuation?: number;  // -2.0 to 2.0
  offset?: number;       // -10.0 to 10.0
}
```

### 5.2 React Flow Mapping

```typescript
// Utility functions for React Flow integration

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
- [ ] JSON Schema file (`schemas/patch.json`)
- [ ] TypeScript type definitions (`@quiver/types` package)
- [ ] React Flow mapping utilities (example code)
- [ ] Validation function for patch JSON

---

## Implementation Order

| Phase | Name | Priority | Effort | Notes |
|-------|------|----------|--------|-------|
| 1 | Introspection API | High | Medium | Enables knob/slider generation |
| 2 | Signal Semantics | High | Low | Enables cable coloring |
| 3 | Module Catalog | Medium | Low | Enables module browser |
| 5 | Serialization Contract | High | Low | Documentation + types |
| 4 | Real-Time Bridge | Medium | High | WebSocket infrastructure |

**Recommended order:** 5 → 2 → 1 → 3 → 4

Start with serialization contract (it's just documentation), then signal semantics (quick win for cable colors), then introspection for parameter UIs.

---

## File Structure

```
src/
├── introspection.rs     # ParamInfo, ModuleIntrospection trait
├── serialize.rs         # (existing) + JSON schema docs
└── lib.rs               # Re-exports

schemas/
└── patch.schema.json    # JSON Schema for validation

examples/
└── react-flow-bridge/   # TypeScript example project
    ├── src/
    │   ├── types.ts     # Generated from Rust types
    │   ├── mapping.ts   # patchToReactFlow, reactFlowToPatch
    │   └── api.ts       # Fetch wrappers
    └── package.json
```

---

## What We're NOT Building

The following are explicitly **out of scope** because React Flow handles them:

| Feature | Use Instead |
|---------|-------------|
| Node dragging | React Flow `onNodeDrag` |
| Cable bezier rendering | React Flow edges |
| Hit testing | React Flow built-in |
| Pan/zoom | React Flow viewport |
| Undo/redo | React state + `use-undoable` or similar |
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

---

## References

- [React Flow](https://reactflow.dev/) - React library for node-based UIs
- [xyflow](https://github.com/xyflow/xyflow) - Framework-agnostic core
- [Quiver Serialization](../src/serialize.rs) - Existing implementation
