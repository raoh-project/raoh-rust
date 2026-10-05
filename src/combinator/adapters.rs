use crate::decoder::{Decoder, Nullish};
use crate::issue::{Issue, Issues};
use crate::path::Path;
use std::borrow::Cow;

/// The decoder [`Decoder::map`] returns.
#[derive(Clone, Debug)]
pub struct Map<D, F> {
    inner: D,
    f: F,
}

impl<D, F> Map<D, F> {
    pub(crate) fn new(inner: D, f: F) -> Self {
        Self { inner, f }
    }
}

impl<I: ?Sized, D, F, U> Decoder<I> for Map<D, F>
where
    D: Decoder<I>,
    F: Fn(D::Output) -> U,
{
    type Output = U;

    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<U, Issues> {
        self.inner.decode_at(input, path).map(&self.f)
    }
}

/// The decoder [`Decoder::and_then`] returns.
#[derive(Clone, Debug)]
pub struct AndThen<D, F> {
    inner: D,
    f: F,
}

impl<D, F> AndThen<D, F> {
    pub(crate) fn new(inner: D, f: F) -> Self {
        Self { inner, f }
    }
}

impl<I: ?Sized, D, F, U, E> Decoder<I> for AndThen<D, F>
where
    D: Decoder<I>,
    F: Fn(D::Output) -> Result<U, E>,
    E: Into<Issues>,
{
    type Output = U;

    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<U, Issues> {
        let value = self.inner.decode_at(input, path)?;
        (self.f)(value).map_err(|e| e.into().rebase(path))
    }
}

/// The decoder [`Decoder::pipe`] returns.
#[derive(Clone, Debug)]
pub struct Pipe<A, B> {
    first: A,
    then: B,
}

impl<A, B> Pipe<A, B> {
    pub(crate) fn new(first: A, then: B) -> Self {
        Self { first, then }
    }
}

impl<I: ?Sized, A, B> Decoder<I> for Pipe<A, B>
where
    A: Decoder<I>,
    B: Decoder<A::Output>,
{
    type Output = B::Output;

    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<B::Output, Issues> {
        let value = self.first.decode_at(input, path)?;
        self.then.decode_at(&value, path)
    }
}

/// The decoder [`Decoder::refine`] returns.
#[derive(Clone, Debug)]
pub struct Refine<D, P> {
    inner: D,
    predicate: P,
    code: Cow<'static, str>,
    message: String,
}

impl<D, P> Refine<D, P> {
    pub(crate) fn new(inner: D, predicate: P, code: Cow<'static, str>, message: String) -> Self {
        Self {
            inner,
            predicate,
            code,
            message,
        }
    }
}

impl<I: ?Sized, D, P> Decoder<I> for Refine<D, P>
where
    D: Decoder<I>,
    P: Fn(&D::Output) -> bool,
{
    type Output = D::Output;

    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<D::Output, Issues> {
        let value = self.inner.decode_at(input, path)?;
        if (self.predicate)(&value) {
            Ok(value)
        } else {
            Err(Issue::new(self.code.clone())
                .with_message(self.message.clone())
                .at(path.to_pointer())
                .into())
        }
    }
}

/// The decoder [`Decoder::with_default`] returns.
#[derive(Clone, Debug)]
pub struct WithDefault<D, T> {
    inner: D,
    value: T,
}

impl<D, T> WithDefault<D, T> {
    pub(crate) fn new(inner: D, value: T) -> Self {
        Self { inner, value }
    }
}

impl<I: ?Sized + Nullish, D, T> Decoder<I> for WithDefault<D, T>
where
    D: Decoder<I, Output = T>,
    T: Clone,
{
    type Output = T;

    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<T, Issues> {
        if input.is_null_or_missing() {
            Ok(self.value.clone())
        } else {
            self.inner.decode_at(input, path)
        }
    }
}

/// The decoder [`Decoder::recover`] returns.
#[derive(Clone, Debug)]
pub struct Recover<D, T> {
    inner: D,
    value: T,
}

impl<D, T> Recover<D, T> {
    pub(crate) fn new(inner: D, value: T) -> Self {
        Self { inner, value }
    }
}

impl<I: ?Sized, D, T> Decoder<I> for Recover<D, T>
where
    D: Decoder<I, Output = T>,
    T: Clone,
{
    type Output = T;

    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<T, Issues> {
        Ok(self
            .inner
            .decode_at(input, path)
            .unwrap_or_else(|_| self.value.clone()))
    }
}

/// The decoder [`Decoder::recover_with`] returns.
#[derive(Clone, Debug)]
pub struct RecoverWith<D, F> {
    inner: D,
    f: F,
}

impl<D, F> RecoverWith<D, F> {
    pub(crate) fn new(inner: D, f: F) -> Self {
        Self { inner, f }
    }
}

impl<I: ?Sized, D, F> Decoder<I> for RecoverWith<D, F>
where
    D: Decoder<I>,
    F: Fn(&Issues) -> D::Output,
{
    type Output = D::Output;

    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<D::Output, Issues> {
        Ok(self
            .inner
            .decode_at(input, path)
            .unwrap_or_else(|issues| (self.f)(&issues)))
    }
}
