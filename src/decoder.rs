//! The decoder abstraction and the adapters it composes with.

use crate::combinator::{AndThen, Map, Pipe, Recover, RecoverWith, Refine, WithDefault};
use crate::issue::Issues;
use crate::path::Path;
use std::borrow::Cow;

/// Reads an input of type `I` into a value, or reports every issue it found.
///
/// A decoder is a specification: what it gives for an input depends on that input and its path
/// alone, so it can be applied any number of times, from any number of threads at once. It may
/// keep what it has worked out, as [`pattern`](crate::json::StringDecoder::pattern) keeps its
/// matchers and [`lazy`](crate::lazy) the decoder it builds, but nothing that changes a result. A
/// decoder of your own keeps to this too. The adapters it offers build new decoders without running anything; the concrete types they return
/// are meant to be hidden behind `impl Decoder<I, Output = T>`, and [`boxed`](Self::boxed) erases
/// them where a type has to be named.
///
/// ```
/// use raoh::json::prelude::*;
///
/// #[derive(Debug)]
/// struct Age(u32);
///
/// fn age() -> impl Decoder<Json, Output = Age> {
///     u32().range(0..=150).map(Age)
/// }
///
/// let issues = age().decode(&json!(200)).unwrap_err();
/// assert_eq!(issues.iter().next().unwrap().code(), "out_of_range");
/// ```
pub trait Decoder<I: ?Sized> {
    /// What a successful decode gives.
    type Output;

    /// Decodes `input` found at `path`. Issues are reported at `path` or below it.
    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<Self::Output, Issues>;

    /// Decodes `input` as the root of what is read.
    fn decode(&self, input: &I) -> Result<Self::Output, Issues> {
        self.decode_at(input, &Path::ROOT)
    }

    /// A decoder whose output is this one's transformed by `f`, which cannot fail.
    fn map<F, U>(self, f: F) -> Map<Self, F>
    where
        Self: Sized,
        F: Fn(Self::Output) -> U,
    {
        Map::new(self, f)
    }

    /// A decoder that hands this one's output to `f`, which can fail.
    ///
    /// The issues `f` returns are read as relative to where this decoder is: an issue at the
    /// root is reported at this decoder's path, and one at `/end` below it. `f` should not add
    /// the path itself.
    ///
    /// ```
    /// use raoh::json::prelude::*;
    /// use raoh::Issue;
    ///
    /// #[derive(Debug)]
    /// struct Period { start: i64, end: i64 }
    ///
    /// impl Period {
    ///     fn new(start: i64, end: i64) -> Result<Self, Issue> {
    ///         if start > end {
    ///             return Err(Issue::new("invalid_value").with_message("start must not be after end"));
    ///         }
    ///         Ok(Self { start, end })
    ///     }
    /// }
    ///
    /// let period = object((field("start", i64()), field("end", i64())))
    ///     .and_then(|(start, end)| Period::new(start, end));
    /// let decoder = object((field("period", period),));
    ///
    /// let issues = decoder.decode(&json!({"period": {"start": 2, "end": 1}})).unwrap_err();
    /// assert_eq!(issues.iter().next().unwrap().path().to_string(), "/period");
    /// ```
    fn and_then<F, U, E>(self, f: F) -> AndThen<Self, F>
    where
        Self: Sized,
        F: Fn(Self::Output) -> Result<U, E>,
        E: Into<Issues>,
    {
        AndThen::new(self, f)
    }

    /// A decoder that hands this one's output to `next` as its input, at the same path.
    fn pipe<D>(self, next: D) -> Pipe<Self, D>
    where
        Self: Sized,
        D: Decoder<Self::Output>,
    {
        Pipe::new(self, next)
    }

    /// A decoder that also requires `predicate` to hold of the output, reporting `code` with
    /// `message` as a custom message when it does not.
    fn refine<P>(
        self,
        predicate: P,
        code: impl Into<Cow<'static, str>>,
        message: impl Into<String>,
    ) -> Refine<Self, P>
    where
        Self: Sized,
        P: Fn(&Self::Output) -> bool,
    {
        Refine::new(self, predicate, code.into(), message.into())
    }

    /// A decoder that gives `value` when the input is null or missing, and otherwise gives what
    /// this one gives for it.
    ///
    /// Whether the input is null or missing is looked at before this decoder runs, so an object
    /// that is there but lacks a member is reported as this decoder reports it, not defaulted.
    ///
    /// ```
    /// use raoh::json::prelude::*;
    ///
    /// let page = object((field("page", i32().with_default(1)),));
    /// assert_eq!(page.decode(&json!({})).unwrap(), (1,));
    /// assert_eq!(page.decode(&json!({"page": null})).unwrap(), (1,));
    /// assert!(page.decode(&json!({"page": "x"})).is_err());
    /// ```
    fn with_default(self, value: Self::Output) -> WithDefault<Self, Self::Output>
    where
        Self: Sized,
        Self::Output: Clone,
        I: Nullish,
    {
        WithDefault::new(self, value)
    }

    /// A decoder that gives `value` whenever this one fails, whatever the issue.
    fn recover(self, value: Self::Output) -> Recover<Self, Self::Output>
    where
        Self: Sized,
        Self::Output: Clone,
    {
        Recover::new(self, value)
    }

    /// A decoder that gives what `f` makes of the issues whenever this one fails.
    ///
    /// ```
    /// use raoh::json::prelude::*;
    ///
    /// let count = i32().recover_with(|issues| -(issues.len() as i32));
    /// assert_eq!(count.decode(&json!("x")).unwrap(), -1);
    /// ```
    fn recover_with<F>(self, f: F) -> RecoverWith<Self, F>
    where
        Self: Sized,
        F: Fn(&Issues) -> Self::Output,
    {
        RecoverWith::new(self, f)
    }

    /// This decoder behind a pointer, so its type can be named and it can be shared across
    /// threads.
    fn boxed(self) -> BoxDecoder<I, Self::Output>
    where
        Self: Sized + Send + Sync + 'static,
    {
        Box::new(self)
    }
}

/// An input that can stand for no value at all, which [`Decoder::with_default`] replaces with its
/// default.
pub trait Nullish {
    /// Whether this is null, or stands for a value that is missing.
    fn is_null_or_missing(&self) -> bool;
}

/// A decoder whose concrete type is erased.
pub type BoxDecoder<I, O> = Box<dyn Decoder<I, Output = O> + Send + Sync>;

impl<I: ?Sized, D: Decoder<I> + ?Sized> Decoder<I> for &D {
    type Output = D::Output;

    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<Self::Output, Issues> {
        (**self).decode_at(input, path)
    }
}

impl<I: ?Sized, D: Decoder<I> + ?Sized> Decoder<I> for Box<D> {
    type Output = D::Output;

    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<Self::Output, Issues> {
        (**self).decode_at(input, path)
    }
}

impl<I: ?Sized, D: Decoder<I> + ?Sized> Decoder<I> for std::sync::Arc<D> {
    type Output = D::Output;

    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<Self::Output, Issues> {
        (**self).decode_at(input, path)
    }
}

/// A decoder from a function, for a one-off decoder not worth a type of its own.
///
/// ```
/// use raoh::{decoder_fn, Decoder, Issue};
///
/// let even = decoder_fn(|n: &i64, _path| {
///     if n % 2 == 0 { Ok(*n) } else { Err(Issue::new("odd").with_message("must be even").into()) }
/// });
/// assert!(even.decode(&3).is_err());
/// ```
pub fn decoder_fn<I, O, F>(f: F) -> FnDecoder<F>
where
    I: ?Sized,
    F: Fn(&I, &Path<'_>) -> Result<O, Issues>,
{
    FnDecoder(f)
}

/// The decoder [`decoder_fn`] returns.
#[derive(Clone, Copy, Debug)]
pub struct FnDecoder<F>(F);

impl<I: ?Sized, O, F> Decoder<I> for FnDecoder<F>
where
    F: Fn(&I, &Path<'_>) -> Result<O, Issues>,
{
    type Output = O;

    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<O, Issues> {
        (self.0)(input, path)
    }
}
