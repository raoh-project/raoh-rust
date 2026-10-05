//! Writing an issue's message in a person's language.

use crate::issue::Issue;
use crate::message_keys;
use crate::meta::MetaValue;
use crate::properties;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, LazyLock};

/// Writes the sentence for an issue that has no custom message.
///
/// [`Issue::message_with`] calls it only when the issue carries no custom message, so an
/// implementation does not look for one. A closure `Fn(&Issue) -> String` is a resolver too.
pub trait MessageResolver {
    /// The sentence for `issue`.
    fn resolve(&self, issue: &Issue) -> String;
}

impl<F: Fn(&Issue) -> String> MessageResolver for F {
    fn resolve(&self, issue: &Issue) -> String {
        self(issue)
    }
}

/// A catalogue of message templates keyed by message key or code, over the catalogue it falls
/// back to.
///
/// A catalogue is a stack of layers, most specific first, as Raoh for Java's
/// `ResourceBundleMessageResolver` reads a locale's `.properties` file before its parent's. An
/// issue is looked up in one layer at a time: by its message key, then by its code, and only when
/// neither has a template in the next layer down. So a layer that translates just
/// `invalid_format` wins over a refined key such as `invalid_format.email` in the layer beneath
/// it. A template's `{name}` placeholders are filled with the message forms of the issue's
/// metadata, as the Raoh Specification writes them; a placeholder naming an entry the metadata
/// lacks stays as it is written. When no layer has a template, the sentence is
/// `validation failed: <code>`.
///
/// ```
/// use raoh::{Issue, MessageResolver, Messages};
///
/// let issue = Issue::new("too_short").with_meta("min", 3);
/// assert_eq!(Messages::japanese().resolve(&issue), "3文字以上で入力してください");
///
/// let mine = Messages::english().with_overrides([("too_short", "{min}+ characters, please")]);
/// assert_eq!(mine.resolve(&issue), "3+ characters, please");
///
/// let french = Messages::from_properties("raoh.invalid_format=format invalide")
///     .unwrap()
///     .falling_back_to(Messages::english());
/// let email = Issue::new("invalid_format").with_message_key("invalid_format.email");
/// assert_eq!(french.resolve(&email), "format invalide");
/// ```
#[derive(Clone, Debug, Default)]
pub struct Messages {
    templates: HashMap<String, String>,
    parent: Option<Arc<Messages>>,
}

/// The English catalogue of the Raoh Specification, word for word, under a layer of this crate's
/// own: the template for `invalid_format.json`, for text that is not JSON, which the specification
/// leaves outside its input model. The layer is over the catalogue so that its key is found before
/// the catalogue's `invalid_format`.
static ENGLISH: LazyLock<Messages> = LazyLock::new(|| {
    Messages::from_properties(include_str!("messages/en.properties"))
        .expect("the English catalogue is well formed")
        .with_overrides([(message_keys::INVALID_FORMAT_JSON, "not valid JSON")])
});

/// The Japanese catalogue of the Raoh Specification under this crate's own Japanese template,
/// over the English one, as `messages_ja.properties` sits over `messages.properties`.
static JAPANESE: LazyLock<Messages> = LazyLock::new(|| {
    Messages::from_properties(include_str!("messages/ja.properties"))
        .expect("the Japanese catalogue is well formed")
        .with_overrides([(message_keys::INVALID_FORMAT_JSON, "JSONとして読めません")])
        .falling_back_to(&ENGLISH)
});

impl Messages {
    /// The English catalogue. It is the one every issue's [`message`](Issue::message) comes
    /// from.
    pub fn english() -> &'static Messages {
        &ENGLISH
    }

    /// The Japanese catalogue, over the English one.
    pub fn japanese() -> &'static Messages {
        &JAPANESE
    }

    /// An empty catalogue, in which every issue reads `validation failed: <code>`.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Reads a catalogue written as Java's `Properties.load` reads a `.properties` file, such as
    /// the `messages*.properties` of Raoh for Java: one `raoh.<key>=<template>` per entry,
    /// `\uXXXX` escapes and continued lines included. The `raoh.` prefix is optional. The
    /// catalogue falls back to nothing; give it one with [`falling_back_to`](Self::falling_back_to).
    pub fn from_properties(text: &str) -> Result<Self, PropertiesError> {
        let pairs = properties::load_properties(text).map_err(|e| PropertiesError {
            line: e.line,
            reason: e.reason,
        })?;
        let templates = pairs
            .into_iter()
            .map(|(key, template)| match key.strip_prefix("raoh.") {
                Some(bare) => (bare.to_owned(), template),
                None => (key, template),
            })
            .collect();
        Ok(Self {
            templates,
            parent: None,
        })
    }

    /// This catalogue with `parent` beneath its last layer, as a locale's file sits over its
    /// parent's.
    pub fn falling_back_to(self, parent: &Messages) -> Self {
        let parent = match self.parent {
            Some(own) => Arc::new((*own).clone().falling_back_to(parent)),
            None => Arc::new(parent.clone()),
        };
        Self {
            templates: self.templates,
            parent: Some(parent),
        }
    }

    /// A layer of `overrides` over this catalogue.
    pub fn with_overrides<K, T>(&self, overrides: impl IntoIterator<Item = (K, T)>) -> Self
    where
        K: Into<String>,
        T: Into<String>,
    {
        Self {
            templates: overrides
                .into_iter()
                .map(|(k, t)| (k.into(), t.into()))
                .collect(),
            parent: Some(Arc::new(self.clone())),
        }
    }

    /// Each layer, most specific first.
    fn layers(&self) -> impl Iterator<Item = &Messages> {
        std::iter::successors(Some(self), |layer| layer.parent.as_deref())
    }

    /// Every key and the template the most specific layer holding it has, in no particular
    /// order.
    pub fn templates(&self) -> impl Iterator<Item = (&str, &str)> {
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for layer in self.layers() {
            for (key, template) in &layer.templates {
                seen.entry(key.as_str()).or_insert(template.as_str());
            }
        }
        seen.into_iter()
    }

    /// The template the most specific layer holding `key` has, if one does.
    pub fn template(&self, key: &str) -> Option<&str> {
        self.layers()
            .find_map(|layer| layer.templates.get(key))
            .map(String::as_str)
    }
}

impl MessageResolver for Messages {
    fn resolve(&self, issue: &Issue) -> String {
        self.layers()
            .find_map(|layer| {
                [issue.message_key(), issue.code()]
                    .into_iter()
                    .find_map(|key| layer.templates.get(key))
            })
            .map(|template| fill(template, issue.meta()))
            .unwrap_or_else(|| format!("validation failed: {}", issue.code()))
    }
}

/// Where [`Messages::from_properties`] stopped reading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertiesError {
    line: usize,
    reason: &'static str,
}

impl PropertiesError {
    /// The line the malformed entry starts on, counting from 1.
    pub fn line(&self) -> usize {
        self.line
    }
}

impl fmt::Display for PropertiesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.reason)
    }
}

impl std::error::Error for PropertiesError {}

/// `template` with each `{name}` replaced by the message form of the metadata entry `name`. A
/// placeholder with no entry of that name stays as it is written.
fn fill(template: &str, meta: &BTreeMap<String, MetaValue>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}').map(|close| (close, &after[..close])) {
            Some((close, name)) if is_placeholder_name(name) => {
                match meta.get(name) {
                    Some(value) => out.push_str(&value.to_string()),
                    None => out.push_str(&rest[open..open + close + 2]),
                }
                rest = &after[close + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn is_placeholder_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{codes, message_keys};

    #[test]
    fn the_message_key_is_looked_up_before_the_code() {
        let issue = Issue::new(codes::OUT_OF_RANGE)
            .with_message_key(message_keys::OUT_OF_RANGE_POSITIVE)
            .with_meta("min", 1);
        assert_eq!(
            Messages::japanese().resolve(&issue),
            "正の値で入力してください"
        );
    }

    #[test]
    fn a_placeholder_with_no_entry_stays_as_it_is_written() {
        let issue = Issue::new(codes::OUT_OF_RANGE).with_meta("min", 1);
        assert_eq!(
            Messages::english().resolve(&issue),
            "must be between 1 and {max}"
        );
    }

    #[test]
    fn a_refined_key_falls_back_to_its_code_in_a_java_catalogue() {
        let java = Messages::from_properties("raoh.invalid_format=bad form").unwrap();
        let issue =
            Issue::new(codes::INVALID_FORMAT).with_message_key(message_keys::INVALID_FORMAT_EMAIL);
        assert_eq!(java.resolve(&issue), "bad form");
    }

    #[test]
    fn lists_and_fractions_are_written_as_java_writes_them() {
        let issue = Issue::new(codes::NOT_ALLOWED).with_meta("allowed", vec!["a", "b"]);
        assert_eq!(Messages::english().resolve(&issue), "must be one of [a, b]");
        let issue = Issue::new(codes::OUT_OF_RANGE)
            .with_message_key(message_keys::OUT_OF_RANGE_MINIMUM)
            .with_meta("min", 1e7);
        assert_eq!(
            Messages::english().resolve(&issue),
            "must be at least 1.0E7"
        );
    }

    #[test]
    fn escaped_catalogues_read_as_java_reads_them() {
        let messages =
            Messages::from_properties("raoh.required=\\u5fc5\\u9808\nraoh.blank : empty").unwrap();
        assert_eq!(messages.template("required"), Some("必須"));
        assert_eq!(messages.template("blank"), Some("empty"));
        assert_eq!(Messages::from_properties("x=\\u12").unwrap_err().line(), 1);
    }

    #[test]
    fn a_partial_translation_wins_over_a_refined_key_beneath_it() {
        let french = Messages::from_properties("raoh.invalid_format=format invalide")
            .unwrap()
            .falling_back_to(Messages::english());
        let email =
            Issue::new(codes::INVALID_FORMAT).with_message_key(message_keys::INVALID_FORMAT_EMAIL);
        assert_eq!(french.resolve(&email), "format invalide");
        let overridden = Messages::english().with_overrides([("invalid_format", "bad form")]);
        assert_eq!(overridden.resolve(&email), "bad form");
    }

    #[test]
    fn falling_back_goes_beneath_every_layer_there_is() {
        let top = Messages::from_properties("raoh.blank=top")
            .unwrap()
            .falling_back_to(&Messages::from_properties("raoh.required=middle").unwrap())
            .falling_back_to(Messages::english());
        assert_eq!(top.resolve(&Issue::new(codes::BLANK)), "top");
        assert_eq!(top.resolve(&Issue::new(codes::REQUIRED)), "middle");
        assert_eq!(
            top.resolve(&Issue::new(codes::TOO_BIG).with_meta("max", 2)),
            "must have at most 2 elements"
        );
    }

    #[test]
    fn a_closure_is_a_resolver() {
        let upper = |issue: &Issue| issue.code().to_uppercase();
        assert_eq!(upper.resolve(&Issue::new("blank")), "BLANK");
    }

    /// Every code and message key has a sentence in each catalogue, and the Japanese one does not
    /// fall back to English for any of them.
    #[test]
    fn the_catalogues_cover_every_code_and_message_key() {
        let english = Messages::english();
        let japanese_alone = Messages::from_properties(include_str!("messages/ja.properties"))
            .unwrap()
            .with_overrides([(message_keys::INVALID_FORMAT_JSON, "")]);
        for key in codes::ALL.iter().chain(message_keys::ALL) {
            assert!(
                english.template(key).is_some(),
                "no English template for {key}"
            );
            assert!(
                japanese_alone.template(key).is_some(),
                "no Japanese template for {key}"
            );
        }
    }
}
