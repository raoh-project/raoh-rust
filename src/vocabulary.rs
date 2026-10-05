//! The codes an [`Issue`](crate::Issue) carries and the message keys that refine them.
//!
//! The codes are shared with Raoh for Java and PHP, so a program that reads them reads the issues
//! of any of them the same way.

/// What kind of problem an issue reports.
pub mod codes {
    /// The value is missing or null.
    pub const REQUIRED: &str = "required";
    /// The string is empty or whitespace only.
    pub const BLANK: &str = "blank";
    /// The string has fewer characters than allowed.
    pub const TOO_SHORT: &str = "too_short";
    /// The string has more characters than allowed.
    pub const TOO_LONG: &str = "too_long";
    /// The string does not have exactly the required number of characters.
    pub const INVALID_LENGTH: &str = "invalid_length";
    /// The number is outside its bounds.
    pub const OUT_OF_RANGE: &str = "out_of_range";
    /// The number is not a multiple of the divisor.
    pub const NOT_MULTIPLE_OF: &str = "not_multiple_of";
    /// The decimal has more fraction digits than allowed.
    pub const INVALID_SCALE: &str = "invalid_scale";
    /// The list has fewer elements than allowed.
    pub const TOO_SMALL: &str = "too_small";
    /// The list has more elements than allowed.
    pub const TOO_BIG: &str = "too_big";
    /// The list does not have exactly the required number of elements.
    pub const INVALID_SIZE: &str = "invalid_size";
    /// The value is not the one required.
    pub const INVALID_VALUE: &str = "invalid_value";
    /// The string does not have the required form.
    pub const INVALID_FORMAT: &str = "invalid_format";
    /// The value is of another JSON type than the one required.
    pub const TYPE_MISMATCH: &str = "type_mismatch";
    /// The object has a member no decoder declares.
    pub const UNKNOWN_FIELD: &str = "unknown_field";
    /// The list lacks a required element.
    pub const MISSING_ELEMENT: &str = "missing_element";
    /// The list lacks some of the required elements.
    pub const MISSING_ELEMENTS: &str = "missing_elements";
    /// The list holds an element more than once.
    pub const DUPLICATE_ELEMENT: &str = "duplicate_element";
    /// The value is not one of the allowed values.
    pub const NOT_ALLOWED: &str = "not_allowed";
    /// None of the alternatives decoded the value.
    pub const ONE_OF_FAILED: &str = "one_of_failed";
    /// A member is missing.
    pub const MISSING_FIELD: &str = "missing_field";

    /// Every code above.
    pub const ALL: &[&str] = &[
        REQUIRED,
        BLANK,
        TOO_SHORT,
        TOO_LONG,
        INVALID_LENGTH,
        OUT_OF_RANGE,
        NOT_MULTIPLE_OF,
        INVALID_SCALE,
        TOO_SMALL,
        TOO_BIG,
        INVALID_SIZE,
        INVALID_VALUE,
        INVALID_FORMAT,
        TYPE_MISMATCH,
        UNKNOWN_FIELD,
        MISSING_ELEMENT,
        MISSING_ELEMENTS,
        DUPLICATE_ELEMENT,
        NOT_ALLOWED,
        ONE_OF_FAILED,
        MISSING_FIELD,
    ];
}

/// Keys that name which check produced an issue, where one code covers several.
///
/// A message key is its code followed by `.` and a refinement, so a catalogue that has a
/// template only for the code still resolves it. An issue whose message key is not one of these
/// uses its code as its key.
///
/// These are the keys of the Raoh Specification's issue catalogue. `invalid_format.json` is this
/// crate's own, for text that is not JSON.
pub mod message_keys {
    /// `out_of_range` from a lower bound.
    pub const OUT_OF_RANGE_MINIMUM: &str = "out_of_range.minimum";
    /// `out_of_range` from an upper bound.
    pub const OUT_OF_RANGE_MAXIMUM: &str = "out_of_range.maximum";
    /// `out_of_range` from both bounds.
    pub const OUT_OF_RANGE_RANGE: &str = "out_of_range.range";
    /// `out_of_range` from `positive()`.
    pub const OUT_OF_RANGE_POSITIVE: &str = "out_of_range.positive";
    /// `out_of_range` from `negative()`.
    pub const OUT_OF_RANGE_NEGATIVE: &str = "out_of_range.negative";
    /// `out_of_range` from `non_negative()`.
    pub const OUT_OF_RANGE_NON_NEGATIVE: &str = "out_of_range.non_negative";
    /// `out_of_range` from `non_positive()`.
    pub const OUT_OF_RANGE_NON_POSITIVE: &str = "out_of_range.non_positive";
    /// `out_of_range` from `before()`.
    pub const OUT_OF_RANGE_BEFORE: &str = "out_of_range.before";
    /// `out_of_range` from `after()`.
    pub const OUT_OF_RANGE_AFTER: &str = "out_of_range.after";
    /// `out_of_range` from `between()`.
    pub const OUT_OF_RANGE_BETWEEN: &str = "out_of_range.between";
    /// `type_mismatch` for a number the type cannot hold.
    pub const TYPE_MISMATCH_NUMERIC_RANGE: &str = "type_mismatch.numeric_range";
    /// `type_mismatch` for a map whose keys are not strings, which no decoder over JSON gives.
    pub const TYPE_MISMATCH_STRING_KEYS: &str = "type_mismatch.string_keys";
    /// `too_small` from `non_empty()`.
    pub const TOO_SMALL_NONEMPTY: &str = "too_small.nonempty";
    /// `invalid_format` from `email()`.
    pub const INVALID_FORMAT_EMAIL: &str = "invalid_format.email";
    /// `invalid_format` from `url()`.
    pub const INVALID_FORMAT_URL: &str = "invalid_format.url";
    /// `invalid_format` from `uri()`.
    pub const INVALID_FORMAT_URI: &str = "invalid_format.uri";
    /// `invalid_format` from `uuid()`.
    pub const INVALID_FORMAT_UUID: &str = "invalid_format.uuid";
    /// `invalid_format` from `ip()`.
    pub const INVALID_FORMAT_IP: &str = "invalid_format.ip";
    /// `invalid_format` from `ipv4()`.
    pub const INVALID_FORMAT_IPV4: &str = "invalid_format.ipv4";
    /// `invalid_format` from `ipv6()`.
    pub const INVALID_FORMAT_IPV6: &str = "invalid_format.ipv6";
    /// `invalid_format` from `ulid()`.
    pub const INVALID_FORMAT_ULID: &str = "invalid_format.ulid";
    /// `invalid_format` from `cuid()`.
    pub const INVALID_FORMAT_CUID: &str = "invalid_format.cuid";
    /// `invalid_format` from `starts_with()`.
    pub const INVALID_FORMAT_STARTS_WITH: &str = "invalid_format.starts_with";
    /// `invalid_format` from `ends_with()`.
    pub const INVALID_FORMAT_ENDS_WITH: &str = "invalid_format.ends_with";
    /// `invalid_format` from `contains()`.
    pub const INVALID_FORMAT_INCLUDES: &str = "invalid_format.includes";
    /// `invalid_format` from `enum_of()`.
    pub const INVALID_FORMAT_ENUM: &str = "invalid_format.enum";
    /// `invalid_format` from `literal()`.
    pub const INVALID_FORMAT_LITERAL: &str = "invalid_format.literal";
    /// `invalid_format` from `instant()`.
    pub const INVALID_FORMAT_INSTANT: &str = "invalid_format.instant";
    /// `invalid_format` from `date()`.
    pub const INVALID_FORMAT_DATE: &str = "invalid_format.date";
    /// `invalid_format` from `time()`.
    pub const INVALID_FORMAT_TIME: &str = "invalid_format.time";
    /// `invalid_format` from `date_time()`.
    pub const INVALID_FORMAT_DATE_TIME: &str = "invalid_format.date_time";
    /// `invalid_format` from `offset_date_time()`.
    pub const INVALID_FORMAT_OFFSET_DATE_TIME: &str = "invalid_format.offset_date_time";
    /// `invalid_format` from text that is not JSON.
    pub const INVALID_FORMAT_JSON: &str = "invalid_format.json";

    /// Every message key above.
    pub const ALL: &[&str] = &[
        OUT_OF_RANGE_MINIMUM,
        OUT_OF_RANGE_MAXIMUM,
        OUT_OF_RANGE_RANGE,
        OUT_OF_RANGE_POSITIVE,
        OUT_OF_RANGE_NEGATIVE,
        OUT_OF_RANGE_NON_NEGATIVE,
        OUT_OF_RANGE_NON_POSITIVE,
        OUT_OF_RANGE_BEFORE,
        OUT_OF_RANGE_AFTER,
        OUT_OF_RANGE_BETWEEN,
        TYPE_MISMATCH_NUMERIC_RANGE,
        TYPE_MISMATCH_STRING_KEYS,
        TOO_SMALL_NONEMPTY,
        INVALID_FORMAT_EMAIL,
        INVALID_FORMAT_URL,
        INVALID_FORMAT_URI,
        INVALID_FORMAT_UUID,
        INVALID_FORMAT_IP,
        INVALID_FORMAT_IPV4,
        INVALID_FORMAT_IPV6,
        INVALID_FORMAT_ULID,
        INVALID_FORMAT_CUID,
        INVALID_FORMAT_STARTS_WITH,
        INVALID_FORMAT_ENDS_WITH,
        INVALID_FORMAT_INCLUDES,
        INVALID_FORMAT_ENUM,
        INVALID_FORMAT_LITERAL,
        INVALID_FORMAT_INSTANT,
        INVALID_FORMAT_DATE,
        INVALID_FORMAT_TIME,
        INVALID_FORMAT_DATE_TIME,
        INVALID_FORMAT_OFFSET_DATE_TIME,
        INVALID_FORMAT_JSON,
    ];
}
