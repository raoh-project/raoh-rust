//! The adapters [`Decoder`](crate::Decoder)'s methods return, and the decoders that combine
//! several.
//!
//! A tuple of decoders over the same input is itself a decoder: it runs every one of them and
//! gives their outputs as a tuple, or every issue any of them found.

mod adapters;
mod lazy;
mod one_of;
mod tuple;

pub use adapters::{AndThen, Map, Pipe, Recover, RecoverWith, Refine, WithDefault};
pub use lazy::{Lazy, lazy};
pub use one_of::{Alternatives, OneOf, one_of};
