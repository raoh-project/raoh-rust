//! The values an issue's metadata holds.

use crate::issue::Issues;
use crate::message::MessageResolver;
use crate::value::float::{float_message, float_same};
use crate::{Date, DateTime, Decimal, Instant, OffsetDateTime, Time, Uri, Uuid};
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};

/// A value of an issue's metadata, typed as the value model of the Raoh Specification types it.
///
/// The type is kept so that a value is written as its type writes it: a `float` bound of 0.1 is
/// `0.1` in a message, not the `f64` nearest to it, and a decimal keeps its scale. Two values are
/// equal as the value model compares them: +0 and -0 differ, every NaN is the same, and `1.5` and
/// `1.50` are different decimals.
///
/// [`Display`](fmt::Display) writes the message form a message template fills a placeholder with.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum MetaValue {
    /// A boolean.
    Bool(bool),
    /// An integer that fits 64 bits.
    Int(i64),
    /// An unsigned integer above the range of [`Int`](Self::Int).
    UInt(u64),
    /// An IEEE 754 binary32 value.
    Float32(f32),
    /// An IEEE 754 binary64 value.
    Float64(f64),
    /// A decimal with its scale.
    Decimal(Decimal),
    /// Text: a string, or the name of a symbol.
    String(String),
    /// A UUID.
    Uuid(Uuid),
    /// A URI.
    Uri(Uri),
    /// A date.
    Date(Date),
    /// A time of day.
    Time(Time),
    /// A date-time with no offset.
    DateTime(DateTime),
    /// A date-time with an offset.
    OffsetDateTime(OffsetDateTime),
    /// An instant.
    Instant(Instant),
    /// A list of values.
    List(Vec<MetaValue>),
    /// Named values, such as each candidate `one_of_failed` lists.
    Record(BTreeMap<String, MetaValue>),
    /// The issues of a failed decode, such as a candidate's.
    Issues(Issues),
}

impl MetaValue {
    /// The text, if this is [`String`](Self::String).
    pub fn as_str(&self) -> Option<&str> {
        match self {
            MetaValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// The integer, if this is one that fits an `i64`.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            MetaValue::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// The elements, if this is a [`List`](Self::List).
    pub fn as_list(&self) -> Option<&[MetaValue]> {
        match self {
            MetaValue::List(items) => Some(items),
            _ => None,
        }
    }

    /// The value as JSON, with the messages of any issues in it written by `resolver`.
    ///
    /// Numbers that JSON writes exactly are numbers; a float that is not finite is the text
    /// `NaN`, `Infinity` or `-Infinity`, and a decimal is its text, so that no digit or scale is
    /// lost. Temporal values, UUIDs and URIs are their text.
    pub fn to_json_with(&self, resolver: &(impl MessageResolver + ?Sized)) -> Value {
        match self {
            MetaValue::Bool(b) => Value::Bool(*b),
            MetaValue::Int(n) => Value::from(*n),
            MetaValue::UInt(n) => Value::from(*n),
            MetaValue::Float32(v) => float_json(f64::from(*v), || float_message(*v)),
            MetaValue::Float64(v) => float_json(*v, || float_message(*v)),
            MetaValue::List(items) => {
                Value::Array(items.iter().map(|v| v.to_json_with(resolver)).collect())
            }
            MetaValue::Record(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json_with(resolver)))
                    .collect::<Map<String, Value>>(),
            ),
            MetaValue::Issues(issues) => issues.to_json_with(resolver),
            other => Value::String(other.to_string()),
        }
    }
}

/// A finite float as the JSON number its message form writes, which reads back as the same
/// float; anything else as the text of its message form.
fn float_json(widened: f64, message: impl Fn() -> String) -> Value {
    let text = message();
    if widened.is_finite()
        && let Ok(number) = serde_json::from_str::<Value>(&text)
    {
        return number;
    }
    Value::String(text)
}

/// The message form: integers in decimal, floats as their canonical decimal (`0.5`, `1.0E7`),
/// decimals as `BigDecimal.toString` writes them, temporal values in ISO 8601, and a list as
/// `[`, its elements separated by `, `, `]`.
impl fmt::Display for MetaValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetaValue::Bool(b) => write!(f, "{b}"),
            MetaValue::Int(n) => write!(f, "{n}"),
            MetaValue::UInt(n) => write!(f, "{n}"),
            MetaValue::Float32(v) => f.write_str(&float_message(*v)),
            MetaValue::Float64(v) => f.write_str(&float_message(*v)),
            MetaValue::Decimal(d) => write!(f, "{d}"),
            MetaValue::String(s) => f.write_str(s),
            MetaValue::Uuid(u) => write!(f, "{u}"),
            MetaValue::Uri(u) => write!(f, "{u}"),
            MetaValue::Date(d) => write!(f, "{d}"),
            MetaValue::Time(t) => write!(f, "{t}"),
            MetaValue::DateTime(d) => write!(f, "{d}"),
            MetaValue::OffsetDateTime(d) => write!(f, "{d}"),
            MetaValue::Instant(i) => write!(f, "{i}"),
            MetaValue::List(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            MetaValue::Record(fields) => {
                f.write_str("{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{k}={v}")?;
                }
                f.write_str("}")
            }
            MetaValue::Issues(issues) => write!(f, "[{}]", issues.to_string().replace('\n', ", ")),
        }
    }
}

impl PartialEq for MetaValue {
    fn eq(&self, other: &Self) -> bool {
        use MetaValue::*;
        match (self, other) {
            (Bool(a), Bool(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (UInt(a), UInt(b)) => a == b,
            (Float32(a), Float32(b)) => float_same(*a, *b),
            (Float64(a), Float64(b)) => float_same(*a, *b),
            (Decimal(a), Decimal(b)) => a == b,
            (String(a), String(b)) => a == b,
            (Uuid(a), Uuid(b)) => a == b,
            (Uri(a), Uri(b)) => a == b,
            (Date(a), Date(b)) => a == b,
            (Time(a), Time(b)) => a == b,
            (DateTime(a), DateTime(b)) => a == b,
            (OffsetDateTime(a), OffsetDateTime(b)) => a == b,
            (Instant(a), Instant(b)) => a == b,
            (List(a), List(b)) => a == b,
            (Record(a), Record(b)) => a == b,
            (Issues(a), Issues(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for MetaValue {}

impl Hash for MetaValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            MetaValue::Bool(b) => b.hash(state),
            MetaValue::Int(n) => n.hash(state),
            MetaValue::UInt(n) => n.hash(state),
            MetaValue::Float32(v) => crate::value::float::Float::canonical_bits(*v).hash(state),
            MetaValue::Float64(v) => crate::value::float::Float::canonical_bits(*v).hash(state),
            MetaValue::Decimal(d) => d.hash(state),
            MetaValue::String(s) => s.hash(state),
            MetaValue::Uuid(u) => u.hash(state),
            MetaValue::Uri(u) => u.hash(state),
            MetaValue::Date(d) => d.hash(state),
            MetaValue::Time(t) => t.hash(state),
            MetaValue::DateTime(d) => d.hash(state),
            MetaValue::OffsetDateTime(d) => d.hash(state),
            MetaValue::Instant(i) => i.hash(state),
            MetaValue::List(items) => items.hash(state),
            MetaValue::Record(fields) => fields.hash(state),
            MetaValue::Issues(issues) => issues.len().hash(state),
        }
    }
}

/// Gives a type of your own the message form of a symbol, the text its `Display` writes, so that
/// it can be metadata: an element `unique` lists among `duplicates`, or the one `contains` looks
/// for. The enum an [`enum_of`](crate::json::enum_of) decodes into is such a type; with
/// [`same_by_eq!`](crate::same_by_eq) too, a list of it takes every list constraint.
///
/// ```
/// use std::fmt;
///
/// #[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// enum Color { Red, Green }
///
/// impl fmt::Display for Color {
///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         f.write_str(match self { Color::Red => "red", Color::Green => "green" })
///     }
/// }
///
/// raoh::same_by_eq!(Color);
/// raoh::meta_by_display!(Color);
///
/// use raoh::json::prelude::*;
/// let colors = enum_of([("red", Color::Red), ("green", Color::Green)]).list().unique();
/// let issues = colors.decode(&json!(["red", "RED"])).unwrap_err();
/// assert_eq!(issues.iter().next().unwrap().message(), "must not contain duplicates: [red]");
/// ```
#[macro_export]
macro_rules! meta_by_display {
    ($($t:ty),* $(,)?) => {
        $(impl ::std::convert::From<$t> for $crate::MetaValue {
            fn from(value: $t) -> Self {
                $crate::MetaValue::String(::std::string::ToString::to_string(&value))
            }
        })*
    };
}

macro_rules! from_signed {
    ($($t:ty),*) => {
        $(impl From<$t> for MetaValue {
            fn from(n: $t) -> Self {
                MetaValue::Int(i64::from(n))
            }
        })*
    };
}

from_signed!(i8, i16, i32, i64, u8, u16, u32);

impl From<u64> for MetaValue {
    fn from(n: u64) -> Self {
        i64::try_from(n).map_or(MetaValue::UInt(n), MetaValue::Int)
    }
}

impl From<usize> for MetaValue {
    fn from(n: usize) -> Self {
        MetaValue::from(n as u64)
    }
}

impl From<isize> for MetaValue {
    fn from(n: isize) -> Self {
        MetaValue::Int(n as i64)
    }
}

macro_rules! from_variant {
    ($($t:ty => $variant:ident),*) => {
        $(impl From<$t> for MetaValue {
            fn from(v: $t) -> Self {
                MetaValue::$variant(v)
            }
        })*
    };
}

from_variant!(
    bool => Bool,
    f32 => Float32,
    f64 => Float64,
    Decimal => Decimal,
    String => String,
    Uuid => Uuid,
    Uri => Uri,
    Date => Date,
    Time => Time,
    DateTime => DateTime,
    OffsetDateTime => OffsetDateTime,
    Instant => Instant,
    Issues => Issues
);

impl From<&str> for MetaValue {
    fn from(s: &str) -> Self {
        MetaValue::String(s.to_owned())
    }
}

impl From<&String> for MetaValue {
    fn from(s: &String) -> Self {
        MetaValue::String(s.clone())
    }
}

impl From<Cow<'_, str>> for MetaValue {
    fn from(s: Cow<'_, str>) -> Self {
        MetaValue::String(s.into_owned())
    }
}

impl<T: Into<MetaValue>> From<Vec<T>> for MetaValue {
    fn from(items: Vec<T>) -> Self {
        MetaValue::List(items.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<MetaValue> + Clone> From<&[T]> for MetaValue {
    fn from(items: &[T]) -> Self {
        MetaValue::List(items.iter().cloned().map(Into::into).collect())
    }
}

impl<T: Into<MetaValue>, const N: usize> From<[T; N]> for MetaValue {
    fn from(items: [T; N]) -> Self {
        MetaValue::List(items.into_iter().map(Into::into).collect())
    }
}

impl<K: Into<String>, V: Into<MetaValue>> FromIterator<(K, V)> for MetaValue {
    /// A [`Record`](Self::Record) of the pairs.
    fn from_iter<I: IntoIterator<Item = (K, V)>>(pairs: I) -> Self {
        MetaValue::Record(
            pairs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Messages;
    use serde_json::json;

    #[test]
    fn floats_keep_their_width_in_messages_and_json() {
        assert_eq!(MetaValue::from(0.1f32).to_string(), "0.1");
        assert_eq!(
            MetaValue::from(0.1f32).to_json_with(Messages::english()),
            json!(0.1)
        );
        assert_eq!(
            MetaValue::from(f64::NAN).to_json_with(Messages::english()),
            json!("NaN")
        );
        assert_eq!(MetaValue::from(-0.0).to_string(), "-0.0");
    }

    #[test]
    fn values_are_equal_as_the_value_model_compares_them() {
        assert_ne!(MetaValue::from(0.0), MetaValue::from(-0.0));
        assert_eq!(MetaValue::from(f64::NAN), MetaValue::from(-f64::NAN));
        let a: Decimal = "1.5".parse().unwrap();
        let b: Decimal = "1.50".parse().unwrap();
        assert_ne!(MetaValue::from(a), MetaValue::from(b));
        assert_ne!(MetaValue::from(1), MetaValue::from(1.0));
    }

    #[test]
    fn lists_are_written_in_brackets() {
        assert_eq!(MetaValue::from(vec!["a", "b"]).to_string(), "[a, b]");
        assert_eq!(MetaValue::from(vec![1.0, 1e7]).to_string(), "[1.0, 1.0E7]");
    }
}
