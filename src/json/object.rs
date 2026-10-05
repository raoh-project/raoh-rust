use super::{Json, View};
use super::{missing, type_mismatch};
use crate::codes;
use crate::decoder::Decoder;
use crate::issue::{Issue, Issues};
use crate::path::Path;
use crate::presence::Presence;
use std::borrow::Cow;
use std::collections::HashSet;

mod sealed {
    pub trait Sealed {}
}

/// A decoder that reads each of `fields` from the same input and gives their values as a tuple.
///
/// `fields` is a [`field`], [`optional_field`], [`presence_field`] or [`flat`], or a tuple of
/// them, or a `Vec` or tuple of such tuples. Every field is read and the issues of every one are
/// reported, in the order the fields are written.
///
/// Each field checks for itself that the input is an object. A required field of an input that is
/// not one, `null` and a missing member included, is `type_mismatch` with `expected` `object` at
/// the field's own path; an optional field reads such an input as not having its member; a flat
/// field hands the input to its decoder as it is. So `object` reports nothing of its own:
/// everything it reports comes from a field.
///
/// A field is not a decoder on its own, so it is only ever read through an object.
///
/// ```
/// use raoh::json::prelude::*;
///
/// let point = object((field("x", i64()), field("y", i64()))).strict();
/// let issues = point.decode(&json!({"x": 1, "y": "2", "z": 3})).unwrap_err();
/// let codes: Vec<&str> = issues.iter().map(|i| i.code()).collect();
/// assert_eq!(codes, ["type_mismatch", "unknown_field"]);
///
/// let issues = point.decode(&json!("not an object")).unwrap_err();
/// let paths: Vec<String> = issues.iter().map(|i| i.path().to_string()).collect();
/// assert_eq!(paths, ["/x", "/y"]);
///
/// let nickname = object((optional_field("nickname", string()),));
/// assert_eq!(nickname.decode(&json!(null)).unwrap(), (None,));
/// ```
pub fn object<F: FieldSet>(fields: F) -> Object<F> {
    Object(fields)
}

/// The decoder [`object`] returns.
#[derive(Clone, Copy, Debug)]
pub struct Object<F>(F);

impl<F: FieldSet> Decoder<Json> for Object<F> {
    type Output = F::Output;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<F::Output, Issues> {
        self.0.decode_fields(input, path)
    }
}

impl<F: FieldSet> Object<F> {
    /// A decoder that also reports, when the input is an object, every member no field names as
    /// `unknown_field` at the member's path, with its name as `field`, in the order of the input's
    /// members and after the fields' own issues.
    ///
    /// # Panics
    ///
    /// When a field is [`flat`]: a flat field reads the whole input, so which members it knows
    /// cannot be told, and a strict object of one would report the members it reads.
    pub fn strict(self) -> Strict<F> {
        let mut names = Vec::new();
        assert!(
            self.0.member_names(&mut names),
            "a strict object cannot have a flat field: which members it reads cannot be told"
        );
        let known = names.into_iter().map(str::to_owned).collect();
        Strict {
            fields: self.0,
            known,
        }
    }
}

/// The decoder [`Object::strict`] returns.
#[derive(Clone, Debug)]
pub struct Strict<F> {
    fields: F,
    known: HashSet<String>,
}

impl<F: FieldSet> Decoder<Json> for Strict<F> {
    type Output = F::Output;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<F::Output, Issues> {
        let result = self.fields.decode_fields(input, path);
        reject_unknown(result, input, path, &self.known)
    }
}

/// A decoder that decodes with `decoder` and, when the input is an object, also reports every
/// member whose name is not in `known` as `unknown_field`, after the decoder's own issues. The
/// decoder's value is not given when there is such a member.
///
/// A member another strict decoder inside this one has already reported is not reported again,
/// so strict decoders one inside another report a member once, by the innermost that does not
/// know it, and accept only members every one of them knows. An issue of another kind at the
/// member does not count: a member whose value has the wrong type and that no strict decoder knows
/// is reported both ways.
///
/// ```
/// use raoh::json::prelude::*;
///
/// let square = object((field("side", i64()),)).map(|(side,)| side * side);
/// let shape = strict(discriminate("kind", (variant("square", square),)), ["kind", "side"]);
/// assert_eq!(shape.decode(&json!({"kind": "square", "side": 3})).unwrap(), 9);
/// let issues = shape.decode(&json!({"kind": "square", "side": 3, "w": 1})).unwrap_err();
/// assert_eq!(issues.iter().next().unwrap().path().to_string(), "/w");
/// ```
pub fn strict<D, S: Into<String>>(
    decoder: D,
    known: impl IntoIterator<Item = S>,
) -> StrictMembers<D> {
    StrictMembers {
        inner: decoder,
        known: known.into_iter().map(Into::into).collect(),
    }
}

/// The decoder [`strict`] returns.
#[derive(Clone, Debug)]
pub struct StrictMembers<D> {
    inner: D,
    known: HashSet<String>,
}

impl<D: Decoder<Json>> Decoder<Json> for StrictMembers<D> {
    type Output = D::Output;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<D::Output, Issues> {
        let result = self.inner.decode_at(input, path);
        reject_unknown(result, input, path, &self.known)
    }
}

/// `result`, followed by `unknown_field` for every member of an object `input` that is not in
/// `known` and that `result` has not already reported unknown.
///
/// Each member is looked up in `known` and among the members `result` reported, both sets built
/// once, so the time this takes grows with the number of members and of issues, not with their
/// product.
fn reject_unknown<T>(
    result: Result<T, Issues>,
    input: &Json,
    path: &Path<'_>,
    known: &HashSet<String>,
) -> Result<T, Issues> {
    let View::Object(members) = input.view() else {
        return result;
    };
    let reported = match &result {
        Ok(_) => HashSet::new(),
        Err(issues) => reported_unknown(issues, path),
    };
    let mut unknown = Issues::new();
    members.each(&mut |name, _| {
        if !known.contains(name) && !reported.contains(name) {
            unknown.push(
                Issue::at_path(&path.key(name), codes::UNKNOWN_FIELD)
                    .with_meta("field", name)
                    .marked_unknown_member(),
            );
        }
    });
    match result {
        Ok(value) if unknown.is_empty() => Ok(value),
        Ok(_) => Err(unknown),
        Err(mut issues) => {
            issues.merge(unknown);
            Err(issues)
        }
    }
}

/// The names of the members of the object at `path` that a strict decoder among `issues` has
/// reported unknown: those of the issues it marked, one segment below `path`.
fn reported_unknown<'a>(issues: &'a Issues, path: &Path<'_>) -> HashSet<&'a str> {
    let mut marked = issues.iter().filter(|i| i.is_unknown_member()).peekable();
    if marked.peek().is_none() {
        return HashSet::new();
    }
    let here = path.to_pointer();
    let depth = here.segments().len();
    marked
        .filter_map(|issue| {
            let segments = issue.path().segments();
            (segments.len() == depth + 1 && segments[..depth] == *here.segments())
                .then(|| segments[depth].as_str())
        })
        .collect()
}

/// The fields [`object`] reads: a [`Field`], [`OptionalField`], [`PresenceField`] or [`Flat`], a
/// tuple or `Vec` of field sets, or one of them [mapped](FieldSet::map) or
/// [boxed](FieldSet::boxed). It cannot be implemented outside this crate.
pub trait FieldSet: sealed::Sealed {
    /// What the fields give.
    type Output;

    /// Reads the fields from `input`, found at `path`.
    #[doc(hidden)]
    fn decode_fields(&self, input: &Json, path: &Path<'_>) -> Result<Self::Output, Issues>;

    /// Adds the name of every member the fields read to `names`, and says whether that is all
    /// they read: false when one is flat and reads the whole input.
    #[doc(hidden)]
    fn member_names<'a>(&'a self, names: &mut Vec<&'a str>) -> bool;

    /// These fields, giving what `f` makes of their value.
    ///
    /// ```
    /// use raoh::json::prelude::*;
    /// use raoh::json::FieldSet;
    ///
    /// let nick = optional_field("nick", string()).map(|n| n.unwrap_or_default());
    /// assert_eq!(object((nick,)).decode(&json!({})).unwrap(), (String::new(),));
    /// ```
    fn map<U, F>(self, f: F) -> MapFields<Self, F>
    where
        Self: Sized,
        F: Fn(Self::Output) -> U,
    {
        MapFields { fields: self, f }
    }

    /// These fields behind a pointer, so that fields of different kinds with the same output can
    /// be kept in one `Vec`, as an object whose fields are decided at run time needs.
    fn boxed(self) -> BoxFieldSet<Self::Output>
    where
        Self: Sized + Send + Sync + 'static,
    {
        Box::new(self)
    }
}

/// A field set whose concrete type is erased.
pub type BoxFieldSet<O> = Box<dyn FieldSet<Output = O> + Send + Sync>;

impl<O> sealed::Sealed for BoxFieldSet<O> {}

impl<O> FieldSet for BoxFieldSet<O> {
    type Output = O;

    fn decode_fields(&self, input: &Json, path: &Path<'_>) -> Result<O, Issues> {
        (**self).decode_fields(input, path)
    }

    fn member_names<'a>(&'a self, names: &mut Vec<&'a str>) -> bool {
        (**self).member_names(names)
    }
}

/// The field set [`FieldSet::map`] returns.
#[derive(Clone, Copy, Debug)]
pub struct MapFields<F, G> {
    fields: F,
    f: G,
}

impl<F, G> sealed::Sealed for MapFields<F, G> {}

impl<F: FieldSet, G: Fn(F::Output) -> U, U> FieldSet for MapFields<F, G> {
    type Output = U;

    fn decode_fields(&self, input: &Json, path: &Path<'_>) -> Result<U, Issues> {
        self.fields.decode_fields(input, path).map(&self.f)
    }

    fn member_names<'a>(&'a self, names: &mut Vec<&'a str>) -> bool {
        self.fields.member_names(names)
    }
}

/// Every field set of the `Vec`, read in order, giving their values in order.
impl<F> sealed::Sealed for Vec<F> {}

impl<F: FieldSet> FieldSet for Vec<F> {
    type Output = Vec<F::Output>;

    fn decode_fields(&self, input: &Json, path: &Path<'_>) -> Result<Self::Output, Issues> {
        let mut values = Vec::with_capacity(self.len());
        let mut issues = Issues::new();
        for fields in self {
            match fields.decode_fields(input, path) {
                Ok(value) => values.push(value),
                Err(found) => issues.merge(found),
            }
        }
        if issues.is_empty() {
            Ok(values)
        } else {
            Err(issues)
        }
    }

    fn member_names<'a>(&'a self, names: &mut Vec<&'a str>) -> bool {
        self.iter()
            .fold(true, |all, fields| fields.member_names(names) && all)
    }
}

/// The member `name` of an object, which must be there.
///
/// A missing member is handed to `decoder` as [`missing()`](super::missing), so a built-in decoder
/// reports it as `required` at the member's path. An input that is not an object is
/// `type_mismatch` with `expected` `object` at the member's path.
pub fn field<D>(name: impl Into<Cow<'static, str>>, decoder: D) -> Field<D> {
    Field {
        name: name.into(),
        decoder,
    }
}

/// The field [`field`] returns.
#[derive(Clone, Debug)]
pub struct Field<D> {
    name: Cow<'static, str>,
    decoder: D,
}

impl<D> sealed::Sealed for Field<D> {}

impl<D: Decoder<Json>> FieldSet for Field<D> {
    type Output = D::Output;

    fn decode_fields(&self, input: &Json, path: &Path<'_>) -> Result<D::Output, Issues> {
        let at = path.key(&self.name);
        match input.view() {
            View::Object(members) => {
                let member = members.get(&self.name).unwrap_or(missing());
                self.decoder.decode_at(member, &at)
            }
            _ => Err(type_mismatch(&at, "object", input).into()),
        }
    }

    fn member_names<'a>(&'a self, names: &mut Vec<&'a str>) -> bool {
        names.push(&self.name);
        true
    }
}

/// The member `name` of an object, which may be left out: `None` when it is missing, or when the
/// input is not an object.
///
/// A member present as `null` is handed to `decoder`; use [`presence_field`] to tell `null` apart,
/// or [`nullable`](super::JsonDecoderExt::nullable) to accept it.
pub fn optional_field<D>(name: impl Into<Cow<'static, str>>, decoder: D) -> OptionalField<D> {
    OptionalField {
        name: name.into(),
        decoder,
    }
}

/// The field [`optional_field`] returns.
#[derive(Clone, Debug)]
pub struct OptionalField<D> {
    name: Cow<'static, str>,
    decoder: D,
}

impl<D> sealed::Sealed for OptionalField<D> {}

impl<D: Decoder<Json>> FieldSet for OptionalField<D> {
    type Output = Option<D::Output>;

    fn decode_fields(&self, input: &Json, path: &Path<'_>) -> Result<Self::Output, Issues> {
        let View::Object(members) = input.view() else {
            return Ok(None);
        };
        members
            .get(&self.name)
            .map(|member| self.decoder.decode_at(member, &path.key(&self.name)))
            .transpose()
    }

    fn member_names<'a>(&'a self, names: &mut Vec<&'a str>) -> bool {
        names.push(&self.name);
        true
    }
}

/// The member `name` of an object, telling a missing member, a `null` one and one with a value
/// apart, as a PATCH request needs to. An input that is not an object has no members, so the
/// member is absent.
pub fn presence_field<D>(name: impl Into<Cow<'static, str>>, decoder: D) -> PresenceField<D> {
    PresenceField {
        name: name.into(),
        decoder,
    }
}

/// The field [`presence_field`] returns.
#[derive(Clone, Debug)]
pub struct PresenceField<D> {
    name: Cow<'static, str>,
    decoder: D,
}

impl<D> sealed::Sealed for PresenceField<D> {}

impl<D: Decoder<Json>> FieldSet for PresenceField<D> {
    type Output = Presence<D::Output>;

    fn decode_fields(&self, input: &Json, path: &Path<'_>) -> Result<Self::Output, Issues> {
        let View::Object(members) = input.view() else {
            return Ok(Presence::Absent);
        };
        match members.get(&self.name) {
            None => Ok(Presence::Absent),
            Some(member) if matches!(member.view(), View::Null) => Ok(Presence::Null),
            Some(member) => self
                .decoder
                .decode_at(member, &path.key(&self.name))
                .map(Presence::Present),
        }
    }

    fn member_names<'a>(&'a self, names: &mut Vec<&'a str>) -> bool {
        names.push(&self.name);
        true
    }
}

/// A field that decodes the whole input rather than a member of it, contributing the decoder's
/// value as one value of the object: the way to read members that belong together, such as the
/// parts of an address, into one value of their own.
///
/// ```
/// use raoh::json::prelude::*;
///
/// let contact = object((field("email", string()), field("phone", string())));
/// let user = object((field("id", i64()), flat(contact)));
/// let (id, (email, _)) = user
///     .decode(&json!({"id": 1, "email": "a@example.com", "phone": "090"}))
///     .unwrap();
/// assert_eq!((id, email.as_str()), (1, "a@example.com"));
/// ```
pub fn flat<D>(decoder: D) -> Flat<D> {
    Flat(decoder)
}

/// The field [`flat`] returns.
#[derive(Clone, Copy, Debug)]
pub struct Flat<D>(D);

impl<D> sealed::Sealed for Flat<D> {}

impl<D: Decoder<Json>> FieldSet for Flat<D> {
    type Output = D::Output;

    fn decode_fields(&self, input: &Json, path: &Path<'_>) -> Result<D::Output, Issues> {
        self.0.decode_at(input, path)
    }

    fn member_names<'a>(&'a self, _: &mut Vec<&'a str>) -> bool {
        false
    }
}

/// Reads every field of the tuple and keeps every issue, in the order the fields are written.
macro_rules! tuple_field_set {
    ($($T:ident $v:ident $idx:tt),+) => {
        impl<$($T: FieldSet),+> sealed::Sealed for ($($T,)+) {}

        impl<$($T: FieldSet),+> FieldSet for ($($T,)+) {
            type Output = ($($T::Output,)+);

            fn decode_fields(
                &self,
                input: &Json,
                path: &Path<'_>,
            ) -> Result<Self::Output, Issues> {
                let mut issues = Issues::new();
                $(
                    let $v = match self.$idx.decode_fields(input, path) {
                        Ok(value) => Some(value),
                        Err(found) => {
                            issues.merge(found);
                            None
                        }
                    };
                )+
                match ($($v,)+) {
                    ($(Some($v),)+) => Ok(($($v,)+)),
                    _ => Err(issues),
                }
            }

            fn member_names<'a>(&'a self, names: &mut Vec<&'a str>) -> bool {
                let mut all = true;
                $( all &= self.$idx.member_names(names); )+
                all
            }
        }
    };
}

for_tuples!(tuple_field_set);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetaValue;
    use crate::json::{JsonDecoderExt, i64, string};
    use serde_json::Value;
    use serde_json::json;

    fn paths(issues: &Issues) -> Vec<String> {
        issues.iter().map(|i| i.path().to_string()).collect()
    }

    #[test]
    fn a_missing_field_is_required_at_its_path() {
        let issues = object((field("name", string()),))
            .decode(&json!({}))
            .unwrap_err();
        let issue = issues.iter().next().unwrap();
        assert_eq!(issue.code(), "required");
        assert_eq!(issue.path().to_string(), "/name");
    }

    #[test]
    fn each_required_field_reports_a_non_object_at_its_own_path() {
        let decoder = object((field("a", i64()), field("b", i64())));
        let issues = decoder.decode(&json!([1])).unwrap_err();
        assert_eq!(paths(&issues), ["/a", "/b"]);
        let issue = issues.iter().next().unwrap();
        assert_eq!(issue.code(), "type_mismatch");
        assert_eq!(issue.meta()["expected"], MetaValue::from("object"));
        assert_eq!(issue.meta()["actual"], MetaValue::from("array"));
        let issues = decoder.decode(&Value::Null).unwrap_err();
        assert_eq!(
            issues.iter().next().unwrap().meta()["actual"],
            MetaValue::from("null")
        );
    }

    #[test]
    fn a_missing_object_is_reported_as_missing_by_its_fields() {
        let decoder = object((field("a", object((field("b", i64()),))),));
        let issues = decoder.decode(&json!({})).unwrap_err();
        let issue = issues.iter().next().unwrap();
        assert_eq!(issue.path().to_string(), "/a/b");
        assert_eq!(issue.meta()["actual"], MetaValue::from("missing"));
    }

    #[test]
    fn optional_and_presence_fields_read_a_non_object_as_having_no_members() {
        for input in [json!("x"), json!(1), json!([]), Value::Null] {
            assert_eq!(
                object((optional_field("n", string()),))
                    .decode(&input)
                    .unwrap(),
                (None,)
            );
            assert_eq!(
                object((presence_field("n", string()),))
                    .decode(&input)
                    .unwrap(),
                (Presence::Absent,)
            );
        }
    }

    #[test]
    fn nested_paths_join() {
        let item = object((field("name", string()),));
        let decoder = object((field("items", item.list()),));
        let issues = decoder
            .decode(&json!({"items": [{"name": "a"}, {}]}))
            .unwrap_err();
        assert_eq!(paths(&issues), ["/items/1/name"]);
    }

    #[test]
    fn keys_holding_slash_and_tilde_are_escaped() {
        let decoder = object((field("a/b", i64()), field("", i64())));
        let issues = decoder.decode(&json!({})).unwrap_err();
        assert_eq!(paths(&issues), ["/a~1b", "/"]);
    }

    #[test]
    fn optional_field_gives_none_when_missing_and_requires_non_null() {
        let decoder = object((optional_field("nick", string()),));
        assert_eq!(decoder.decode(&json!({})).unwrap(), (None,));
        let issues = decoder.decode(&json!({"nick": null})).unwrap_err();
        assert_eq!(issues.iter().next().unwrap().code(), "required");
    }

    #[test]
    fn presence_field_tells_the_three_apart() {
        let decoder = object((presence_field("n", i64()),));
        assert_eq!(decoder.decode(&json!({})).unwrap(), (Presence::Absent,));
        assert_eq!(
            decoder.decode(&json!({"n": null})).unwrap(),
            (Presence::Null,)
        );
        assert_eq!(
            decoder.decode(&json!({"n": 1})).unwrap(),
            (Presence::Present(1),)
        );
    }

    #[test]
    fn flat_fields_report_at_the_position_they_are_written() {
        let contact = object((field("email", string()), field("phone", string())));
        let decoder = object((flat(contact), field("id", i64())));
        let issues = decoder.decode(&json!({"id": "x"})).unwrap_err();
        assert_eq!(paths(&issues), ["/email", "/phone", "/id"]);
    }

    #[test]
    fn strict_reports_every_unknown_member_after_the_fields_issues() {
        let decoder = object((field("a", i64()),)).strict();
        let issues = decoder
            .decode(&json!({"a": "x", "b": 2, "c": 3}))
            .unwrap_err();
        assert_eq!(paths(&issues), ["/a", "/b", "/c"]);
        assert_eq!(
            issues.iter().nth(1).unwrap().meta()["field"],
            MetaValue::from("b")
        );
    }

    #[test]
    fn nested_strict_decoders_report_a_member_once() {
        let inner = strict(object((field("a", i64()),)), ["a"]);
        let outer = strict(inner, ["a"]);
        let issues = outer.decode(&json!({"a": 1, "b": 2})).unwrap_err();
        assert_eq!(paths(&issues), ["/b"]);
        let widest = strict(strict(object((field("a", i64()),)), ["a"]), ["a", "b"]);
        assert_eq!(
            paths(&widest.decode(&json!({"a": 1, "b": 2})).unwrap_err()),
            ["/b"]
        );
    }

    #[test]
    fn an_issue_of_another_kind_does_not_count_as_reported() {
        let decoder = strict(object((field("b", i64()),)), ["a"]);
        let issues = decoder.decode(&json!({"b": "x"})).unwrap_err();
        let codes: Vec<&str> = issues.iter().map(|i| i.code()).collect();
        assert_eq!(codes, ["type_mismatch", "unknown_field"]);
    }

    #[test]
    #[should_panic(expected = "cannot have a flat field")]
    fn a_strict_object_refuses_a_flat_field() {
        let _ = object((field("a", i64()), flat(object((field("b", i64()),))))).strict();
    }

    #[test]
    fn a_vec_of_boxed_fields_is_an_object_decided_at_run_time() {
        let fields: Vec<BoxFieldSet<String>> = vec![
            field("a", string()).boxed(),
            optional_field("b", string())
                .map(|b| b.unwrap_or_default())
                .boxed(),
        ];
        let decoder = object(fields).strict();
        assert_eq!(
            decoder.decode(&json!({"a": "x"})).unwrap(),
            ["x".to_owned(), String::new()]
        );
        assert_eq!(
            paths(&decoder.decode(&json!({"a": "x", "c": 1})).unwrap_err()),
            ["/c"]
        );
    }
}
