use super::steps::Steps;
use super::string::StringDecoder;
use super::text::{Source, read_bool};
use super::unexpected;
use super::{Json, View};
use crate::codes;
use crate::decoder::Decoder;
use crate::issue::{Issue, Issues};
use crate::path::Path;

/// A decoder of a boolean: a JSON boolean, or with [`StringDecoder::to_bool`], a string.
///
/// From JSON, missing or `null` is `required`; any other kind is `type_mismatch`. From a string,
/// `true`, `1`, `yes` and `on` are true and `false`, `0`, `no` and `off` false, in any ASCII case;
/// anything else is `type_mismatch` with `expected` `boolean` and no `actual`.
#[derive(Clone, Debug)]
pub struct BoolDecoder {
    source: Source,
    steps: Steps<bool>,
}

/// A decoder of a JSON boolean.
pub fn bool() -> BoolDecoder {
    BoolDecoder {
        source: Source::Json,
        steps: Steps::default(),
    }
}

impl BoolDecoder {
    pub(crate) fn from_text(string: StringDecoder) -> Self {
        Self {
            source: Source::Text(Box::new(string)),
            steps: Steps::default(),
        }
    }
}

impl Decoder<Json> for BoolDecoder {
    type Output = bool;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<bool, Issues> {
        let found = match &self.source {
            Source::Json => match input.view() {
                View::Bool(b) => Ok(b),
                _ => Err(unexpected(path, "boolean", input)),
            },
            Source::Text(string) => {
                let text = string.decode_at(input, path)?;
                read_bool(&text).ok_or_else(|| {
                    Issue::at_path(path, codes::TYPE_MISMATCH).with_meta("expected", "boolean")
                })
            }
        };
        let value = found.map_err(|issue| self.steps.base_issue(issue))?;
        self.steps.run(value, path)
    }
}

impl BoolDecoder {
    fn exactly(mut self, expected: bool) -> Self {
        self.steps.require(
            move |b| *b == expected,
            move |b| {
                Issue::new(codes::INVALID_VALUE)
                    .with_meta("expected", expected)
                    .with_meta("actual", *b)
            },
        );
        self
    }

    /// Gives the most recent constraint written before this, or the reading of the boolean when
    /// there is none, a custom message that every language shows as written.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.steps.set_message(message.into());
        self
    }

    /// Requires `true`, as agreeing to terms does: `invalid_value` with `expected` and `actual`.
    pub fn is_true(self) -> Self {
        self.exactly(true)
    }

    /// Requires `false`: `invalid_value` with `expected` and `actual`.
    pub fn is_false(self) -> Self {
        self.exactly(false)
    }
}
