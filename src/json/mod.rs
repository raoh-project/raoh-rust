//! Decoders over JSON values.
//!
//! Bring everything into scope with `use raoh::json::prelude::*`.
//!
//! Every decoder here reads a [`Json`]: a value of the Raoh Specification's input model, which is
//! what a JSON text denotes with every number keeping its lexeme. [`from_str`] reads JSON text into
//! a [`Node`], which is one; a [`serde_json::Value`] is one too.
//!
//! A member missing from an object and a member present as `null` are told apart: a missing
//! member is handed to its decoder as [`missing()`], which [`is_missing`] recognises. Both fail a
//! built-in decoder with `required`, but [`nullable`](JsonDecoderExt::nullable) accepts only
//! `null`, and [`presence_field`] reports which of the three it was.
//!
//! # Numbers
//!
//! A decoder reads a number from its lexeme, as the input model has it: `int` takes `1` and
//! refuses `1.0`, `decimal` gives `1.50` with scale 2, and `double` gives -0 for `-0` and `-0.0`.
//! A [`Node`] keeps every lexeme as written.
//!
//! A [`serde_json::Value`] keeps the lexeme only with `serde_json`'s `arbitrary_precision`
//! feature, which this crate's feature of the same name turns on, and then holds every number as
//! a `String` of its own. Without it a number is the `i64`, `u64` or `f64` `serde_json` read, and
//! is read as the text that value is written as: `1.50` is read as `1.5`, `-0` as `-0.0`, which
//! `int` refuses, and an integer past `u64` as a float. Even with the feature, `serde_json`'s
//! parser reads an integer as an `i64` or `u64` before keeping its text, so the text `-0` becomes
//! `0`. To decode JSON text, read it with [`from_str`] rather than `serde_json::from_str`.

mod bool;
mod choice;
mod decimal;
mod dict;
mod ext;
mod input;
pub(crate) mod ip;
mod node;
mod number;
mod object;
mod steps;
mod string;
mod temporal;
mod text;

pub use self::bool::{BoolDecoder, bool};
pub use choice::{
    Discriminate, DiscriminateBy, EnumOf, Literal, Variant, Variants, discriminate,
    discriminate_by, enum_of, literal, variant,
};
pub use decimal::{DecimalDecoder, decimal};
pub use dict::{Dict, dict};
pub use ext::{JsonDecoderExt, ListDecoder, Nullable, ToSet};
pub use input::{Elements, Input, Json, Members, Number, View, is_missing, missing};
pub use node::{JsonObject, Lexeme, Node};
pub use number::{
    F32Decoder, F64Decoder, FloatDecoder, IntDecoder, Integer, SignedInteger, f32, f64, i32, i64,
    u32, u64,
};
pub use object::{
    BoxFieldSet, Field, FieldSet, Flat, MapFields, Object, OptionalField, PresenceField, Strict,
    StrictMembers, field, flat, object, optional_field, presence_field, strict,
};
pub use string::{NormalizationForm, Parse, StringDecoder, UriDecoder, UuidDecoder, string};
pub use temporal::TemporalDecoder;

use crate::codes;
use crate::decoder::Decoder;
use crate::issue::{Issue, Issues};
use crate::path::Path;

/// Everything needed to write decoders over JSON.
pub mod prelude {
    pub use super::{
        Json, JsonDecoderExt, Node, bool, decimal, dict, discriminate, discriminate_by, enum_of,
        f32, f64, field, flat, from_str, i32, i64, literal, object, optional_field, presence_field,
        strict, string, u32, u64, variant,
    };
    pub use crate::{BoxDecoder, Decoder, Issue, Issues, Presence, lazy, one_of};
    pub use serde_json::{Value, json};
}

/// Reads `text` as JSON into a [`Node`] and decodes it with `decoder`. Text that is not JSON is
/// reported as one `invalid_format` issue at the root, under the message key
/// `invalid_format.json`, with the `line` and `column`, both counted from 1 and the column in
/// characters, of where it stopped being JSON. So is an object with a name twice, and arrays and
/// objects nested more than 128 deep.
///
/// ```
/// use raoh::json::prelude::*;
///
/// let issues = from_str(&i64(), "{").unwrap_err();
/// assert_eq!(issues.iter().next().unwrap().code(), "invalid_format");
/// assert!(from_str(&f64(), "-0").unwrap().is_sign_negative());
/// ```
pub fn from_str<D: Decoder<Json>>(decoder: &D, text: &str) -> Result<D::Output, Issues> {
    let node: Node = text.parse()?;
    decoder.decode(&node)
}

pub(crate) fn required(path: &Path<'_>) -> Issue {
    Issue::at_path(path, codes::REQUIRED)
}

pub(crate) fn type_mismatch(path: &Path<'_>, expected: &'static str, found: &Json) -> Issue {
    Issue::at_path(path, codes::TYPE_MISMATCH)
        .with_meta("expected", expected)
        .with_meta("actual", found.view().kind())
}

/// `required` for a missing or null value, `type_mismatch` for anything else.
pub(crate) fn unexpected(path: &Path<'_>, expected: &'static str, found: &Json) -> Issue {
    if found.view().is_null_or_missing() {
        required(path)
    } else {
        type_mismatch(path, expected, found)
    }
}
