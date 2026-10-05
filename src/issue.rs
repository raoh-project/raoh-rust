//! What a failed decode reports.

use crate::message::{MessageResolver, Messages};
use crate::meta::MetaValue;
use crate::path::{Path, Pointer};
use indexmap::IndexMap;
use serde::ser::{Serialize, SerializeSeq, Serializer};
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

/// One problem found in the input: where it is, what kind it is, and what else its code says.
///
/// An issue carries no sentence of its own. Its `code`, `message_key` and `meta` are what a
/// program reads; the sentence a person reads is written from them by a [`MessageResolver`], by
/// default the English catalogue [`Messages::english`]. A message given with
/// [`with_message`](Self::with_message) replaces that sentence in every language.
///
/// ```
/// use raoh::{Issue, Messages};
///
/// let issue = Issue::new("too_short").with_meta("min", 3);
/// assert_eq!(issue.message(), "must be at least 3 characters");
/// assert_eq!(issue.message_with(Messages::japanese()), "3文字以上で入力してください");
///
/// let mine = Issue::new("checksum").with_message("the check digit does not match");
/// assert_eq!(mine.message_with(Messages::japanese()), "the check digit does not match");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    inner: Box<Inner>,
}

/// Held behind a box so an `Issue`, and a `Result` whose error is one, stays one pointer wide.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Inner {
    path: Pointer,
    code: Cow<'static, str>,
    message_key: Cow<'static, str>,
    meta: BTreeMap<String, MetaValue>,
    message: Option<String>,
    /// Whether a strict decoder reported this member as one it does not know, so that a strict
    /// decoder around it does not report the member again.
    unknown_member: bool,
}

impl Issue {
    /// An issue with `code` at the root, with the code as its message key.
    ///
    /// Returned from a function given to [`Decoder::and_then`](crate::Decoder::and_then), it is
    /// moved to the path the decoder is at.
    pub fn new(code: impl Into<Cow<'static, str>>) -> Self {
        let code = code.into();
        Self {
            inner: Box::new(Inner {
                path: Pointer::root(),
                message_key: code.clone(),
                code,
                meta: BTreeMap::new(),
                message: None,
                unknown_member: false,
            }),
        }
    }

    /// An issue at `path`.
    pub(crate) fn at_path(path: &Path<'_>, code: impl Into<Cow<'static, str>>) -> Self {
        Self::new(code).at(path.to_pointer())
    }

    /// This issue at `path` instead.
    pub fn at(mut self, path: Pointer) -> Self {
        self.inner.path = path;
        self
    }

    /// This issue with `key` naming the check that produced it.
    pub fn with_message_key(mut self, key: impl Into<Cow<'static, str>>) -> Self {
        self.inner.message_key = key.into();
        self
    }

    /// This issue with one more entry of metadata.
    pub fn with_meta(mut self, key: impl Into<String>, value: impl Into<MetaValue>) -> Self {
        self.inner.meta.insert(key.into(), value.into());
        self
    }

    /// This issue with `message` as its sentence in every language, in place of the catalogue's.
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.inner.message = Some(message.into());
        self
    }

    /// Where in the input the problem is.
    pub fn path(&self) -> &Pointer {
        &self.inner.path
    }

    /// What kind of problem it is, one of [`codes`](crate::codes) or a caller's own.
    pub fn code(&self) -> &str {
        &self.inner.code
    }

    /// The key a catalogue looks up first: the code, or a refinement of it from
    /// [`message_keys`](crate::message_keys).
    pub fn message_key(&self) -> &str {
        &self.inner.message_key
    }

    /// What else the code says about the problem, such as the bound a value fell outside of,
    /// in the order of its keys, as Raoh for Java keeps it.
    pub fn meta(&self) -> &BTreeMap<String, MetaValue> {
        &self.inner.meta
    }

    /// This issue marked as a strict decoder's report of a member it does not know.
    pub(crate) fn marked_unknown_member(mut self) -> Self {
        self.inner.unknown_member = true;
        self
    }

    /// Whether a strict decoder reported this issue's member as one it does not know.
    pub(crate) fn is_unknown_member(&self) -> bool {
        self.inner.unknown_member
    }

    /// The message given with [`with_message`](Self::with_message), if there is one.
    pub fn custom_message(&self) -> Option<&str> {
        self.inner.message.as_deref()
    }

    /// The sentence a person reads, in English.
    pub fn message(&self) -> String {
        self.message_with(Messages::english())
    }

    /// The sentence a person reads: the custom message if there is one, or what `resolver` writes.
    pub fn message_with(&self, resolver: &(impl MessageResolver + ?Sized)) -> String {
        match &self.inner.message {
            Some(message) => message.clone(),
            None => resolver.resolve(self),
        }
    }

    /// This issue read as relative to `prefix`.
    pub fn rebase(mut self, prefix: &Path<'_>) -> Self {
        if !prefix.is_root() {
            self.inner.path = self.inner.path.prefixed(prefix);
        }
        self
    }

    /// The issue as a JSON object of `path`, `code`, `message` and `meta`, with the message written
    /// by `resolver`.
    pub fn to_json_with(&self, resolver: &(impl MessageResolver + ?Sized)) -> Value {
        let mut object = Map::new();
        object.insert("path".into(), self.inner.path.to_string().into());
        object.insert("code".into(), self.code().into());
        object.insert("message".into(), self.message_with(resolver).into());
        let meta: Map<String, Value> = self
            .inner
            .meta
            .iter()
            .map(|(k, v)| (k.clone(), v.to_json_with(resolver)))
            .collect();
        object.insert("meta".into(), Value::Object(meta));
        Value::Object(object)
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.inner.path.is_root() {
            write!(f, "(root): {}", self.message())
        } else {
            write!(f, "{}: {}", self.inner.path, self.message())
        }
    }
}

impl std::error::Error for Issue {}

/// Written as `{"path", "code", "message", "meta"}` with the English message, as
/// [`Issues::to_json`] writes each issue.
impl Serialize for Issue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_json_with(Messages::english()).serialize(serializer)
    }
}

/// Every issue a decode found, in the order it found them.
///
/// A decode that fails returns at least one issue.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Issues(Vec<Issue>);

impl Issues {
    /// No issues.
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Each issue in order.
    pub fn iter(&self) -> std::slice::Iter<'_, Issue> {
        self.0.iter()
    }

    /// The issues as a slice.
    pub fn as_slice(&self) -> &[Issue] {
        &self.0
    }

    /// Adds `issue` at the end.
    pub fn push(&mut self, issue: Issue) {
        self.0.push(issue);
    }

    /// Adds every issue of `other` at the end.
    pub fn merge(&mut self, other: Issues) {
        self.0.extend(other.0);
    }

    /// These issues read as relative to `prefix`.
    pub fn rebase(self, prefix: &Path<'_>) -> Self {
        if prefix.is_root() {
            return self;
        }
        Self(self.0.into_iter().map(|i| i.rebase(prefix)).collect())
    }

    /// The English messages grouped by the JSON Pointer of their path, in the order each path
    /// was first seen.
    pub fn flatten(&self) -> IndexMap<String, Vec<String>> {
        self.flatten_with(Messages::english())
    }

    /// As [`flatten`](Self::flatten), with the messages written by `resolver`.
    pub fn flatten_with(
        &self,
        resolver: &(impl MessageResolver + ?Sized),
    ) -> IndexMap<String, Vec<String>> {
        let mut grouped: IndexMap<String, Vec<String>> = IndexMap::new();
        for issue in &self.0 {
            grouped
                .entry(issue.path().to_string())
                .or_default()
                .push(issue.message_with(resolver));
        }
        grouped
    }

    /// The issues as a JSON array of `{"path", "code", "message", "meta"}` objects with English
    /// messages, the form Raoh for Java and PHP give them in. `serde_json::to_value(&issues)`
    /// gives the same.
    pub fn to_json(&self) -> Value {
        self.to_json_with(Messages::english())
    }

    /// As [`to_json`](Self::to_json), with the messages written by `resolver`.
    pub fn to_json_with(&self, resolver: &(impl MessageResolver + ?Sized)) -> Value {
        Value::Array(self.0.iter().map(|i| i.to_json_with(resolver)).collect())
    }
}

impl From<Issue> for Issues {
    fn from(issue: Issue) -> Self {
        Self(vec![issue])
    }
}

impl From<Vec<Issue>> for Issues {
    fn from(issues: Vec<Issue>) -> Self {
        Self(issues)
    }
}

impl FromIterator<Issue> for Issues {
    fn from_iter<T: IntoIterator<Item = Issue>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl Extend<Issue> for Issues {
    fn extend<T: IntoIterator<Item = Issue>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}

impl IntoIterator for Issues {
    type Item = Issue;
    type IntoIter = std::vec::IntoIter<Issue>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Issues {
    type Item = &'a Issue;
    type IntoIter = std::slice::Iter<'a, Issue>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl fmt::Display for Issues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, issue) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            issue.fmt(f)?;
        }
        Ok(())
    }
}

impl std::error::Error for Issues {}

/// Written as [`Issues::to_json`] writes them.
impl Serialize for Issues {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for issue in &self.0 {
            seq.serialize_element(issue)?;
        }
        seq.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes;
    use serde_json::json;

    fn at(segment: &str) -> Pointer {
        [segment].into_iter().collect()
    }

    #[test]
    fn rebase_puts_the_prefix_before_the_issue_path() {
        let issue = Issue::new(codes::REQUIRED).at(at("id"));
        let user = Path::ROOT.key("user");
        assert_eq!(issue.rebase(&user).path().to_string(), "/user/id");
    }

    #[test]
    fn flatten_keeps_the_order_paths_were_first_seen() {
        let issues: Issues = vec![
            Issue::new(codes::REQUIRED).at(at("b")),
            Issue::new(codes::BLANK).at(at("a")),
            Issue::new(codes::BLANK).at(at("b")),
        ]
        .into();
        let flat = issues.flatten();
        assert_eq!(flat.keys().collect::<Vec<_>>(), ["/b", "/a"]);
        assert_eq!(flat["/b"], ["is required", "must not be blank"]);
    }

    #[test]
    fn serialize_is_the_same_as_to_json() {
        let issues: Issues = Issue::new(codes::TOO_SHORT)
            .with_meta("min", 3)
            .at(at("name"))
            .into();
        let expected = json!([{
            "path": "/name",
            "code": "too_short",
            "message": "must be at least 3 characters",
            "meta": {"min": 3}
        }]);
        assert_eq!(issues.to_json(), expected);
        assert_eq!(serde_json::to_value(&issues).unwrap(), expected);
    }

    #[test]
    fn a_custom_message_is_kept_in_every_language() {
        let issue = Issue::new(codes::REQUIRED).with_message("give a name");
        assert_eq!(issue.message(), "give a name");
        assert_eq!(issue.message_with(Messages::japanese()), "give a name");
        let issues: Issues = issue.into();
        assert_eq!(
            issues.to_json_with(Messages::japanese())[0]["message"],
            "give a name"
        );
    }

    #[test]
    fn a_code_no_catalogue_knows_says_which_code_it_is() {
        assert_eq!(
            Issue::new("checksum").message(),
            "validation failed: checksum"
        );
    }
}
