//! Layer 1: Typed Module Combinators
//!
//! This module provides Arrow-style combinators for composing signal processing modules
//! with compile-time type checking. These combinators enable functional composition
//! of DSP chains that compile down to tight, inlinable loops.

use std::marker::PhantomData;

/// A signal processing module with typed input and output.
///
/// This is the fundamental abstraction for DSP processing in quiver.
/// Modules are stateful processors that transform input samples to output samples.
pub trait Module: Send {
    /// Input signal type
    type In;
    /// Output signal type
    type Out;

    /// Process a single sample
    fn tick(&mut self, input: Self::In) -> Self::Out;

    /// Process a block of samples (override for optimization)
    fn process(&mut self, input: &[Self::In], output: &mut [Self::Out])
    where
        Self::In: Clone,
    {
        for (i, o) in input.iter().zip(output.iter_mut()) {
            *o = self.tick(i.clone());
        }
    }

    /// Reset internal state to initial conditions
    fn reset(&mut self);

    /// Notify module of sample rate changes
    fn set_sample_rate(&mut self, _sample_rate: f64) {}
}

/// Extension trait providing combinator methods for all modules
pub trait ModuleExt: Module + Sized {
    /// Chain this module with another (sequential composition: `>>>`)
    fn then<M: Module<In = Self::Out>>(self, next: M) -> Chain<Self, M> {
        Chain { first: self, second: next }
    }

    /// Run two modules in parallel (`***`)
    fn parallel<M: Module>(self, other: M) -> Parallel<Self, M> {
        Parallel { left: self, right: other }
    }

    /// Split input to two parallel processors (`&&&`)
    fn fanout<M: Module<In = Self::In>>(self, other: M) -> Fanout<Self, M>
    where
        Self::In: Clone,
    {
        Fanout { left: self, right: other }
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
        Contramap { module: self, f, _phantom: PhantomData }
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
        First { module: self, _phantom: PhantomData }
    }

    /// Apply this module only to the second element of a tuple
    fn second<C>(self) -> Second<Self, C> {
        Second { module: self, _phantom: PhantomData }
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
        Self { _phantom: PhantomData }
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
        Self { f, _phantom: PhantomData }
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
        Self { _phantom: PhantomData }
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
        Self { _phantom: PhantomData }
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
}
