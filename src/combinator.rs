//! # Layer 1: Typed Module Combinators
//!
//! This module provides **Arrow-style combinators** for composing signal processing
//! modules with *structural* (arity/shape) compile-time type checking. These
//! combinators enable functional composition of DSP chains that compile down to tight,
//! inlinable loops with zero runtime overhead.
//!
//! ## What "type-safe" means here (and what it does not)
//!
//! The combinators give **structural** type safety: the compiler guarantees that the
//! *shape* of a signal matches — a mono `f64` connects to a mono `f64`, a stereo
//! `(f64, f64)` to a stereo `(f64, f64)`, and tuple arities line up. This is real and
//! useful: `.then`, `.parallel`, and `.fanout` cannot be mis-wired shape-wise.
//!
//! It is **not** *semantic* signal-kind safety. [`Module::In`] / [`Module::Out`] are bare
//! Rust types (in practice `f64`), so an `Audio` output and a `VoltPerOctave` pitch input
//! are the *same* type and will chain silently. Semantic [`SignalKind`] checking (Audio vs
//! CV vs V/Oct vs Gate) is enforced by the **graph layer** (Layer 2 / Layer 3, see
//! [`crate::port`] and [`crate::graph`]), not by these combinators.
//!
//! [`SignalKind`]: crate::port::SignalKind
//!
//! ## Category Theory Background
//!
//! In category theory, an **Arrow** is a generalization of functions that allows for
//! composition while carrying additional structure (like state). The combinators here
//! implement the Arrow interface. The left column below is the **conceptual
//! Haskell/`Control.Arrow` notation** used throughout the functional-programming
//! literature — it is **not** Rust syntax. `>>>`, `***`, and `&&&` are *not* Rust
//! operators, and this crate deliberately ships **no operator overloads** for them.
//! Use the method in the right column instead:
//!
//! | Conceptual (Haskell) | Real Quiver API      | Meaning                         |
//! |----------------------|----------------------|---------------------------------|
//! | `arr f`              | `arr(f)`             | Lift a pure `Fn` into a module  |
//! | `f >>> g`            | `f.then(g)`          | Sequential composition          |
//! | `first f`            | `f.first()`          | Apply to first tuple element    |
//! | `f *** g`            | `f.parallel(g)`      | Independent parallel processing |
//! | `f &&& g`            | `f.fanout(g)`        | Split one input to two          |
//!
//! ```text
//! arr:     (a -> b) -> Arrow a b                         // Lift pure function
//! (>>>):   Arrow a b -> Arrow b c -> Arrow a c           // Sequential composition
//! first:   Arrow a b -> Arrow (a,c) (b,c)                // Apply to first element
//! (***):   Arrow a b -> Arrow c d -> Arrow (a,c) (b,d)   // Parallel
//! (&&&):   Arrow a b -> Arrow a c -> Arrow a (b,c)       // Fanout
//! ```
//!
//! The real methods live on [`ModuleExt`]; the `arr` primitive is the free function
//! [`arr`].
//!
//! ## Arrow Laws
//!
//! These combinators satisfy the Arrow laws, ensuring predictable behavior. They are
//! checked by the `arrow_law_*` tests in this module (using the real `.then`/`.first`
//! API over deterministic input sequences against stateful test modules):
//!
//! - **Identity**: `id.then(f)` behaves as `f` (and `f.then(id)` as `f`)
//! - **Associativity**: `(f.then(g)).then(h)` behaves as `f.then(g.then(h))`
//! - **First distributes**: `f.then(g).first()` behaves as `f.first().then(g.first())`
//!
//! ## Zero-Cost Abstraction
//!
//! Due to Rust's monomorphization, combinator chains compile to the same code as
//! hand-written loops:
//!
//! ```text
//! // This combinator chain...
//! let synth = vco.then(vcf).then(vca);
//!
//! // ...compiles to essentially:
//! fn tick(&mut self) -> f64 {
//!     self.vca.tick(self.vcf.tick(self.vco.tick(())))
//! }
//! ```
//!
//! ## Example: Composing with the real API
//!
//! ```rust
//! use quiver::combinator::{arr, Module, ModuleExt};
//!
//! // `.then` is sequential composition (conceptually `>>>`).
//! let mut chain = arr(|x: f64| x + 1.0).then(arr(|x: f64| x * 2.0));
//! assert_eq!(chain.tick(3.0), 8.0); // (3 + 1) * 2
//!
//! // `.parallel` (conceptually `***`) processes two independent signals.
//! let mut stereo = arr(|l: f64| l * 0.5).parallel(arr(|r: f64| r * 0.25));
//! assert_eq!(stereo.tick((2.0, 4.0)), (1.0, 1.0));
//!
//! // `.fanout` (conceptually `&&&`) sends one input to two processors.
//! let mut split = arr(|x: f64| x + 1.0).fanout(arr(|x: f64| x - 1.0));
//! assert_eq!(split.tick(5.0), (6.0, 4.0));
//! ```
//!
//! ## Bridging to the Patch Graph (Layer 3)
//!
//! The combinator [`Module`] trait and the engine's [`GraphModule`] trait are distinct
//! worlds: shipped DSP modules (`Vco`, `Svf`, `Vca`, …) implement [`GraphModule`], so they
//! cannot be dropped into `.then(...)` directly. Two adapters bridge them:
//!
//! - [`GraphModuleAdapter`] wraps any [`GraphModule`] (choosing one input and one output
//!   port) and exposes it as a `Module<In = f64, Out = f64>`, so real modules *can* be
//!   composed with `.then`/`.parallel`/`.fanout`.
//! - [`ModuleGraphAdapter`] wraps any `Module<In = f64, Out = f64>` as a one-in/one-out
//!   [`GraphModule`], so a combinator chain can be `patch.add(...)`-ed into a [`Patch`].
//!
//! ```rust
//! use quiver::combinator::{GraphModuleAdapter, Module, ModuleExt};
//! use quiver::modules::{Svf, Vco};
//!
//! // Drive the Vco's `voct` input (port 0) and take its `saw` output (port 12).
//! let vco = GraphModuleAdapter::new(Vco::new(44_100.0), 0, 12);
//! // Svf: pick its first audio in/out automatically (`in` -> `lp`).
//! let svf = GraphModuleAdapter::from_audio_ports(Svf::new(44_100.0)).unwrap();
//!
//! let mut voice = vco.then(svf);
//! let mut peak = 0.0_f64;
//! for _ in 0..256 {
//!     peak = peak.max(voice.tick(0.0).abs()); // 0 V/oct = middle C
//! }
//! assert!(peak > 0.0, "a real Vco -> Svf combinator chain should make sound");
//! ```
//!
//! [`Module`]: crate::combinator::Module
//! [`GraphModule`]: crate::port::GraphModule
//! [`GraphModuleAdapter`]: crate::combinator::GraphModuleAdapter
//! [`ModuleGraphAdapter`]: crate::combinator::ModuleGraphAdapter
//! [`Patch`]: crate::graph::Patch

use crate::port::{GraphModule, PortDef, PortId, PortSpec, PortValues, SignalKind};
use alloc::string::String;
use alloc::vec;
use core::marker::PhantomData;

/// A signal processing module with typed input and output.
///
/// This is the fundamental abstraction for DSP processing in Quiver. Modules are
/// **stateful processors** that transform input samples to output samples. The
/// associated types `In` and `Out` enable compile-time verification of signal *shape*
/// (arity): a `Module<Out = f64>` only chains into a `Module<In = f64>`, a stereo
/// `(f64, f64)` only into a `(f64, f64)`, and so on. This is *structural* type safety;
/// it does **not** check semantic [`SignalKind`](crate::port::SignalKind) (Audio vs CV
/// vs V/Oct) — both are `f64` here. Signal-kind compatibility is validated by the graph
/// layer ([`crate::port`] / [`crate::graph`]).
///
/// # Mathematical Model
///
/// A module represents a morphism in the category of signals:
///
/// ```text
/// M : In → Out
/// ```
///
/// The `tick` method computes one step of this transformation, potentially updating
/// internal state (like oscillator phase or filter memory).
///
/// # Implementing Module
///
/// ```
/// use quiver::prelude::*;
///
/// struct Amplifier { gain: f64 }
///
/// impl Module for Amplifier {
///     type In = f64;
///     type Out = f64;
///
///     fn tick(&mut self, input: f64) -> f64 {
///         input * self.gain
///     }
///
///     fn reset(&mut self) {
///         // Amplifier is stateless, nothing to reset
///     }
/// }
///
/// let mut amp = Amplifier { gain: 2.0 };
/// assert_eq!(amp.tick(0.5), 1.0);
/// ```
///
/// # Thread Safety
///
/// All modules must be `Send` to allow audio processing on dedicated threads.
pub trait Module: Send {
    /// Input signal type (e.g., `f64` for mono, `(f64, f64)` for stereo)
    type In;
    /// Output signal type
    type Out;

    /// Process a single sample, advancing internal state by one time step.
    ///
    /// This is the core DSP function. For a VCO, this updates phase and outputs
    /// a waveform sample. For a filter, this processes through the filter stages.
    fn tick(&mut self, input: Self::In) -> Self::Out;

    /// Process a block of samples for efficiency.
    ///
    /// Override this method for SIMD optimization or when block processing is
    /// more efficient than sample-by-sample. The default implementation simply
    /// calls `tick` in a loop.
    fn process(&mut self, input: &[Self::In], output: &mut [Self::Out])
    where
        Self::In: Clone,
    {
        for (i, o) in input.iter().zip(output.iter_mut()) {
            *o = self.tick(i.clone());
        }
    }

    /// Reset internal state to initial conditions.
    ///
    /// Called when starting a new note, reinitializing the synth, etc.
    /// For oscillators, this typically resets phase. For filters, clears memory.
    fn reset(&mut self);

    /// Notify module of sample rate changes.
    ///
    /// Modules with time-dependent behavior (filters, delays, envelopes) should
    /// recalculate coefficients here.
    fn set_sample_rate(&mut self, _sample_rate: f64) {}
}

/// Extension trait providing combinator methods for all modules
pub trait ModuleExt: Module + Sized {
    /// Chain this module with another (sequential composition: `>>>`)
    fn then<M: Module<In = Self::Out>>(self, next: M) -> Chain<Self, M> {
        Chain {
            first: self,
            second: next,
        }
    }

    /// Run two modules in parallel (`***`)
    fn parallel<M: Module>(self, other: M) -> Parallel<Self, M> {
        Parallel {
            left: self,
            right: other,
        }
    }

    /// Split input to two parallel processors (`&&&`)
    fn fanout<M: Module<In = Self::In>>(self, other: M) -> Fanout<Self, M>
    where
        Self::In: Clone,
    {
        Fanout {
            left: self,
            right: other,
        }
    }

    /// Transform output with a pure function
    fn map<F, U>(self, f: F) -> Map<Self, F>
    where
        F: Fn(Self::Out) -> U,
    {
        Map { module: self, f }
    }

    /// Transform input with a pure function
    fn contramap<F, U>(self, f: F) -> Contramap<Self, F, U>
    where
        F: Fn(U) -> Self::In,
    {
        Contramap {
            module: self,
            f,
            _phantom: PhantomData,
        }
    }

    /// Create a feedback loop with a single-sample unit delay.
    ///
    /// The `combine` closure is called as `combine(external_input, previous_output)`:
    ///
    /// - the **first** argument is this tick's external input,
    /// - the **second** argument is the module's output from the *previous* tick,
    ///   delayed by exactly one sample for causality.
    ///
    /// On the very first tick (and after [`reset`](Module::reset)), the previous-output
    /// argument is `Self::Out::default()` (i.e. `0.0` for `f64`). The combined value is
    /// fed to the wrapped module, and its output is both returned and stored as the next
    /// tick's feedback signal.
    ///
    /// # Example: a one-pole low-pass via feedback
    ///
    /// ```
    /// use quiver::combinator::{Identity, Module, ModuleExt};
    ///
    /// let coeff = 0.5;
    /// // y[n] = input * (1 - coeff) + previous_output * coeff
    /// let mut one_pole = Identity::<f64>::new()
    ///     .feedback(move |input, previous| input * (1.0 - coeff) + previous * coeff);
    ///
    /// // First tick: previous_output defaults to 0.0.
    /// assert!((one_pole.tick(1.0) - 0.5).abs() < 1e-12); // 1*0.5 + 0*0.5
    /// // Second tick: previous_output is now 0.5.
    /// assert!((one_pole.tick(1.0) - 0.75).abs() < 1e-12); // 1*0.5 + 0.5*0.5
    /// ```
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

    /// Apply this module only to the first element of a tuple
    fn first<C>(self) -> First<Self, C> {
        First {
            module: self,
            _phantom: PhantomData,
        }
    }

    /// Apply this module only to the second element of a tuple
    fn second<C>(self) -> Second<Self, C> {
        Second {
            module: self,
            _phantom: PhantomData,
        }
    }
}

// Blanket implementation for all modules
impl<M: Module> ModuleExt for M {}

/// Sequential composition: processes through first module, then second
pub struct Chain<A, B> {
    pub first: A,
    pub second: B,
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

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.first.set_sample_rate(sample_rate);
        self.second.set_sample_rate(sample_rate);
    }
}

/// Parallel composition: processes two independent signals simultaneously
pub struct Parallel<A, B> {
    pub left: A,
    pub right: B,
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

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.left.set_sample_rate(sample_rate);
        self.right.set_sample_rate(sample_rate);
    }
}

/// Fanout: splits a single input to two parallel processors
pub struct Fanout<A, B> {
    pub left: A,
    pub right: B,
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

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.left.set_sample_rate(sample_rate);
        self.right.set_sample_rate(sample_rate);
    }
}

/// Feedback loop with a mandatory single-sample delay for causality.
///
/// Each tick computes `combine(external_input, previous_output)` and feeds the result to
/// the wrapped module. `previous_output` is the module's output from the previous tick
/// (the `delay_buffer`), which starts at `M::Out::default()` on the first tick and after
/// [`reset`](Module::reset). See [`ModuleExt::feedback`] for the argument-order contract
/// and a runnable example.
pub struct Feedback<M: Module, F> {
    pub module: M,
    pub combine: F,
    /// The previous tick's output, delayed one sample. `Default` on the first tick.
    pub delay_buffer: M::Out,
}

impl<M, F, Combined> Module for Feedback<M, F>
where
    M: Module<In = Combined>,
    F: Fn(M::Out, M::Out) -> Combined + Send,
    M::Out: Default + Clone + Send,
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

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.module.set_sample_rate(sample_rate);
    }
}

/// Transform output with a pure function
pub struct Map<M, F> {
    pub module: M,
    pub f: F,
}

impl<M, F, U> Module for Map<M, F>
where
    M: Module,
    F: Fn(M::Out) -> U + Send,
{
    type In = M::In;
    type Out = U;

    #[inline]
    fn tick(&mut self, input: Self::In) -> Self::Out {
        (self.f)(self.module.tick(input))
    }

    fn reset(&mut self) {
        self.module.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.module.set_sample_rate(sample_rate);
    }
}

/// Transform input with a pure function
pub struct Contramap<M, F, U> {
    pub module: M,
    pub f: F,
    pub _phantom: PhantomData<U>,
}

impl<M, F, U> Module for Contramap<M, F, U>
where
    M: Module,
    F: Fn(U) -> M::In + Send,
    U: Send,
{
    type In = U;
    type Out = M::Out;

    #[inline]
    fn tick(&mut self, input: Self::In) -> Self::Out {
        self.module.tick((self.f)(input))
    }

    fn reset(&mut self) {
        self.module.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.module.set_sample_rate(sample_rate);
    }
}

/// Duplicate a signal
pub struct Split<T> {
    _phantom: PhantomData<T>,
}

impl<T> Split<T> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<T> Default for Split<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Send> Module for Split<T> {
    type In = T;
    type Out = (T, T);

    #[inline]
    fn tick(&mut self, input: Self::In) -> Self::Out {
        (input.clone(), input)
    }

    fn reset(&mut self) {}
}

/// Combine two signals with a function
pub struct Merge<T, F> {
    pub f: F,
    _phantom: PhantomData<T>,
}

impl<T, F> Merge<T, F>
where
    F: Fn(T, T) -> T,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            _phantom: PhantomData,
        }
    }
}

impl<T, F> Module for Merge<T, F>
where
    T: Send,
    F: Fn(T, T) -> T + Send,
{
    type In = (T, T);
    type Out = T;

    #[inline]
    fn tick(&mut self, (a, b): Self::In) -> Self::Out {
        (self.f)(a, b)
    }

    fn reset(&mut self) {}
}

/// Swap tuple elements
pub struct Swap<A, B> {
    _phantom: PhantomData<(A, B)>,
}

impl<A, B> Swap<A, B> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<A, B> Default for Swap<A, B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Send, B: Send> Module for Swap<A, B> {
    type In = (A, B);
    type Out = (B, A);

    #[inline]
    fn tick(&mut self, (a, b): Self::In) -> Self::Out {
        (b, a)
    }

    fn reset(&mut self) {}
}

/// Process first element, pass through second
pub struct First<M, C> {
    pub module: M,
    pub _phantom: PhantomData<C>,
}

impl<M, C> Module for First<M, C>
where
    M: Module,
    C: Send,
{
    type In = (M::In, C);
    type Out = (M::Out, C);

    #[inline]
    fn tick(&mut self, (a, c): Self::In) -> Self::Out {
        (self.module.tick(a), c)
    }

    fn reset(&mut self) {
        self.module.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.module.set_sample_rate(sample_rate);
    }
}

/// Pass through first element, process second
pub struct Second<M, C> {
    pub module: M,
    pub _phantom: PhantomData<C>,
}

impl<M, C> Module for Second<M, C>
where
    M: Module,
    C: Send,
{
    type In = (C, M::In);
    type Out = (C, M::Out);

    #[inline]
    fn tick(&mut self, (c, a): Self::In) -> Self::Out {
        (c, self.module.tick(a))
    }

    fn reset(&mut self) {
        self.module.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.module.set_sample_rate(sample_rate);
    }
}

/// Identity: pass-through module (categorical identity)
pub struct Identity<T> {
    _phantom: PhantomData<T>,
}

impl<T> Identity<T> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<T> Default for Identity<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send> Module for Identity<T> {
    type In = T;
    type Out = T;

    #[inline]
    fn tick(&mut self, input: Self::In) -> Self::Out {
        input
    }

    fn reset(&mut self) {}
}

/// Constant: emit a constant value (ignores input)
pub struct Constant<T> {
    pub value: T,
}

impl<T> Constant<T> {
    pub fn new(value: T) -> Self {
        Self { value }
    }
}

impl<T: Clone + Send> Module for Constant<T> {
    type In = ();
    type Out = T;

    #[inline]
    fn tick(&mut self, _input: Self::In) -> Self::Out {
        self.value.clone()
    }

    fn reset(&mut self) {}
}

/// A stateless module that lifts a pure function into the [`Module`] world.
///
/// This is the Arrow `arr` primitive: `arr : (a -> b) -> Arrow a b`. Unlike
/// [`ModuleExt::map`] (which post-composes a function onto an *existing* module), `arr`
/// builds a standalone module directly from a function, carrying no state. Construct it
/// with the free function [`arr`].
pub struct Arr<F, A> {
    f: F,
    _phantom: PhantomData<A>,
}

/// Lift a pure function into a [`Module`] (the Arrow `arr` primitive).
///
/// `arr(f)` produces a stateless module whose `tick` is exactly `f`. This is the missing
/// Arrow primitive that lets combinator laws be expressed and tested against plain
/// functions.
///
/// # Examples
///
/// ```
/// use quiver::combinator::{arr, Module};
///
/// let mut double = arr(|x: f64| x * 2.0);
/// assert_eq!(double.tick(21.0), 42.0);
/// ```
pub fn arr<F, A, B>(f: F) -> Arr<F, A>
where
    F: Fn(A) -> B,
{
    Arr {
        f,
        _phantom: PhantomData,
    }
}

impl<F, A, B> Module for Arr<F, A>
where
    F: Fn(A) -> B + Send,
    A: Send,
{
    type In = A;
    type Out = B;

    #[inline]
    fn tick(&mut self, input: Self::In) -> Self::Out {
        (self.f)(input)
    }

    fn reset(&mut self) {}
}

/// Adapts a [`GraphModule`] (the engine's multi-port, type-erased module trait) into a
/// single-in / single-out combinator [`Module`], so real DSP modules such as
/// [`Vco`](crate::modules::Vco) or [`Svf`](crate::modules::Svf) can be composed with
/// `.then`, `.parallel`, and `.fanout`.
///
/// One input port and one output port of the wrapped module are chosen at construction
/// (by id, via [`new`](GraphModuleAdapter::new), or automatically from the first audio
/// ports, via [`from_audio_ports`](GraphModuleAdapter::from_audio_ports)). Every other
/// input port keeps its [`PortDef`] default; the wrapped module still reads those on each
/// `tick`, so pulse-width, cutoff, etc. behave as if unpatched in a graph.
pub struct GraphModuleAdapter<G: GraphModule> {
    module: G,
    input_port: PortId,
    output_port: PortId,
    inputs: PortValues,
    outputs: PortValues,
}

impl<G: GraphModule> GraphModuleAdapter<G> {
    /// Wrap `module`, driving input port `input_port` and reading output port
    /// `output_port` on every `tick`.
    pub fn new(module: G, input_port: PortId, output_port: PortId) -> Self {
        let mut inputs = PortValues::new();
        for port in &module.port_spec().inputs {
            inputs.set(port.id, port.default);
        }
        Self {
            module,
            input_port,
            output_port,
            inputs,
            outputs: PortValues::new(),
        }
    }

    /// Wrap `module`, automatically choosing its first [`SignalKind::Audio`] input and
    /// first [`SignalKind::Audio`] output port.
    ///
    /// Returns `None` if the module exposes no audio input or no audio output (e.g. a
    /// `Vco`, whose only inputs are CV/pitch — use [`new`](GraphModuleAdapter::new) with
    /// an explicit port id for those).
    pub fn from_audio_ports(module: G) -> Option<Self> {
        let (input_port, output_port) = {
            let spec = module.port_spec();
            let input_port = spec.inputs.iter().find(|p| p.kind == SignalKind::Audio)?.id;
            let output_port = spec
                .outputs
                .iter()
                .find(|p| p.kind == SignalKind::Audio)?
                .id;
            (input_port, output_port)
        };
        Some(Self::new(module, input_port, output_port))
    }

    /// Borrow the wrapped [`GraphModule`].
    pub fn inner(&self) -> &G {
        &self.module
    }

    /// Consume the adapter, returning the wrapped [`GraphModule`].
    pub fn into_inner(self) -> G {
        self.module
    }
}

impl<G: GraphModule> Module for GraphModuleAdapter<G> {
    type In = f64;
    type Out = f64;

    #[inline]
    fn tick(&mut self, input: Self::In) -> Self::Out {
        self.inputs.set(self.input_port, input);
        self.module.tick(&self.inputs, &mut self.outputs);
        self.outputs.get_or(self.output_port, 0.0)
    }

    fn reset(&mut self) {
        self.module.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.module.set_sample_rate(sample_rate);
    }
}

/// Adapts a single-in / single-out combinator [`Module`] into a [`GraphModule`], so a
/// combinator chain (e.g. `a.then(b).then(c)`) can be added to a
/// [`Patch`](crate::graph::Patch) via `patch.add(...)`.
///
/// The generated [`PortSpec`] has exactly one input port (id `0`) and one output port
/// (id `10`), both [`SignalKind::Audio`], following the crate's input-ids-from-0 /
/// output-ids-from-10 convention.
pub struct ModuleGraphAdapter<M> {
    module: M,
    spec: PortSpec,
}

impl<M> ModuleGraphAdapter<M>
where
    M: Module<In = f64, Out = f64>,
{
    /// Wrap `module` with a one-in (`"in"`, id `0`) / one-out (`"out"`, id `10`) audio
    /// port spec.
    pub fn new(module: M) -> Self {
        Self::with_ports(module, "in", "out")
    }

    /// Wrap `module`, naming its single input (id `0`) and output (id `10`) ports.
    pub fn with_ports(
        module: M,
        input_name: impl Into<String>,
        output_name: impl Into<String>,
    ) -> Self {
        let spec = PortSpec {
            inputs: vec![PortDef::new(0, input_name, SignalKind::Audio)],
            outputs: vec![PortDef::new(10, output_name, SignalKind::Audio)],
        };
        Self { module, spec }
    }
}

impl<M> GraphModule for ModuleGraphAdapter<M>
where
    M: Module<In = f64, Out = f64> + Sync,
{
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let x = inputs.get_or(0, 0.0);
        let y = self.module.tick(x);
        outputs.set(10, y);
    }

    fn reset(&mut self) {
        self.module.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.module.set_sample_rate(sample_rate);
    }

    fn type_id(&self) -> &'static str {
        "combinator_chain"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};

    // Simple test module that multiplies by a constant
    struct Gain {
        factor: f64,
    }

    impl Module for Gain {
        type In = f64;
        type Out = f64;

        fn tick(&mut self, input: Self::In) -> Self::Out {
            input * self.factor
        }

        fn reset(&mut self) {}
    }

    #[test]
    fn test_chain() {
        let mut chain = Gain { factor: 2.0 }.then(Gain { factor: 3.0 });
        assert!((chain.tick(1.0) - 6.0).abs() < 1e-10);
    }

    #[test]
    fn test_parallel() {
        let mut par = Gain { factor: 2.0 }.parallel(Gain { factor: 3.0 });
        let (a, b) = par.tick((1.0, 1.0));
        assert!((a - 2.0).abs() < 1e-10);
        assert!((b - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_fanout() {
        let mut fan = Gain { factor: 2.0 }.fanout(Gain { factor: 3.0 });
        let (a, b) = fan.tick(1.0);
        assert!((a - 2.0).abs() < 1e-10);
        assert!((b - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_map() {
        let mut mapped = Gain { factor: 2.0 }.map(|x| x + 1.0);
        assert!((mapped.tick(1.0) - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_identity() {
        let mut id = Identity::<f64>::new();
        assert!((id.tick(42.0) - 42.0).abs() < 1e-10);
    }

    #[test]
    fn test_constant() {
        let mut c = Constant::new(42.0_f64);
        assert!((c.tick(()) - 42.0).abs() < 1e-10);
    }

    #[test]
    fn test_split() {
        let mut split = Split::<f64>::new();
        let (a, b) = split.tick(5.0);
        assert!((a - 5.0).abs() < 1e-10);
        assert!((b - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_merge() {
        let mut merge = Merge::new(|a: f64, b: f64| a + b);
        assert!((merge.tick((2.0, 3.0)) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_swap() {
        let mut swap = Swap::<i32, f64>::new();
        assert_eq!(swap.tick((1, 2.0)), (2.0, 1));
    }

    // Additional tests for 100% coverage

    // Test module with sample_rate awareness
    struct SampleRateAware {
        sample_rate: f64,
        count: u32,
    }

    impl SampleRateAware {
        fn new() -> Self {
            Self {
                sample_rate: 44100.0,
                count: 0,
            }
        }
    }

    impl Module for SampleRateAware {
        type In = f64;
        type Out = f64;

        fn tick(&mut self, input: Self::In) -> Self::Out {
            self.count += 1;
            input * self.sample_rate / 44100.0
        }

        fn reset(&mut self) {
            self.count = 0;
        }

        fn set_sample_rate(&mut self, sample_rate: f64) {
            self.sample_rate = sample_rate;
        }
    }

    #[test]
    fn test_chain_reset_and_sample_rate() {
        let mut chain = SampleRateAware::new().then(SampleRateAware::new());

        chain.tick(1.0);
        chain.tick(1.0);

        // Reset should reset both modules
        chain.reset();
        assert_eq!(chain.first.count, 0);
        assert_eq!(chain.second.count, 0);

        // Set sample rate should propagate
        chain.set_sample_rate(48000.0);
        assert_eq!(chain.first.sample_rate, 48000.0);
        assert_eq!(chain.second.sample_rate, 48000.0);
    }

    #[test]
    fn test_parallel_reset_and_sample_rate() {
        let mut par = SampleRateAware::new().parallel(SampleRateAware::new());

        par.tick((1.0, 1.0));
        par.tick((1.0, 1.0));

        par.reset();
        par.set_sample_rate(48000.0);

        let result = par.tick((1.0, 1.0));
        assert!(result.0.abs() < 10.0);
    }

    #[test]
    fn test_fanout_reset_and_sample_rate() {
        let mut fan = SampleRateAware::new().fanout(SampleRateAware::new());

        fan.tick(1.0);
        fan.tick(1.0);

        fan.reset();
        fan.set_sample_rate(48000.0);

        let result = fan.tick(1.0);
        assert!(result.0.abs() < 10.0);
    }

    #[test]
    fn test_feedback_reset_and_sample_rate() {
        let feedback_fn = |x: f64, prev: f64| x + prev * 0.5;
        let mut fb = SampleRateAware::new().feedback(feedback_fn);

        for _ in 0..10 {
            fb.tick(1.0);
        }

        fb.reset();
        fb.set_sample_rate(48000.0);
    }

    #[test]
    fn test_map_reset_and_sample_rate() {
        let mut mapped = SampleRateAware::new().map(|x| x + 1.0);

        mapped.tick(1.0);
        mapped.tick(1.0);

        mapped.reset();
        mapped.set_sample_rate(48000.0);

        let result = mapped.tick(1.0);
        assert!(result.abs() < 10.0);
    }

    #[test]
    fn test_contramap() {
        let mut contra = Gain { factor: 2.0 }.contramap(|x: f64| x + 1.0);
        assert!((contra.tick(1.0) - 4.0).abs() < 1e-10); // (1+1) * 2 = 4

        // Test reset and sample_rate
        contra.reset();
        contra.set_sample_rate(48000.0);
    }

    #[test]
    fn test_contramap_reset_and_sample_rate() {
        let mut contra = SampleRateAware::new().contramap(|x: f64| x * 2.0);

        contra.tick(1.0);
        contra.reset();
        contra.set_sample_rate(48000.0);

        let result = contra.tick(1.0);
        assert!(result.abs() < 10.0);
    }

    #[test]
    fn test_first() {
        let mut first = Gain { factor: 2.0 }.first::<i32>();
        let (a, b) = first.tick((3.0, 42));
        assert!((a - 6.0).abs() < 1e-10);
        assert_eq!(b, 42);
    }

    #[test]
    fn test_first_reset_and_sample_rate() {
        let mut first = SampleRateAware::new().first::<i32>();

        first.tick((1.0, 0));
        first.reset();
        first.set_sample_rate(48000.0);

        let (result, _) = first.tick((1.0, 0));
        assert!(result.abs() < 10.0);
    }

    #[test]
    fn test_second() {
        let mut second = Gain { factor: 2.0 }.second::<i32>();
        let (a, b) = second.tick((42, 3.0));
        assert_eq!(a, 42);
        assert!((b - 6.0).abs() < 1e-10);
    }

    #[test]
    fn test_second_reset_and_sample_rate() {
        let mut second = SampleRateAware::new().second::<i32>();

        second.tick((0, 1.0));
        second.reset();
        second.set_sample_rate(48000.0);

        let (_, result) = second.tick((0, 1.0));
        assert!(result.abs() < 10.0);
    }

    #[test]
    fn test_identity_reset() {
        let mut id = Identity::<f64>::new();
        id.reset(); // Should not panic
        assert!((id.tick(42.0) - 42.0).abs() < 1e-10);
    }

    #[test]
    fn test_identity_default() {
        let id: Identity<f64> = Identity::default();
        assert!(std::mem::size_of_val(&id) == 0);
    }

    #[test]
    fn test_constant_reset() {
        let mut c = Constant::new(42.0_f64);
        c.reset(); // Should not panic
        assert!((c.tick(()) - 42.0).abs() < 1e-10);
    }

    #[test]
    fn test_split_reset() {
        let mut split = Split::<f64>::new();
        split.reset(); // Should not panic
    }

    #[test]
    fn test_split_default() {
        let split: Split<f64> = Split::default();
        let (a, b) = Split::<f64>::new().tick(1.0);
        assert!((a - 1.0).abs() < 1e-10);
        assert!((b - 1.0).abs() < 1e-10);
        let _ = split;
    }

    #[test]
    fn test_merge_reset() {
        let mut merge = Merge::new(|a: f64, b: f64| a + b);
        merge.reset(); // Should not panic
    }

    #[test]
    fn test_swap_reset() {
        let mut swap = Swap::<i32, f64>::new();
        swap.reset(); // Should not panic
    }

    #[test]
    fn test_swap_default() {
        let swap: Swap<i32, f64> = Swap::default();
        let _ = swap;
    }

    #[test]
    fn test_process_block() {
        let mut gain = Gain { factor: 2.0 };
        let input = vec![1.0, 2.0, 3.0, 4.0];
        let mut output = vec![0.0; 4];
        gain.process(&input, &mut output);
        assert_eq!(output, vec![2.0, 4.0, 6.0, 8.0]);
    }

    // =========================================================================
    // Q052: `arr` primitive and Arrow-law tests
    // =========================================================================

    // Stateful test module: running sum. Makes law tests non-trivial (an
    // associativity/identity check on a stateless module would be vacuous).
    struct Accum {
        sum: f64,
    }

    impl Accum {
        fn new() -> Self {
            Self { sum: 0.0 }
        }
    }

    impl Module for Accum {
        type In = f64;
        type Out = f64;

        fn tick(&mut self, input: Self::In) -> Self::Out {
            self.sum += input;
            self.sum
        }

        fn reset(&mut self) {
            self.sum = 0.0;
        }
    }

    // Deterministic input sequence used by all law checks.
    const SEQ: [f64; 8] = [0.5, -0.3, 1.0, 2.0, -1.5, 0.25, 3.0, -0.7];

    fn run_mono<M: Module<In = f64, Out = f64>>(mut m: M) -> Vec<f64> {
        SEQ.iter().map(|&x| m.tick(x)).collect()
    }

    // Feeds SEQ as the processed element and the sample index as the pass-through
    // element, so `first`/`second` pass-through behavior is also exercised.
    fn run_pair<M: Module<In = (f64, f64), Out = (f64, f64)>>(mut m: M) -> Vec<(f64, f64)> {
        SEQ.iter()
            .enumerate()
            .map(|(i, &x)| m.tick((x, i as f64)))
            .collect()
    }

    fn assert_seq_close(a: &[f64], b: &[f64]) {
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < 1e-12, "sequence mismatch: {x} != {y}");
        }
    }

    #[test]
    fn test_arr_basic() {
        let mut m = arr(|x: f64| x * 2.0 + 1.0);
        assert!((m.tick(3.0) - 7.0).abs() < 1e-12);
        // Stateless: reset / set_sample_rate are no-ops but must not panic.
        m.reset();
        m.set_sample_rate(48_000.0);
        assert!((m.tick(0.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn arrow_law_identity() {
        // id.then(f) == f  and  f.then(id) == f
        let reference = run_mono(Accum::new());
        let left_id = run_mono(Identity::<f64>::new().then(Accum::new()));
        let right_id = run_mono(Accum::new().then(Identity::<f64>::new()));
        assert_seq_close(&left_id, &reference);
        assert_seq_close(&right_id, &reference);
    }

    #[test]
    fn arrow_law_associativity() {
        // (f.then(g)).then(h) == f.then(g.then(h))
        // f, h are stateful (Accum); g is a scaling module.
        let lhs = run_mono((Accum::new().then(Gain { factor: 2.0 })).then(Accum::new()));
        let rhs = run_mono(Accum::new().then(Gain { factor: 2.0 }.then(Accum::new())));
        assert_seq_close(&lhs, &rhs);
    }

    #[test]
    fn arrow_law_first_distributes() {
        // first(f.then(g)) == first(f).then(first(g))
        let lhs = run_pair(Accum::new().then(Gain { factor: 3.0 }).first::<f64>());
        let rhs = run_pair(
            Accum::new()
                .first::<f64>()
                .then(Gain { factor: 3.0 }.first::<f64>()),
        );
        assert_eq!(lhs.len(), rhs.len());
        for (a, b) in lhs.iter().zip(&rhs) {
            assert!((a.0 - b.0).abs() < 1e-12, "processed element differs");
            assert!((a.1 - b.1).abs() < 1e-12, "pass-through element differs");
        }
        // Sanity: the pass-through element is carried unchanged (equals the index).
        for (i, pair) in lhs.iter().enumerate() {
            assert!((pair.1 - i as f64).abs() < 1e-12);
        }
    }

    // =========================================================================
    // Q050: GraphModule <-> Module bridge adapters
    // =========================================================================

    // A minimal GraphModule for precise port-selection checks: out(10) = 2 * in(0).
    struct DoublerGm {
        spec: PortSpec,
    }

    impl DoublerGm {
        fn new() -> Self {
            Self {
                spec: PortSpec {
                    inputs: vec![PortDef::new(0, "in", SignalKind::Audio)],
                    outputs: vec![PortDef::new(10, "out", SignalKind::Audio)],
                },
            }
        }
    }

    impl GraphModule for DoublerGm {
        fn port_spec(&self) -> &PortSpec {
            &self.spec
        }
        fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
            outputs.set(10, inputs.get_or(0, 0.0) * 2.0);
        }
        fn reset(&mut self) {}
        fn set_sample_rate(&mut self, _sample_rate: f64) {}
    }

    #[test]
    fn test_graph_module_adapter_drives_selected_ports() {
        // GraphModule -> Module: chosen input (0) is driven, chosen output (10) is read.
        let mut adapter = GraphModuleAdapter::new(DoublerGm::new(), 0, 10);
        assert!((adapter.tick(3.0) - 6.0).abs() < 1e-12);
        assert!((adapter.tick(-2.5) - (-5.0)).abs() < 1e-12);
        // inner() exposes the wrapped module.
        assert_eq!(adapter.inner().port_spec().outputs[0].id, 10);
        // reset / set_sample_rate forward without panic.
        adapter.reset();
        adapter.set_sample_rate(48_000.0);
    }

    #[test]
    fn test_graph_module_adapter_from_audio_ports() {
        use crate::modules::{Svf, Vco};
        // Svf has an audio input ("in") and audio outputs -> Some.
        let svf = GraphModuleAdapter::from_audio_ports(Svf::new(44_100.0));
        assert!(svf.is_some());
        // Vco's inputs are all CV/pitch/gate (no Audio input) -> None.
        assert!(GraphModuleAdapter::from_audio_ports(Vco::new(44_100.0)).is_none());
    }

    #[test]
    fn test_graph_module_adapter_vco_svf_chain_produces_audio() {
        use crate::modules::{Svf, Vco};
        // Flagship Q050 check: a REAL Vco adapted and chained into a REAL Svf,
        // via the combinator `.then`, produces nonzero audio.
        let vco = GraphModuleAdapter::new(Vco::new(44_100.0), 0, 12); // voct in, saw out
        let svf = GraphModuleAdapter::from_audio_ports(Svf::new(44_100.0)).unwrap();
        let mut chain = vco.then(svf);

        let mut peak = 0.0_f64;
        for _ in 0..512 {
            peak = peak.max(chain.tick(0.0).abs()); // 0 V/oct = middle C
        }
        assert!(
            peak > 1e-6,
            "expected nonzero audio from Vco -> Svf combinator chain, got {peak}"
        );
    }

    #[test]
    fn test_module_graph_adapter_direct() {
        // Module -> GraphModule: one-in/one-out spec, tick maps in(0) -> out(10).
        let mut node = ModuleGraphAdapter::new(arr(|x: f64| x * 3.0));
        assert_eq!(node.port_spec().inputs.len(), 1);
        assert_eq!(node.port_spec().outputs.len(), 1);
        assert_eq!(node.port_spec().inputs[0].id, 0);
        assert_eq!(node.port_spec().outputs[0].id, 10);
        assert_eq!(node.type_id(), "combinator_chain");

        let mut inputs = PortValues::new();
        inputs.set(0, 2.0);
        let mut outputs = PortValues::new();
        node.tick(&inputs, &mut outputs);
        assert!((outputs.get_or(10, 0.0) - 6.0).abs() < 1e-12);

        node.reset();
        node.set_sample_rate(48_000.0);
    }

    #[test]
    fn test_module_graph_adapter_in_patch() {
        use crate::graph::Patch;
        use crate::modules::{StereoOutput, Vco};

        // A combinator chain wrapped as a GraphModule node, added to a Patch.
        let mut patch = Patch::new(44_100.0);
        let chain = arr(|x: f64| x * 0.5).then(arr(|x: f64| x + 0.1));
        let node = patch.add("chain", ModuleGraphAdapter::new(chain));
        let vco = patch.add("vco", Vco::new(44_100.0));
        let out = patch.add("out", StereoOutput::new());

        patch.connect(vco.out("saw"), node.in_("in")).unwrap();
        patch.connect(node.out("out"), out.in_("left")).unwrap();
        patch.set_output(out.id());
        patch.compile().unwrap();

        let mut peak = 0.0_f64;
        for _ in 0..512 {
            let (left, _right) = patch.tick();
            peak = peak.max(left.abs());
        }
        assert!(
            peak > 1e-6,
            "combinator chain wrapped as a GraphModule should tick inside a Patch, got {peak}"
        );
    }
}
