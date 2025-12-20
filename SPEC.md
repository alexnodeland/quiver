# Quiver: Modular Audio Synthesis Library

## Product Requirements Document

**Version:** 1.0.0  
**Last Updated:** December 2024

-----

## Table of Contents

1. [Executive Summary](#1-executive-summary)
1. [Goals and Non-Goals](#2-goals-and-non-goals)
1. [Architecture Overview](#3-architecture-overview)
1. [Layer 1: Typed Module Combinators](#4-layer-1-typed-module-combinators)
1. [Layer 2: Signal Conventions and Port System](#5-layer-2-signal-conventions-and-port-system)
1. [Layer 3: Patch Graph](#6-layer-3-patch-graph)
1. [Core DSP Modules](#7-core-dsp-modules)
1. [Analog Modeling Primitives](#8-analog-modeling-primitives)
1. [External I/O Integration](#9-external-io-integration)
1. [Building a Complete Synthesizer](#10-building-a-complete-synthesizer)
1. [Serialization and Persistence](#11-serialization-and-persistence)
1. [Performance Considerations](#12-performance-considerations)
1. [Testing Strategy](#13-testing-strategy)
1. [Future Extensions](#14-future-extensions)
1. [Development Roadmap](#15-development-roadmap)
1. [Appendices](#16-appendices)

-----

## 1. Executive Summary

`quiver` is a Rust library for building modular audio synthesis systems using a hybrid architecture that combines type-safe Arrow-style combinators for DSP construction with a flexible graph-based patching system for arbitrary signal routing.

### 1.1 The Name

The name “quiver” captures multiple dimensions of the library’s identity:

- **Mathematical**: In category theory, a *quiver* is a directed graph—a collection of nodes connected by arrows. This precisely describes the architecture: modules are nodes, patch cables are arrows, and compositions form paths through the graph.
- **Functional**: The library embraces Arrow-style abstractions for signal processing. A quiver holds arrows; so does this library.
- **Musical**: A quiver of sound—a trembling, resonant quality. The slight vibrato of an analog oscillator, the shimmer of a modulated filter.
- **Visceral**: The responsive, alive feeling of a well-designed instrument that reacts instantly to the performer’s intent.

### 1.2 Design Philosophy

1. **Combinators for construction, graphs for topology** — Use typed functional combinators to build optimized DSP chains; use a runtime graph representation for flexible, arbitrary signal routing.
1. **Hardware-inspired semantics** — Model voltage-like signal conventions (1V/octave pitch, gates, triggers), normalled connections, input summing, and the knob+CV paradigm drawn from hardware modular synthesis.
1. **Analog character through modeling** — Provide primitives for analog nonlinearity, thermal drift, and component variation to achieve warmth and organic behavior.
1. **Pure processing core, flexible I/O** — The synthesis engine is deterministic and testable; audio and MIDI I/O are integration points that connect to external crates.
1. **Zero-cost abstractions where possible** — The combinator layer should compile down to tight, inlinable DSP loops; the graph layer trades some dynamism for flexibility.

-----

## 2. Goals and Non-Goals

### 2.1 Goals

- Provide a comprehensive, composable library for building virtual modular synthesizers
- Enable both programmatic patch construction and runtime-configurable topologies
- Support real-time audio processing with predictable performance
- Model hardware-inspired signal conventions accurately (voltage ranges, gates, triggers, 1V/octave)
- Offer analog-modeling primitives for realistic sonic character
- Enable serialization/deserialization of patches for save/load functionality
- Support integration with standard audio (cpal, jack) and MIDI (midir) crates
- Provide clear patterns for building complete, playable instruments
- Support eventual GUI integration for visual patching

### 2.2 Non-Goals

- Not a complete DAW or standalone application (library only)
- Not targeting embedded/bare-metal environments (assumes std)
- Not attempting cycle-accurate emulation of specific hardware
- Not providing MIDI/OSC/audio I/O directly (integration points only)
- Not a plugin format (though wrappers can be built on top)

-----

## 3. Architecture Overview

### 3.1 System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     User Application                             │
│              (Standalone synth, plugin, etc.)                    │
├─────────────────────────────────────────────────────────────────┤
│                 External I/O Integration                         │
│         (MIDI input, Audio output, OSC, etc.)                   │
├──────────────────────────┬──────────────────────────────────────┤
│   Layer 3: Patch Graph   │   Serialization / UI Binding         │
│   (Topology & Routing)   │   (serde, reflection)                │
├──────────────────────────┼──────────────────────────────────────┤
│   Layer 2: Port Bridge   │   Signal Conventions                 │
│   (Type Erasure)         │   (V/Oct, Gates, Normalling)         │
├──────────────────────────┴──────────────────────────────────────┤
│                 Layer 1: Typed Combinators                       │
│              (Arrow-style DSP composition)                       │
├─────────────────────────────────────────────────────────────────┤
│                    Analog Modeling Primitives                    │
│           (Saturation, drift, noise, nonlinearity)               │
├─────────────────────────────────────────────────────────────────┤
│                      Core DSP Primitives                         │
│            (Oscillators, filters, envelopes, etc.)               │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 Data Flow in a Running Synthesizer

```
┌──────────────┐     ┌─────────────────────────────────────────────┐
│ MIDI Device  │────▶│              MIDI Thread                    │
└──────────────┘     │  Parse messages, update AtomicF64 values    │
                     └─────────────────┬───────────────────────────┘
                                       │ (lock-free atomics)
                                       ▼
┌──────────────┐     ┌─────────────────────────────────────────────┐
│  Sound Card  │◀────│             Audio Thread                    │
└──────────────┘     │  patch.tick() → (L, R) samples              │
                     └─────────────────┬───────────────────────────┘
                                       │
                                       ▼
                     ┌─────────────────────────────────────────────┐
                     │            Quiver Patch Graph               │
                     │                                             │
                     │  ExternalInput ──▶ VCO ──▶ VCF ──▶ VCA     │
                     │       │            ▲       ▲       ▲        │
                     │       │            │       │       │        │
                     │       └──▶ Env ────┴───────┴───────┘        │
                     │                                             │
                     └─────────────────────────────────────────────┘
```

### 3.3 The Hybrid Approach

The architecture separates two concerns:

**Typed Combinators (Layer 1)** — For building optimized, type-safe DSP chains:

```rust
// Compile-time checked, inlinable, zero-cost
let filter_chain = input
    .then(Saturator::new(0.3))
    .then(Svf::lowpass(44100.0))
    .then(Vca::new());
```

**Patch Graph (Layer 3)** — For arbitrary runtime topology:

```rust
// Flexible routing, serializable, UI-friendly
patch.connect(lfo.out("tri"), filter.in_("cutoff"))?;
patch.connect(lfo.out("tri"), vca.in_("cv"))?;  // Same source, two destinations
```

Use combinators inside modules for tight DSP; use the graph for patch-level routing.

-----

## 4. Layer 1: Typed Module Combinators

### 4.1 Core Trait

The fundamental abstraction is a stateful signal processor with typed inputs and outputs:

```rust
/// A signal processing module with typed input and output
pub trait Module: Send {
    /// Input signal type
    type In;
    /// Output signal type
    type Out;

    /// Process a single sample
    fn tick(&mut self, input: Self::In) -> Self::Out;

    /// Process a block of samples (override for optimization)
    fn process(&mut self, input: &[Self::In], output: &mut [Self::Out]) {
        for (i, o) in input.iter().zip(output.iter_mut()) {
            *o = self.tick(i.clone());
        }
    }

    /// Reset internal state to initial conditions
    fn reset(&mut self);

    /// Notify module of sample rate changes
    fn set_sample_rate(&mut self, _sample_rate: f64) {}
}
```

### 4.2 Arrow-Style Combinators

These combinators enable functional composition of modules:

#### Sequential Composition (`>>>` / `then`)

Connects two modules in series:

```rust
pub struct Chain<A, B> {
    first: A,
    second: B,
}

impl<A, B> Module for Chain<A, B>
where
    A: Module,
    B: Module<In = A::Out>,
{
    type In = A::In;
    type Out = B::Out;

    #[inline]
    fn tick(&mut self, input: Self::In) -> Self::Out {
        self.second.tick(self.first.tick(input))
    }

    fn reset(&mut self) {
        self.first.reset();
        self.second.reset();
    }
}
```

#### Parallel Composition (`***` / `parallel`)

Processes two independent signals simultaneously:

```rust
pub struct Parallel<A, B> {
    left: A,
    right: B,
}

impl<A, B> Module for Parallel<A, B>
where
    A: Module,
    B: Module,
{
    type In = (A::In, B::In);
    type Out = (A::Out, B::Out);

    #[inline]
    fn tick(&mut self, (a, b): Self::In) -> Self::Out {
        (self.left.tick(a), self.right.tick(b))
    }

    fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }
}
```

#### Fanout (`&&&` / `fanout`)

Splits a single input to two parallel processors:

```rust
pub struct Fanout<A, B> {
    left: A,
    right: B,
}

impl<A, B> Module for Fanout<A, B>
where
    A: Module,
    B: Module<In = A::In>,
    A::In: Clone,
{
    type In = A::In;
    type Out = (A::Out, B::Out);

    #[inline]
    fn tick(&mut self, input: Self::In) -> Self::Out {
        (self.left.tick(input.clone()), self.right.tick(input))
    }

    fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }
}
```

#### Feedback with Unit Delay

Enables feedback loops with mandatory single-sample delay for causality:

```rust
pub struct Feedback<M, F> {
    module: M,
    combine: F,
    delay_buffer: M::Out,
}

impl<M, F, Combined> Module for Feedback<M, F>
where
    M: Module<In = Combined>,
    F: Fn(M::Out, M::Out) -> Combined,
    M::Out: Default + Clone,
{
    type In = M::Out;
    type Out = M::Out;

    fn tick(&mut self, input: Self::In) -> Self::Out {
        let combined = (self.combine)(input, self.delay_buffer.clone());
        let output = self.module.tick(combined);
        self.delay_buffer = output.clone();
        output
    }

    fn reset(&mut self) {
        self.module.reset();
        self.delay_buffer = M::Out::default();
    }
}
```

### 4.3 Complete Combinator Reference

|Combinator       |Type Signature                     |Description                        |
|-----------------|-----------------------------------|-----------------------------------|
|`Chain<A, B>`    |`A::In → B::Out`                   |Sequential composition             |
|`Parallel<A, B>` |`(A::In, B::In) → (A::Out, B::Out)`|Independent parallel processing    |
|`Fanout<A, B>`   |`T → (A::Out, B::Out)`             |Split input to two paths           |
|`Feedback<M, F>` |`T → T`                            |Feedback loop with unit delay      |
|`Map<M, F>`      |`M::In → U`                        |Transform output with pure function|
|`Contramap<M, F>`|`U → M::Out`                       |Transform input with pure function |
|`Split<T>`       |`T → (T, T)`                       |Duplicate signal                   |
|`Merge<T, F>`    |`(T, T) → T`                       |Combine two signals                |
|`Swap<A, B>`     |`(A, B) → (B, A)`                  |Reorder tuple elements             |
|`First<M, C>`    |`(M::In, C) → (M::Out, C)`         |Process first, pass through second |
|`Second<M, C>`   |`(C, M::In) → (C, M::Out)`         |Pass through first, process second |
|`Identity<T>`    |`T → T`                            |Pass-through (categorical identity)|
|`Constant<T>`    |`() → T`                           |Emit constant value                |

### 4.4 Extension Trait

```rust
pub trait ModuleExt: Module + Sized {
    fn then<M: Module<In = Self::Out>>(self, next: M) -> Chain<Self, M> {
        Chain { first: self, second: next }
    }

    fn parallel<M: Module>(self, other: M) -> Parallel<Self, M> {
        Parallel { left: self, right: other }
    }

    fn fanout<M: Module<In = Self::In>>(self, other: M) -> Fanout<Self, M>
    where
        Self::In: Clone,
    {
        Fanout { left: self, right: other }
    }

    fn map<F, U>(self, f: F) -> Map<Self, F>
    where
        F: Fn(Self::Out) -> U,
    {
        Map { module: self, f }
    }

    fn feedback<F>(self, combine: F) -> Feedback<Self, F>
    where
        Self::Out: Default + Clone,
    {
        Feedback {
            module: self,
            combine,
            delay_buffer: Self::Out::default(),
        }
    }
}

impl<M: Module> ModuleExt for M {}
```

### 4.5 Usage Example

```rust
// Build a simple subtractive voice using combinators
fn subtractive_voice(sample_rate: f64) -> impl Module<In = VoiceInput, Out = f64> {
    // Oscillator -> Saturation -> Filter -> VCA
    Oscillator::new(sample_rate)
        .then(Saturator::soft(0.3))
        .then(StateVariableFilter::lowpass(sample_rate))
        .then(Vca::new())
}

#[derive(Clone)]
struct VoiceInput {
    pitch: f64,      // V/Oct
    gate: f64,       // Gate signal
    cutoff: f64,     // Filter cutoff
    amplitude: f64,  // VCA level
}
```

-----

## 5. Layer 2: Signal Conventions and Port System

### 5.1 Signal Types

Hardware modular synthesizers use semantic conventions for voltage ranges and meanings. `quiver` models these to enable realistic behavior and meaningful validation:

```rust
/// Semantic signal classification following hardware modular conventions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalKind {
    /// Audio signal, AC-coupled, typically ±5V peak
    Audio,

    /// Bipolar control voltage, ±5V (LFO, pitch bend, modulation)
    CvBipolar,

    /// Unipolar control voltage, 0–10V (envelope, velocity, expression)
    CvUnipolar,

    /// Pitch CV following 1V/octave standard
    /// Reference: 0V = C4 (middle C, 261.63 Hz)
    VoltPerOctave,

    /// Gate signal, binary state: 0V (low) or +5V (high)
    /// Remains high while note/event is active
    Gate,

    /// Trigger signal, short pulse (~1–10ms) at +5V
    /// Used for instantaneous events
    Trigger,

    /// Clock signal, regular trigger pulses at tempo
    Clock,
}

impl SignalKind {
    /// Returns the typical voltage range (min, max) for this signal type
    pub fn voltage_range(&self) -> (f64, f64) {
        match self {
            SignalKind::Audio => (-5.0, 5.0),
            SignalKind::CvBipolar => (-5.0, 5.0),
            SignalKind::CvUnipolar => (0.0, 10.0),
            SignalKind::VoltPerOctave => (-5.0, 5.0),  // ~C-1 to C9
            SignalKind::Gate => (0.0, 5.0),
            SignalKind::Trigger => (0.0, 5.0),
            SignalKind::Clock => (0.0, 5.0),
        }
    }

    /// Whether multiple signals of this kind should be summed when connected
    pub fn is_summable(&self) -> bool {
        matches!(
            self,
            SignalKind::Audio
                | SignalKind::CvBipolar
                | SignalKind::CvUnipolar
                | SignalKind::VoltPerOctave
        )
    }

    /// Threshold voltage for high/low detection
    pub fn gate_threshold(&self) -> Option<f64> {
        match self {
            SignalKind::Gate | SignalKind::Trigger | SignalKind::Clock => Some(2.5),
            _ => None,
        }
    }
}
```

### 5.2 Port Definitions

```rust
pub type PortId = u32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDef {
    /// Unique identifier within the module
    pub id: PortId,

    /// Human-readable name (e.g., "cutoff", "voct", "out")
    pub name: &'static str,

    /// Signal type for validation and UI hints
    pub kind: SignalKind,

    /// Default value when no cable connected
    pub default: f64,

    /// For inputs: internal source when unpatched (normalled connection)
    pub normalled_to: Option<PortId>,

    /// Whether this input has an associated attenuverter control
    pub has_attenuverter: bool,
}

impl PortDef {
    pub fn new(id: PortId, name: &'static str, kind: SignalKind) -> Self {
        Self {
            id,
            name,
            kind,
            default: 0.0,
            normalled_to: None,
            has_attenuverter: false,
        }
    }

    pub fn with_default(mut self, default: f64) -> Self {
        self.default = default;
        self
    }

    pub fn with_attenuverter(mut self) -> Self {
        self.has_attenuverter = true;
        self
    }

    pub fn normalled_to(mut self, port: PortId) -> Self {
        self.normalled_to = Some(port);
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortSpec {
    pub inputs: Vec<PortDef>,
    pub outputs: Vec<PortDef>,
}

impl PortSpec {
    pub fn input_by_name(&self, name: &str) -> Option<&PortDef> {
        self.inputs.iter().find(|p| p.name == name)
    }

    pub fn output_by_name(&self, name: &str) -> Option<&PortDef> {
        self.outputs.iter().find(|p| p.name == name)
    }
}
```

### 5.3 Modulated Parameters (Knob + CV)

Hardware modular modules typically combine panel controls with CV inputs. This paradigm requires explicit modeling:

```rust
/// A parameter that combines a base value (knob) with CV modulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModulatedParam {
    /// Base value from panel knob (typically 0.0–1.0 normalized)
    pub base: f64,

    /// Incoming CV voltage (set during tick)
    pub cv: f64,

    /// Attenuverter setting (-1.0 to 1.0)
    /// Positive: CV adds to base
    /// Negative: CV subtracts from base (inverted)
    pub attenuverter: f64,

    /// Output range mapping
    pub range: ParamRange,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ParamRange {
    /// Linear mapping from normalized (0–1) to (min, max)
    Linear { min: f64, max: f64 },

    /// Exponential mapping, useful for frequency/time controls
    Exponential { min: f64, max: f64 },

    /// V/Oct: input is in volts, output is frequency multiplier
    VoltPerOctave { base_freq: f64 },
}

impl ModulatedParam {
    pub fn new(range: ParamRange) -> Self {
        Self {
            base: 0.5,
            cv: 0.0,
            attenuverter: 1.0,
            range,
        }
    }

    /// Compute the effective parameter value
    pub fn value(&self) -> f64 {
        let modulated = self.base + (self.cv * self.attenuverter);
        self.range.apply(modulated)
    }

    /// Update CV from port value
    pub fn set_cv(&mut self, cv: f64) {
        self.cv = cv;
    }
}

impl ParamRange {
    pub fn apply(&self, normalized: f64) -> f64 {
        match self {
            ParamRange::Linear { min, max } => {
                min + normalized.clamp(0.0, 1.0) * (max - min)
            }
            ParamRange::Exponential { min, max } => {
                let clamped = normalized.clamp(0.0, 1.0);
                min * (max / min).powf(clamped)
            }
            ParamRange::VoltPerOctave { base_freq } => {
                base_freq * 2.0_f64.powf(normalized)
            }
        }
    }
}
```

### 5.4 Type-Erased Graph Module Interface

To place typed modules into the runtime graph, we need a common interface:

```rust
/// Type-erased module interface for graph-based patching
pub trait GraphModule: Send + Sync {
    /// Returns the module's port specification
    fn port_spec(&self) -> &PortSpec;

    /// Process one sample given port values
    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues);

    /// Process a block of samples (optional optimization)
    fn process_block(
        &mut self,
        inputs: &BlockPortValues,
        outputs: &mut BlockPortValues,
        frames: usize,
    ) {
        for i in 0..frames {
            let in_frame = inputs.frame(i);
            let mut out_frame = PortValues::new();
            self.tick(&in_frame, &mut out_frame);
            outputs.set_frame(i, out_frame);
        }
    }

    /// Reset internal state
    fn reset(&mut self);

    /// Set sample rate
    fn set_sample_rate(&mut self, sample_rate: f64);

    /// Get parameter definitions for UI binding
    fn params(&self) -> &[ParamDef] {
        &[]
    }

    /// Get a parameter value
    fn get_param(&self, _id: ParamId) -> Option<f64> {
        None
    }

    /// Set a parameter value
    fn set_param(&mut self, _id: ParamId, _value: f64) {}

    /// Serialize module state
    fn serialize_state(&self) -> Option<serde_json::Value> {
        None
    }

    /// Deserialize module state
    fn deserialize_state(&mut self, _state: &serde_json::Value) -> Result<(), String> {
        Ok(())
    }
}

/// Runtime port values container
#[derive(Debug, Clone, Default)]
pub struct PortValues {
    values: HashMap<PortId, f64>,
}

impl PortValues {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, id: PortId) -> Option<f64> {
        self.values.get(&id).copied()
    }

    pub fn get_or(&self, id: PortId, default: f64) -> f64 {
        self.values.get(&id).copied().unwrap_or(default)
    }

    pub fn set(&mut self, id: PortId, value: f64) {
        self.values.insert(id, value);
    }

    /// Accumulate (sum) a value into a port (for input mixing)
    pub fn accumulate(&mut self, id: PortId, value: f64) {
        *self.values.entry(id).or_insert(0.0) += value;
    }

    pub fn has(&self, id: PortId) -> bool {
        self.values.contains_key(&id)
    }
}
```

-----

## 6. Layer 3: Patch Graph

### 6.1 Core Graph Types

```rust
use slotmap::{SlotMap, DefaultKey};

pub type NodeId = DefaultKey;
pub type CableId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortRef {
    pub node: NodeId,
    pub port: PortId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cable {
    pub from: PortRef,
    pub to: PortRef,
    /// Optional attenuation (0.0–1.0)
    pub attenuation: Option<f64>,
}

struct Node {
    module: Box<dyn GraphModule>,
    name: String,
    position: Option<(f32, f32)>,  // For UI layout
}

pub struct Patch {
    nodes: SlotMap<NodeId, Node>,
    cables: Vec<Cable>,

    // Execution state
    execution_order: Vec<NodeId>,
    buffers: HashMap<PortRef, f64>,

    // Configuration
    sample_rate: f64,

    // Output node
    output_node: Option<NodeId>,
}
```

### 6.2 Patch Construction API

```rust
impl Patch {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            nodes: SlotMap::new(),
            cables: Vec::new(),
            execution_order: Vec::new(),
            buffers: HashMap::new(),
            sample_rate,
            output_node: None,
        }
    }

    /// Add a module to the patch
    pub fn add<M: GraphModule + 'static>(
        &mut self,
        name: impl Into<String>,
        mut module: M,
    ) -> NodeHandle {
        module.set_sample_rate(self.sample_rate);
        let spec = module.port_spec().clone();
        let id = self.nodes.insert(Node {
            module: Box::new(module),
            name: name.into(),
            position: None,
        });
        self.invalidate();
        NodeHandle { id, spec }
    }

    /// Connect an output port to an input port
    pub fn connect(&mut self, from: PortRef, to: PortRef) -> Result<CableId, PatchError> {
        self.validate_output_port(from)?;
        self.validate_input_port(to)?;

        let cable = Cable {
            from,
            to,
            attenuation: None,
        };
        self.cables.push(cable);
        self.invalidate();
        Ok(self.cables.len() - 1)
    }

    /// Connect one output to multiple inputs (mult)
    pub fn mult(&mut self, from: PortRef, to: &[PortRef]) -> Result<Vec<CableId>, PatchError> {
        to.iter().map(|&dest| self.connect(from, dest)).collect()
    }

    /// Disconnect a cable by ID
    pub fn disconnect(&mut self, cable_id: CableId) -> Result<(), PatchError> {
        if cable_id >= self.cables.len() {
            return Err(PatchError::InvalidCable);
        }
        self.cables.remove(cable_id);
        self.invalidate();
        Ok(())
    }

    /// Set the output node for the patch
    pub fn set_output(&mut self, node: NodeId) {
        self.output_node = Some(node);
    }

    /// Set a parameter on a module
    pub fn set_param(&mut self, node: NodeId, param: ParamId, value: f64) {
        if let Some(n) = self.nodes.get_mut(node) {
            n.module.set_param(param, value);
        }
    }

    fn invalidate(&mut self) {
        self.execution_order.clear();
    }

    fn validate_output_port(&self, port_ref: PortRef) -> Result<(), PatchError> {
        let node = self.nodes.get(port_ref.node).ok_or(PatchError::InvalidNode)?;
        node.module
            .port_spec()
            .outputs
            .iter()
            .find(|p| p.id == port_ref.port)
            .ok_or(PatchError::InvalidPort)?;
        Ok(())
    }

    fn validate_input_port(&self, port_ref: PortRef) -> Result<(), PatchError> {
        let node = self.nodes.get(port_ref.node).ok_or(PatchError::InvalidNode)?;
        node.module
            .port_spec()
            .inputs
            .iter()
            .find(|p| p.id == port_ref.port)
            .ok_or(PatchError::InvalidPort)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum PatchError {
    InvalidNode,
    InvalidPort,
    InvalidCable,
    CycleDetected { nodes: Vec<NodeId> },
    CompilationFailed(String),
}
```

### 6.3 Node Handle for Ergonomic Port References

```rust
#[derive(Clone)]
pub struct NodeHandle {
    id: NodeId,
    spec: PortSpec,
}

impl NodeHandle {
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Reference an output port by name
    pub fn out(&self, name: &str) -> PortRef {
        let port = self
            .spec
            .output_by_name(name)
            .unwrap_or_else(|| panic!("Unknown output port: {}", name));
        PortRef {
            node: self.id,
            port: port.id,
        }
    }

    /// Reference an input port by name
    pub fn in_(&self, name: &str) -> PortRef {
        let port = self
            .spec
            .input_by_name(name)
            .unwrap_or_else(|| panic!("Unknown input port: {}", name));
        PortRef {
            node: self.id,
            port: port.id,
        }
    }
}
```

### 6.4 Graph Compilation

```rust
impl Patch {
    /// Compile the patch into an executable order
    pub fn compile(&mut self) -> Result<(), PatchError> {
        let order = self.topological_sort()?;
        self.execution_order = order;

        // Pre-allocate output buffers
        self.buffers.clear();
        for (id, node) in &self.nodes {
            for output in &node.module.port_spec().outputs {
                self.buffers.insert(PortRef { node: id, port: output.id }, 0.0);
            }
        }

        Ok(())
    }

    fn topological_sort(&self) -> Result<Vec<NodeId>, PatchError> {
        let mut in_degree: HashMap<NodeId, usize> =
            self.nodes.keys().map(|k| (k, 0)).collect();
        let mut successors: HashMap<NodeId, Vec<NodeId>> =
            self.nodes.keys().map(|k| (k, vec![])).collect();

        for cable in &self.cables {
            *in_degree.entry(cable.to.node).or_insert(0) += 1;
            successors
                .entry(cable.from.node)
                .or_default()
                .push(cable.to.node);
        }

        // Kahn's algorithm
        let mut queue: VecDeque<NodeId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut result = Vec::with_capacity(self.nodes.len());

        while let Some(node) = queue.pop_front() {
            result.push(node);
            for &succ in successors.get(&node).unwrap_or(&vec![]) {
                let deg = in_degree.get_mut(&succ).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(succ);
                }
            }
        }

        if result.len() != self.nodes.len() {
            let in_cycle: Vec<NodeId> = in_degree
                .into_iter()
                .filter(|(_, deg)| *deg > 0)
                .map(|(id, _)| id)
                .collect();
            return Err(PatchError::CycleDetected { nodes: in_cycle });
        }

        Ok(result)
    }
}
```

### 6.5 Graph Execution

```rust
impl Patch {
    /// Process a single sample, returning stereo output
    pub fn tick(&mut self) -> (f64, f64) {
        for &node_id in &self.execution_order {
            let inputs = self.gather_inputs(node_id);
            let mut outputs = PortValues::new();

            // Process the module
            self.nodes[node_id].module.tick(&inputs, &mut outputs);

            // Store outputs in buffers
            self.scatter_outputs(node_id, &outputs);
        }

        self.read_output()
    }

    fn gather_inputs(&self, node_id: NodeId) -> PortValues {
        let node = &self.nodes[node_id];
        let spec = node.module.port_spec();
        let mut values = PortValues::new();

        for input in &spec.inputs {
            let port_ref = PortRef {
                node: node_id,
                port: input.id,
            };

            // Sum all incoming cables (hardware-style input mixing)
            let mut sum = 0.0;
            let mut has_connection = false;

            for cable in &self.cables {
                if cable.to == port_ref {
                    has_connection = true;
                    let value = self.buffers.get(&cable.from).copied().unwrap_or(0.0);
                    let attenuated = cable.attenuation.map(|a| value * a).unwrap_or(value);
                    sum += attenuated;
                }
            }

            if has_connection {
                values.set(input.id, sum);
            } else if let Some(normalled) = input.normalled_to {
                // Use normalled (internal) connection
                let normalled_ref = PortRef {
                    node: node_id,
                    port: normalled,
                };
                if let Some(&v) = self.buffers.get(&normalled_ref) {
                    values.set(input.id, v);
                } else {
                    values.set(input.id, input.default);
                }
            } else {
                // Use default value
                values.set(input.id, input.default);
            }
        }

        values
    }

    fn scatter_outputs(&mut self, node_id: NodeId, outputs: &PortValues) {
        for (&port_id, &value) in &outputs.values {
            let port_ref = PortRef {
                node: node_id,
                port: port_id,
            };
            self.buffers.insert(port_ref, value);
        }
    }

    fn read_output(&self) -> (f64, f64) {
        if let Some(output_node) = self.output_node {
            let left = self
                .buffers
                .get(&PortRef {
                    node: output_node,
                    port: 0,  // Assuming port 0 is left
                })
                .copied()
                .unwrap_or(0.0);
            let right = self
                .buffers
                .get(&PortRef {
                    node: output_node,
                    port: 1,  // Assuming port 1 is right
                })
                .copied()
                .unwrap_or(left);  // Mono fallback
            (left, right)
        } else {
            (0.0, 0.0)
        }
    }
}
```

-----

## 7. Core DSP Modules

### 7.1 Oscillators

#### Voltage-Controlled Oscillator (VCO)

```rust
pub struct Vco {
    phase: f64,
    sample_rate: f64,
    last_sync: f64,
    spec: PortSpec,
}

impl Vco {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
            last_sync: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "voct", SignalKind::VoltPerOctave),
                    PortDef::new(1, "fm", SignalKind::CvBipolar).with_attenuverter(),
                    PortDef::new(2, "pw", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(3, "sync", SignalKind::Gate),
                ],
                outputs: vec![
                    PortDef::new(10, "sin", SignalKind::Audio),
                    PortDef::new(11, "tri", SignalKind::Audio),
                    PortDef::new(12, "saw", SignalKind::Audio),
                    PortDef::new(13, "sqr", SignalKind::Audio),
                ],
            },
        }
    }
}

impl GraphModule for Vco {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let voct = inputs.get_or(0, 0.0);
        let fm = inputs.get_or(1, 0.0);
        let pw = inputs.get_or(2, 0.5).clamp(0.05, 0.95);
        let sync = inputs.get_or(3, 0.0);

        // V/Oct to frequency: 0V = C4 (261.63 Hz)
        let base_freq = 261.63 * 2.0_f64.powf(voct);
        let freq = base_freq * 2.0_f64.powf(fm);

        // Hard sync on rising edge
        if sync > 2.5 && self.last_sync <= 2.5 {
            self.phase = 0.0;
        }
        self.last_sync = sync;

        // Generate waveforms (±5V range)
        let sin = (self.phase * TAU).sin() * 5.0;
        let tri = (1.0 - 4.0 * (self.phase - 0.5).abs()) * 5.0;
        let saw = (2.0 * self.phase - 1.0) * 5.0;
        let sqr = if self.phase < pw { 5.0 } else { -5.0 };

        outputs.set(10, sin);
        outputs.set(11, tri);
        outputs.set(12, saw);
        outputs.set(13, sqr);

        // Advance phase
        self.phase = (self.phase + freq / self.sample_rate).fract();
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.last_sync = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }
}
```

#### Low-Frequency Oscillator (LFO)

```rust
pub struct Lfo {
    phase: f64,
    sample_rate: f64,
    last_reset: f64,
    spec: PortSpec,
}

impl Lfo {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
            last_reset: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "rate", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(1, "depth", SignalKind::CvUnipolar).with_default(10.0),
                    PortDef::new(2, "reset", SignalKind::Trigger),
                ],
                outputs: vec![
                    PortDef::new(10, "sin", SignalKind::CvBipolar),
                    PortDef::new(11, "tri", SignalKind::CvBipolar),
                    PortDef::new(12, "saw", SignalKind::CvBipolar),
                    PortDef::new(13, "sqr", SignalKind::CvBipolar),
                    PortDef::new(14, "sin_uni", SignalKind::CvUnipolar),
                ],
            },
        }
    }
}

impl GraphModule for Lfo {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let rate_cv = inputs.get_or(0, 0.5);
        let depth = inputs.get_or(1, 10.0) / 10.0;  // Normalize to 0-1
        let reset = inputs.get_or(2, 0.0);

        // Map rate CV (0-1) to frequency (0.01 Hz - 30 Hz, exponential)
        let freq = 0.01 * (3000.0_f64).powf(rate_cv.clamp(0.0, 1.0));

        // Reset on trigger
        if reset > 2.5 && self.last_reset <= 2.5 {
            self.phase = 0.0;
        }
        self.last_reset = reset;

        // Generate waveforms scaled by depth (±5V * depth)
        let scale = 5.0 * depth;
        let sin = (self.phase * TAU).sin() * scale;
        let tri = (1.0 - 4.0 * (self.phase - 0.5).abs()) * scale;
        let saw = (2.0 * self.phase - 1.0) * scale;
        let sqr = if self.phase < 0.5 { scale } else { -scale };
        let sin_uni = ((self.phase * TAU).sin() * 0.5 + 0.5) * depth * 10.0;

        outputs.set(10, sin);
        outputs.set(11, tri);
        outputs.set(12, saw);
        outputs.set(13, sqr);
        outputs.set(14, sin_uni);

        self.phase = (self.phase + freq / self.sample_rate).fract();
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.last_reset = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }
}
```

### 7.2 Filters

#### State Variable Filter (SVF)

```rust
pub struct Svf {
    low: f64,
    band: f64,
    sample_rate: f64,
    spec: PortSpec,
}

impl Svf {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            low: 0.0,
            band: 0.0,
            sample_rate,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "cutoff", SignalKind::CvUnipolar)
                        .with_default(0.5)
                        .with_attenuverter(),
                    PortDef::new(2, "res", SignalKind::CvUnipolar)
                        .with_default(0.0)
                        .with_attenuverter(),
                    PortDef::new(3, "fm", SignalKind::CvBipolar).with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "lp", SignalKind::Audio),
                    PortDef::new(11, "bp", SignalKind::Audio),
                    PortDef::new(12, "hp", SignalKind::Audio),
                    PortDef::new(13, "notch", SignalKind::Audio),
                ],
            },
        }
    }
}

impl GraphModule for Svf {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let cutoff_cv = inputs.get_or(1, 0.5) + inputs.get_or(3, 0.0);
        let res = inputs.get_or(2, 0.0).clamp(0.0, 1.0);

        // Map cutoff CV (0-1) to frequency (20 Hz - 20 kHz, exponential)
        let cutoff_hz = 20.0 * (1000.0_f64).powf(cutoff_cv.clamp(0.0, 1.0));
        let f = 2.0 * (PI * cutoff_hz / self.sample_rate).sin();
        let q = 1.0 - res * 0.9;  // Resonance: higher res = lower damping

        // SVF topology
        let high = input - self.low - q * self.band;
        self.band += f * high;
        self.low += f * self.band;
        let notch = high + self.low;

        outputs.set(10, self.low);
        outputs.set(11, self.band);
        outputs.set(12, high);
        outputs.set(13, notch);
    }

    fn reset(&mut self) {
        self.low = 0.0;
        self.band = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }
}
```

### 7.3 Envelope Generators

#### ADSR Envelope

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
enum AdsrStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

pub struct Adsr {
    stage: AdsrStage,
    level: f64,
    sample_rate: f64,
    last_gate: f64,
    spec: PortSpec,
}

impl Adsr {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            stage: AdsrStage::Idle,
            level: 0.0,
            sample_rate,
            last_gate: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "gate", SignalKind::Gate),
                    PortDef::new(1, "retrig", SignalKind::Trigger),
                    PortDef::new(2, "attack", SignalKind::CvUnipolar)
                        .with_default(0.1)
                        .with_attenuverter(),
                    PortDef::new(3, "decay", SignalKind::CvUnipolar)
                        .with_default(0.3)
                        .with_attenuverter(),
                    PortDef::new(4, "sustain", SignalKind::CvUnipolar)
                        .with_default(0.7)
                        .with_attenuverter(),
                    PortDef::new(5, "release", SignalKind::CvUnipolar)
                        .with_default(0.4)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "env", SignalKind::CvUnipolar),
                    PortDef::new(11, "inv", SignalKind::CvUnipolar),
                    PortDef::new(12, "eoc", SignalKind::Trigger),
                ],
            },
        }
    }

    fn cv_to_time(&self, cv: f64) -> f64 {
        // Map 0-1 CV to 1ms - 10s (exponential)
        0.001 * (10000.0_f64).powf(cv.clamp(0.0, 1.0))
    }
}

impl GraphModule for Adsr {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let gate = inputs.get_or(0, 0.0);
        let retrig = inputs.get_or(1, 0.0);
        let attack_time = self.cv_to_time(inputs.get_or(2, 0.1));
        let decay_time = self.cv_to_time(inputs.get_or(3, 0.3));
        let sustain_level = inputs.get_or(4, 0.7).clamp(0.0, 1.0);
        let release_time = self.cv_to_time(inputs.get_or(5, 0.4));

        let gate_high = gate > 2.5;
        let gate_rising = gate_high && self.last_gate <= 2.5;
        let gate_falling = !gate_high && self.last_gate > 2.5;
        let retrig_rising = retrig > 2.5;

        // State transitions
        if gate_rising || (retrig_rising && gate_high) {
            self.stage = AdsrStage::Attack;
        } else if gate_falling && self.stage != AdsrStage::Idle {
            self.stage = AdsrStage::Release;
        }

        // Calculate rates
        let attack_rate = 1.0 / (attack_time * self.sample_rate);
        let decay_rate = 1.0 / (decay_time * self.sample_rate);
        let release_rate = 1.0 / (release_time * self.sample_rate);

        // Process current stage
        let mut eoc = 0.0;
        match self.stage {
            AdsrStage::Idle => {
                self.level = 0.0;
            }
            AdsrStage::Attack => {
                self.level += attack_rate;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay => {
                self.level -= decay_rate;
                if self.level <= sustain_level {
                    self.level = sustain_level;
                    self.stage = AdsrStage::Sustain;
                }
            }
            AdsrStage::Sustain => {
                self.level = sustain_level;
            }
            AdsrStage::Release => {
                self.level -= release_rate;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = AdsrStage::Idle;
                    eoc = 5.0;  // End-of-cycle trigger
                }
            }
        }

        self.last_gate = gate;

        // Output scaled to standard modular levels
        outputs.set(10, self.level * 10.0);          // 0-10V unipolar
        outputs.set(11, (1.0 - self.level) * 10.0);  // Inverted
        outputs.set(12, eoc);
    }

    fn reset(&mut self) {
        self.stage = AdsrStage::Idle;
        self.level = 0.0;
        self.last_gate = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }
}
```

### 7.4 Utility Modules

#### Voltage-Controlled Amplifier (VCA)

```rust
pub struct Vca {
    spec: PortSpec,
}

impl Vca {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "in", SignalKind::Audio),
                    PortDef::new(1, "cv", SignalKind::CvUnipolar)
                        .with_default(10.0)
                        .with_attenuverter(),
                ],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }
}

impl GraphModule for Vca {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        let cv = inputs.get_or(1, 10.0).clamp(0.0, 10.0) / 10.0;
        outputs.set(10, input * cv);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}
}
```

#### Mixer

```rust
pub struct Mixer<const N: usize> {
    spec: PortSpec,
}

impl<const N: usize> Mixer<N> {
    pub fn new() -> Self {
        let inputs = (0..N)
            .map(|i| {
                PortDef::new(i as PortId, leak_string(format!("ch{}", i)), SignalKind::Audio)
                    .with_attenuverter()
            })
            .collect();

        Self {
            spec: PortSpec {
                inputs,
                outputs: vec![PortDef::new(100, "out", SignalKind::Audio)],
            },
        }
    }
}

impl<const N: usize> GraphModule for Mixer<N> {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let sum: f64 = (0..N).map(|i| inputs.get_or(i as PortId, 0.0)).sum();
        outputs.set(100, sum);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}
}

fn leak_string(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}
```

#### Offset (DC Bias)

```rust
pub struct Offset {
    offset: f64,
    spec: PortSpec,
}

impl Offset {
    pub fn new(offset: f64) -> Self {
        Self {
            offset,
            spec: PortSpec {
                inputs: vec![PortDef::new(0, "in", SignalKind::CvBipolar)],
                outputs: vec![PortDef::new(10, "out", SignalKind::CvBipolar)],
            },
        }
    }
}

impl GraphModule for Offset {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        outputs.set(10, input + self.offset);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}
}
```

#### Unit Delay (For Feedback Loops)

```rust
pub struct UnitDelay {
    buffer: f64,
    spec: PortSpec,
}

impl UnitDelay {
    pub fn new() -> Self {
        Self {
            buffer: 0.0,
            spec: PortSpec {
                inputs: vec![PortDef::new(0, "in", SignalKind::Audio)],
                outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
            },
        }
    }
}

impl GraphModule for UnitDelay {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let input = inputs.get_or(0, 0.0);
        outputs.set(10, self.buffer);
        self.buffer = input;
    }

    fn reset(&mut self) {
        self.buffer = 0.0;
    }

    fn set_sample_rate(&mut self, _: f64) {}
}
```

#### Noise Generator

```rust
pub struct NoiseGenerator {
    pink: PinkNoise,
    spec: PortSpec,
}

impl NoiseGenerator {
    pub fn new() -> Self {
        Self {
            pink: PinkNoise::new(),
            spec: PortSpec {
                inputs: vec![],
                outputs: vec![
                    PortDef::new(10, "white", SignalKind::Audio),
                    PortDef::new(11, "pink", SignalKind::Audio),
                ],
            },
        }
    }
}

impl GraphModule for NoiseGenerator {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, _inputs: &PortValues, outputs: &mut PortValues) {
        let white = rand::random::<f64>() * 2.0 - 1.0;
        let pink = self.pink.next();

        outputs.set(10, white * 5.0);
        outputs.set(11, pink * 5.0);
    }

    fn reset(&mut self) {
        self.pink = PinkNoise::new();
    }

    fn set_sample_rate(&mut self, _: f64) {}
}
```

### 7.5 Sequencing

#### Step Sequencer

```rust
pub struct StepSequencer<const N: usize> {
    steps: [f64; N],
    gates: [bool; N],
    current: usize,
    last_clock: f64,
    last_reset: f64,
    spec: PortSpec,
}

impl<const N: usize> StepSequencer<N> {
    pub fn new() -> Self {
        Self {
            steps: [0.0; N],
            gates: [true; N],
            current: 0,
            last_clock: 0.0,
            last_reset: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "clock", SignalKind::Clock),
                    PortDef::new(1, "reset", SignalKind::Trigger),
                ],
                outputs: vec![
                    PortDef::new(10, "cv", SignalKind::VoltPerOctave),
                    PortDef::new(11, "gate", SignalKind::Gate),
                    PortDef::new(12, "trig", SignalKind::Trigger),
                ],
            },
        }
    }

    pub fn set_step(&mut self, index: usize, voltage: f64, gate: bool) {
        if index < N {
            self.steps[index] = voltage;
            self.gates[index] = gate;
        }
    }
}

impl<const N: usize> GraphModule for StepSequencer<N> {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let clock = inputs.get_or(0, 0.0);
        let reset = inputs.get_or(1, 0.0);

        let clock_rising = clock > 2.5 && self.last_clock <= 2.5;
        let reset_rising = reset > 2.5 && self.last_reset <= 2.5;

        let mut trigger = 0.0;

        if reset_rising {
            self.current = 0;
            trigger = 5.0;
        } else if clock_rising {
            self.current = (self.current + 1) % N;
            trigger = 5.0;
        }

        self.last_clock = clock;
        self.last_reset = reset;

        let cv = self.steps[self.current];
        let gate = if self.gates[self.current] && clock > 2.5 {
            5.0
        } else {
            0.0
        };

        outputs.set(10, cv);
        outputs.set(11, gate);
        outputs.set(12, trigger);
    }

    fn reset(&mut self) {
        self.current = 0;
        self.last_clock = 0.0;
        self.last_reset = 0.0;
    }

    fn set_sample_rate(&mut self, _: f64) {}
}
```

### 7.6 Output Module

#### Stereo Output

```rust
pub struct StereoOutput {
    spec: PortSpec,
}

impl StereoOutput {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "left", SignalKind::Audio),
                    PortDef::new(1, "right", SignalKind::Audio).normalled_to(0),  // Mono fallback
                ],
                outputs: vec![
                    PortDef::new(0, "left", SignalKind::Audio),
                    PortDef::new(1, "right", SignalKind::Audio),
                ],
            },
        }
    }
}

impl GraphModule for StereoOutput {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let left = inputs.get_or(0, 0.0);
        let right = inputs.get_or(1, left);  // Mono fallback

        outputs.set(0, left);
        outputs.set(1, right);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}
}
```

-----

## 8. Analog Modeling Primitives

### 8.1 Saturation and Soft Clipping

```rust
pub mod saturation {
    /// Hyperbolic tangent saturation (tube-like warmth)
    pub fn tanh_sat(x: f64, drive: f64) -> f64 {
        (x * drive).tanh() / drive.tanh().max(0.001)
    }

    /// Soft clipping with adjustable knee
    pub fn soft_clip(x: f64, threshold: f64) -> f64 {
        if x.abs() < threshold {
            x
        } else {
            let sign = x.signum();
            let excess = x.abs() - threshold;
            sign * (threshold + excess / (1.0 + excess))
        }
    }

    /// Asymmetric saturation (generates even harmonics)
    pub fn asym_sat(x: f64, pos_drive: f64, neg_drive: f64) -> f64 {
        if x >= 0.0 {
            (x * pos_drive).tanh()
        } else {
            (x * neg_drive).tanh()
        }
    }

    /// Diode-style hard clipping
    pub fn diode_clip(x: f64, forward_voltage: f64) -> f64 {
        let vf = forward_voltage;
        if x > vf {
            vf + (x - vf) * 0.1
        } else if x < -vf {
            -vf + (x + vf) * 0.1
        } else {
            x
        }
    }

    /// Wavefolder (generates complex harmonics)
    pub fn fold(x: f64, threshold: f64) -> f64 {
        let mut y = x;
        while y.abs() > threshold {
            if y > threshold {
                y = 2.0 * threshold - y;
            } else if y < -threshold {
                y = -2.0 * threshold - y;
            }
        }
        y
    }
}
```

### 8.2 Component Variation and Drift

```rust
/// Models real-world component imperfection
pub struct ComponentModel {
    /// Base tolerance (e.g., 0.01 for 1% resistor)
    pub tolerance: f64,

    /// Temperature coefficient (drift per degree C)
    pub temp_coef: f64,

    /// Current operating temperature offset from nominal
    pub temp_offset: f64,

    /// Random offset applied at instantiation
    pub instance_offset: f64,
}

impl ComponentModel {
    pub fn new(tolerance: f64, temp_coef: f64) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        Self {
            tolerance,
            temp_coef,
            temp_offset: 0.0,
            instance_offset: rng.gen_range(-tolerance..tolerance),
        }
    }

    /// Perfect component (no variation)
    pub fn perfect() -> Self {
        Self {
            tolerance: 0.0,
            temp_coef: 0.0,
            temp_offset: 0.0,
            instance_offset: 0.0,
        }
    }

    /// Get the effective value multiplier
    pub fn factor(&self) -> f64 {
        1.0 + self.instance_offset + (self.temp_offset * self.temp_coef)
    }

    /// Apply component variation to a value
    pub fn apply(&self, value: f64) -> f64 {
        value * self.factor()
    }

    /// Update temperature offset
    pub fn set_temperature(&mut self, temp_offset: f64) {
        self.temp_offset = temp_offset;
    }
}

/// Thermal drift simulation
pub struct ThermalModel {
    /// Current virtual temperature
    temperature: f64,

    /// Ambient temperature
    ambient: f64,

    /// Heat generated per unit of signal energy
    heat_rate: f64,

    /// Cooling rate (thermal dissipation)
    cool_rate: f64,
}

impl ThermalModel {
    pub fn new(ambient: f64, heat_rate: f64, cool_rate: f64) -> Self {
        Self {
            temperature: ambient,
            ambient,
            heat_rate,
            cool_rate,
        }
    }

    /// Update temperature based on signal energy
    pub fn update(&mut self, signal_energy: f64, dt: f64) {
        let heating = signal_energy * self.heat_rate;
        let cooling = (self.temperature - self.ambient) * self.cool_rate;
        self.temperature += (heating - cooling) * dt;
    }

    /// Get current temperature offset from ambient
    pub fn offset(&self) -> f64 {
        self.temperature - self.ambient
    }
}
```

### 8.3 Noise Generators

```rust
pub mod noise {
    use rand::Rng;

    /// White noise (flat spectrum)
    pub fn white() -> f64 {
        rand::thread_rng().gen_range(-1.0..1.0)
    }

    /// Pink noise (1/f spectrum) using Voss-McCartney algorithm
    pub struct PinkNoise {
        rows: [f64; 16],
        running_sum: f64,
        index: u32,
    }

    impl PinkNoise {
        pub fn new() -> Self {
            Self {
                rows: [0.0; 16],
                running_sum: 0.0,
                index: 0,
            }
        }

        pub fn next(&mut self) -> f64 {
            let mut rng = rand::thread_rng();
            self.index = self.index.wrapping_add(1);

            let changed_bits = (self.index ^ (self.index - 1)).trailing_ones() as usize;

            for i in 0..changed_bits.min(16) {
                self.running_sum -= self.rows[i];
                self.rows[i] = rng.gen_range(-1.0..1.0);
                self.running_sum += self.rows[i];
            }

            self.running_sum / 16.0
        }
    }

    /// Power supply ripple (low frequency hum)
    pub struct PowerSupplyNoise {
        phase: f64,
        frequency: f64,  // 50 or 60 Hz
        sample_rate: f64,
        amplitude: f64,
    }

    impl PowerSupplyNoise {
        pub fn new(sample_rate: f64, frequency: f64, amplitude: f64) -> Self {
            Self {
                phase: 0.0,
                frequency,
                sample_rate,
                amplitude,
            }
        }

        pub fn next(&mut self) -> f64 {
            let out = (self.phase * std::f64::consts::TAU).sin() * self.amplitude;
            self.phase = (self.phase + self.frequency / self.sample_rate).fract();
            out + white() * self.amplitude * 0.1
        }
    }
}
```

### 8.4 Analog-Modeled VCO

```rust
pub struct AnalogVco {
    phase: f64,
    sample_rate: f64,

    // Analog modeling
    freq_component: ComponentModel,
    thermal: ThermalModel,
    dc_offset: f64,

    // State
    last_output: f64,
    last_sync: f64,

    spec: PortSpec,
}

impl AnalogVco {
    pub fn new(sample_rate: f64) -> Self {
        use rand::Rng;
        Self {
            phase: 0.0,
            sample_rate,
            freq_component: ComponentModel::new(0.02, 0.0001),  // 2% tolerance
            thermal: ThermalModel::new(25.0, 0.01, 0.001),
            dc_offset: rand::thread_rng().gen_range(-0.01..0.01),
            last_output: 0.0,
            last_sync: 0.0,
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "voct", SignalKind::VoltPerOctave),
                    PortDef::new(1, "fm", SignalKind::CvBipolar).with_attenuverter(),
                    PortDef::new(2, "pw", SignalKind::CvUnipolar).with_default(0.5),
                    PortDef::new(3, "sync", SignalKind::Gate),
                ],
                outputs: vec![
                    PortDef::new(10, "sin", SignalKind::Audio),
                    PortDef::new(11, "tri", SignalKind::Audio),
                    PortDef::new(12, "saw", SignalKind::Audio),
                    PortDef::new(13, "sqr", SignalKind::Audio),
                ],
            },
        }
    }
}

impl GraphModule for AnalogVco {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let voct = inputs.get_or(0, 0.0);
        let fm = inputs.get_or(1, 0.0);
        let pw = inputs.get_or(2, 0.5).clamp(0.05, 0.95);
        let sync = inputs.get_or(3, 0.0);

        // Apply component tolerance and thermal drift to frequency
        let base_freq = 261.63 * 2.0_f64.powf(voct);
        let freq = self.freq_component.apply(base_freq);
        let freq = freq * (1.0 + self.thermal.offset() * 0.001);  // Thermal detuning
        let freq = freq * 2.0_f64.powf(fm);

        // Update thermal model
        self.thermal
            .update(self.last_output.powi(2), 1.0 / self.sample_rate);

        // Hard sync
        if sync > 2.5 && self.last_sync <= 2.5 {
            self.phase = 0.0;
        }
        self.last_sync = sync;

        // Generate waveforms with slight analog imperfections
        let sin = (self.phase * TAU).sin();
        let tri = 1.0 - 4.0 * (self.phase - 0.5).abs();
        let saw = 2.0 * self.phase - 1.0;
        let sqr = if self.phase < pw { 1.0 } else { -1.0 };

        // Add DC offset and slight asymmetric saturation
        let saw = saturation::asym_sat(saw + self.dc_offset, 1.0, 0.98);

        self.last_output = saw;
        self.phase = (self.phase + freq / self.sample_rate).fract();

        // Output at ±5V
        outputs.set(10, sin * 5.0);
        outputs.set(11, tri * 5.0);
        outputs.set(12, saw * 5.0);
        outputs.set(13, sqr * 5.0);
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.last_output = 0.0;
        self.last_sync = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }
}
```

-----

## 9. External I/O Integration

### 9.1 Design Philosophy

`quiver` is a pure signal processing engine. The patch consumes control signals and produces audio samples. Actual I/O—talking to sound cards, MIDI devices, OSC sockets—is delegated to external crates. This keeps the core library:

- **Deterministic**: Patches can be tested and rendered offline
- **Portable**: No platform-specific audio code in the core
- **Flexible**: Users choose their preferred I/O stack

### 9.2 External Input Module

The bridge between the outside world and the patch:

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Atomic f64 for lock-free communication between threads
pub struct AtomicF64(AtomicU64);

impl AtomicF64 {
    pub fn new(value: f64) -> Self {
        Self(AtomicU64::new(value.to_bits()))
    }

    pub fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }

    pub fn set(&self, value: f64) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }
}

/// External input source - reads from an atomic value set by another thread
pub struct ExternalInput {
    value: Arc<AtomicF64>,
    spec: PortSpec,
}

impl ExternalInput {
    pub fn new(value: Arc<AtomicF64>, kind: SignalKind) -> Self {
        Self {
            value,
            spec: PortSpec {
                inputs: vec![],
                outputs: vec![PortDef::new(0, "out", kind)],
            },
        }
    }

    /// Create for pitch CV
    pub fn voct(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::VoltPerOctave)
    }

    /// Create for gate signals
    pub fn gate(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::Gate)
    }

    /// Create for general CV
    pub fn cv(value: Arc<AtomicF64>) -> Self {
        Self::new(value, SignalKind::CvUnipolar)
    }
}

impl GraphModule for ExternalInput {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, _inputs: &PortValues, outputs: &mut PortValues) {
        outputs.set(0, self.value.get());
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}
}
```

### 9.3 MIDI Handling

MIDI messages are parsed and converted to CV/gate signals:

```rust
/// MIDI state that can be updated from a MIDI thread
pub struct MidiState {
    pub pitch: Arc<AtomicF64>,
    pub gate: Arc<AtomicF64>,
    pub velocity: Arc<AtomicF64>,
    pub mod_wheel: Arc<AtomicF64>,
    pub pitch_bend: Arc<AtomicF64>,
    pub aftertouch: Arc<AtomicF64>,

    // Internal state for note handling
    held_notes: Vec<u8>,
}

impl MidiState {
    pub fn new() -> Self {
        Self {
            pitch: Arc::new(AtomicF64::new(0.0)),
            gate: Arc::new(AtomicF64::new(0.0)),
            velocity: Arc::new(AtomicF64::new(0.0)),
            mod_wheel: Arc::new(AtomicF64::new(0.0)),
            pitch_bend: Arc::new(AtomicF64::new(0.0)),
            aftertouch: Arc::new(AtomicF64::new(0.0)),
            held_notes: Vec::new(),
        }
    }

    /// Process a MIDI message
    pub fn handle_message(&mut self, msg: &[u8]) {
        match msg {
            // Note On (with velocity > 0)
            [status, note, vel] if status & 0xF0 == 0x90 && *vel > 0 => {
                self.held_notes.push(*note);
                self.pitch.set(Self::note_to_voct(*note));
                self.velocity.set(*vel as f64 / 127.0 * 10.0);
                self.gate.set(5.0);
            }
            // Note Off (or Note On with velocity 0)
            [status, note, _]
                if status & 0xF0 == 0x80 || (status & 0xF0 == 0x90) =>
            {
                self.held_notes.retain(|&n| n != *note);
                if self.held_notes.is_empty() {
                    self.gate.set(0.0);
                } else {
                    // Legato: retrigger to last held note
                    let last = *self.held_notes.last().unwrap();
                    self.pitch.set(Self::note_to_voct(last));
                }
            }
            // Control Change
            [status, cc, value] if status & 0xF0 == 0xB0 => {
                let v = *value as f64 / 127.0 * 10.0;
                match cc {
                    1 => self.mod_wheel.set(v),
                    _ => {}
                }
            }
            // Pitch Bend
            [status, lsb, msb] if status & 0xF0 == 0xE0 => {
                let bend_raw = (*lsb as u16) | ((*msb as u16) << 7);
                // ±2 semitones = ±2/12 V
                let bend = (bend_raw as f64 - 8192.0) / 8192.0 * (2.0 / 12.0);
                self.pitch_bend.set(bend);
            }
            // Channel Aftertouch
            [status, pressure] if status & 0xF0 == 0xD0 => {
                self.aftertouch.set(*pressure as f64 / 127.0 * 10.0);
            }
            _ => {}
        }
    }

    /// Convert MIDI note to V/Oct (0V = C4 = MIDI note 60)
    fn note_to_voct(note: u8) -> f64 {
        (note as f64 - 60.0) / 12.0
    }
}
```

### 9.4 Audio Output Integration

Example using the `cpal` crate:

```rust
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub fn run_audio_output(
    mut patch: Patch,
    sample_rate: u32,
) -> Result<cpal::Stream, Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("No output device found")?;

    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Fixed(256),
    };

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            for frame in data.chunks_mut(2) {
                let (left, right) = patch.tick();
                // Scale from ±5V to ±1.0 and convert to f32
                frame[0] = (left / 5.0).clamp(-1.0, 1.0) as f32;
                frame[1] = (right / 5.0).clamp(-1.0, 1.0) as f32;
            }
        },
        |err| eprintln!("Audio stream error: {}", err),
        None,
    )?;

    stream.play()?;
    Ok(stream)
}
```

### 9.5 Integration Summary

|Concern     |quiver’s Role                      |External Crate|
|------------|-----------------------------------|--------------|
|Audio output|Produces samples via `patch.tick()`|`cpal`, `jack`|
|Audio input |`ExternalInput` modules            |`cpal`, `jack`|
|MIDI input  |`MidiState` → `ExternalInput`      |`midir`       |
|MIDI output |Future `CvToMidi` module           |`midir`       |
|OSC         |`ExternalInput` / `ExternalOutput` |`rosc`        |

-----

## 10. Building a Complete Synthesizer

### 10.1 Architecture Overview

A complete synthesizer consists of:

1. **MIDI Thread**: Receives MIDI, updates atomic state
1. **Audio Thread**: Runs `patch.tick()`, outputs samples
1. **Patch**: The signal processing graph
1. **External Inputs**: Bridges from MIDI state to patch

```
┌──────────────┐     ┌─────────────────────────────────────────────┐
│ MIDI Device  │────▶│              MIDI Thread                    │
└──────────────┘     │  MidiState.handle_message()                 │
                     │  Updates: pitch, gate, velocity, etc.       │
                     └─────────────────┬───────────────────────────┘
                                       │ Arc<AtomicF64> (lock-free)
                                       ▼
┌──────────────┐     ┌─────────────────────────────────────────────┐
│  Sound Card  │◀────│             Audio Thread                    │
└──────────────┘     │  loop { patch.tick() → samples }            │
                     └─────────────────────────────────────────────┘
```

### 10.2 Complete Example: Monosynth

```rust
use quiver::prelude::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use midir::MidiInput;
use std::sync::Arc;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let sample_rate = 44100.0;

    // === 1. Create shared MIDI state ===
    let midi_state = MidiState::new();

    // === 2. Build the patch ===
    let mut patch = Patch::new(sample_rate);

    // External inputs (bridges from MIDI thread)
    let pitch_in = patch.add(
        "pitch",
        ExternalInput::voct(Arc::clone(&midi_state.pitch)),
    );
    let gate_in = patch.add(
        "gate",
        ExternalInput::gate(Arc::clone(&midi_state.gate)),
    );
    let mod_in = patch.add(
        "mod",
        ExternalInput::cv(Arc::clone(&midi_state.mod_wheel)),
    );
    let bend_in = patch.add(
        "bend",
        ExternalInput::new(
            Arc::clone(&midi_state.pitch_bend),
            SignalKind::CvBipolar,
        ),
    );

    // Sound sources
    let vco1 = patch.add("vco1", AnalogVco::new(sample_rate));
    let vco2 = patch.add("vco2", AnalogVco::new(sample_rate));
    let noise = patch.add("noise", NoiseGenerator::new());

    // VCO2 detune
    let detune = patch.add("detune", Offset::new(0.04));  // ~half semitone

    // Pitch bend adder
    let pitch_sum = patch.add("pitch_sum", Mixer::<2>::new());

    // Oscillator mixer
    let osc_mix = patch.add("osc_mix", Mixer::<3>::new());

    // Filter
    let vcf = patch.add("vcf", Svf::new(sample_rate));

    // Amplifier
    let vca = patch.add("vca", Vca::new());

    // Modulation
    let lfo = patch.add("lfo", Lfo::new(sample_rate));
    let filter_env = patch.add("flt_env", Adsr::new(sample_rate));
    let amp_env = patch.add("amp_env", Adsr::new(sample_rate));

    // Output
    let output = patch.add("output", StereoOutput::new());

    // === 3. Patch cables ===

    // Pitch + bend -> pitch sum
    patch.connect(pitch_in.out("out"), pitch_sum.in_("ch0"))?;
    patch.connect(bend_in.out("out"), pitch_sum.in_("ch1"))?;

    // Pitch to oscillators
    patch.connect(pitch_sum.out("out"), vco1.in_("voct"))?;
    patch.connect(pitch_sum.out("out"), detune.in_("in"))?;
    patch.connect(detune.out("out"), vco2.in_("voct"))?;

    // Gate to envelopes
    patch.connect(gate_in.out("out"), filter_env.in_("gate"))?;
    patch.connect(gate_in.out("out"), amp_env.in_("gate"))?;

    // Oscillators to mixer
    patch.connect(vco1.out("saw"), osc_mix.in_("ch0"))?;
    patch.connect(vco2.out("sqr"), osc_mix.in_("ch1"))?;
    patch.connect(noise.out("pink"), osc_mix.in_("ch2"))?;

    // Mixer through filter
    patch.connect(osc_mix.out("out"), vcf.in_("in"))?;

    // Filter modulation
    patch.connect(filter_env.out("env"), vcf.in_("cutoff"))?;
    patch.connect(lfo.out("tri"), vcf.in_("fm"))?;

    // Mod wheel controls LFO depth
    patch.connect(mod_in.out("out"), lfo.in_("depth"))?;

    // Filter to VCA
    patch.connect(vcf.out("lp"), vca.in_("in"))?;
    patch.connect(amp_env.out("env"), vca.in_("cv"))?;

    // VCA to output
    patch.connect(vca.out("out"), output.in_("left"))?;
    patch.connect(vca.out("out"), output.in_("right"))?;

    patch.set_output(output.id());
    patch.compile()?;

    // Set some initial parameters
    patch.set_param(lfo.id(), 0, 0.3);        // LFO rate
    patch.set_param(vcf.id(), 2, 0.4);        // Filter resonance
    patch.set_param(filter_env.id(), 2, 0.2); // Attack
    patch.set_param(filter_env.id(), 3, 0.4); // Decay

    // === 4. Start MIDI input ===
    let midi_state_for_callback = midi_state.clone();

    let midi_in = MidiInput::new("quiver-synth")?;
    let ports = midi_in.ports();
    let port = ports.first().ok_or("No MIDI input found")?;

    println!("Connecting to MIDI: {}", midi_in.port_name(port)?);

    let _midi_conn = midi_in.connect(
        port,
        "quiver-input",
        move |_timestamp, message, _| {
            midi_state_for_callback.handle_message(message);
        },
        (),
    )?;

    // === 5. Start audio output ===
    let host = cpal::default_host();
    let device = host.default_output_device().unwrap();

    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(sample_rate as u32),
        buffer_size: cpal::BufferSize::Fixed(256),
    };

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _| {
            for frame in data.chunks_mut(2) {
                let (left, right) = patch.tick();
                frame[0] = (left / 5.0).clamp(-1.0, 1.0) as f32;
                frame[1] = (right / 5.0).clamp(-1.0, 1.0) as f32;
            }
        },
        |err| eprintln!("Audio error: {}", err),
        None,
    )?;

    stream.play()?;

    println!("Synthesizer running. Press Ctrl+C to quit.");
    println!("Play some MIDI notes!");

    // Keep running
    std::thread::park();

    Ok(())
}
```

### 10.3 Signal Flow Diagram

```
                      MIDI Input
                          │
            ┌─────────────┴─────────────┐
            ▼                           ▼
       ┌─────────┐                ┌──────────┐
       │ pitch   │                │   gate   │
       │ + bend  │                └────┬─────┘
       └────┬────┘                     │
            │                     ┌────┴────┐
       ┌────┴────┐                ▼         ▼
       ▼         ▼          ┌─────────┐ ┌─────────┐
   ┌──────┐  ┌──────┐       │ Flt Env │ │ Amp Env │
   │ VCO1 │  │ VCO2 │       └────┬────┘ └────┬────┘
   │ saw  │  │ sqr  │            │           │
   └──┬───┘  └──┬───┘            │           │
      │         │                │           │
      │    ┌────┘                │           │
      │    │    ┌───────┐        │           │
      │    │    │ Noise │        │           │
      │    │    └───┬───┘        │           │
      ▼    ▼        ▼            │           │
   ┌──────────────────┐          │           │
   │      Mixer       │          │           │
   └────────┬─────────┘          │           │
            │                    │           │
            ▼                    │           │
   ┌────────────────┐            │           │
   │      VCF       │◀───────────┘           │
   │   (lowpass)    │◀─── LFO (via mod wheel)│
   └────────┬───────┘                        │
            │                                │
            ▼                                │
   ┌────────────────┐                        │
   │      VCA       │◀───────────────────────┘
   └────────┬───────┘
            │
            ▼
   ┌────────────────┐
   │  Stereo Out    │
   └────────┬───────┘
            │
            ▼
        🔊 Speakers
```

-----

## 11. Serialization and Persistence

### 11.1 Patch Serialization Format

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchDef {
    /// Schema version for forward compatibility
    pub version: u32,

    /// Patch metadata
    pub name: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,

    /// Module instances
    pub modules: Vec<ModuleDef>,

    /// Cable connections
    pub cables: Vec<CableDef>,

    /// Parameter values
    pub parameters: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDef {
    /// Unique instance name
    pub name: String,

    /// Module type identifier
    pub module_type: String,

    /// UI position (optional)
    pub position: Option<(f32, f32)>,

    /// Module-specific state
    pub state: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CableDef {
    /// Source: "module_name.port_name"
    pub from: String,

    /// Destination: "module_name.port_name"
    pub to: String,

    /// Optional attenuation
    pub attenuation: Option<f64>,
}
```

### 11.2 Module Registry

```rust
pub type ModuleFactory = Box<dyn Fn(f64) -> Box<dyn GraphModule> + Send + Sync>;

pub struct ModuleRegistry {
    factories: HashMap<String, ModuleFactory>,
    metadata: HashMap<String, ModuleMetadata>,
}

#[derive(Debug, Clone)]
pub struct ModuleMetadata {
    pub type_id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub port_spec: PortSpec,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            factories: HashMap::new(),
            metadata: HashMap::new(),
        };

        // Register built-in modules
        registry.register::<Vco>("vco", "Oscillators", "Voltage-controlled oscillator");
        registry.register::<AnalogVco>("analog_vco", "Oscillators", "VCO with analog modeling");
        registry.register::<Lfo>("lfo", "Modulation", "Low-frequency oscillator");
        registry.register::<Svf>("svf", "Filters", "State-variable filter");
        registry.register::<Adsr>("adsr", "Envelopes", "ADSR envelope generator");
        registry.register::<Vca>("vca", "Utilities", "Voltage-controlled amplifier");
        registry.register::<NoiseGenerator>("noise", "Sources", "Noise generator");

        registry
    }

    pub fn register<M>(&mut self, type_id: &str, category: &str, description: &str)
    where
        M: GraphModule + Default + 'static,
    {
        self.factories.insert(
            type_id.to_string(),
            Box::new(|sample_rate| {
                let mut module = M::default();
                module.set_sample_rate(sample_rate);
                Box::new(module)
            }),
        );

        let instance = M::default();
        self.metadata.insert(
            type_id.to_string(),
            ModuleMetadata {
                type_id: type_id.to_string(),
                name: type_id.to_string(),
                category: category.to_string(),
                description: description.to_string(),
                port_spec: instance.port_spec().clone(),
            },
        );
    }

    pub fn instantiate(
        &self,
        type_id: &str,
        sample_rate: f64,
    ) -> Option<Box<dyn GraphModule>> {
        self.factories.get(type_id).map(|f| f(sample_rate))
    }

    pub fn list_modules(&self) -> impl Iterator<Item = &ModuleMetadata> {
        self.metadata.values()
    }
}
```

### 11.3 Loading and Saving Patches

```rust
impl Patch {
    pub fn to_def(&self) -> PatchDef {
        let modules = self
            .nodes
            .iter()
            .map(|(_, node)| ModuleDef {
                name: node.name.clone(),
                module_type: node.module.type_id().to_string(),
                position: node.position,
                state: node.module.serialize_state(),
            })
            .collect();

        let cables = self
            .cables
            .iter()
            .map(|cable| {
                let from_node = &self.nodes[cable.from.node];
                let to_node = &self.nodes[cable.to.node];

                let from_port = from_node
                    .module
                    .port_spec()
                    .outputs
                    .iter()
                    .find(|p| p.id == cable.from.port)
                    .map(|p| p.name)
                    .unwrap_or("unknown");

                let to_port = to_node
                    .module
                    .port_spec()
                    .inputs
                    .iter()
                    .find(|p| p.id == cable.to.port)
                    .map(|p| p.name)
                    .unwrap_or("unknown");

                CableDef {
                    from: format!("{}.{}", from_node.name, from_port),
                    to: format!("{}.{}", to_node.name, to_port),
                    attenuation: cable.attenuation,
                }
            })
            .collect();

        PatchDef {
            version: 1,
            name: "Untitled".to_string(),
            author: None,
            description: None,
            tags: vec![],
            modules,
            cables,
            parameters: HashMap::new(),
        }
    }

    pub fn from_def(
        def: &PatchDef,
        registry: &ModuleRegistry,
        sample_rate: f64,
    ) -> Result<Self, PatchError> {
        let mut patch = Patch::new(sample_rate);
        let mut name_to_handle: HashMap<String, NodeHandle> = HashMap::new();

        // Instantiate modules
        for module_def in &def.modules {
            let module = registry
                .instantiate(&module_def.module_type, sample_rate)
                .ok_or_else(|| {
                    PatchError::CompilationFailed(format!(
                        "Unknown module type: {}",
                        module_def.module_type
                    ))
                })?;

            let handle = patch.add(&module_def.name, module);
            name_to_handle.insert(module_def.name.clone(), handle);
        }

        // Create cables
        for cable_def in &def.cables {
            let (from_module, from_port) = parse_port_ref(&cable_def.from)?;
            let (to_module, to_port) = parse_port_ref(&cable_def.to)?;

            let from_handle = name_to_handle.get(from_module).ok_or_else(|| {
                PatchError::CompilationFailed(format!("Unknown module: {}", from_module))
            })?;

            let to_handle = name_to_handle.get(to_module).ok_or_else(|| {
                PatchError::CompilationFailed(format!("Unknown module: {}", to_module))
            })?;

            patch.connect(from_handle.out(from_port), to_handle.in_(to_port))?;
        }

        patch.compile()?;
        Ok(patch)
    }
}

fn parse_port_ref(s: &str) -> Result<(&str, &str), PatchError> {
    let parts: Vec<&str> = s.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err(PatchError::CompilationFailed(format!(
            "Invalid port reference: {}",
            s
        )));
    }
    Ok((parts[0], parts[1]))
}
```

-----

## 12. Performance Considerations

### 12.1 Block Processing

For efficiency, modules should support block-based processing:

```rust
pub trait GraphModule {
    // ... existing methods ...

    /// Process multiple samples at once
    fn process_block(
        &mut self,
        inputs: &BlockPortValues,
        outputs: &mut BlockPortValues,
        frames: usize,
    ) {
        // Default: delegate to per-sample tick
        for i in 0..frames {
            let in_frame = inputs.frame(i);
            let mut out_frame = PortValues::new();
            self.tick(&in_frame, &mut out_frame);
            outputs.set_frame(i, out_frame);
        }
    }
}

/// Block-oriented port values
pub struct BlockPortValues {
    buffers: HashMap<PortId, Vec<f64>>,
    block_size: usize,
}

impl BlockPortValues {
    pub fn new(block_size: usize) -> Self {
        Self {
            buffers: HashMap::new(),
            block_size,
        }
    }

    pub fn get_buffer(&self, port: PortId) -> Option<&[f64]> {
        self.buffers.get(&port).map(|v| v.as_slice())
    }

    pub fn get_buffer_mut(&mut self, port: PortId) -> &mut Vec<f64> {
        self.buffers
            .entry(port)
            .or_insert_with(|| vec![0.0; self.block_size])
    }

    pub fn frame(&self, index: usize) -> PortValues {
        let mut values = PortValues::new();
        for (&port, buffer) in &self.buffers {
            if index < buffer.len() {
                values.set(port, buffer[index]);
            }
        }
        values
    }

    pub fn set_frame(&mut self, index: usize, values: PortValues) {
        for (&port, &value) in &values.values {
            let buffer = self.get_buffer_mut(port);
            if index < buffer.len() {
                buffer[index] = value;
            }
        }
    }
}
```

### 12.2 SIMD Optimization

```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

impl Vco {
    /// SIMD-optimized block processing
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    pub unsafe fn process_block_simd(
        &mut self,
        freq: &[f64],
        output: &mut [f64],
    ) {
        // Process 4 samples at a time
        let chunks = freq.len() / 4;

        for i in 0..chunks {
            let offset = i * 4;

            // Load frequencies
            let freq_vec = _mm256_loadu_pd(freq.as_ptr().add(offset));

            // Compute phases (simplified)
            let phase_inc = _mm256_div_pd(freq_vec, _mm256_set1_pd(self.sample_rate));

            // Generate output (would need proper sin approximation)
            // This is illustrative; real implementation would be more complex

            _mm256_storeu_pd(output.as_mut_ptr().add(offset), phase_inc);
        }

        // Handle remaining samples
        for i in (chunks * 4)..freq.len() {
            output[i] = self.tick_single(freq[i]);
        }
    }
}
```

### 12.3 Memory Layout Optimization

```rust
/// Cache-friendly module storage
pub struct OptimizedPatch {
    // Hot data: processing state (accessed every sample)
    module_states: Vec<Box<dyn GraphModule>>,

    // Warm data: port buffers (reused each block)
    buffer_pool: BufferPool,

    // Cold data: topology (rarely accessed during processing)
    topology: Arc<PatchTopology>,

    // Execution plan (computed once at compile time)
    execution_plan: Vec<ExecutionStep>,
}

struct BufferPool {
    buffers: Vec<AlignedBuffer>,
    assignments: HashMap<PortRef, usize>,
}

#[repr(align(64))]  // Cache line alignment
struct AlignedBuffer {
    data: [f64; 256],  // Fixed block size
}
```

### 12.4 Performance Guidelines

1. **Avoid allocations in the audio path**: Pre-allocate all buffers
1. **Minimize cache misses**: Keep hot data together
1. **Use block processing**: Amortize per-sample overhead
1. **Consider SIMD**: For computationally intensive modules
1. **Profile before optimizing**: Use tools like `perf` or `cargo flamegraph`

-----

## 13. Testing Strategy

### 13.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vco_frequency_accuracy() {
        let mut vco = Vco::new(44100.0);
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Test A4 (440 Hz) - should be at ~0.69 V/Oct above C4
        inputs.set(0, (440.0_f64 / 261.63).log2());

        // Collect samples
        let period_samples = (44100.0 / 440.0) as usize;
        let mut samples = Vec::new();

        for _ in 0..period_samples * 10 {
            vco.tick(&inputs, &mut outputs);
            samples.push(outputs.get(10).unwrap());
        }

        // Verify frequency via zero-crossing analysis
        let measured_freq = measure_frequency(&samples, 44100.0);
        assert!(
            (measured_freq - 440.0).abs() < 1.0,
            "Expected ~440Hz, got {}",
            measured_freq
        );
    }

    #[test]
    fn adsr_attack_decay_sustain() {
        let mut adsr = Adsr::new(1000.0);  // 1kHz for easy math
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();

        // Set fast attack (10ms), fast decay (10ms), 50% sustain
        inputs.set(2, 0.25);  // attack
        inputs.set(3, 0.25);  // decay
        inputs.set(4, 0.5);   // sustain

        // Gate on
        inputs.set(0, 5.0);

        // Run for 50ms
        let mut levels = Vec::new();
        for _ in 0..50 {
            adsr.tick(&inputs, &mut outputs);
            levels.push(outputs.get(10).unwrap());
        }

        // Should reach peak around 10ms, sustain around 20ms
        let peak_idx = levels
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();

        assert!(peak_idx >= 8 && peak_idx <= 15, "Peak at wrong time: {}", peak_idx);
    }

    #[test]
    fn patch_topological_sort() {
        let mut patch = Patch::new(44100.0);

        let a = patch.add("a", Vca::new());
        let b = patch.add("b", Vca::new());
        let c = patch.add("c", Vca::new());

        // A -> B -> C
        patch.connect(a.out("out"), b.in_("in")).unwrap();
        patch.connect(b.out("out"), c.in_("in")).unwrap();

        patch.compile().unwrap();

        let order = &patch.execution_order;
        let a_pos = order.iter().position(|&x| x == a.id()).unwrap();
        let b_pos = order.iter().position(|&x| x == b.id()).unwrap();
        let c_pos = order.iter().position(|&x| x == c.id()).unwrap();

        assert!(a_pos < b_pos, "A should come before B");
        assert!(b_pos < c_pos, "B should come before C");
    }

    #[test]
    fn patch_cycle_detection() {
        let mut patch = Patch::new(44100.0);

        let a = patch.add("a", Vca::new());
        let b = patch.add("b", Vca::new());

        // Create cycle: A -> B -> A
        patch.connect(a.out("out"), b.in_("in")).unwrap();
        patch.connect(b.out("out"), a.in_("in")).unwrap();

        let result = patch.compile();
        assert!(matches!(result, Err(PatchError::CycleDetected { .. })));
    }

    fn measure_frequency(samples: &[f64], sample_rate: f64) -> f64 {
        // Simple zero-crossing frequency estimation
        let crossings: Vec<usize> = samples
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] <= 0.0 && w[1] > 0.0)
            .map(|(i, _)| i)
            .collect();

        if crossings.len() < 2 {
            return 0.0;
        }

        let avg_period = (crossings.last().unwrap() - crossings.first().unwrap()) as f64
            / (crossings.len() - 1) as f64;

        sample_rate / avg_period
    }
}
```

### 13.2 Integration Tests

```rust
#[test]
fn full_synth_produces_audio() {
    // Build a minimal synth
    let mut patch = build_test_synth(44100.0);

    // Simulate gate on
    // (would need external inputs connected)

    // Run for 1 second
    let mut samples = Vec::new();
    for _ in 0..44100 {
        let (l, _) = patch.tick();
        samples.push(l);
    }

    // Verify audio was generated
    let max_level = samples.iter().map(|x| x.abs()).fold(0.0, f64::max);
    assert!(max_level > 0.1, "Should produce audible output");
}

#[test]
fn patch_serialization_roundtrip() {
    let registry = ModuleRegistry::new();

    // Create patch
    let mut patch = Patch::new(44100.0);
    let vco = patch.add("vco", Vco::new(44100.0));
    let vcf = patch.add("vcf", Svf::new(44100.0));
    patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
    patch.compile().unwrap();

    // Serialize
    let def = patch.to_def();
    let json = serde_json::to_string_pretty(&def).unwrap();

    // Deserialize
    let loaded_def: PatchDef = serde_json::from_str(&json).unwrap();
    let loaded_patch = Patch::from_def(&loaded_def, &registry, 44100.0).unwrap();

    // Verify structure
    assert_eq!(patch.nodes.len(), loaded_patch.nodes.len());
    assert_eq!(patch.cables.len(), loaded_patch.cables.len());
}
```

-----

## 14. Future Extensions

### 14.1 Polyphony

```rust
pub struct PolyVoice<V: GraphModule + Clone, const N: usize> {
    voices: [V; N],
    voice_states: [VoiceState; N],
    allocator: VoiceAllocator,
}

#[derive(Clone, Copy)]
enum VoiceState {
    Free,
    Active { note: u8 },
    Releasing { note: u8 },
}

pub enum VoiceAllocator {
    RoundRobin { next: usize },
    LeastRecent,
    LowestNote,
    HighestNote,
}
```

### 14.2 Visual Patching UI

```rust
pub struct PatchEditor {
    patch: Patch,
    module_positions: HashMap<NodeId, (f32, f32)>,
    selected: Selection,
    dragging: Option<DragState>,
}

pub enum EditorMessage {
    AddModule { type_id: String, position: (f32, f32) },
    RemoveModule { node: NodeId },
    MoveModule { node: NodeId, position: (f32, f32) },
    StartCable { from: PortRef },
    CompleteCable { to: PortRef },
    CancelCable,
    RemoveCable { cable: CableId },
    SetParam { node: NodeId, param: ParamId, value: f64 },
}
```

### 14.3 Plugin Format Wrappers

```rust
pub trait PluginWrapper {
    fn create_vst3(patch: Patch) -> Vst3Plugin;
    fn create_clap(patch: Patch) -> ClapPlugin;
    fn create_au(patch: Patch) -> AudioUnitPlugin;
}

/// Automatic parameter discovery for plugins
pub fn discover_parameters(patch: &Patch) -> Vec<PluginParameter> {
    patch
        .nodes
        .iter()
        .flat_map(|(id, node)| {
            node.module.params().iter().map(move |p| PluginParameter {
                id: combine_ids(id, p.id),
                name: format!("{}_{}", node.name, p.name),
                default: p.default,
                range: p.range,
            })
        })
        .collect()
}
```

### 14.4 Macro DSL for Patches

```rust
// Future: macro for terse patch definitions
let patch = patch! {
    sample_rate: 44100.0,

    modules {
        vco1: AnalogVco,
        vco2: AnalogVco,
        vcf: Svf,
        vca: Vca,
        env: Adsr,
        lfo: Lfo,
    }

    cables {
        pitch_in -> vco1.voct, vco2.voct;
        gate_in -> env.gate;
        vco1.saw -> vcf.in;
        vco2.sqr -> vcf.in;
        env.env -> vcf.cutoff, vca.cv;
        lfo.tri -> vcf.fm;
        vcf.lp -> vca.in;
        vca.out -> output.left, output.right;
    }

    params {
        vco2.detune: 0.05,
        vcf.res: 0.7,
        lfo.rate: 0.3,
    }
};
```

-----

## 15. Development Roadmap

### Phase 1: Core Foundation (MVP)

- [x] Core `Module` trait and combinators
- [x] Basic primitives: VCO, VCF, VCA, ADSR, LFO
- [x] Graph-based `Patch` with topological sort
- [x] Port system with signal types
- [x] Basic serialization

### Phase 2: Hardware Fidelity

- [ ] Normalled connections
- [ ] Input summing
- [ ] Knob + CV parameter model
- [ ] Signal kind validation
- [ ] More modules: sequencer, quantizer, S&H, slew limiter

### Phase 3: Analog Modeling

- [ ] Saturation primitives
- [ ] Component tolerance modeling
- [ ] Thermal drift
- [ ] Noise injection
- [ ] Analog-modeled VCO and VCF variants

### Phase 4: External I/O

- [ ] `ExternalInput` / `ExternalOutput` modules
- [ ] `MidiState` for MIDI handling
- [ ] cpal integration example
- [ ] midir integration example

### Phase 5: Performance

- [ ] Block processing
- [ ] SIMD optimization for core modules
- [ ] Memory layout optimization
- [ ] Benchmarking suite

### Phase 6: Polish

- [ ] Complete serialization/deserialization
- [ ] Module registry and factory system
- [ ] Comprehensive documentation
- [ ] Example patches and synthesizers

### Phase 7: Extensions

- [ ] Polyphony support
- [ ] UI binding infrastructure
- [ ] Plugin format wrappers
- [ ] Patch DSL macro

-----

## 16. Appendices

### Appendix A: Signal Voltage Reference

These conventions are inspired by hardware modular synthesizers and provide a consistent framework for signal interoperability.

|Signal Type|Voltage Range   |Notes                   |
|-----------|----------------|------------------------|
|Audio      |±5V (10Vpp)     |AC-coupled in hardware  |
|CV Bipolar |±5V             |LFO, pitch bend         |
|CV Unipolar|0–10V           |Envelope, velocity      |
|V/Oct      |±5V             |0V = C4, +1V = +1 octave|
|Gate       |0V / +5V        |Binary state            |
|Trigger    |0V → +5V pulse  |1–10ms duration         |
|Clock      |Regular triggers|Tempo-synced            |

### Appendix B: Port Naming Conventions

|Port Name|Type       |Description           |
|---------|-----------|----------------------|
|`in`     |Audio      |Primary audio input   |
|`out`    |Audio      |Primary audio output  |
|`voct`   |V/Oct      |Pitch CV input        |
|`fm`     |CV Bipolar |Frequency modulation  |
|`cutoff` |CV Unipolar|Filter cutoff         |
|`res`    |CV Unipolar|Filter resonance      |
|`pw`     |CV Unipolar|Pulse width           |
|`gate`   |Gate       |Gate input            |
|`trig`   |Trigger    |Trigger input         |
|`cv`     |CV Unipolar|Generic CV input      |
|`sync`   |Gate       |Hard sync input       |
|`reset`  |Trigger    |Reset to initial state|
|`clock`  |Clock      |Clock input           |

### Appendix C: Module Categories

|Category   |Modules                    |
|-----------|---------------------------|
|Oscillators|VCO, AnalogVco, LFO        |
|Filters    |SVF, Ladder, Comb          |
|Envelopes  |ADSR, AR, Function         |
|Amplifiers |VCA, Mixer                 |
|Utilities  |Offset, Mult, Attenuverter |
|Sequencing |StepSequencer, Quantizer   |
|Effects    |Delay, Reverb, Chorus      |
|Noise      |NoiseGenerator             |
|I/O        |ExternalInput, StereoOutput|

### Appendix D: References and Inspirations

**Functional Programming & Category Theory:**

- Arrow abstraction: https://www.haskell.org/arrows/
- Faust DSP language: https://faust.grame.fr/

**Hardware Modular Synthesis (Inspiration):**

- VCV Rack (open source virtual modular): https://vcvrack.com/
- Mutable Instruments (open source modules): https://mutable-instruments.net/

**DSP & Filter Design:**

- “Designing Software Synthesizer Plug-Ins in C++” by Will Pirkle
- “The Art of VA Filter Design” by Vadim Zavalishin

**Rust Audio:**

- cpal (cross-platform audio): https://github.com/RustAudio/cpal
- midir (MIDI): https://github.com/Boddlnagg/midir
