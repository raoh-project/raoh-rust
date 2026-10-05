use super::steps::Steps;
use super::unexpected;
use super::{Json, View};
use crate::combinator::Map;
use crate::decoder::Decoder;
use crate::issue::{Issue, Issues};
use crate::meta::MetaValue;
use crate::path::Path;
use crate::value::same::{ByValue, Same, Set};
use crate::{codes, message_keys};
use std::collections::HashSet;

/// What every decoder over a JSON value can also do.
pub trait JsonDecoderExt: Decoder<Json> + Sized {
    /// A decoder that gives `None` for `null` and `Some` of this decoder's output otherwise.
    ///
    /// A missing member is not `null`: it is still handed to this decoder, which reports it as
    /// `required`. To accept a member that may be left out, use
    /// [`optional_field`](super::optional_field).
    fn nullable(self) -> Nullable<Self> {
        Nullable(self)
    }

    /// A decoder of a JSON array whose every element this decoder reads.
    ///
    /// Missing or `null` is `required`; any other type is `type_mismatch`. The issues of every
    /// element are reported, each under its index.
    fn list(self) -> ListDecoder<Self> {
        ListDecoder {
            element: self,
            steps: Steps::default(),
        }
    }
}

impl<D: Decoder<Json>> JsonDecoderExt for D {}

/// The decoder [`JsonDecoderExt::nullable`] returns.
#[derive(Clone, Copy, Debug)]
pub struct Nullable<D>(D);

impl<D: Decoder<Json>> Decoder<Json> for Nullable<D> {
    type Output = Option<D::Output>;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<Self::Output, Issues> {
        if matches!(input.view(), View::Null) {
            Ok(None)
        } else {
            self.0.decode_at(input, path).map(Some)
        }
    }
}

/// The decoder [`JsonDecoderExt::list`] returns.
///
/// Constraints on the whole list run once every element has decoded, in the order they are
/// written, and the first to fail is the one reported. Those that compare elements, `unique`,
/// `contains` and `contains_all`, compare them as the value model of the Raoh Specification
/// does, through [`MetaValue`]: for floats +0 and -0 differ and NaN is NaN, and decimals of
/// different scales differ.
pub struct ListDecoder<D: Decoder<Json>> {
    element: D,
    steps: Steps<Vec<D::Output>>,
}

impl<D: Decoder<Json> + Clone> Clone for ListDecoder<D> {
    fn clone(&self) -> Self {
        Self {
            element: self.element.clone(),
            steps: self.steps.clone(),
        }
    }
}

impl<D: Decoder<Json> + std::fmt::Debug> std::fmt::Debug for ListDecoder<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListDecoder")
            .field("element", &self.element)
            .field("steps", &self.steps)
            .finish()
    }
}

impl<D: Decoder<Json>> Decoder<Json> for ListDecoder<D> {
    type Output = Vec<D::Output>;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<Self::Output, Issues> {
        let View::Array(items) = input.view() else {
            return Err(self.steps.base_issue(unexpected(path, "array", input)));
        };
        let mut values = Vec::with_capacity(items.len());
        let mut issues = Issues::new();
        for (i, item) in items.iter().enumerate() {
            match self.element.decode_at(item, &path.index(i)) {
                Ok(value) => values.push(value),
                Err(found) => issues.merge(found),
            }
        }
        if !issues.is_empty() {
            return Err(issues);
        }
        self.steps.run(values, path)
    }
}

impl<D: Decoder<Json>> ListDecoder<D>
where
    D::Output: 'static,
{
    /// Gives the most recent constraint written before this, or the type check when there is
    /// none, a custom message that every language shows as written.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.steps.set_message(message.into());
        self
    }

    /// Requires at least one element: `too_small` with `min` 1 and `actual` 0.
    pub fn non_empty(mut self) -> Self {
        self.steps
            .require(|items| !items.is_empty(), |_| non_empty_issue());
        self
    }

    /// Requires at least `n` elements: `too_small` with `min` and `actual`.
    pub fn min_size(mut self, n: usize) -> Self {
        self.steps.require(
            move |items| items.len() >= n,
            move |items| min_size_issue(n, items.len()),
        );
        self
    }

    /// Allows at most `n` elements: `too_big` with `max` and `actual`.
    pub fn max_size(mut self, n: usize) -> Self {
        self.steps.require(
            move |items| items.len() <= n,
            move |items| max_size_issue(n, items.len()),
        );
        self
    }

    /// Requires exactly `n` elements: `invalid_size` with `expected` and `actual`.
    pub fn size(mut self, n: usize) -> Self {
        self.steps.require(
            move |items| items.len() == n,
            move |items| size_issue(n, items.len()),
        );
        self
    }

    /// Requires no element to occur twice, by [`Same`]: `duplicate_element` with `duplicates`,
    /// which lists each repeated element once, ordered by where it first became a duplicate, its
    /// second occurrence: `[1, 2, 2, 1]` lists 2 before 1.
    pub fn unique(mut self) -> Self
    where
        D::Output: Same + Clone + Into<MetaValue>,
    {
        self.steps.require(
            |items| {
                let mut seen = HashSet::with_capacity(items.len());
                items.iter().all(|item| seen.insert(ByValue(item)))
            },
            |items| {
                let mut seen = HashSet::with_capacity(items.len());
                let mut repeated = HashSet::new();
                let mut duplicates: Vec<MetaValue> = Vec::new();
                for item in items {
                    if !seen.insert(ByValue(item)) && repeated.insert(ByValue(item)) {
                        duplicates.push(item.clone().into());
                    }
                }
                Issue::new(codes::DUPLICATE_ELEMENT).with_meta("duplicates", duplicates)
            },
        );
        self
    }

    /// Requires an element the same as `element`, by [`Same`]: `missing_element` with
    /// `expected`.
    pub fn contains(mut self, element: D::Output) -> Self
    where
        D::Output: Same + Clone + Into<MetaValue> + Send + Sync,
    {
        let expected: MetaValue = element.clone().into();
        self.steps.require(
            move |items| items.iter().any(|item| item.same(&element)),
            move |_| Issue::new(codes::MISSING_ELEMENT).with_meta("expected", expected.clone()),
        );
        self
    }

    /// Requires every one of `elements` to occur, by [`Same`]: `missing_elements` with
    /// `expected`, the elements as given, and `missing`, in the order given, each given element
    /// that does not occur, as many times as it was given.
    pub fn contains_all(mut self, elements: impl IntoIterator<Item = D::Output>) -> Self
    where
        D::Output: Same + Clone + Into<MetaValue> + Send + Sync,
    {
        let wanted: Vec<D::Output> = elements.into_iter().collect();
        let expected: Vec<MetaValue> = wanted.iter().cloned().map(Into::into).collect();
        let missing = move |items: &Vec<D::Output>| -> Vec<D::Output> {
            let present: HashSet<ByValue<&D::Output>> = items.iter().map(ByValue).collect();
            wanted
                .iter()
                .filter(|e| !present.contains(&ByValue(*e)))
                .cloned()
                .collect()
        };
        let check = missing.clone();
        self.steps.require(
            move |items| check(items).is_empty(),
            move |items| {
                Issue::new(codes::MISSING_ELEMENTS)
                    .with_meta("expected", expected.clone())
                    .with_meta("missing", missing(items))
            },
        );
        self
    }

    /// A decoder of the [`Set`] of the elements, each once by [`Same`], in the order each first
    /// occurs. Any element type with sameness has one, floats included, where -0 and +0 are two
    /// elements and every NaN one.
    ///
    /// ```
    /// use raoh::json::prelude::*;
    ///
    /// assert_eq!(from_str(&f64().list().to_set(), "[0.0, -0, 0]").unwrap().len(), 2);
    /// ```
    pub fn to_set(self) -> ToSet<D>
    where
        D::Output: Same,
    {
        self.map(collect_set as fn(Vec<D::Output>) -> Set<D::Output>)
    }
}

/// The decoder [`ListDecoder::to_set`] returns.
pub type ToSet<D> = Map<ListDecoder<D>, fn(Vec<Element<D>>) -> Set<Element<D>>>;

type Element<D> = <D as Decoder<Json>>::Output;

fn collect_set<T: Same>(items: Vec<T>) -> Set<T> {
    items.into_iter().collect()
}

pub(crate) fn non_empty_issue() -> Issue {
    Issue::new(codes::TOO_SMALL)
        .with_message_key(message_keys::TOO_SMALL_NONEMPTY)
        .with_meta("min", 1)
        .with_meta("actual", 0)
}

pub(crate) fn min_size_issue(min: usize, actual: usize) -> Issue {
    Issue::new(codes::TOO_SMALL)
        .with_meta("min", min)
        .with_meta("actual", actual)
}

pub(crate) fn max_size_issue(max: usize, actual: usize) -> Issue {
    Issue::new(codes::TOO_BIG)
        .with_meta("max", max)
        .with_meta("actual", actual)
}

pub(crate) fn size_issue(expected: usize, actual: usize) -> Issue {
    Issue::new(codes::INVALID_SIZE)
        .with_meta("expected", expected)
        .with_meta("actual", actual)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::{f64, i64, missing, string};
    use serde_json::Value;
    use serde_json::json;

    fn first<T: std::fmt::Debug>(result: Result<T, Issues>) -> Issue {
        result.unwrap_err().into_iter().next().unwrap()
    }

    #[test]
    fn nullable_accepts_null_but_not_missing() {
        let decoder = string().nullable();
        assert_eq!(decoder.decode(&Value::Null).unwrap(), None);
        let issues = decoder.decode(missing()).unwrap_err();
        assert_eq!(issues.iter().next().unwrap().code(), "required");
    }

    #[test]
    fn every_element_issue_is_reported_under_its_index() {
        let issues = i64().list().decode(&json!([1, "a", 3, true])).unwrap_err();
        let paths: Vec<String> = issues.iter().map(|i| i.path().to_string()).collect();
        assert_eq!(paths, ["/1", "/3"]);
    }

    #[test]
    fn unique_reports_each_duplicate_once_where_it_first_repeats() {
        let issue = first(i64().list().unique().decode(&json!([1, 2, 2, 1])));
        assert_eq!(issue.meta()["duplicates"], MetaValue::from(vec![2, 1]));
        assert_eq!(issue.message(), "must not contain duplicates: [2, 1]");
    }

    #[test]
    fn floats_are_compared_as_the_value_model_compares_them() {
        let input: Value = serde_json::from_str("[0.0, -0.0]").unwrap();
        assert!(f64().list().unique().decode(&input).is_ok());
        assert!(f64().list().contains(-0.0).decode(&json!([0.0])).is_err());
    }

    #[test]
    fn contains_all_lists_what_is_missing_as_often_as_given() {
        let issue = first(i64().list().contains_all([1, 3, 3]).decode(&json!([2])));
        assert_eq!(issue.meta()["missing"], MetaValue::from(vec![1, 3, 3]));
        assert_eq!(
            issue.message(),
            "must contain all of [1, 3, 3] (missing: [1, 3, 3])"
        );
        assert!(
            i64()
                .list()
                .contains_all([1, 3])
                .decode(&json!([3, 1]))
                .is_ok()
        );
    }

    #[test]
    fn to_set_keeps_each_element_once() {
        let set = i64().list().to_set().decode(&json!([1, 2, 1, 3])).unwrap();
        assert_eq!(set.into_iter().collect::<Vec<_>>(), [1, 2, 3]);
    }

    #[test]
    fn non_empty_uses_its_own_message_key() {
        let issues = i64().list().non_empty().decode(&json!([])).unwrap_err();
        assert_eq!(
            issues.iter().next().unwrap().message_key(),
            "too_small.nonempty"
        );
    }
}
