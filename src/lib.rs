//! # Quiver: Modular Audio Synthesis Library
//!
//! > *"A quiver is a directed graph—nodes connected by arrows. In audio, our nodes are
//! > modules, our arrows are patch cables, and signal flows through their composition."*
//!
//! `quiver` is a Rust library for building modular audio synthesis systems. It combines
//! the mathematical elegance of category theory with the tactile joy of patching a
//! hardware modular synthesizer.
//!
//! ## Feature Flags
//!
//! - `std` (default): Full standard library support including OSC, plugin wrappers,
//!   visualization tools, and module development kit. Implies `alloc`.
//! - `alloc`: Enables serialization (JSON save/load), presets, and basic I/O modules
//!   for `no_std` environments with heap allocation (e.g., WASM).
//! - `simd`: Enables SIMD vectorization for block processing (works with any tier).
//!
//! Without any features, the library operates in `no_std` mode with `alloc`,
//! providing core DSP modules for embedded systems and WebAssembly targets.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

// Conditional collection types: HashMap in std mode, BTreeMap in no_std
#[cfg(feature = "std")]
pub(crate) type StdMap<K, V> = std::collections::HashMap<K, V>;
#[cfg(not(feature = "std"))]
pub(crate) type StdMap<K, V> = alloc::collections::BTreeMap<K, V>;

pub mod analog;
pub mod combinator;
pub mod graph;
pub mod modules;
pub mod polyphony;
pub mod port;
pub mod rng;
pub mod simd;

// Alloc-tier modules (work with no_std + alloc)
#[cfg(feature = "alloc")]
pub mod introspection;
#[cfg(feature = "alloc")]
pub mod io;
#[cfg(feature = "alloc")]
pub mod presets;
#[cfg(feature = "alloc")]
pub mod serialize;

// Std-only modules (require full std for network, plugins, etc.)
#[cfg(feature = "std")]
pub mod extended_io;
#[cfg(feature = "std")]
pub mod mdk;
#[cfg(feature = "std")]
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
        ports_compatible, BlockPortValues, Compatibility, GraphModule, ModulatedParam, ParamDef,
        ParamId, ParamRange, PortDef, PortId, PortInfo, PortSpec, PortValues, SignalColors,
        SignalKind,
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

    // RNG (no_std compatible)
    pub use crate::rng::{Rng, SeedableRng};

    // ========================================================================
    // Alloc-tier exports (work with no_std + alloc)
    // ========================================================================

    // External I/O (works with alloc via core::sync::atomic + alloc::sync::Arc)
    #[cfg(feature = "alloc")]
    pub use crate::io::{AtomicF64, ExternalInput, ExternalOutput, MidiState};

    // Introspection API (GUI parameter discovery)
    #[cfg(feature = "alloc")]
    pub use crate::introspection::{
        ControlType, ModuleIntrospection, ParamCurve, ParamInfo, ValueFormat,
    };

    // Serialization (works with alloc via serde_json alloc feature)
    #[cfg(feature = "alloc")]
    pub use crate::serialize::{
        CableDef, CatalogResponse, ModuleCatalogEntry, ModuleDef, ModuleMetadata, ModuleRegistry,
        PatchDef, PortSummary, ValidationError, ValidationResult,
    };

    // Preset Library (works with alloc - just data structures)
    #[cfg(feature = "alloc")]
    pub use crate::presets::{
        ClassicPresets, PresetCategory, PresetInfo, PresetLibrary, SoundDesignPresets,
        TutorialPresets,
    };

    // ========================================================================
    // Std-only exports (require full std)
    // ========================================================================

    // Extended I/O (requires std for network, plugins, etc.)
    #[cfg(feature = "std")]
    pub use crate::extended_io::{
        AudioBusConfig, OscBinding, OscInput, OscMessage, OscPattern, OscReceiver, OscValue,
        PluginCategory, PluginInfo, PluginParameter, PluginWrapper, WebAudioConfig,
        WebAudioProcessor, WebAudioWorklet,
    };

    // Module Development Kit (requires std)
    #[cfg(feature = "std")]
    pub use crate::mdk::{
        AudioAnalysis, DocFormat, DocGenerator, ModuleCategory, ModulePresets, ModuleTemplate,
        ModuleTestHarness, PortTemplate, StateFieldTemplate, TestResult, TestSuiteResult,
    };

    // Visual Tools (requires std)
    #[cfg(feature = "std")]
    pub use crate::visual::{
        AutomationData, AutomationPoint, AutomationRecorder, AutomationTrack, DotExporter,
        DotStyle, LevelMeter, Scope, SpectrumAnalyzer, TriggerMode,
    };
}

// Re-export key types at crate root for convenience
pub use prelude::*;
