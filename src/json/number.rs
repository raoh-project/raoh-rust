use super::Json;
use super::steps::Steps;
use super::string::StringDecoder;
use super::text::{Integral, Source, read_integer};
use super::{View, required};
use crate::decoder::Decoder;
use crate::issue::{Issue, Issues};
use crate::meta::MetaValue;
use crate::path::Path;
use crate::value::float::{Float, float_order, float_same};
use crate::{codes, message_keys};
use std::cmp::Ordering;
use std::ops::RangeInclusive;

mod sealed {
    pub trait Sealed {}
    impl Sealed for i32 {}
    impl Sealed for i64 {}
    impl Sealed for u32 {}
    impl Sealed for u64 {}
}

/// An integer type a JSON number can be decoded into. It cannot be implemented outside this
/// crate.
pub trait Integer: sealed::Sealed + Copy + Ord + Into<MetaValue> + Send + Sync + 'static {
    /// What an issue names the type in `expected`.
    const EXPECTED: &'static str;
    /// The smallest positive value.
    const ONE: Self;

    /// `value` as this type, if it holds it.
    fn from_integer(value: i128) -> Option<Self>;

    /// Whether `self` is a multiple of `divisor`, which is not zero.
    fn is_multiple_of(self, divisor: Self) -> bool;

    /// Whether `self` is zero.
    fn is_zero(self) -> bool;
}

/// An integer type that holds negative values. It cannot be implemented outside this crate.
pub trait SignedInteger: Integer {
    /// Zero.
    const ZERO: Self;
    /// The largest negative value.
    const MINUS_ONE: Self;
}

macro_rules! integer {
    ($t:ty, $expected:literal) => {
        impl Integer for $t {
            const EXPECTED: &'static str = $expected;
            const ONE: Self = 1;

            fn from_integer(value: i128) -> Option<Self> {
                <$t>::try_from(value).ok()
            }

            fn is_multiple_of(self, divisor: Self) -> bool {
                self.checked_rem(divisor).is_none_or(|r| r == 0)
            }

            fn is_zero(self) -> bool {
                self == 0
            }
        }
    };
}

integer!(i32, "integer");
integer!(i64, "long");
integer!(u32, "integer");
integer!(u64, "long");

impl SignedInteger for i32 {
    const ZERO: Self = 0;
    const MINUS_ONE: Self = -1;
}

impl SignedInteger for i64 {
    const ZERO: Self = 0;
    const MINUS_ONE: Self = -1;
}

/// A decoder of an integer into `T`: a JSON number written as one, or with
/// [`StringDecoder::to_int`] and [`StringDecoder::to_long`], a string.
///
/// From JSON, missing or `null` is `required`. A value of another kind, and a number with a
/// fraction or an exponent, is `type_mismatch` with the kind found as `actual` (`number` for such
/// a number): `1.0` and `1e2` are not integers, as the text they are written with says. An integer
/// `T` cannot hold is `type_mismatch` under the message key `type_mismatch.numeric_range`, with
/// `expected` alone.
///
/// From a string, the text is `[+-]?[0-9]+`, leading zeros allowed; anything else is
/// `type_mismatch` without `actual`, since the string was the kind expected and only its text
/// failed to read.
///
/// Constraints run in the order they are written, and the first to fail is the one reported.
#[derive(Clone, Debug)]
pub struct IntDecoder<T> {
    source: Source,
    steps: Steps<T>,
}

impl<T> IntDecoder<T> {
    fn from_source(source: Source) -> Self {
        Self {
            source,
            steps: Steps::default(),
        }
    }

    pub(crate) fn from_text(string: StringDecoder) -> Self {
        Self::from_source(Source::Text(Box::new(string)))
    }
}

/// A decoder of a JSON integer into an `i32`.
pub fn i32() -> IntDecoder<i32> {
    IntDecoder::from_source(Source::Json)
}

/// A decoder of a JSON integer into an `i64`.
pub fn i64() -> IntDecoder<i64> {
    IntDecoder::from_source(Source::Json)
}

/// A decoder of a JSON integer into a `u32`.
pub fn u32() -> IntDecoder<u32> {
    IntDecoder::from_source(Source::Json)
}

/// A decoder of a JSON integer into a `u64`.
pub fn u64() -> IntDecoder<u64> {
    IntDecoder::from_source(Source::Json)
}

fn expected(path: &Path<'_>, expected: &'static str) -> Issue {
    Issue::at_path(path, codes::TYPE_MISMATCH).with_meta("expected", expected)
}

fn numeric_range(path: &Path<'_>, name: &'static str) -> Issue {
    expected(path, name).with_message_key(message_keys::TYPE_MISMATCH_NUMERIC_RANGE)
}

impl<T: Integer> Decoder<Json> for IntDecoder<T> {
    type Output = T;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<T, Issues> {
        let found = match &self.source {
            Source::Json => match input.view() {
                View::Number(n) => match n.integral() {
                    Integral::Value(v) => {
                        T::from_integer(v).ok_or_else(|| numeric_range(path, T::EXPECTED))
                    }
                    Integral::TooLarge => Err(numeric_range(path, T::EXPECTED)),
                    Integral::Not => Err(expected(path, T::EXPECTED).with_meta("actual", "number")),
                },
                view if view.is_null_or_missing() => Err(required(path)),
                view => Err(expected(path, T::EXPECTED).with_meta("actual", view.kind())),
            },
            Source::Text(string) => {
                let text = string.decode_at(input, path)?;
                match read_integer(&text) {
                    Integral::Value(v) => {
                        T::from_integer(v).ok_or_else(|| numeric_range(path, T::EXPECTED))
                    }
                    Integral::TooLarge => Err(numeric_range(path, T::EXPECTED)),
                    Integral::Not => Err(expected(path, T::EXPECTED)),
                }
            }
        };
        let value = found.map_err(|issue| self.steps.base_issue(issue))?;
        self.steps.run(value, path)
    }
}

fn out_of_range(key: &'static str) -> Issue {
    Issue::new(codes::OUT_OF_RANGE).with_message_key(key)
}

impl<T: Integer> IntDecoder<T> {
    /// Gives the most recent constraint written before this, or the reading of the integer when
    /// there is none, a custom message that every language shows as written.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.steps.set_message(message.into());
        self
    }

    /// Requires at least `min`: `out_of_range` with `min` and `actual`.
    pub fn min(mut self, min: T) -> Self {
        self.steps.require(
            move |v| *v >= min,
            move |v| {
                out_of_range(message_keys::OUT_OF_RANGE_MINIMUM)
                    .with_meta("min", min)
                    .with_meta("actual", *v)
            },
        );
        self
    }

    /// Allows at most `max`: `out_of_range` with `max` and `actual`.
    pub fn max(mut self, max: T) -> Self {
        self.steps.require(
            move |v| *v <= max,
            move |v| {
                out_of_range(message_keys::OUT_OF_RANGE_MAXIMUM)
                    .with_meta("max", max)
                    .with_meta("actual", *v)
            },
        );
        self
    }

    /// Requires a value within `range`, both ends included: `out_of_range` with `min`, `max` and
    /// `actual`.
    pub fn range(mut self, range: RangeInclusive<T>) -> Self {
        let (min, max) = range.into_inner();
        self.steps.require(
            move |v| min <= *v && *v <= max,
            move |v| {
                out_of_range(message_keys::OUT_OF_RANGE_RANGE)
                    .with_meta("min", min)
                    .with_meta("max", max)
                    .with_meta("actual", *v)
            },
        );
        self
    }

    /// Requires a value above zero: `out_of_range` with `min` 1 and `actual`.
    pub fn positive(mut self) -> Self {
        self.steps.require(
            |v| *v >= T::ONE,
            |v| {
                out_of_range(message_keys::OUT_OF_RANGE_POSITIVE)
                    .with_meta("min", T::ONE)
                    .with_meta("actual", *v)
            },
        );
        self
    }

    /// Requires a multiple of `divisor`: `not_multiple_of` with `divisor` and `actual`.
    ///
    /// # Panics
    ///
    /// When `divisor` is zero.
    pub fn multiple_of(mut self, divisor: T) -> Self {
        assert!(!divisor.is_zero(), "divisor must not be zero");
        self.steps.require(
            move |v| v.is_multiple_of(divisor),
            move |v| {
                Issue::new(codes::NOT_MULTIPLE_OF)
                    .with_meta("divisor", divisor)
                    .with_meta("actual", *v)
            },
        );
        self
    }

    /// Requires one of `allowed`: `not_allowed` with `allowed` in ascending order, and `actual`.
    pub fn one_of(mut self, allowed: impl IntoIterator<Item = T>) -> Self {
        let mut allowed: Vec<T> = allowed.into_iter().collect();
        allowed.sort();
        allowed.dedup();
        let check = allowed.clone();
        self.steps.require(
            move |v| check.binary_search(v).is_ok(),
            move |v| {
                Issue::new(codes::NOT_ALLOWED)
                    .with_meta("allowed", allowed.clone())
                    .with_meta("actual", *v)
            },
        );
        self
    }
}

impl<T: SignedInteger> IntDecoder<T> {
    /// Requires a value below zero: `out_of_range` with `max` -1 and `actual`.
    pub fn negative(mut self) -> Self {
        self.steps.require(
            |v| *v <= T::MINUS_ONE,
            |v| {
                out_of_range(message_keys::OUT_OF_RANGE_NEGATIVE)
                    .with_meta("max", T::MINUS_ONE)
                    .with_meta("actual", *v)
            },
        );
        self
    }

    /// Requires zero or above: `out_of_range` with `min` 0 and `actual`.
    pub fn non_negative(mut self) -> Self {
        self.steps.require(
            |v| *v >= T::ZERO,
            |v| {
                out_of_range(message_keys::OUT_OF_RANGE_NON_NEGATIVE)
                    .with_meta("min", T::ZERO)
                    .with_meta("actual", *v)
            },
        );
        self
    }

    /// Requires zero or below: `out_of_range` with `max` 0 and `actual`.
    pub fn non_positive(mut self) -> Self {
        self.steps.require(
            |v| *v <= T::ZERO,
            |v| {
                out_of_range(message_keys::OUT_OF_RANGE_NON_POSITIVE)
                    .with_meta("max", T::ZERO)
                    .with_meta("actual", *v)
            },
        );
        self
    }
}

/// A decoder of a JSON number into a float, `f32` or `f64`.
///
/// Any JSON number is read from its text and rounded to the nearest value of the type, once. A
/// number whose magnitude rounds beyond the type's range is `type_mismatch` under the message key
/// `type_mismatch.numeric_range`; a number with a minus sign whose value is zero, or that rounds
/// to zero, is -0. Missing or `null` is `required`, and any other kind `type_mismatch`.
///
/// The bounds compare in the float order of the value model: -0 is below +0, so `negative` takes
/// -0 and `non_negative` refuses it, and NaN is above everything. Bounds appear in messages as the
/// shortest decimal that reads back as the bound, such as `0.1` for an `f32` and `1.0E7`.
#[derive(Clone, Debug)]
pub struct FloatDecoder<F> {
    steps: Steps<F>,
}

/// The decoder [`f32()`] returns.
pub type F32Decoder = FloatDecoder<f32>;

/// The decoder [`f64()`] returns.
pub type F64Decoder = FloatDecoder<f64>;

/// A decoder of a JSON number into an `f32`, with `expected` `float`.
pub fn f32() -> F32Decoder {
    FloatDecoder {
        steps: Steps::default(),
    }
}

/// A decoder of a JSON number into an `f64`, with `expected` `double`.
pub fn f64() -> F64Decoder {
    FloatDecoder {
        steps: Steps::default(),
    }
}

impl<F: Float> Decoder<Json> for FloatDecoder<F> {
    type Output = F;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<F, Issues> {
        let found = match input.view() {
            View::Number(n) => match F::from_number(&n) {
                Some(v) if !v.infinite() => Ok(v),
                _ => Err(numeric_range(path, F::EXPECTED)),
            },
            view if view.is_null_or_missing() => Err(required(path)),
            view => Err(expected(path, F::EXPECTED).with_meta("actual", view.kind())),
        };
        let value = found.map_err(|issue| self.steps.base_issue(issue))?;
        self.steps.run(value, path)
    }
}

impl<F: Float> FloatDecoder<F> {
    fn bound(
        mut self,
        ok: impl Fn(Ordering) -> bool + Send + Sync + 'static,
        against: F,
        key: &'static str,
        bounds: Vec<(&'static str, F)>,
    ) -> Self {
        self.steps.require(
            move |v| ok(float_order(*v, against)),
            move |v| {
                bounds
                    .iter()
                    .fold(out_of_range(key), |issue, (name, bound)| {
                        issue.with_meta(*name, *bound)
                    })
                    .with_meta("actual", *v)
            },
        );
        self
    }

    /// Gives the most recent constraint written before this, or the reading of the number when
    /// there is none, a custom message that every language shows as written.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.steps.set_message(message.into());
        self
    }

    /// Requires at least `min`: `out_of_range` with `min` and `actual`.
    pub fn min(self, min: F) -> Self {
        self.bound(
            Ordering::is_ge,
            min,
            message_keys::OUT_OF_RANGE_MINIMUM,
            vec![("min", min)],
        )
    }

    /// Allows at most `max`: `out_of_range` with `max` and `actual`.
    pub fn max(self, max: F) -> Self {
        self.bound(
            Ordering::is_le,
            max,
            message_keys::OUT_OF_RANGE_MAXIMUM,
            vec![("max", max)],
        )
    }

    /// Requires a value within `range`, both ends included: `out_of_range` with `min`, `max` and
    /// `actual`.
    pub fn range(mut self, range: RangeInclusive<F>) -> Self {
        let (min, max) = range.into_inner();
        self.steps.require(
            move |v| float_order(*v, min).is_ge() && float_order(*v, max).is_le(),
            move |v| {
                out_of_range(message_keys::OUT_OF_RANGE_RANGE)
                    .with_meta("min", min)
                    .with_meta("max", max)
                    .with_meta("actual", *v)
            },
        );
        self
    }

    /// Requires a value above +0: `out_of_range` with `min` 0 and `actual`. -0 is not above +0.
    pub fn positive(self) -> Self {
        self.bound(
            Ordering::is_gt,
            F::ZERO,
            message_keys::OUT_OF_RANGE_POSITIVE,
            vec![("min", F::ZERO)],
        )
    }

    /// Requires a value below +0: `out_of_range` with `max` 0 and `actual`. -0 is below +0.
    pub fn negative(self) -> Self {
        self.bound(
            Ordering::is_lt,
            F::ZERO,
            message_keys::OUT_OF_RANGE_NEGATIVE,
            vec![("max", F::ZERO)],
        )
    }

    /// Requires +0 or above: `out_of_range` with `min` 0 and `actual`. -0 is below +0.
    pub fn non_negative(self) -> Self {
        self.bound(
            Ordering::is_ge,
            F::ZERO,
            message_keys::OUT_OF_RANGE_NON_NEGATIVE,
            vec![("min", F::ZERO)],
        )
    }

    /// Requires +0 or below: `out_of_range` with `max` 0 and `actual`.
    pub fn non_positive(self) -> Self {
        self.bound(
            Ordering::is_le,
            F::ZERO,
            message_keys::OUT_OF_RANGE_NON_POSITIVE,
            vec![("max", F::ZERO)],
        )
    }

    /// Requires one of `allowed`, compared as the value model compares floats (+0 and -0 differ,
    /// and NaN is NaN): `not_allowed` with `allowed` in the float order, and `actual`.
    pub fn one_of(mut self, allowed: impl IntoIterator<Item = F>) -> Self {
        let mut allowed: Vec<F> = allowed.into_iter().collect();
        allowed.sort_by(|a, b| float_order(*a, *b));
        allowed.dedup_by(|a, b| float_same(*a, *b));
        let check = allowed.clone();
        self.steps.require(
            move |v| check.iter().any(|a| float_same(*a, *v)),
            move |v| {
                Issue::new(codes::NOT_ALLOWED)
                    .with_meta("allowed", allowed.clone())
                    .with_meta("actual", *v)
            },
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn first(result: Result<impl std::fmt::Debug, Issues>) -> Issue {
        result.unwrap_err().into_iter().next().unwrap()
    }

    fn number(text: &str) -> crate::json::Node {
        text.parse().unwrap()
    }

    #[test]
    fn a_number_that_is_not_an_integer_names_what_it_is() {
        for issue in [
            first(i32().decode(&json!(1.5))),
            first(u64().decode(&json!(1e2))),
        ] {
            assert_eq!(issue.message_key(), "type_mismatch");
            assert_eq!(issue.meta()["actual"], MetaValue::from("number"));
        }
    }

    #[test]
    fn an_integer_the_type_cannot_hold_is_outside_its_range() {
        for (issue, expected) in [
            (first(i32().decode(&json!(3_000_000_000_i64))), "integer"),
            (first(i64().decode(&json!(u64::MAX))), "long"),
            (first(u32().decode(&json!(-1))), "integer"),
            (first(u64().decode(&json!(i64::MIN))), "long"),
        ] {
            assert_eq!(issue.code(), "type_mismatch");
            assert_eq!(issue.message_key(), "type_mismatch.numeric_range");
            assert_eq!(issue.meta().len(), 1);
            assert_eq!(issue.meta()["expected"], MetaValue::from(expected));
            assert_eq!(
                issue.message(),
                format!("value is outside the {expected} range")
            );
        }
        assert_eq!(u64().decode(&json!(u64::MAX)).unwrap(), u64::MAX);
        assert_eq!(i64().decode(&json!(i64::MIN)).unwrap(), i64::MIN);
    }

    #[test]
    fn range_reports_both_bounds() {
        let issue = first(u32().range(0..=150).decode(&json!(200)));
        assert_eq!(issue.message_key(), "out_of_range.range");
        assert_eq!(issue.meta()["min"], MetaValue::from(0));
        assert_eq!(issue.meta()["max"], MetaValue::from(150));
        assert_eq!(issue.meta()["actual"], MetaValue::from(200));
        assert_eq!(issue.message(), "must be between 0 and 150");
    }

    #[test]
    fn signed_positive_and_negative_bounds_follow_java() {
        assert_eq!(
            first(i32().negative().decode(&json!(0))).meta()["max"],
            MetaValue::from(-1)
        );
        assert_eq!(
            first(i32().positive().decode(&json!(0))).meta()["min"],
            MetaValue::from(1)
        );
    }

    #[test]
    fn multiple_of_does_not_overflow() {
        assert!(i64().multiple_of(-1).decode(&json!(i64::MIN)).is_ok());
        assert_eq!(
            first(i64().multiple_of(3).decode(&json!(4))).code(),
            "not_multiple_of"
        );
    }

    #[test]
    fn floats_are_rounded_once_and_keep_a_negative_zero() {
        assert_eq!(f32().decode(&number("0.1")).unwrap(), 0.1f32);
        assert!(f64().decode(&number("-0.0")).unwrap().is_sign_negative());
        assert_eq!(
            first(f64().decode(&number("1e400"))).message_key(),
            "type_mismatch.numeric_range"
        );
        assert_eq!(
            first(f32().decode(&number("1e39"))).meta()["expected"],
            MetaValue::from("float")
        );
    }

    #[test]
    fn the_text_minus_zero_is_negative_zero_for_floats_and_zero_for_integers() {
        let minus_zero = number("-0");
        assert!(f64().decode(&minus_zero).unwrap().is_sign_negative());
        assert!(f32().decode(&minus_zero).unwrap().is_sign_negative());
        assert_eq!(i32().decode(&minus_zero).unwrap(), 0);
        assert!(i64().decode(&number("-0.0")).is_err());
    }

    #[test]
    fn a_serde_json_number_is_read_from_its_text_when_it_keeps_one() {
        let minus_zero =
            serde_json::Value::Number(serde_json::Number::from_string_unchecked("-0".into()));
        assert!(f64().decode(&minus_zero).unwrap().is_sign_negative());
        assert_eq!(i32().decode(&minus_zero).unwrap(), 0);
        let big: serde_json::Value =
            serde_json::from_str("123456789012345678901234567890").unwrap();
        assert_eq!(
            first(i64().decode(&big)).message_key(),
            "type_mismatch.numeric_range"
        );
    }

    #[test]
    fn float_bounds_use_the_float_order() {
        assert!(f64().negative().decode(&number("-0.0")).is_ok());
        assert!(f64().non_negative().decode(&number("-0.0")).is_err());
        assert!(f64().one_of([0.0]).decode(&number("-0.0")).is_err());
        assert_eq!(
            first(f64().min(1e7).decode(&json!(1))).message(),
            "must be at least 1.0E7"
        );
        assert_eq!(
            first(f32().min(0.1).decode(&json!(0.05))).message(),
            "must be at least 0.1"
        );
    }
}
