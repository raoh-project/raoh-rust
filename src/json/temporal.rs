use super::Json;
use super::steps::Steps;
use super::string::StringDecoder;
use crate::decoder::Decoder;
use crate::issue::{Issue, Issues};
use crate::path::Path;
use crate::value::temporal::Chronological;
use crate::{codes, message_keys};
use std::cmp::Ordering;

/// A decoder of a temporal value read from a string: what [`StringDecoder::instant`],
/// [`date`](StringDecoder::date), [`time`](StringDecoder::time),
/// [`date_time`](StringDecoder::date_time) and
/// [`offset_date_time`](StringDecoder::offset_date_time) return.
///
/// Text that is not one is `invalid_format`. The bounds compare chronologically, and an
/// offset date-time by its instant alone, so `09:00Z` is not before `10:00+01:00`.
///
/// ```
/// use raoh::json::prelude::*;
/// use raoh::Date;
///
/// let start: Date = "2024-01-01".parse().unwrap();
/// let day = string().date().after(start);
/// assert!(day.decode(&json!("2024-01-02")).is_ok());
/// let issues = day.decode(&json!("2024-01-01")).unwrap_err();
/// assert_eq!(issues.iter().next().unwrap().message(), "must be after 2024-01-01");
/// ```
#[derive(Clone, Debug)]
pub struct TemporalDecoder<T> {
    string: StringDecoder,
    steps: Steps<T>,
}

impl<T> TemporalDecoder<T> {
    pub(crate) fn new(string: StringDecoder) -> Self {
        Self {
            string,
            steps: Steps::default(),
        }
    }
}

impl<T: Chronological> Decoder<Json> for TemporalDecoder<T> {
    type Output = T;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<T, Issues> {
        let text = self.string.decode_at(input, path)?;
        let value = T::read(&text).ok_or_else(|| {
            self.steps
                .base_issue(Issue::at_path(path, codes::INVALID_FORMAT).with_message_key(T::KEY))
        })?;
        self.steps.run(value, path)
    }
}

impl<T: Chronological> TemporalDecoder<T> {
    /// Gives the most recent constraint written before this, or the reading of the text when
    /// there is none, a custom message that every language shows as written.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.steps.set_message(message.into());
        self
    }

    /// Requires a value strictly before `bound`: `out_of_range` with `before` and `actual`.
    pub fn before(mut self, bound: T) -> Self {
        self.steps.require(
            move |v| v.chronological_cmp(&bound) == Ordering::Less,
            move |v| {
                Issue::new(codes::OUT_OF_RANGE)
                    .with_message_key(message_keys::OUT_OF_RANGE_BEFORE)
                    .with_meta("before", bound)
                    .with_meta("actual", *v)
            },
        );
        self
    }

    /// Requires a value strictly after `bound`: `out_of_range` with `after` and `actual`.
    pub fn after(mut self, bound: T) -> Self {
        self.steps.require(
            move |v| v.chronological_cmp(&bound) == Ordering::Greater,
            move |v| {
                Issue::new(codes::OUT_OF_RANGE)
                    .with_message_key(message_keys::OUT_OF_RANGE_AFTER)
                    .with_meta("after", bound)
                    .with_meta("actual", *v)
            },
        );
        self
    }

    /// Requires a value from `from` to `to`, both included: `out_of_range` with `from`, `to` and
    /// `actual`.
    pub fn between(mut self, from: T, to: T) -> Self {
        self.steps.require(
            move |v| v.chronological_cmp(&from).is_ge() && v.chronological_cmp(&to).is_le(),
            move |v| {
                Issue::new(codes::OUT_OF_RANGE)
                    .with_message_key(message_keys::OUT_OF_RANGE_BETWEEN)
                    .with_meta("from", from)
                    .with_meta("to", to)
                    .with_meta("actual", *v)
            },
        );
        self
    }
}
