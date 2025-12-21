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
//! - `std` (default): Enables standard library support, including threading, I/O,
//!   serialization, and random number generation.
//! - `simd`: Enables SIMD vectorization for block processing.
//!
//! Without the `std` feature, the library operates in `no_std` mode with `alloc`,
//! suitable for embedded systems and WebAssembly targets.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod analog;
pub mod combinator;
pub mod graph;
pub mod modules;
pub mod polyphony;
pub mod port;
pub mod rng;
pub mod simd;

// Std-only modules (require threading, I/O, or full serialization)
#[cfg(feature = "std")]
pub mod extended_io;
#[cfg(feature = "std")]
pub mod io;
#[cfg(feature = "std")]
pub mod mdk;
#[cfg(feature = "std")]
pub mod presets;
#[cfg(feature = "std")]
pub mod serialize;
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
    // Std-only exports
    // ========================================================================

    // External I/O (requires std for Arc/atomics)
    #[cfg(feature = "std")]
    pub use crate::io::{AtomicF64, ExternalInput, MidiState};

    // Serialization (requires std for serde_json)
    #[cfg(feature = "std")]
    pub use crate::serialize::{CableDef, ModuleDef, ModuleMetadata, ModuleRegistry, PatchDef};

    // Phase 4: Extended I/O (requires std for Arc/atomics)
    #[cfg(feature = "std")]
    pub use crate::extended_io::{
        AudioBusConfig, OscBinding, OscInput, OscMessage, OscPattern, OscReceiver, OscValue,
        PluginCategory, PluginInfo, PluginParameter, PluginWrapper, WebAudioConfig,
        WebAudioProcessor, WebAudioWorklet,
    };

    // Phase 5: Module Development Kit (requires std)
    #[cfg(feature = "std")]
    pub use crate::mdk::{
        AudioAnalysis, DocFormat, DocGenerator, ModuleCategory, ModulePresets, ModuleTemplate,
        ModuleTestHarness, PortTemplate, StateFieldTemplate, TestResult, TestSuiteResult,
    };

    // Phase 5: Preset Library (requires std)
    #[cfg(feature = "std")]
    pub use crate::presets::{
        ClassicPresets, PresetCategory, PresetInfo, PresetLibrary, SoundDesignPresets,
        TutorialPresets,
    };

    // Phase 5: Visual Tools (requires std)
    #[cfg(feature = "std")]
    pub use crate::visual::{
        AutomationData, AutomationPoint, AutomationRecorder, AutomationTrack, DotExporter,
        DotStyle, LevelMeter, Scope, SpectrumAnalyzer, TriggerMode,
    };
}

// Re-export key types at crate root for convenience
pub use prelude::*;
