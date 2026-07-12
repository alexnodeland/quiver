//! # Quiver: Modular Audio Synthesis Library
//!
//! > *"A quiver is a directed graph—nodes connected by arrows. In audio, our nodes are
//! > modules, our arrows are patch cables, and signal flows through their composition."*
//!
//! `quiver` is a Rust library for building modular audio synthesis systems. It combines
//! the mathematical elegance of category theory with the tactile joy of patching a
//! hardware modular synthesizer.
//!
//! ## Quick Start
//!
//! ```
//! use quiver::prelude::*;
//!
//! // Build a patch at CD-quality sample rate.
//! let sr = 44_100.0;
//! let mut patch = Patch::new(sr);
//!
//! // Add modules — each `add` returns a `NodeHandle` used to reference its ports.
//! let vco = patch.add("vco", Vco::new(sr));
//! let vcf = patch.add("vcf", Svf::new(sr));
//! let vca = patch.add("vca", Vca::new());
//! let env = patch.add("env", Adsr::new(sr));
//! let out = patch.add("out", StereoOutput::new());
//!
//! // Patch cables: VCO -> VCF -> VCA -> stereo out.
//! patch.connect(vco.out("saw"), vcf.in_("in")).unwrap();
//! patch.connect(vcf.out("lp"), vca.in_("in")).unwrap();
//! patch.connect(vca.out("out"), out.in_("left")).unwrap();
//! patch.connect(vca.out("out"), out.in_("right")).unwrap();
//!
//! // The envelope shapes both the filter cutoff and the amplitude.
//! patch.connect(env.out("env"), vcf.in_("cutoff")).unwrap();
//! patch.connect(env.out("env"), vca.in_("cv")).unwrap();
//!
//! // Choose the output module, then compile before processing.
//! patch.set_output(out.id());
//! patch.compile().unwrap();
//!
//! // Advance the patch by one sample of stereo audio.
//! let (_left, _right) = patch.tick();
//! ```
//!
//! ## Feature Flags
//!
//! - `std` (default): Full standard library support including OSC, plugin wrappers,
//!   visualization tools, and module development kit. Implies `alloc`.
//! - `alloc`: Enables serialization (JSON save/load), presets, and basic I/O modules
//!   for `no_std` environments with heap allocation (e.g., WASM).
//! - `simd`: Enables SIMD vectorization for block processing (works with any tier).
//! - `wasm`: WebAssembly bindings via `wasm-bindgen` and TypeScript types via `tsify`
//!   (implies `alloc`).
//!
//! Without any features, the library operates in `no_std` mode with `alloc`,
//! providing core DSP modules for embedded systems and WebAssembly targets.

#![cfg_attr(not(feature = "std"), no_std)]
// Enables the (nightly-only) `doc(cfg(...))` feature-badge attribute, but only
// when building under the `docsrs` cfg that docs.rs sets (and that this
// crate's own `#[cfg_attr(docsrs, doc(cfg(feature = "...")))]` annotations key
// off of, see [package.metadata.docs.rs] in Cargo.toml). On an ordinary stable
// build this line expands to nothing, so it does not require nightly locally.
#![cfg_attr(docsrs, feature(doc_cfg))]

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
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
pub mod introspection;
#[cfg(feature = "alloc")]
mod introspection_impls; // ModuleIntrospection implementations for all modules
#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
pub mod io;
#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
pub mod observer;
#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
pub mod presets;
#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
pub mod scala;
#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
pub mod serialize;

// Std-only offline rendering (WAV export).
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod render;

// Std-only modules (require full std for network, plugins, etc.)
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod extended_io;
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod mdk;
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod visual;

// WASM bindings (requires wasm feature)
#[cfg(feature = "wasm")]
#[cfg_attr(docsrs, doc(cfg(feature = "wasm")))]
pub mod wasm;

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
        Cable, CableId, CompatibilityResult, NodeHandle, NodeId, Patch, PatchError, PatchMeta,
        PortRef, ValidationMode,
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

    // Phase 4 Modules: Advanced DSP
    pub use crate::modules::{
        ArpPattern, Arpeggiator, ChordMemory, ChordType, FormantOsc, Granular, ParametricEq,
        PitchShifter, Reverb, Vocoder, Wavetable, WavetableType,
    };

    // Remediation wave modules: sample playback (Q142), opt-in oversampling for
    // nonlinear stages (Q143), mid/side utilities (Q150), sidechain ducker (Q148),
    // and the canonical `Wavefolder` export (Q149).
    pub use crate::modules::{
        Ducker, MidSideDecode, MidSideEncode, Oversample, Oversampler, SamplePlayer, Wavefolder,
    };

    // Analog Modeling
    pub use crate::analog::{noise, saturation, AnalogVco, ComponentModel, ThermalModel};

    // Phase 3: Enhanced Analog Modeling
    pub use crate::analog::{HighFrequencyRolloff, VoctTrackingModel};

    // Phase 4: Polyphony Support
    pub use crate::polyphony::{
        AllocationMode, PolyPatch, UnisonConfig, Voice, VoiceAllocator, VoiceControl, VoiceInput,
        VoiceMixer, VoiceState,
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
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
    pub use crate::io::{AtomicF64, ExternalInput, ExternalOutput, MidiState};

    // Introspection API (GUI parameter discovery)
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
    pub use crate::introspection::{
        ControlType, ModuleIntrospection, ParamCurve, ParamInfo, ValueFormat,
    };

    // Real-Time State Bridge (GUI live value streaming)
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
    pub use crate::observer::{
        calculate_peak_db, calculate_rms_db, GateDetector, LevelMeterState, ObservableValue,
        ObserverConfig, StateObserver, SubscriptionTarget,
    };

    // Serialization (works with alloc via serde_json alloc feature)
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
    pub use crate::serialize::{
        CableDef, CatalogResponse, ModuleCatalogEntry, ModuleDef, ModuleMetadata, ModuleRegistry,
        PatchDef, PortSummary, ValidationError, ValidationResult, CURRENT_PATCH_VERSION,
    };

    // Preset Library (works with alloc - just data structures)
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
    pub use crate::presets::{
        ClassicPresets, PresetCategory, PresetInfo, PresetLibrary, SoundDesignPresets,
        TutorialPresets,
    };

    // Scala (.scl) microtuning parser (Q146)
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
    pub use crate::scala::{ScalaError, ScalaScale};

    // ========================================================================
    // Std-only exports (require full std)
    // ========================================================================

    // Extended I/O (requires std for network, plugins, etc.)
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    pub use crate::extended_io::{
        AudioBusConfig, OscBinding, OscInput, OscMessage, OscPattern, OscReceiver, OscValue,
        PluginCategory, PluginInfo, PluginParameter, PluginWrapper, WebAudioConfig,
        WebAudioProcessor, WebAudioWorklet,
    };

    // Module Development Kit (requires std)
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    pub use crate::mdk::{
        AudioAnalysis, DocFormat, DocGenerator, ModuleCategory, ModulePresets, ModuleTemplate,
        ModuleTestHarness, PortTemplate, StateFieldTemplate, TestResult, TestSuiteResult,
    };

    // Visual Tools (requires std)
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    pub use crate::visual::{
        AutomationData, AutomationPoint, AutomationRecorder, AutomationTrack, DotExporter,
        DotStyle, LevelMeter, Scope, SpectrumAnalyzer, TriggerMode,
    };

    // Offline rendering / WAV export (requires std) (Q145)
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    pub use crate::render::{render, render_to_wav};

    // WASM bindings (requires wasm feature)
    #[cfg(feature = "wasm")]
    #[cfg_attr(docsrs, doc(cfg(feature = "wasm")))]
    pub use crate::wasm::{QuiverEngine, QuiverError};
}

// Re-export the curated [`prelude`] at the crate root for convenience, so `use quiver::*`
// (or `use quiver::Vco`) works for quick starts and examples — a common pattern for
// batteries-included crates.
//
// This is intentional and additive: it does not hide the granular paths. If you prefer
// tight imports and want to avoid pulling ~150 names into scope, keep importing directly
// from the submodules instead — e.g. `use quiver::modules::Vco;`,
// `use quiver::graph::Patch;`, `use quiver::port::SignalKind;`. Both styles remain
// available; pick per call site.
pub use prelude::*;
