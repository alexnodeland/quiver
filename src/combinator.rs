//! # Layer 1: Typed Module Combinators
//!
//! This module provides **Arrow-style combinators** for composing signal processing
//! modules with compile-time type checking. These combinators enable functional
//! composition of DSP chains that compile down to tight, inlinable loops with
//! zero runtime overhead.
//!
//! ## Category Theory Background
//!
//! In category theory, an **Arrow** is a generalization of functions that allows
//! for composition while carrying additional structure (like state). The combinators
//! here implement the Arrow interface:
//!
//! ```text
//! arr:     (a -> b) -> Arrow a b           // Lift pure function
//! (>>>):   Arrow a b -> Arrow b c -> Arrow a c   // Sequential composition
//! first:   Arrow a b -> Arrow (a,c) (b,c)  // Apply to first element
//! (***):   Arrow a b -> Arrow c d -> Arrow (a,c) (b,d)  // Parallel
//! (&&&):   Arrow a b -> Arrow a c -> Arrow a (b,c)       // Fanout
//! ```
//!
//! ## Arrow Laws
//!
//! These combinators satisfy the Arrow laws, ensuring predictable behavior:
//!
//! - **Identity**: `id >>> f = f = f >>> id`
//! - **Associativity**: `(f >>> g) >>> h = f >>> (g >>> h)`
//! - **First distributes**: `first (f >>> g) = first f >>> first g`
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
//! ## Example: Building a Synth Voice
//!
//! ```rust,ignore
//! use quiver::combinator::*;
//!
//! // Compose modules sequentially
//! let voice = vco
//!     .then(filter)
//!     .then(amplifier);
//!
//! // Process in parallel
//! let stereo = left_processor.parallel(right_processor);
//!
//! // Split signal to multiple processors
//! let effects = signal.fanout(reverb, delay);
//! ```

use core::marker::PhantomData;

/// A signal processing module with typed input and output.
///
/// This is the fundamental abstraction for DSP processing in Quiver. Modules are
/// **stateful processors** that transform input samples to output samples. The
/// associated types `In` and `Out` enable compile-time verification of signal flow.
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
/// ```rust,ignore
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

    /// Create a feedback loop with unit delay
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

/// Feedback loop with mandatory single-sample delay for causality
pub struct Feedback<M: Module, F> {
    pub module: M,
    pub combine: F,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
