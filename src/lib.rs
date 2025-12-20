//! # Quiver: Modular Audio Synthesis Library
//!
//! > *"A quiver is a directed graph—nodes connected by arrows. In audio, our nodes are
//! > modules, our arrows are patch cables, and signal flows through their composition."*
//!
//! `quiver` is a Rust library for building modular audio synthesis systems. It combines
//! the mathematical elegance of category theory with the tactile joy of patching a
//! hardware modular synthesizer.
//!
//! ## Design Philosophy
//!
//! Quiver bridges two worlds:
//!
//! - **Mathematical Rigor**: Arrow-style functional combinators with compile-time type safety
//! - **Hardware Semantics**: Voltage standards from Eurorack (±5V audio, 1V/octave, gates)
//!
//! The name comes from **category theory**, where a quiver is a directed graph forming
//! the foundation for morphisms and composition—exactly what a modular synthesizer is.
//!
//! ## Three-Layer Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │  Layer 3: Patch Graph                   │  Runtime topology
//! │  - Dynamic patching at runtime          │  "Eurorack in software"
//! │  - Topological sort for processing      │
//! ├─────────────────────────────────────────┤
//! │  Layer 2: Port System                   │  Signal conventions
//! │  - SignalKind (Audio, CV, V/Oct, Gate)  │  "Hardware semantics"
//! │  - PortDef, PortSpec, GraphModule       │
//! ├─────────────────────────────────────────┤
//! │  Layer 1: Typed Combinators             │  Type-safe composition
//! │  - Module trait with associated types   │  "Arrow category"
//! │  - Chain, Parallel, Fanout, Feedback    │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Signal Conventions (Eurorack-inspired)
//!
//! | Signal Type | Range | Description |
//! |-------------|-------|-------------|
//! | Audio | ±5V | AC-coupled audio signals |
//! | CV Unipolar | 0-10V | Filter cutoff, LFO rate |
//! | CV Bipolar | ±5V | Pan, FM depth |
//! | V/Oct | ±10V | Pitch (0V = C4 = 261.63 Hz) |
//! | Gate | 0V or 5V | Sustained on/off |
//! | Trigger | 0V or 5V | Brief pulse (1-10ms) |
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use quiver::prelude::*;
//!
//! // Create a patch at 44.1kHz sample rate
//! let mut patch = Patch::new(44100.0);
//!
//! // Add modules (classic subtractive: VCO → VCF → VCA → Output)
//! let vco = patch.add("vco", Vco::new(44100.0));
//! let vcf = patch.add("vcf", Svf::new(44100.0));
//! let vca = patch.add("vca", Vca::new());
//! let output = patch.add("output", StereoOutput::new());
//!
//! // Patch cables (like a real modular synthesizer)
//! patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
//! patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
//! patch.connect(vca.out("out"), output.in_("left")).unwrap();
//!
//! // Compile (performs topological sort) and process
//! patch.set_output(output.id());
//! patch.compile().unwrap();
//!
//! // Generate audio sample by sample
//! let (left, right) = patch.tick();
//! ```
//!
//! ## Key Features
//!
//! - **50+ DSP Modules**: VCO, VCF, ADSR, LFO, Mixer, Sequencer, Ring Mod, and more
//! - **Type-Safe Composition**: Layer 1 combinators catch errors at compile time
//! - **Polyphony**: Voice allocation with multiple steal modes, unison, detune
//! - **Analog Modeling**: Saturation curves, component tolerance, thermal drift
//! - **Performance**: SIMD vectorization, block processing, lazy evaluation
//! - **Serialization**: Save/load patches as JSON with ModuleRegistry
//! - **Extended I/O**: OSC protocol, plugin wrappers, Web Audio integration
//!
//! ## Mathematical Foundations
//!
//! Layer 1 implements **Arrow** semantics from category theory:
//!
//! ```text
//! chain:    (A → B) → (B → C) → (A → C)         // Sequential: f >>> g
//! parallel: (A → B) → (C → D) → ((A,C) → (B,D)) // Parallel:   f *** g
//! fanout:   (A → B) → (A → C) → (A → (B,C))     // Split:      f &&& g
//! first:    (A → B) → ((A,C) → (B,C))           // First element only
//! ```
//!
//! These combinators satisfy the Arrow laws, ensuring predictable composition.
//!
//! ## Module Documentation
//!
//! - [`combinator`] - Layer 1: Type-safe Arrow combinators
//! - [`port`] - Layer 2: Signal types and port definitions
//! - [`graph`] - Layer 3: Runtime patch graph
//! - [`modules`] - Core DSP modules (VCO, VCF, ADSR, etc.)
//! - [`analog`] - Analog modeling (saturation, drift, noise)
//! - [`polyphony`] - Voice allocation and management
//! - [`simd`] - Block processing and SIMD optimization
//! - [`serialize`] - Patch serialization to JSON
//! - [`mdk`] - Module Development Kit for custom modules

pub mod analog;
pub mod combinator;
pub mod extended_io;
pub mod graph;
pub mod io;
pub mod mdk;
pub mod modules;
pub mod polyphony;
pub mod port;
pub mod presets;
pub mod serialize;
pub mod simd;
pub mod visual;

/// Prelude module for convenient imports
pub mod prelude {
    // Layer 1: Combinators
    pub use crate::combinator::{
        Chain, Constant, Contramap, Fanout, Feedback, First, Identity, Map, Merge, Module,
        ModuleExt, Parallel, Second, Split, Swap,
    };

    // Layer 2: Port System
    pub use crate::port::{
        BlockPortValues, GraphModule, ModulatedParam, ParamDef, ParamId, ParamRange, PortDef,
        PortId, PortSpec, PortValues, SignalKind,
    };

    // Layer 3: Patch Graph
    pub use crate::graph::{
        Cable, CableId, CompatibilityResult, NodeHandle, NodeId, Patch, PatchError, PortRef,
        ValidationMode,
    };

    // Core DSP Modules
    pub use crate::modules::{
        Adsr, Attenuverter, Clock, Lfo, Mixer, Multiple, NoiseGenerator, Offset, Quantizer,
        SampleAndHold, Scale, SlewLimiter, StepSequencer, StereoOutput, Svf, UnitDelay, Vca, Vco,
    };

    // Phase 2 Modules
    pub use crate::modules::{
        BernoulliGate, Comparator, Crossfader, LogicAnd, LogicNot, LogicOr, LogicXor, Max, Min,
        PrecisionAdder, Rectifier, RingModulator, VcSwitch,
    };

    // Phase 3 Modules
    pub use crate::modules::{Crosstalk, DiodeLadderFilter, GroundLoop};

    // Analog Modeling
    pub use crate::analog::{noise, saturation, AnalogVco, ComponentModel, ThermalModel};

    // Phase 3: Enhanced Analog Modeling
    pub use crate::analog::{HighFrequencyRolloff, VoctTrackingModel};

    // External I/O
    pub use crate::io::{AtomicF64, ExternalInput, MidiState};

    // Serialization
    pub use crate::serialize::{CableDef, ModuleDef, ModuleMetadata, ModuleRegistry, PatchDef};

    // Phase 4: Polyphony Support
    pub use crate::polyphony::{
        AllocationMode, PolyPatch, UnisonConfig, Voice, VoiceAllocator, VoiceInput, VoiceMixer,
        VoiceState,
    };

    // Phase 4: SIMD and Block Processing
    pub use crate::simd::{
        AudioBlock, BlockProcessor, LazyBlock, LazySignal, ProcessContext, RingBuffer, StereoBlock,
        DEFAULT_BLOCK_SIZE, SIMD_BLOCK_SIZE,
    };

    // Phase 4: Extended I/O
    pub use crate::extended_io::{
        AudioBusConfig, OscBinding, OscInput, OscMessage, OscPattern, OscReceiver, OscValue,
        PluginCategory, PluginInfo, PluginParameter, PluginWrapper, WebAudioConfig,
        WebAudioProcessor, WebAudioWorklet,
    };

    // Phase 5: Module Development Kit
    pub use crate::mdk::{
        AudioAnalysis, DocFormat, DocGenerator, ModuleCategory, ModulePresets, ModuleTemplate,
        ModuleTestHarness, PortTemplate, StateFieldTemplate, TestResult, TestSuiteResult,
    };

    // Phase 5: Preset Library
    pub use crate::presets::{
        ClassicPresets, PresetCategory, PresetInfo, PresetLibrary, SoundDesignPresets,
        TutorialPresets,
    };

    // Phase 5: Visual Tools
    pub use crate::visual::{
        AutomationData, AutomationPoint, AutomationRecorder, AutomationTrack, DotExporter,
        DotStyle, LevelMeter, Scope, SpectrumAnalyzer, TriggerMode,
    };
}

// Re-export key types at crate root for convenience
pub use prelude::*;
