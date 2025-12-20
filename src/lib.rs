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

pub mod combinator;
pub mod port;
pub mod graph;
pub mod modules;
pub mod analog;
pub mod io;
pub mod serialize;

/// Prelude module for convenient imports
pub mod prelude {
    // Layer 1: Combinators
    pub use crate::combinator::{
        Module, ModuleExt, Chain, Parallel, Fanout, Feedback, Map, Contramap,
        Split, Merge, Swap, First, Second, Identity, Constant,
    };

    // Layer 2: Port System
    pub use crate::port::{
        SignalKind, PortId, PortDef, PortSpec, PortValues, BlockPortValues,
        ModulatedParam, ParamRange, ParamId, ParamDef, GraphModule,
    };

    // Layer 3: Patch Graph
    pub use crate::graph::{
        Patch, NodeId, CableId, PortRef, Cable, NodeHandle, PatchError,
    };

    // Core DSP Modules
    pub use crate::modules::{
        Vco, Lfo, Svf, Adsr, Vca, Mixer, Offset, UnitDelay,
        NoiseGenerator, StepSequencer, StereoOutput,
    };

    // Analog Modeling
    pub use crate::analog::{
        saturation, ComponentModel, ThermalModel, noise, AnalogVco,
    };

    // External I/O
    pub use crate::io::{
        AtomicF64, ExternalInput, MidiState,
    };

    // Serialization
    pub use crate::serialize::{
        PatchDef, ModuleDef, CableDef, ModuleRegistry, ModuleMetadata,
    };
}

// Re-export key types at crate root for convenience
pub use prelude::*;
