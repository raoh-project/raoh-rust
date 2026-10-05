//! Raoh turns untyped boundary input into typed domain values.
//!
//! It follows parse, don't validate: a domain value is built only once everything it is built
//! from has been read, so a value that exists is a valid one. What was wrong with the input comes
//! back as [`Issues`], every one of them with the [JSON Pointer](Pointer) of where it was found,
//! rather than the first.
//!
//! Raoh reads JSON text, with [`json::from_str`], or a [`serde_json::Value`], into the domain.
//!
//! ```
//! use raoh::json::prelude::*;
//!
//! #[derive(Debug)]
//! struct Email(String);
//! #[derive(Debug)]
//! struct User { email: Email, age: u32 }
//!
//! fn email() -> impl Decoder<Json, Output = Email> {
//!     string().trim().lowercase().email().map(Email)
//! }
//!
//! fn user() -> impl Decoder<Json, Output = User> {
//!     object((
//!         field("email", email()),
//!         field("age", u32().range(0..=150)),
//!     ))
//!     .map(|(email, age)| User { email, age })
//! }
//!
//! let issues = user().decode(&json!({"email": "nope", "age": 200})).unwrap_err();
//! let paths: Vec<String> = issues.iter().map(|i| i.path().to_string()).collect();
//! assert_eq!(paths, ["/email", "/age"]);
//! ```
//!
//! Independent parts are combined with a tuple, which reports the issues of every part;
//! [`Decoder::and_then`] checks what depends on several parts together, once they have all been
//! read.
//!
//! # The Raoh Specification
//!
//! The decoders behave as the [Raoh Specification](https://github.com/raoh-project/raoh-specification)
//! says, and are checked against its cases by `scripts/conformance.sh`: which inputs each accepts,
//! the issues each reports, their paths, metadata and messages, and the value model, in which a
//! decimal keeps its scale, a float has two zeros and one NaN, and an offset date-time keeps its
//! offset. The decoders read the specification's input model, a JSON value whose numbers keep the
//! text they were written with: see [`json`].

#![warn(missing_docs)]

#[macro_use]
mod tuples;

pub mod combinator;
mod decoder;
pub mod encode;
mod issue;
pub mod json;
mod message;
mod meta;
mod path;
mod presence;
mod properties;
mod value;
mod vocabulary;

pub use combinator::{lazy, one_of};
pub use decoder::{BoxDecoder, Decoder, FnDecoder, Nullish, decoder_fn};
pub use issue::{Issue, Issues};
pub use message::{MessageResolver, Messages, PropertiesError};
pub use meta::MetaValue;
pub use path::{Path, Pointer, Segment};
pub use presence::Presence;
pub use value::decimal::{Decimal, ParseDecimalError};
pub use value::float::{Float, float_order, float_same};
pub use value::same::{Same, Set};
pub use value::temporal::{
    Chronological, Date, DateTime, Instant, OffsetDateTime, ParseTemporalError, Time,
};
pub use value::uri::{ParseUriError, Uri};
pub use value::uuid::{ParseUuidError, Uuid};

#[doc = include_str!("../README.md")]
#[cfg(doctest)]
struct ReadmeDoctests;
pub use vocabulary::{codes, message_keys};
