use super::Json;
use super::steps::Steps;
use super::string::StringDecoder;
use super::text::Source;
use super::{View, required};
use crate::decoder::Decoder;
use crate::issue::{Issue, Issues};
use crate::path::Path;
use crate::value::decimal::Decimal;
use crate::{codes, message_keys};
use std::cmp::Ordering;
use std::ops::RangeInclusive;

/// A decoder of a decimal: a JSON number, read from the text it was written with and keeping its
/// scale, or with [`StringDecoder::to_decimal`], a string.
///
/// From JSON, missing or `null` is `required`, and any other kind `type_mismatch` with `expected`
/// `number`. The scale is the one written; a [`serde_json::Value`] keeps it only with
/// `serde_json`'s `arbitrary_precision` on, see [the module documentation](super).
///
/// From a string, the text is `[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?`; anything else,
/// and an exponent the scale cannot hold, is `type_mismatch` with `expected` `decimal` and no
/// `actual`.
///
/// The bounds compare by value, so `1.5` and `1.50` are equal to them, and `multiple_of` and
/// `scale` read the decimal as it is.
///
/// ```
/// use raoh::json::prelude::*;
/// use raoh::Decimal;
///
/// let price = string().to_decimal().scale(2).positive();
/// let read = price.decode(&json!("12.50")).unwrap();
/// assert_eq!(read.to_string(), "12.50");
/// assert_eq!(read.scale(), 2);
/// ```
#[derive(Clone, Debug)]
pub struct DecimalDecoder {
    source: Source,
    steps: Steps<Decimal>,
}

/// A decoder of a JSON number into a [`Decimal`].
pub fn decimal() -> DecimalDecoder {
    DecimalDecoder {
        source: Source::Json,
        steps: Steps::default(),
    }
}

impl DecimalDecoder {
    pub(crate) fn from_text(string: StringDecoder) -> Self {
        Self {
            source: Source::Text(Box::new(string)),
            steps: Steps::default(),
        }
    }
}

impl Decoder<Json> for DecimalDecoder {
    type Output = Decimal;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<Decimal, Issues> {
        let found = match &self.source {
            Source::Json => match input.view() {
                View::Number(n) => Decimal::read(&n.lexeme()).ok_or_else(|| {
                    Issue::at_path(path, codes::TYPE_MISMATCH)
                        .with_meta("expected", "number")
                        .with_meta("actual", "number")
                }),
                view if view.is_null_or_missing() => Err(required(path)),
                view => Err(Issue::at_path(path, codes::TYPE_MISMATCH)
                    .with_meta("expected", "number")
                    .with_meta("actual", view.kind())),
            },
            Source::Text(string) => {
                let text = string.decode_at(input, path)?;
                Decimal::read(&text).ok_or_else(|| {
                    Issue::at_path(path, codes::TYPE_MISMATCH).with_meta("expected", "decimal")
                })
            }
        };
        let value = found.map_err(|issue| self.steps.base_issue(issue))?;
        self.steps.run(value, path)
    }
}

impl DecimalDecoder {
    fn bound(
        mut self,
        ok: impl Fn(Ordering) -> bool + Send + Sync + 'static,
        against: Decimal,
        key: &'static str,
        bounds: Vec<(&'static str, Decimal)>,
    ) -> Self {
        self.steps.require(
            move |v| ok(v.numeric_cmp(&against)),
            move |v| {
                bounds
                    .iter()
                    .fold(
                        Issue::new(codes::OUT_OF_RANGE).with_message_key(key),
                        |issue, (name, bound)| issue.with_meta(*name, bound.clone()),
                    )
                    .with_meta("actual", v.clone())
            },
        );
        self
    }

    /// Gives the most recent constraint written before this, or the reading of the decimal when
    /// there is none, a custom message that every language shows as written.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.steps.set_message(message.into());
        self
    }

    /// Requires at least `min`: `out_of_range` with `min` and `actual`.
    pub fn min(self, min: Decimal) -> Self {
        self.bound(
            Ordering::is_ge,
            min.clone(),
            message_keys::OUT_OF_RANGE_MINIMUM,
            vec![("min", min)],
        )
    }

    /// Allows at most `max`: `out_of_range` with `max` and `actual`.
    pub fn max(self, max: Decimal) -> Self {
        self.bound(
            Ordering::is_le,
            max.clone(),
            message_keys::OUT_OF_RANGE_MAXIMUM,
            vec![("max", max)],
        )
    }

    /// Requires a value within `range`, both ends included: `out_of_range` with `min`, `max` and
    /// `actual`.
    pub fn range(mut self, range: RangeInclusive<Decimal>) -> Self {
        let (min, max) = range.into_inner();
        self.steps.require(
            {
                let (min, max) = (min.clone(), max.clone());
                move |v| v.numeric_cmp(&min).is_ge() && v.numeric_cmp(&max).is_le()
            },
            move |v| {
                Issue::new(codes::OUT_OF_RANGE)
                    .with_message_key(message_keys::OUT_OF_RANGE_RANGE)
                    .with_meta("min", min.clone())
                    .with_meta("max", max.clone())
                    .with_meta("actual", v.clone())
            },
        );
        self
    }

    /// Requires a value above zero: `out_of_range` with `min` 0 and `actual`.
    pub fn positive(self) -> Self {
        self.bound(
            Ordering::is_gt,
            Decimal::zero(),
            message_keys::OUT_OF_RANGE_POSITIVE,
            vec![("min", Decimal::zero())],
        )
    }

    /// Requires a value below zero: `out_of_range` with `max` 0 and `actual`.
    pub fn negative(self) -> Self {
        self.bound(
            Ordering::is_lt,
            Decimal::zero(),
            message_keys::OUT_OF_RANGE_NEGATIVE,
            vec![("max", Decimal::zero())],
        )
    }

    /// Requires zero or above: `out_of_range` with `min` 0 and `actual`.
    pub fn non_negative(self) -> Self {
        self.bound(
            Ordering::is_ge,
            Decimal::zero(),
            message_keys::OUT_OF_RANGE_NON_NEGATIVE,
            vec![("min", Decimal::zero())],
        )
    }

    /// Requires zero or below: `out_of_range` with `max` 0 and `actual`.
    pub fn non_positive(self) -> Self {
        self.bound(
            Ordering::is_le,
            Decimal::zero(),
            message_keys::OUT_OF_RANGE_NON_POSITIVE,
            vec![("max", Decimal::zero())],
        )
    }

    /// Requires an integer multiple of `divisor`: `not_multiple_of` with `divisor` and `actual`.
    ///
    /// # Panics
    ///
    /// When `divisor` is zero.
    pub fn multiple_of(mut self, divisor: Decimal) -> Self {
        assert!(!divisor.is_zero(), "divisor must not be zero");
        let check = divisor.clone();
        self.steps.require(
            move |v| v.is_multiple_of(&check),
            move |v| {
                Issue::new(codes::NOT_MULTIPLE_OF)
                    .with_meta("divisor", divisor.clone())
                    .with_meta("actual", v.clone())
            },
        );
        self
    }

    /// Allows a scale of at most `max`, the digits after the point: `invalid_scale` with
    /// `maxScale` and `actualScale`.
    pub fn scale(mut self, max: i32) -> Self {
        self.steps.require(
            move |v| v.scale() <= max,
            move |v| {
                Issue::new(codes::INVALID_SCALE)
                    .with_meta("maxScale", max)
                    .with_meta("actualScale", v.scale())
            },
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetaValue;
    use serde_json::json;

    fn d(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    #[test]
    fn scale_counts_fraction_digits() {
        let issues = decimal().scale(1).decode(&json!(1.25)).unwrap_err();
        let issue = issues.iter().next().unwrap();
        assert_eq!(issue.code(), "invalid_scale");
        assert_eq!(issue.meta()["actualScale"], MetaValue::from(2));
    }

    #[test]
    fn positive_reports_zero_as_the_bound() {
        let issues = decimal().positive().decode(&json!(0)).unwrap_err();
        assert_eq!(
            issues.iter().next().unwrap().meta()["min"],
            MetaValue::from(Decimal::zero())
        );
    }

    #[test]
    fn bounds_compare_by_value() {
        assert!(decimal().max(d("10")).decode(&json!(10)).is_ok());
        let issues = decimal()
            .min(d("0.0005"))
            .decode(&json!(0.0001))
            .unwrap_err();
        assert_eq!(
            issues.iter().next().unwrap().message(),
            "must be at least 0.0005"
        );
    }

    #[test]
    fn text_is_read_with_its_scale() {
        let read = |s: &str| {
            string()
                .to_decimal()
                .decode(&json!(s))
                .map(|d| d.to_string())
        };
        assert_eq!(read("1e3").unwrap(), "1E+3");
        assert_eq!(read(".5").unwrap(), "0.5");
        let issues = read("1,5").unwrap_err();
        let issue = issues.iter().next().unwrap();
        assert_eq!(issue.meta()["expected"], MetaValue::from("decimal"));
        assert!(!issue.meta().contains_key("actual"));
    }

    use crate::json::string;
}
