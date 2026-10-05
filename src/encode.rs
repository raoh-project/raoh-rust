//! Encoders: the way back from domain values to JSON.
//!
//! An [`Encoder`] writes a value as a [`serde_json::Value`]. It cannot fail: a value that exists
//! has been checked, so there is nothing left to report.
//!
//! ```
//! use raoh::encode::{self, Encoder};
//! use serde_json::json;
//!
//! struct User { name: String, nickname: Option<String> }
//!
//! let user = encode::object::<User>()
//!     .property("name", |u: &User| u.name.clone(), encode::string())
//!     .property_with_default("nickname", |u: &User| u.nickname.clone(), encode::string(), "-".into());
//!
//! let written = user.encode(&User { name: "Ken".into(), nickname: None });
//! assert_eq!(written, json!({"name": "Ken", "nickname": "-"}));
//! ```

use serde_json::{Map, Value};
use std::sync::Arc;

/// Writes a `T` as JSON.
///
/// A function `Fn(&T) -> Value` is an encoder too.
pub trait Encoder<T: ?Sized> {
    /// The JSON `value` is written as.
    fn encode(&self, value: &T) -> Value;
}

impl<T: ?Sized, F: Fn(&T) -> Value> Encoder<T> for F {
    fn encode(&self, value: &T) -> Value {
        self(value)
    }
}

/// An encoder whose concrete type is erased.
pub type BoxEncoder<T> = Box<dyn Encoder<T> + Send + Sync>;

/// An encoder of a string as a JSON string.
pub fn string() -> StringEncoder {
    StringEncoder
}

/// The encoder [`string`] returns.
#[derive(Clone, Copy, Debug, Default)]
pub struct StringEncoder;

impl Encoder<str> for StringEncoder {
    fn encode(&self, value: &str) -> Value {
        Value::String(value.to_owned())
    }
}

impl Encoder<String> for StringEncoder {
    fn encode(&self, value: &String) -> Value {
        Value::String(value.clone())
    }
}

type Write<T> = Arc<dyn Fn(&T) -> Value + Send + Sync>;

/// An encoder of a `T` as a JSON object with no members yet: add them with
/// [`property`](ObjectEncoder::property) and
/// [`property_with_default`](ObjectEncoder::property_with_default). The members are written in
/// the order they are added.
pub fn object<T>() -> ObjectEncoder<T> {
    ObjectEncoder {
        properties: Vec::new(),
    }
}

/// The encoder [`object`] returns.
pub struct ObjectEncoder<T> {
    properties: Vec<(String, Write<T>)>,
}

impl<T> Clone for ObjectEncoder<T> {
    fn clone(&self) -> Self {
        Self {
            properties: self.properties.clone(),
        }
    }
}

impl<T> std::fmt::Debug for ObjectEncoder<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectEncoder")
            .field(
                "members",
                &self.properties.iter().map(|(n, _)| n).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl<T> ObjectEncoder<T> {
    fn with(mut self, name: impl Into<String>, write: Write<T>) -> Self {
        let name = name.into();
        assert!(
            self.properties.iter().all(|(written, _)| *written != name),
            "two properties write the member '{name}'"
        );
        self.properties.push((name, write));
        self
    }

    /// This encoder with the member `name`, whose value is what `encoder` writes for what `getter`
    /// reads from the value being encoded.
    ///
    /// # Panics
    ///
    /// When a property already writes the member `name`.
    pub fn property<P, G, E>(self, name: impl Into<String>, getter: G, encoder: E) -> Self
    where
        G: Fn(&T) -> P + Send + Sync + 'static,
        E: Encoder<P> + Send + Sync + 'static,
    {
        self.with(name, Arc::new(move |value| encoder.encode(&getter(value))))
    }

    /// This encoder with the member `name`, whose value is what `encoder` writes for what `getter`
    /// reads, or for `default` when `getter` reads `None`.
    ///
    /// # Panics
    ///
    /// When a property already writes the member `name`.
    pub fn property_with_default<P, G, E>(
        self,
        name: impl Into<String>,
        getter: G,
        encoder: E,
        default: P,
    ) -> Self
    where
        G: Fn(&T) -> Option<P> + Send + Sync + 'static,
        E: Encoder<P> + Send + Sync + 'static,
        P: Send + Sync + 'static,
    {
        self.with(
            name,
            Arc::new(move |value| match getter(value) {
                Some(read) => encoder.encode(&read),
                None => encoder.encode(&default),
            }),
        )
    }
}

impl<T> Encoder<T> for ObjectEncoder<T> {
    fn encode(&self, value: &T) -> Value {
        Value::Object(
            self.properties
                .iter()
                .map(|(name, write)| (name.clone(), write(value)))
                .collect::<Map<String, Value>>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_property_with_default_writes_the_default_for_none() {
        let encoder = object::<Option<String>>().property_with_default(
            "value",
            Option::clone,
            string(),
            "default".to_owned(),
        );
        assert_eq!(encoder.encode(&None), json!({"value": "default"}));
        assert_eq!(
            encoder.encode(&Some("hello".into())),
            json!({"value": "hello"})
        );
    }

    #[test]
    #[should_panic(expected = "two properties write the member 'a'")]
    fn two_properties_cannot_write_one_member() {
        let _ = object::<String>()
            .property("a", String::clone, string())
            .property("a", String::clone, string());
    }
}
