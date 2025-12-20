//! # Quiver: Modular Audio Synthesis Library
//!
//! `quiver` is a Rust library for building modular audio synthesis systems using a hybrid
//! architecture that combines type-safe Arrow-style combinators for DSP construction with
//! a flexible graph-based patching system for arbitrary signal routing.
//!
//! ## Architecture
//!
//! The library is organized in three layers:
//!
//! - **Layer 1: Typed Combinators** - Arrow-style DSP composition with compile-time type checking
//! - **Layer 2: Port System** - Signal conventions, port definitions, and type-erased graph interface
//! - **Layer 3: Patch Graph** - Runtime-configurable topology with arbitrary signal routing
//!
//! ## Phase 4 Features
//!
//! - **Polyphony Support** - Voice allocation, per-voice modules, unison/spread
//! - **Performance Optimization** - SIMD vectorization, block processing, lazy evaluation
//! - **Extended I/O** - OSC protocol, plugin wrapper infrastructure, Web Audio interface
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use quiver::prelude::*;
//!
//! // Create a patch at 44.1kHz sample rate
//! let mut patch = Patch::new(44100.0);
//!
//! // Add modules
//! let vco = patch.add("vco", Vco::new(44100.0));
//! let vcf = patch.add("vcf", Svf::new(44100.0));
//! let vca = patch.add("vca", Vca::new());
//! let output = patch.add("output", StereoOutput::new());
//!
//! // Connect them
//! patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
//! patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
//! patch.connect(vca.out("out"), output.in_("left")).unwrap();
//!
//! // Compile and run
//! patch.set_output(output.id());
//! patch.compile().unwrap();
//!
//! // Process audio
//! let (left, right) = patch.tick();
//! ```

pub mod analog;
pub mod combinator;
pub mod extended_io;
pub mod graph;
pub mod io;
pub mod mdk;
pub mod modules;
pub mod polyphony;
pub mod port;
pub mod serialize;
pub mod simd;

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
        ModuleCategory, ModulePresets, ModuleTemplate, PortTemplate, StateFieldTemplate,
    };
}

// Re-export key types at crate root for convenience
pub use prelude::*;
