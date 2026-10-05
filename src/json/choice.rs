use super::string::{StringDecoder, string};
use super::{Json, View};
use super::{missing, type_mismatch};
use crate::decoder::Decoder;
use crate::issue::{Issue, Issues};
use crate::path::Path;
use crate::{codes, message_keys};
use std::borrow::Cow;

/// A decoder of a string naming one of `variants`, matched without regard to ASCII case.
///
/// The string is read with [`string()`], or with the decoder given to [`using`](EnumOf::using),
/// whose issues are reported as they are. Only `A`–`Z` and `a`–`z` are folded: `"RED"` matches
/// `red`, and a non-ASCII letter matches only itself. A string naming none is `invalid_format`
/// with the names, ASCII lower-cased and sorted by code point, as `allowed`.
///
/// ```
/// use raoh::json::prelude::*;
///
/// #[derive(Clone, Debug, PartialEq)]
/// enum Color { Red, Green }
///
/// let color = enum_of([("red", Color::Red), ("green", Color::Green)]).using(string().trim());
/// assert_eq!(color.decode(&json!(" RED ")).unwrap(), Color::Red);
/// assert!(color.decode(&json!("blue")).is_err());
/// ```
///
/// # Panics
///
/// When two names are equal under ASCII case folding, such as `a` and `A`.
pub fn enum_of<'a, T: Clone>(
    variants: impl IntoIterator<Item = (&'a str, T)>,
) -> EnumOf<T, StringDecoder> {
    let variants: Vec<(String, T)> = variants
        .into_iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), value))
        .collect();
    for (i, (name, _)) in variants.iter().enumerate() {
        if variants[..i].iter().any(|(earlier, _)| earlier == name) {
            panic!("enum names equal to '{name}' under ASCII case folding appear twice");
        }
    }
    let mut allowed: Vec<String> = variants.iter().map(|(name, _)| name.clone()).collect();
    allowed.sort();
    EnumOf {
        variants,
        allowed,
        string: string(),
        message: None,
    }
}

/// The decoder [`enum_of`] returns.
#[derive(Clone, Debug)]
pub struct EnumOf<T, S> {
    variants: Vec<(String, T)>,
    allowed: Vec<String>,
    string: S,
    message: Option<String>,
}

impl<T, S> EnumOf<T, S> {
    /// This decoder reading the string with `string`, such as `string().trim()`, rather than with
    /// [`string()`].
    pub fn using<U: Decoder<Json, Output = String>>(self, string: U) -> EnumOf<T, U> {
        EnumOf {
            variants: self.variants,
            allowed: self.allowed,
            string,
            message: self.message,
        }
    }

    /// Gives the issue of a string that names no variant a custom message, which every language
    /// shows as written. The string decoder's issues keep their own.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

impl<T: Clone, S: Decoder<Json, Output = String>> Decoder<Json> for EnumOf<T, S> {
    type Output = T;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<T, Issues> {
        let name = self.string.decode_at(input, path)?.to_ascii_lowercase();
        self.variants
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, value)| value.clone())
            .ok_or_else(|| {
                let issue = Issue::at_path(path, codes::INVALID_FORMAT)
                    .with_message_key(message_keys::INVALID_FORMAT_ENUM)
                    .with_meta("allowed", self.allowed.clone());
                with_message(issue, &self.message).into()
            })
    }
}

fn with_message(issue: Issue, message: &Option<String>) -> Issue {
    match message {
        Some(message) => issue.with_message(message.clone()),
        None => issue,
    }
}

/// A decoder of a string that must be exactly `expected`: `invalid_format` with `expected`
/// otherwise.
///
/// The string is read with [`string()`], or with the decoder given to
/// [`using`](Literal::using), whose issues are reported as they are.
///
/// ```
/// use raoh::json::prelude::*;
///
/// let version = literal("v1").using(string().trim().lowercase());
/// assert_eq!(version.decode(&json!(" V1 ")).unwrap(), "v1");
/// ```
pub fn literal(expected: impl Into<String>) -> Literal<StringDecoder> {
    Literal {
        expected: expected.into(),
        string: string(),
        message: None,
    }
}

/// The decoder [`literal`] returns.
#[derive(Clone, Debug)]
pub struct Literal<S> {
    expected: String,
    string: S,
    message: Option<String>,
}

impl<S> Literal<S> {
    /// This decoder reading the string with `string` rather than with [`string()`].
    pub fn using<U: Decoder<Json, Output = String>>(self, string: U) -> Literal<U> {
        Literal {
            expected: self.expected,
            string,
            message: self.message,
        }
    }

    /// Gives the issue of a string that is not the literal a custom message, which every language
    /// shows as written. The string decoder's issues keep their own.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

impl<S: Decoder<Json, Output = String>> Decoder<Json> for Literal<S> {
    type Output = String;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<String, Issues> {
        let found = self.string.decode_at(input, path)?;
        if found == self.expected {
            Ok(found)
        } else {
            let issue = Issue::at_path(path, codes::INVALID_FORMAT)
                .with_message_key(message_keys::INVALID_FORMAT_LITERAL)
                .with_meta("expected", self.expected.clone());
            Err(with_message(issue, &self.message).into())
        }
    }
}

/// The tags of `variants`, sorted by code point.
///
/// # Panics
///
/// When two variants have the same tag.
fn sorted_tags<V: Variants>(variants: &V) -> Vec<String> {
    let tags = variants.tags();
    for (i, tag) in tags.iter().enumerate() {
        if tags[..i].contains(tag) {
            panic!("duplicate variant tag '{tag}'");
        }
    }
    let mut allowed: Vec<String> = tags.into_iter().map(str::to_owned).collect();
    allowed.sort();
    allowed
}

/// A decoder of an object whose string member `tag_field` names the variant that decodes it.
///
/// `variants` is a tuple, or a `Vec`, of [`variant`]s with the same output. The tag is read as
/// [`field`](super::field) reads a member, with [`string()`]: an input that is not an object is
/// `type_mismatch` with `expected` `object` at the tag's path, and a missing or `null` tag is
/// `required` there. A tag naming none of the variants is `not_allowed` there, with the tags
/// sorted by code point as `allowed` and no `actual`.
///
/// ```
/// use raoh::json::prelude::*;
///
/// #[derive(Debug, PartialEq)]
/// enum Contact { Email(String), Phone(String) }
///
/// let contact = discriminate(
///     "type",
///     (
///         variant("email", object((field("address", string().email()),)).map(|(a,)| Contact::Email(a))),
///         variant("phone", object((field("number", string()),)).map(|(n,)| Contact::Phone(n))),
///     ),
/// );
/// let found = contact.decode(&json!({"type": "phone", "number": "03-0000-0000"})).unwrap();
/// assert_eq!(found, Contact::Phone("03-0000-0000".into()));
/// ```
///
/// # Panics
///
/// When two variants have the same tag.
pub fn discriminate<V: Variants>(
    tag_field: impl Into<Cow<'static, str>>,
    variants: V,
) -> Discriminate<V> {
    let allowed = sorted_tags(&variants);
    Discriminate {
        tag_field: tag_field.into(),
        variants,
        allowed,
    }
}

/// The decoder [`discriminate`] returns.
#[derive(Clone, Debug)]
pub struct Discriminate<V> {
    tag_field: Cow<'static, str>,
    variants: V,
    allowed: Vec<String>,
}

fn not_allowed(path: &Path<'_>, tag_field: &str, allowed: &[String]) -> Issues {
    Issue::at_path(&path.key(tag_field), codes::NOT_ALLOWED)
        .with_meta("allowed", allowed.to_vec())
        .into()
}

impl<V: Variants> Decoder<Json> for Discriminate<V> {
    type Output = V::Output;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<V::Output, Issues> {
        let at = path.key(&self.tag_field);
        let tag = match input.view() {
            View::Object(members) => {
                let member = members.get(&self.tag_field).unwrap_or(missing());
                string().decode_at(member, &at)?
            }
            _ => return Err(type_mismatch(&at, "object", input).into()),
        };
        self.variants
            .decode_variant(&tag, input, path)
            .unwrap_or_else(|| Err(not_allowed(path, &self.tag_field, &self.allowed)))
    }
}

/// A decoder like [`discriminate`] whose tag is what `tag` gives for the whole input, so that
/// the tag can be read in any way, trimmed or lower-cased for instance.
///
/// The tag decoder's issues are reported as they are. A tag naming none of the variants is
/// `not_allowed` at the path of the member `tag_field`, as [`discriminate`] reports it.
///
/// ```
/// use raoh::json::prelude::*;
///
/// let kind = object((field("kind", string().trim().lowercase()),)).map(|(k,)| k);
/// let shape = discriminate_by(
///     "kind",
///     kind,
///     (variant("square", object((field("side", i64()),)).map(|(s,)| s * s)),),
/// );
/// assert_eq!(shape.decode(&json!({"kind": " Square ", "side": 3})).unwrap(), 9);
/// ```
///
/// # Panics
///
/// When two variants have the same tag.
pub fn discriminate_by<T, V: Variants>(
    tag_field: impl Into<Cow<'static, str>>,
    tag: T,
    variants: V,
) -> DiscriminateBy<T, V> {
    let allowed = sorted_tags(&variants);
    DiscriminateBy {
        tag_field: tag_field.into(),
        tag,
        variants,
        allowed,
    }
}

/// The decoder [`discriminate_by`] returns.
#[derive(Clone, Debug)]
pub struct DiscriminateBy<T, V> {
    tag_field: Cow<'static, str>,
    tag: T,
    variants: V,
    allowed: Vec<String>,
}

impl<T: Decoder<Json, Output = String>, V: Variants> Decoder<Json> for DiscriminateBy<T, V> {
    type Output = V::Output;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<V::Output, Issues> {
        let tag = self.tag.decode_at(input, path)?;
        self.variants
            .decode_variant(&tag, input, path)
            .unwrap_or_else(|| Err(not_allowed(path, &self.tag_field, &self.allowed)))
    }
}

/// One case of [`discriminate`]: the tag that names it and the decoder of its input.
pub fn variant<D>(tag: impl Into<Cow<'static, str>>, decoder: D) -> Variant<D> {
    Variant {
        tag: tag.into(),
        decoder,
    }
}

/// The case [`variant`] returns.
#[derive(Clone, Debug)]
pub struct Variant<D> {
    tag: Cow<'static, str>,
    decoder: D,
}

mod sealed {
    pub trait Sealed {}
}

/// A tuple or `Vec` of [`Variant`]s with the same output. It cannot be implemented outside this
/// crate.
pub trait Variants: sealed::Sealed {
    /// What each variant gives.
    type Output;

    /// The tag of every variant.
    #[doc(hidden)]
    fn tags(&self) -> Vec<&str>;

    /// The result of the variant named `tag`, or `None` when no variant has that tag.
    #[doc(hidden)]
    fn decode_variant(
        &self,
        tag: &str,
        input: &Json,
        path: &Path<'_>,
    ) -> Option<Result<Self::Output, Issues>>;
}

impl<D> sealed::Sealed for Vec<Variant<D>> {}

impl<D: Decoder<Json>> Variants for Vec<Variant<D>> {
    type Output = D::Output;

    fn tags(&self) -> Vec<&str> {
        self.iter().map(|v| v.tag.as_ref()).collect()
    }

    fn decode_variant(
        &self,
        tag: &str,
        input: &Json,
        path: &Path<'_>,
    ) -> Option<Result<Self::Output, Issues>> {
        self.iter()
            .find(|v| v.tag == tag)
            .map(|v| v.decoder.decode_at(input, path))
    }
}

macro_rules! variants {
    ($First:ident $_first:ident $first:tt $(, $T:ident $_v:ident $idx:tt)*) => {
        impl<$First, $($T),*> sealed::Sealed for (Variant<$First>, $(Variant<$T>,)*) {}

        impl<$First: Decoder<Json>, $($T: Decoder<Json, Output = $First::Output>),*> Variants
            for (Variant<$First>, $(Variant<$T>,)*)
        {
            type Output = $First::Output;

            fn tags(&self) -> Vec<&str> {
                vec![&self.$first.tag $(, &self.$idx.tag)*]
            }

            fn decode_variant(
                &self,
                tag: &str,
                input: &Json,
                path: &Path<'_>,
            ) -> Option<Result<Self::Output, Issues>> {
                if self.$first.tag == tag {
                    return Some(self.$first.decoder.decode_at(input, path));
                }
                $(
                    if self.$idx.tag == tag {
                        return Some(self.$idx.decoder.decode_at(input, path));
                    }
                )*
                None
            }
        }
    };
}

for_tuples!(variants);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetaValue;
    use crate::json::{field, i64, object};
    use serde_json::json;

    fn shape() -> impl Decoder<Json, Output = i64> {
        discriminate(
            "kind",
            (
                variant("square", object((field("side", i64()),)).map(|(s,)| s * s)),
                variant(
                    "rect",
                    object((field("w", i64()), field("h", i64()))).map(|(w, h)| w * h),
                ),
            ),
        )
    }

    fn first<T: std::fmt::Debug>(result: Result<T, Issues>) -> Issue {
        result.unwrap_err().into_iter().next().unwrap()
    }

    #[test]
    fn the_tag_picks_the_variant() {
        assert_eq!(
            shape()
                .decode(&json!({"kind": "rect", "w": 2, "h": 3}))
                .unwrap(),
            6
        );
    }

    #[test]
    fn an_unknown_tag_is_not_allowed_at_the_tag_path() {
        let issue = first(shape().decode(&json!({"kind": "circle"})));
        assert_eq!(issue.code(), "not_allowed");
        assert_eq!(issue.path().to_string(), "/kind");
        assert_eq!(
            issue.meta()["allowed"],
            MetaValue::from(vec!["rect", "square"])
        );
        assert!(!issue.meta().contains_key("actual"));
        assert_eq!(issue.message(), "must be one of [rect, square]");
    }

    #[test]
    fn a_missing_tag_is_required_and_a_non_object_is_reported_at_the_tag() {
        assert_eq!(first(shape().decode(&json!({}))).code(), "required");
        let issue = first(shape().decode(&json!("rect")));
        assert_eq!(issue.code(), "type_mismatch");
        assert_eq!(issue.path().to_string(), "/kind");
        assert_eq!(issue.meta()["expected"], MetaValue::from("object"));
    }

    #[test]
    #[should_panic(expected = "duplicate variant tag 'a'")]
    fn duplicate_tags_panic() {
        discriminate("t", (variant("a", i64()), variant("a", i64())));
    }

    #[test]
    fn enum_names_fold_ascii_case_only() {
        let decoder = enum_of([("red", 1), ("straße", 2), ("K", 3)]);
        assert_eq!(decoder.decode(&json!("RED")).unwrap(), 1);
        assert!(decoder.decode(&json!("STRASSE")).is_err());
        assert_eq!(decoder.decode(&json!("Straße")).unwrap(), 2);
        // U+212A KELVIN SIGN lower-cases to 'k' in Unicode, but is not ASCII.
        assert!(decoder.decode(&json!("\u{212a}")).is_err());
        let issue = first(decoder.decode(&json!("blue")));
        assert_eq!(
            issue.meta()["allowed"],
            MetaValue::from(vec!["k", "red", "straße"])
        );
    }

    #[test]
    #[should_panic(expected = "under ASCII case folding appear twice")]
    fn enum_names_equal_under_folding_panic() {
        enum_of([("a", 1), ("A", 2)]);
    }

    #[test]
    fn a_message_goes_to_the_form_and_not_to_its_string_decoder() {
        let decoder = enum_of([("red", 1)]).message("pick a colour");
        assert_eq!(
            first(decoder.decode(&json!("blue"))).message(),
            "pick a colour"
        );
        assert_eq!(
            first(decoder.decode(&json!(1))).message(),
            "expected string"
        );
        let decoder = literal("v1").message("say v1");
        assert_eq!(first(decoder.decode(&json!("v2"))).message(), "say v1");
    }

    #[test]
    fn literal_requires_the_exact_string() {
        let issues = literal("v1").decode(&json!("v2")).unwrap_err();
        let issue = issues.iter().next().unwrap();
        assert_eq!(issue.meta()["expected"], MetaValue::from("v1"));
        assert_eq!(issue.message(), "invalid value");
    }
}
