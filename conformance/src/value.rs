//! The types and values of the value model as the runner holds them, and their observations
//! (spec/value-model.md, spec/observation.md).

use indexmap::{IndexMap, IndexSet};
use raoh::{
    Date, DateTime, Decimal, Instant, Issue, Issues, MetaValue, OffsetDateTime, Presence, Time,
    Uri, Uuid,
};
use serde_json::{Map, Value};
use std::hash::{Hash, Hasher};

/// A type of the value model, as catalog/operations.json writes one.
#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    Bool,
    Int32,
    Int64,
    Float32,
    Float64,
    Decimal,
    String,
    Symbol(Vec<String>),
    Uuid,
    Uri,
    Date,
    Time,
    DateTime,
    OffsetDateTime,
    Instant,
    List(Box<Ty>),
    Set(Box<Ty>),
    Map(Box<Ty>),
    Product(Vec<Ty>),
    Presence(Box<Ty>),
    Optional(Box<Ty>),
    Nullable(Box<Ty>),
}

impl Ty {
    /// The kind a feature names an operation on this receiver by: `int32`, `list`, `map`.
    pub fn kind(&self) -> &'static str {
        match self {
            Ty::Bool => "bool",
            Ty::Int32 => "int32",
            Ty::Int64 => "int64",
            Ty::Float32 => "float32",
            Ty::Float64 => "float64",
            Ty::Decimal => "decimal",
            Ty::String => "string",
            Ty::Symbol(_) => "symbol",
            Ty::Uuid => "uuid",
            Ty::Uri => "uri",
            Ty::Date => "date",
            Ty::Time => "time",
            Ty::DateTime => "datetime",
            Ty::OffsetDateTime => "offset_datetime",
            Ty::Instant => "instant",
            Ty::List(_) => "list",
            Ty::Set(_) => "set",
            Ty::Map(_) => "map",
            Ty::Product(_) => "product",
            Ty::Presence(_) => "presence",
            Ty::Optional(_) => "optional",
            Ty::Nullable(_) => "nullable",
        }
    }
}

/// A value of the value model. Scalars are held as raoh's [`MetaValue`], which compares them as
/// the value model does; a symbol is its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum V {
    Scalar(MetaValue),
    List(Vec<V>),
    Set(IndexSet<V>),
    Map(IndexMap<String, V>),
    Product(Vec<V>),
    /// An optional's or a nullable's value: `None` is empty or null.
    Opt(Option<Box<V>>),
    Presence(Presence<Box<V>>),
}

impl Hash for V {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            V::Scalar(m) => m.hash(state),
            V::List(items) | V::Product(items) => items.hash(state),
            V::Set(items) => items.len().hash(state),
            V::Map(members) => members.len().hash(state),
            V::Opt(v) => v.hash(state),
            V::Presence(p) => p.hash(state),
        }
    }
}

/// The value model's sameness, which `V`'s `Eq` already is: scalars compare as raoh's
/// `MetaValue` does, sets and maps in any order.
impl raoh::Same for V {
    fn same(&self, other: &Self) -> bool {
        self == other
    }

    fn same_hash<H: Hasher>(&self, state: &mut H) {
        self.hash(state);
    }
}

impl From<V> for MetaValue {
    /// The value as metadata, which only a scalar or a list of scalars is in the suite.
    fn from(v: V) -> Self {
        match v {
            V::Scalar(m) => m,
            V::List(items) | V::Product(items) => {
                MetaValue::List(items.into_iter().map(Into::into).collect())
            }
            other => MetaValue::String(format!("{other:?}")),
        }
    }
}

impl V {
    pub fn scalar(m: impl Into<MetaValue>) -> Self {
        V::Scalar(m.into())
    }

    pub fn as_i32(&self) -> Result<i32, String> {
        match self {
            V::Scalar(MetaValue::Int(n)) => i32::try_from(*n).map_err(|e| e.to_string()),
            other => Err(format!("{other:?} is not an int32")),
        }
    }

    pub fn into_string(self) -> Result<String, String> {
        match self {
            V::Scalar(MetaValue::String(s)) => Ok(s),
            other => Err(format!("{other:?} is not a string")),
        }
    }

    pub fn into_product(self) -> Result<Vec<V>, String> {
        match self {
            V::Product(items) => Ok(items),
            other => Err(format!("{other:?} is not a product")),
        }
    }
}

/// Reads `json` as an observation of `ty`, as a case writes a value argument or an encoder's
/// input.
pub fn read(ty: &Ty, json: &Value) -> Result<V, String> {
    let wrong = || format!("{json} is not an observation of {ty:?}");
    let text = || json.as_str().ok_or_else(wrong);
    Ok(match ty {
        Ty::Bool => V::scalar(json.as_bool().ok_or_else(wrong)?),
        Ty::Int32 => V::scalar(
            json.as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .ok_or_else(wrong)?,
        ),
        Ty::Int64 => V::scalar(json.as_i64().ok_or_else(wrong)?),
        Ty::Float32 => V::scalar(read_float::<f32>(json).ok_or_else(wrong)?),
        Ty::Float64 => V::scalar(read_float::<f64>(json).ok_or_else(wrong)?),
        Ty::Decimal => V::scalar(text()?.parse::<Decimal>().map_err(|e| e.to_string())?),
        Ty::String | Ty::Symbol(_) => V::scalar(text()?),
        Ty::Uuid => V::scalar(text()?.parse::<Uuid>().map_err(|e| e.to_string())?),
        Ty::Uri => V::scalar(text()?.parse::<Uri>().map_err(|e| e.to_string())?),
        Ty::Date => V::scalar(text()?.parse::<Date>().map_err(|e| e.to_string())?),
        Ty::Time => V::scalar(text()?.parse::<Time>().map_err(|e| e.to_string())?),
        Ty::DateTime => V::scalar(text()?.parse::<DateTime>().map_err(|e| e.to_string())?),
        Ty::OffsetDateTime => V::scalar(
            text()?
                .parse::<OffsetDateTime>()
                .map_err(|e| e.to_string())?,
        ),
        Ty::Instant => V::scalar(text()?.parse::<Instant>().map_err(|e| e.to_string())?),
        Ty::List(element) => V::List(
            json.as_array()
                .ok_or_else(wrong)?
                .iter()
                .map(|e| read(element, e))
                .collect::<Result<_, _>>()?,
        ),
        Ty::Set(element) => V::Set(
            json.as_array()
                .ok_or_else(wrong)?
                .iter()
                .map(|e| read(element, e))
                .collect::<Result<_, _>>()?,
        ),
        Ty::Map(element) => V::Map(
            json.as_object()
                .ok_or_else(wrong)?
                .iter()
                .map(|(k, e)| Ok((k.clone(), read(element, e)?)))
                .collect::<Result<_, String>>()?,
        ),
        Ty::Product(parts) => {
            let items = json.as_array().ok_or_else(wrong)?;
            if items.len() != parts.len() {
                return Err(wrong());
            }
            V::Product(
                parts
                    .iter()
                    .zip(items)
                    .map(|(t, e)| read(t, e))
                    .collect::<Result<_, _>>()?,
            )
        }
        Ty::Presence(inner) => match json {
            Value::String(s) if s == "absent" => V::Presence(Presence::Absent),
            Value::String(s) if s == "null" => V::Presence(Presence::Null),
            Value::Object(o) if o.len() == 1 && o.contains_key("present") => {
                V::Presence(Presence::Present(Box::new(read(inner, &o["present"])?)))
            }
            _ => return Err(wrong()),
        },
        Ty::Optional(inner) | Ty::Nullable(inner) => match json {
            Value::Null => V::Opt(None),
            other => V::Opt(Some(Box::new(read(inner, other)?))),
        },
    })
}

/// A float written as a JSON number, read from its text so that it is rounded to the type once,
/// or as a tag.
fn read_float<F: std::str::FromStr>(json: &Value) -> Option<F> {
    match json {
        Value::Number(n) => n.to_string().parse().ok(),
        Value::Object(tag) if tag.len() == 1 => {
            let text = match tag.get("float")?.as_str()? {
                "-0" => "-0",
                "NaN" => "NaN",
                "+Infinity" => "inf",
                "-Infinity" => "-inf",
                _ => return None,
            };
            text.parse().ok()
        }
        _ => None,
    }
}

/// A JSON number of the text `text`, which serde_json keeps as written.
fn number(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| panic!("{text} is not a JSON number"))
}

/// Writes `v`, a value of `ty`, as its observation.
pub fn observe(ty: &Ty, v: &V) -> Result<Value, String> {
    let mismatch = || format!("{v:?} is not a value of {ty:?}");
    Ok(match (ty, v) {
        (_, V::Scalar(m)) => observe_meta(m),
        (Ty::List(element), V::List(items)) => Value::Array(
            items
                .iter()
                .map(|item| observe(element, item))
                .collect::<Result<_, _>>()?,
        ),
        (Ty::Set(element), V::Set(items)) => Value::Array(
            items
                .iter()
                .map(|item| observe(element, item))
                .collect::<Result<_, _>>()?,
        ),
        (Ty::Map(element), V::Map(members)) => Value::Object(
            members
                .iter()
                .map(|(k, item)| Ok((k.clone(), observe(element, item)?)))
                .collect::<Result<Map<_, _>, String>>()?,
        ),
        (Ty::Product(parts), V::Product(items)) if parts.len() == items.len() => Value::Array(
            parts
                .iter()
                .zip(items)
                .map(|(t, item)| observe(t, item))
                .collect::<Result<_, _>>()?,
        ),
        (Ty::Optional(inner) | Ty::Nullable(inner), V::Opt(o)) => match o {
            None => Value::Null,
            Some(item) => observe(inner, item)?,
        },
        (Ty::Presence(inner), V::Presence(p)) => match p {
            Presence::Absent => Value::String("absent".into()),
            Presence::Null => Value::String("null".into()),
            Presence::Present(item) => {
                let mut o = Map::new();
                o.insert("present".into(), observe(inner, item)?);
                Value::Object(o)
            }
        },
        _ => return Err(mismatch()),
    })
}

/// Writes a scalar, or metadata, as its observation: by the type raoh holds it as.
pub fn observe_meta(m: &MetaValue) -> Value {
    match m {
        MetaValue::Bool(b) => Value::Bool(*b),
        MetaValue::Int(n) => Value::from(*n),
        MetaValue::UInt(n) => Value::from(*n),
        MetaValue::Float32(f) => observe_float(f64::from(*f), *f == 0.0, || m.to_string()),
        MetaValue::Float64(f) => observe_float(*f, *f == 0.0, || m.to_string()),
        MetaValue::List(items) => Value::Array(items.iter().map(observe_meta).collect()),
        MetaValue::Record(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), observe_meta(v)))
                .collect(),
        ),
        MetaValue::Issues(issues) => Value::Array(issues.iter().map(candidate_issue).collect()),
        // Decimals, strings, UUIDs, URIs and temporal values are written as their text.
        other => Value::String(other.to_string()),
    }
}

/// A float as a JSON number, its canonical decimal, which raoh's message form writes; or as a tag
/// where JSON cannot carry it.
fn observe_float(widened: f64, zero: bool, message: impl Fn() -> String) -> Value {
    let tag = |t: &str| {
        let mut o = Map::new();
        o.insert("float".into(), Value::String(t.into()));
        Value::Object(o)
    };
    if widened.is_nan() {
        tag("NaN")
    } else if widened.is_infinite() {
        tag(if widened > 0.0 {
            "+Infinity"
        } else {
            "-Infinity"
        })
    } else if zero && widened.is_sign_negative() {
        tag("-0")
    } else if zero {
        number("0")
    } else {
        number(&message())
    }
}

/// An issue as a case writes one, with its English message.
pub fn write_issue(issue: &Issue) -> Value {
    let mut o = Map::new();
    o.insert("path".into(), Value::String(issue.path().to_string()));
    o.insert("code".into(), Value::String(issue.code().into()));
    o.insert(
        "message_key".into(),
        Value::String(issue.message_key().into()),
    );
    o.insert("message".into(), Value::String(issue.message()));
    o.insert("meta".into(), meta_object(issue));
    Value::Object(o)
}

/// An issue a `one_of_failed` lists, as raoh-java writes it: a path, a code, a message and
/// metadata, and no message key.
fn candidate_issue(issue: &Issue) -> Value {
    let mut o = Map::new();
    o.insert("path".into(), Value::String(issue.path().to_string()));
    o.insert("code".into(), Value::String(issue.code().into()));
    o.insert("message".into(), Value::String(issue.message()));
    o.insert("meta".into(), meta_object(issue));
    Value::Object(o)
}

fn meta_object(issue: &Issue) -> Value {
    Value::Object(
        issue
            .meta()
            .iter()
            .map(|(k, v)| (k.clone(), observe_meta(v)))
            .collect(),
    )
}

pub fn write_issues(issues: &Issues) -> Value {
    Value::Array(issues.iter().map(write_issue).collect())
}
