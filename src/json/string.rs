use super::bool::BoolDecoder;
use super::decimal::DecimalDecoder;
use super::ip;
use super::number::IntDecoder;
use super::steps::Steps;
use super::temporal::TemporalDecoder;
use super::unexpected;
use super::{Json, View};
use crate::decoder::Decoder;
use crate::issue::{Issue, Issues};
use crate::path::Path;
use crate::value::temporal::{Date, DateTime, Instant, OffsetDateTime, Time};
use crate::value::uri::Uri;
use crate::value::uuid::Uuid;
use crate::{codes, message_keys};
use notation199x::{
    Form, OwnedMatcher, Pattern, PatternRead, is_white_space, read_pattern, scalar_count,
};
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, TryLockError};

/// A decoder of a JSON string.
///
/// Missing or `null` is `required`; any other kind is `type_mismatch`. Constraints and
/// transformations run in the order they are written, and the first constraint to fail is the one
/// reported. An empty string is accepted unless [`non_blank`](Self::non_blank) says otherwise.
///
/// Whitespace, case and normalization follow Unicode 18.0.0 whatever Rust release the crate is
/// built with, and a length is counted in Unicode scalar values, as the Raoh Specification has
/// them; they come from notation-199x, which Raoh for Java, Go and Rust share.
///
/// ```
/// use raoh::json::prelude::*;
///
/// let name = string().trim().non_blank().max_length(5);
/// assert_eq!(name.decode(&json!("  Ken ")).unwrap(), "Ken");
/// assert_eq!(name.decode(&json!("   ")).unwrap_err().iter().next().unwrap().code(), "blank");
/// ```
#[derive(Clone, Debug, Default)]
pub struct StringDecoder {
    steps: Steps<String>,
}

/// A decoder of a JSON string.
pub fn string() -> StringDecoder {
    StringDecoder::default()
}

impl Decoder<Json> for StringDecoder {
    type Output = String;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<String, Issues> {
        match input.view() {
            View::String(s) => self.steps.run(s.to_owned(), path),
            _ => Err(self.steps.base_issue(unexpected(path, "string", input))),
        }
    }
}

fn invalid_format(key: &'static str) -> Issue {
    Issue::new(codes::INVALID_FORMAT).with_message_key(key)
}

/// The forms of Unicode normalization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NormalizationForm {
    /// Canonical decomposition followed by canonical composition.
    #[default]
    Nfc,
    /// Canonical decomposition.
    Nfd,
    /// Compatibility decomposition followed by canonical composition.
    Nfkc,
    /// Compatibility decomposition.
    Nfkd,
}

impl NormalizationForm {
    fn form(self) -> Form {
        match self {
            NormalizationForm::Nfc => Form::Nfc,
            NormalizationForm::Nfd => Form::Nfd,
            NormalizationForm::Nfkc => Form::Nfkc,
            NormalizationForm::Nfkd => Form::Nfkd,
        }
    }
}

impl StringDecoder {
    fn transform(mut self, f: impl Fn(&str) -> String + Send + Sync + 'static) -> Self {
        self.steps.transform(move |s| f(&s));
        self
    }

    fn format(
        mut self,
        ok: impl Fn(&str) -> bool + Send + Sync + 'static,
        key: &'static str,
    ) -> Self {
        self.steps
            .require(move |s| ok(s), move |_| invalid_format(key));
        self
    }

    /// Gives the most recent constraint written before this, or the type check when there is
    /// none, a custom message that every language shows as written. Transformations such as
    /// [`trim`](Self::trim) are passed over, as they cannot fail.
    ///
    /// ```
    /// use raoh::json::prelude::*;
    ///
    /// let code = string().min_length(3).message("too short a code");
    /// let issues = code.decode(&json!("ab")).unwrap_err();
    /// assert_eq!(issues.iter().next().unwrap().message(), "too short a code");
    ///
    /// let name = string().trim().message("give a name");
    /// let issues = name.decode(&Value::Null).unwrap_err();
    /// assert_eq!(issues.iter().next().unwrap().message(), "give a name");
    /// ```
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.steps.set_message(message.into());
        self
    }

    /// Removes from both ends the characters with Unicode's `White_Space` property, which include
    /// U+3000 and U+00A0 and not control characters such as NUL. It is the set
    /// [`non_blank`](Self::non_blank) uses.
    pub fn trim(self) -> Self {
        self.transform(|s| s.trim_matches(is_white_space).to_owned())
    }

    /// Converts to lower case with Unicode 18.0.0's default case mapping: the full mappings,
    /// context-dependent ones included (a capital sigma becomes a final sigma at the end of a
    /// word), and no language's tailoring.
    pub fn lowercase(self) -> Self {
        self.transform(lowercase)
    }

    /// Converts to upper case with Unicode 18.0.0's default case mapping: the full mappings
    /// (`ß` becomes `SS`), and no language's tailoring.
    pub fn uppercase(self) -> Self {
        self.transform(uppercase)
    }

    /// Applies Unicode 18.0.0 normalization in `form`.
    pub fn normalize(self, form: NormalizationForm) -> Self {
        let form = form.form();
        // ASCII text is in every normalization form already.
        self.transform(move |s| {
            if s.is_ascii() {
                s.to_owned()
            } else {
                notation199x::normalize(form, s)
            }
        })
    }

    /// Requires a character that is not whitespace, in the sense [`trim`](Self::trim) uses:
    /// `blank`. An empty string is blank.
    pub fn non_blank(mut self) -> Self {
        self.steps.require(
            |s| !s.chars().all(is_white_space),
            |_| Issue::new(codes::BLANK),
        );
        self
    }

    /// Requires at least `n` characters, counted as Unicode scalar values: `too_short` with `min`
    /// and `actual`.
    pub fn min_length(mut self, n: usize) -> Self {
        self.steps.require(
            move |s| scalar_count(s) >= n,
            move |s| {
                Issue::new(codes::TOO_SHORT)
                    .with_meta("min", n)
                    .with_meta("actual", scalar_count(s))
            },
        );
        self
    }

    /// Allows at most `n` characters, counted as Unicode scalar values: `too_long` with `max` and
    /// `actual`.
    pub fn max_length(mut self, n: usize) -> Self {
        self.steps.require(
            move |s| scalar_count(s) <= n,
            move |s| {
                Issue::new(codes::TOO_LONG)
                    .with_meta("max", n)
                    .with_meta("actual", scalar_count(s))
            },
        );
        self
    }

    /// Requires exactly `n` characters, counted as Unicode scalar values: `invalid_length` with
    /// `expected` and `actual`.
    pub fn length(mut self, n: usize) -> Self {
        self.steps.require(
            move |s| scalar_count(s) == n,
            move |s| {
                Issue::new(codes::INVALID_LENGTH)
                    .with_meta("expected", n)
                    .with_meta("actual", scalar_count(s))
            },
        );
        self
    }

    /// Requires the string to start with `prefix`: `invalid_format` with `prefix`.
    pub fn starts_with(mut self, prefix: impl Into<String>) -> Self {
        let prefix = prefix.into();
        let expected = prefix.clone();
        self.steps.require(
            move |s| s.starts_with(expected.as_str()),
            move |_| {
                invalid_format(message_keys::INVALID_FORMAT_STARTS_WITH)
                    .with_meta("prefix", prefix.clone())
            },
        );
        self
    }

    /// Requires the string to end with `suffix`: `invalid_format` with `suffix`.
    pub fn ends_with(mut self, suffix: impl Into<String>) -> Self {
        let suffix = suffix.into();
        let expected = suffix.clone();
        self.steps.require(
            move |s| s.ends_with(expected.as_str()),
            move |_| {
                invalid_format(message_keys::INVALID_FORMAT_ENDS_WITH)
                    .with_meta("suffix", suffix.clone())
            },
        );
        self
    }

    /// Requires the string to contain `substring`: `invalid_format` with `substring`.
    pub fn contains(mut self, substring: impl Into<String>) -> Self {
        let substring = substring.into();
        let expected = substring.clone();
        self.steps.require(
            move |s| s.contains(expected.as_str()),
            move |_| {
                invalid_format(message_keys::INVALID_FORMAT_INCLUDES)
                    .with_meta("substring", substring.clone())
            },
        );
        self
    }

    /// Requires one of `allowed`: `not_allowed` with `allowed` sorted by code point, and `actual`.
    pub fn one_of<S: Into<String>>(mut self, allowed: impl IntoIterator<Item = S>) -> Self {
        let mut allowed: Vec<String> = allowed.into_iter().map(Into::into).collect();
        allowed.sort();
        allowed.dedup();
        let check = allowed.clone();
        self.steps.require(
            move |s| check.binary_search(s).is_ok(),
            move |s| {
                Issue::new(codes::NOT_ALLOWED)
                    .with_meta("allowed", allowed.clone())
                    .with_meta("actual", s.clone())
            },
        );
        self
    }

    /// Requires the whole string to be one of the strings `pattern` denotes: `invalid_format` with
    /// `pattern` as written.
    ///
    /// The pattern is written in the pattern language of the Raoh Specification, which Souther
    /// shares, and means the same set of strings in every implementation: matching is over
    /// Unicode scalar values, case-sensitive and of the whole string; `.` is every character but
    /// the line terminators, and `\d`, `\w` and `\s` are ASCII. A value is matched in one pass
    /// over it, whatever the pattern, and what one match works out is kept for the next.
    ///
    /// ```
    /// use raoh::json::prelude::*;
    ///
    /// let code = string().pattern(r"[a-z]+\d");
    /// assert!(code.decode(&json!("abc1")).is_ok());
    /// assert!(code.decode(&json!("abc1x")).is_err());
    /// ```
    ///
    /// # Panics
    ///
    /// When `pattern` is not a pattern of the language, such as one with a back reference, or is
    /// past one of its limits: a count above 134217727, groups nested more than 200 deep, or more
    /// than 250000 states.
    pub fn pattern(mut self, pattern: &str) -> Self {
        let compiled = match read_pattern(pattern) {
            PatternRead::Pattern(compiled) => compiled,
            PatternRead::Refused(refused) => panic!("{pattern:?} is not a pattern: {refused:?}"),
            PatternRead::Beyond(beyond) => {
                panic!("the pattern {pattern:?} is past a limit: {beyond:?}")
            }
        };
        let matchers = Matchers::new(compiled);
        let pattern = pattern.to_owned();
        self.steps.require(
            move |s| matchers.matches(s),
            move |_| Issue::new(codes::INVALID_FORMAT).with_meta("pattern", pattern.clone()),
        );
        self
    }

    /// Requires an email address in an ASCII profile of RFC 5321's `Mailbox`,
    /// `Dot-string "@" Domain`: `invalid_format`.
    ///
    /// The local part is atoms of RFC 5322 `atext` joined by single dots, at most 64 octets; the
    /// domain is labels of letters, digits and inner hyphens joined by single dots, each at most
    /// 63 octets and the whole at most 255; the address is at most 254 octets. A quoted local
    /// part, an address literal and non-ASCII text are outside the profile. Only the syntax is
    /// checked: `a@localhost` is accepted.
    pub fn email(self) -> Self {
        self.format(is_email, message_keys::INVALID_FORMAT_EMAIL)
    }

    /// Requires an IPv4 address as RFC 3986 writes one: four decimal numbers from 0 to 255 joined
    /// by `.`, with no leading zero: `invalid_format`.
    pub fn ipv4(self) -> Self {
        self.format(ip::is_ipv4, message_keys::INVALID_FORMAT_IPV4)
    }

    /// Requires an IPv6 address in the RFC 4291 text form: `invalid_format`. An embedded dotted
    /// quad (`::ffff:192.0.2.1`) is allowed and brackets (`[::1]`) are not. A zone ID
    /// (`fe80::1%eth0`) is allowed on a link-local or non-global multicast address and decided by
    /// its text alone, not by the host's interfaces.
    pub fn ipv6(self) -> Self {
        self.format(ip::is_ipv6, message_keys::INVALID_FORMAT_IPV6)
    }

    /// Requires an address [`ipv4`](Self::ipv4) or [`ipv6`](Self::ipv6) accepts:
    /// `invalid_format`.
    pub fn ip(self) -> Self {
        self.format(
            |s| ip::is_ipv4(s) || ip::is_ipv6(s),
            message_keys::INVALID_FORMAT_IP,
        )
    }

    /// Requires a ULID in its canonical text form: 26 characters of Crockford's base 32 in either
    /// case (the digits and the letters but I, L, O and U), whose value fits 128 bits, that is, at
    /// most `7ZZZZZZZZZZZZZZZZZZZZZZZZZ`: `invalid_format`. The string is given unchanged.
    pub fn ulid(self) -> Self {
        self.format(
            |s| {
                s.len() == 26
                    && s.as_bytes()[0] <= b'7'
                    && s.bytes().all(|b| {
                        b.is_ascii_digit()
                            || (b.is_ascii_alphabetic()
                                && !matches!(b.to_ascii_uppercase(), b'I' | b'L' | b'O' | b'U'))
                    })
            },
            message_keys::INVALID_FORMAT_ULID,
        )
    }

    /// Requires a CUID of version 1, `c` followed by 24 lower-case ASCII letters or digits:
    /// `invalid_format`.
    pub fn cuid(self) -> Self {
        self.format(
            |s| {
                s.len() == 25
                    && s.starts_with('c')
                    && s[1..]
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            },
            message_keys::INVALID_FORMAT_CUID,
        )
    }

    /// A decoder that parses the string into a `T` with [`FromStr`]: `invalid_format` when it does
    /// not parse.
    ///
    /// ```
    /// use raoh::json::prelude::*;
    /// use std::net::IpAddr;
    ///
    /// let addr = string().parse::<IpAddr>();
    /// assert!(addr.decode(&json!("::1")).is_ok());
    /// ```
    pub fn parse<T: FromStr>(self) -> Parse<T> {
        Parse {
            string: self,
            message: None,
            target: PhantomData,
        }
    }

    /// A decoder that reads the string as a [`Uuid`]: 32 hexadecimal digits in either case grouped
    /// 8-4-4-4-12 by hyphens, of any version. Braces, a `urn:uuid:` prefix, missing hyphens and
    /// shortened groups are `invalid_format`.
    pub fn uuid(self) -> UuidDecoder {
        UuidDecoder {
            string: self,
            message: None,
        }
    }

    /// A decoder that reads the string as a [`Uri`], as RFC 3986's `URI` production derives one:
    /// `invalid_format` when it is not one, a relative reference included.
    pub fn uri(self) -> UriDecoder {
        UriDecoder {
            string: self,
            web: false,
            message: None,
        }
    }

    /// A decoder that reads the string as a [`Uri`] that is an `http` or `https` URL, as RFC 9110
    /// requires one: its scheme is `http` or `https` in any case, and it has an authority with a
    /// host that is not empty. Anything else is `invalid_format`. The host is RFC 3986's, not a
    /// DNS name, and the port is not checked against a range.
    pub fn url(self) -> UriDecoder {
        UriDecoder {
            string: self,
            web: true,
            message: None,
        }
    }

    /// A decoder that reads the string as an `i32`: an optional sign and ASCII digits, leading
    /// zeros allowed. See [`IntDecoder`].
    pub fn to_int(self) -> IntDecoder<i32> {
        IntDecoder::from_text(self)
    }

    /// A decoder that reads the string as an `i64`, as [`to_int`](Self::to_int) reads an `i32`.
    pub fn to_long(self) -> IntDecoder<i64> {
        IntDecoder::from_text(self)
    }

    /// A decoder that reads the string as a decimal, keeping the scale it is written with. See
    /// [`DecimalDecoder`].
    pub fn to_decimal(self) -> DecimalDecoder {
        DecimalDecoder::from_text(self)
    }

    /// A decoder that reads the string as a boolean: `true`, `1`, `yes` or `on`, or `false`, `0`,
    /// `no` or `off`, in any ASCII case. See [`BoolDecoder`].
    pub fn to_bool(self) -> BoolDecoder {
        BoolDecoder::from_text(self)
    }

    /// A decoder that reads the string as an [`Instant`]: a date, an upper-case `T`, `hh:mm:ss`
    /// with an optional fraction of one to nine digits, and an offset, `Z` or `±hh:mm[:ss]` of at
    /// most 18 hours, which is applied. `24:00:00` is the start of the next day; a second 60 is
    /// refused. Anything else is `invalid_format`.
    pub fn instant(self) -> TemporalDecoder<Instant> {
        TemporalDecoder::new(self)
    }

    /// A decoder that reads the string as a [`Date`], `yyyy-mm-dd`, of a day that exists. A year
    /// is four digits from 0000 to 9999, or a sign and five or more digits, or `-` and four
    /// digits other than 0000. Anything else is `invalid_format`.
    pub fn date(self) -> TemporalDecoder<Date> {
        TemporalDecoder::new(self)
    }

    /// A decoder that reads the string as a [`Time`]: `hh:mm`, `hh:mm:ss`, or `hh:mm:ss` and a
    /// fraction of one to nine digits, from 00:00 to 23:59:59. Anything else is `invalid_format`.
    pub fn time(self) -> TemporalDecoder<Time> {
        TemporalDecoder::new(self)
    }

    /// A decoder that reads the string as a [`DateTime`]: a date, an upper-case `T`, and a time.
    /// Anything else is `invalid_format`.
    pub fn date_time(self) -> TemporalDecoder<DateTime> {
        TemporalDecoder::new(self)
    }

    /// A decoder that reads the string as an [`OffsetDateTime`]: a date-time followed by `Z` or
    /// `±hh:mm[:ss]` of at most 18 hours, which is kept. `+00:00` and `-00:00` are `Z`. Anything
    /// else is `invalid_format`.
    pub fn offset_date_time(self) -> TemporalDecoder<OffsetDateTime> {
        TemporalDecoder::new(self)
    }
}

/// The matchers of one pattern, kept for the threads that decode with it.
///
/// A matcher keeps what its matches work out for the next, so a value is matched in lookups rather
/// than worked out afresh. It is not shared during a match, so there is a slot for a matcher for
/// each thread the machine runs at once, and the pattern is shared by them all. A thread tries the
/// slot its number falls on first, and so finds the matcher it used before, then the others; a
/// slot is locked only while its matcher matches, and a thread that finds every slot in use
/// matches with a matcher of its own that is not kept. Each slot sits apart from the others in
/// memory, so that threads at different slots do not slow each other down. What is kept stays
/// bounded however many threads there are: a matcher keeps about two megabytes at most.
struct Matchers {
    pattern: Arc<Pattern>,
    slots: Box<[Slot]>,
}

/// One matcher's place, aligned to a line of the processor's cache of its own.
#[repr(align(128))]
struct Slot(Mutex<Option<OwnedMatcher<Arc<Pattern>>>>);

/// The number each thread is given the first time it matches, which picks the slot it tries first.
static THREADS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static THREAD: usize = THREADS.fetch_add(1, Ordering::Relaxed);
}

impl Matchers {
    fn new(pattern: Pattern) -> Self {
        let slots = std::thread::available_parallelism().map_or(1, usize::from);
        Self {
            pattern: Arc::new(pattern),
            slots: (0..slots).map(|_| Slot(Mutex::new(None))).collect(),
        }
    }

    fn matches(&self, subject: &str) -> bool {
        let first = THREAD.with(|n| *n) % self.slots.len();
        for i in 0..self.slots.len() {
            let slot = &self.slots[(first + i) % self.slots.len()].0;
            let mut held = match slot.try_lock() {
                Ok(held) => held,
                // A panic inside a match leaves nothing kept wrong, since what a matcher keeps
                // changes how fast a match is and never what it answers.
                Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => continue,
            };
            return held
                .get_or_insert_with(|| OwnedMatcher::new(Arc::clone(&self.pattern)))
                .matches(subject);
        }
        OwnedMatcher::new(&*self.pattern).matches(subject)
    }
}

/// The decoder [`StringDecoder::parse`] returns.
pub struct Parse<T> {
    string: StringDecoder,
    message: Option<String>,
    target: PhantomData<fn() -> T>,
}

impl<T> std::fmt::Debug for Parse<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parse")
            .field("string", &self.string)
            .field("target", &std::any::type_name::<T>())
            .finish()
    }
}

impl<T> Clone for Parse<T> {
    fn clone(&self) -> Self {
        Self {
            string: self.string.clone(),
            message: self.message.clone(),
            target: PhantomData,
        }
    }
}

impl<T> Parse<T> {
    /// Gives the issue a string that does not parse is reported with a custom message.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

fn conversion_failed(path: &Path<'_>, key: &'static str, custom: &Option<String>) -> Issues {
    let issue = Issue::at_path(path, codes::INVALID_FORMAT).with_message_key(key);
    match custom {
        Some(custom) => issue.with_message(custom.clone()).into(),
        None => issue.into(),
    }
}

impl<T: FromStr> Decoder<Json> for Parse<T> {
    type Output = T;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<T, Issues> {
        let s = self.string.decode_at(input, path)?;
        s.parse()
            .map_err(|_| conversion_failed(path, codes::INVALID_FORMAT, &self.message))
    }
}

/// The decoder [`StringDecoder::uuid`] returns.
#[derive(Clone, Debug)]
pub struct UuidDecoder {
    string: StringDecoder,
    message: Option<String>,
}

impl UuidDecoder {
    /// Gives the issue a string that is not a UUID is reported with a custom message.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

impl Decoder<Json> for UuidDecoder {
    type Output = Uuid;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<Uuid, Issues> {
        let s = self.string.decode_at(input, path)?;
        s.parse()
            .map_err(|_| conversion_failed(path, message_keys::INVALID_FORMAT_UUID, &self.message))
    }
}

/// The decoder [`StringDecoder::uri`] and [`StringDecoder::url`] return.
#[derive(Clone, Debug)]
pub struct UriDecoder {
    string: StringDecoder,
    web: bool,
    message: Option<String>,
}

impl UriDecoder {
    /// Gives the issue a string that is not a URI, or not a URL, is reported with a custom
    /// message.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

impl Decoder<Json> for UriDecoder {
    type Output = Uri;

    fn decode_at(&self, input: &Json, path: &Path<'_>) -> Result<Uri, Issues> {
        let s = self.string.decode_at(input, path)?;
        let key = if self.web {
            message_keys::INVALID_FORMAT_URL
        } else {
            message_keys::INVALID_FORMAT_URI
        };
        s.parse::<Uri>()
            .ok()
            .filter(|uri| !self.web || uri.is_web_url())
            .ok_or_else(|| conversion_failed(path, key, &self.message))
    }
}

/// Unicode's default lowercase mapping. For ASCII text that is ASCII's, which is written without
/// looking the characters up in Unicode's tables: no ASCII character maps to anything else, and
/// the one context the mapping has, a final sigma, is not ASCII.
fn lowercase(s: &str) -> String {
    if s.is_ascii() {
        s.to_ascii_lowercase()
    } else {
        notation199x::lowercase(s)
    }
}

/// Unicode's default uppercase mapping, ASCII's for ASCII text, as [`lowercase`] is.
fn uppercase(s: &str) -> String {
    if s.is_ascii() {
        s.to_ascii_uppercase()
    } else {
        notation199x::uppercase(s)
    }
}

/// `atext` of RFC 5322: the ASCII letters and digits and ``!#$%&'*+-/=?^_`{|}~``.
fn is_atext(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
}

/// A label of a domain: a letter or digit, optionally followed by letters, digits and hyphens
/// ending in a letter or digit, at most 63 octets.
fn is_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    (1..=63).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
}

/// The ASCII profile of RFC 5321's `Mailbox` that `email()` accepts.
fn is_email(s: &str) -> bool {
    if s.len() > 254 || !s.is_ascii() {
        return false;
    }
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    let local_ok = (1..=64).contains(&local.len())
        && local
            .split('.')
            .all(|atom| !atom.is_empty() && atom.bytes().all(is_atext));
    let domain_ok = domain.len() <= 255 && domain.split('.').all(is_label);
    local_ok && domain_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetaValue;
    use serde_json::Value;
    use serde_json::json;

    fn first<T: std::fmt::Debug>(result: Result<T, Issues>) -> Issue {
        result.unwrap_err().into_iter().next().unwrap()
    }

    #[test]
    fn missing_and_null_are_required_and_other_types_mismatch() {
        assert_eq!(first(string().decode(&Value::Null)).code(), "required");
        let issue = first(string().decode(&json!(1)));
        assert_eq!(issue.code(), "type_mismatch");
        assert_eq!(issue.meta()["actual"], MetaValue::from("number"));
    }

    #[test]
    fn the_first_failing_constraint_is_reported() {
        let issues = string()
            .min_length(3)
            .email()
            .decode(&json!("a"))
            .unwrap_err();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues.iter().next().unwrap().code(), "too_short");
    }

    #[test]
    fn length_counts_scalar_values() {
        assert!(string().max_length(2).decode(&json!("日本")).is_ok());
        assert!(string().length(1).decode(&json!("😀")).is_ok());
    }

    #[test]
    fn trim_and_non_blank_share_unicode_white_space() {
        let trim = |s: &str| string().trim().decode(&json!(s)).unwrap();
        assert_eq!(trim("\u{3000}a\u{a0}"), "a");
        assert_eq!(trim("\u{0}a\u{1f}"), "\u{0}a\u{1f}");
        assert_eq!(trim("\u{85}a\u{2028}"), "a");
        assert_eq!(trim("\u{feff}a\u{180e}"), "\u{feff}a\u{180e}");
        for blank in ["", "\u{a0}", "\u{3000}", "\u{2007}"] {
            assert_eq!(
                first(string().non_blank().decode(&json!(blank))).code(),
                "blank"
            );
        }
        for not_blank in ["\u{1c}", "\u{0}", "\u{200b}"] {
            assert!(string().non_blank().decode(&json!(not_blank)).is_ok());
        }
    }

    #[test]
    fn ascii_text_is_mapped_as_unicode_maps_it() {
        let ascii: String = (0u8..128).map(char::from).collect();
        assert_eq!(lowercase(&ascii), notation199x::lowercase(&ascii));
        assert_eq!(uppercase(&ascii), notation199x::uppercase(&ascii));
        for form in [Form::Nfc, Form::Nfd, Form::Nfkc, Form::Nfkd] {
            assert_eq!(notation199x::normalize(form, &ascii), ascii);
        }
    }

    #[test]
    fn case_follows_unicode_without_tailoring() {
        let lower = |s: &str| string().lowercase().decode(&json!(s)).unwrap();
        let upper = |s: &str| string().uppercase().decode(&json!(s)).unwrap();
        assert_eq!(lower("ΟΔΟΣ"), "οδος");
        assert_eq!(lower("İ"), "i\u{307}");
        assert_eq!(upper("ß"), "SS");
    }

    #[test]
    fn email_follows_the_ascii_profile() {
        for ok in [
            "a@b.co",
            "first.last+tag@sub.example.com",
            "a@localhost",
            "a@123",
            "!#$%&'*+-/=?^_`{|}~@x",
        ] {
            assert!(is_email(ok), "{ok}");
        }
        for bad in [
            "@b.co",
            "a@@b.co",
            "a b@c.co",
            ".a@b",
            "a.@b",
            "a..b@c",
            "a@b..c",
            "a@-b",
            "a@b-",
            "a@",
            "\"a\"@b",
            "a@[127.0.0.1]",
            "ä@b",
        ] {
            assert!(!is_email(bad), "{bad}");
        }
        assert!(!is_email(&format!("{}@b", "a".repeat(65))));
        assert!(!is_email(&format!("a@{}", "b".repeat(64))));
    }

    #[test]
    fn ulid_takes_either_case_up_to_128_bits() {
        let ulid = |s: &str| string().ulid().decode(&json!(s)).is_ok();
        assert!(ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"));
        assert!(ulid("01arz3ndektsv4rrffq69g5fav"));
        assert!(ulid("7ZZZZZZZZZZZZZZZZZZZZZZZZZ"));
        assert!(!ulid("8ZZZZZZZZZZZZZZZZZZZZZZZZZ"));
        assert!(!ulid("01ARZ3NDEKTSV4RRFFQ69G5FAI"));
    }

    #[test]
    fn one_of_sorts_by_code_point() {
        let issue = first(
            string()
                .one_of(["\u{1f600}", "\u{ff21}"])
                .decode(&json!("z")),
        );
        assert_eq!(
            issue.meta()["allowed"],
            MetaValue::from(vec!["\u{ff21}", "\u{1f600}"])
        );
        assert_eq!(issue.message(), "must be one of [\u{ff21}, \u{1f600}]");
    }

    #[test]
    fn a_pattern_matches_the_whole_string() {
        let issue = first(string().pattern("[a-z]+").decode(&json!("ab1")));
        assert_eq!(issue.meta()["pattern"], MetaValue::from("[a-z]+"));
        assert!(string().pattern(r"\d{3}").decode(&json!("123")).is_ok());
        assert!(string().pattern(r"\d{3}").decode(&json!("١٢٣")).is_err());
    }

    #[test]
    fn threads_matching_at_once_get_the_same_answers() {
        let decoder = std::sync::Arc::new(string().pattern("[a-z]+[0-9]+"));
        // More threads than slots, so that some match with a matcher of their own.
        let threads = 4 * std::thread::available_parallelism().map_or(1, usize::from);
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let decoder = std::sync::Arc::clone(&decoder);
                std::thread::spawn(move || {
                    for i in 0..200 {
                        let good = format!("ab{t}{i}");
                        let bad = format!("{t}{i}ab");
                        assert!(decoder.decode(&json!(good)).is_ok(), "{good}");
                        assert!(decoder.decode(&json!(bad)).is_err(), "{bad}");
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    #[should_panic(expected = "is not a pattern")]
    fn a_back_reference_is_not_a_pattern() {
        let _ = string().pattern(r"(a)\1");
    }

    #[test]
    fn format_issues_name_the_check_in_their_key() {
        let issue = first(string().email().decode(&json!("x")));
        assert_eq!(issue.code(), "invalid_format");
        assert_eq!(issue.message_key(), "invalid_format.email");
        assert_eq!(issue.message(), "not a valid email");
    }

    #[test]
    fn a_message_goes_to_the_latest_constraint_past_transformations() {
        let decoder = string().min_length(3).trim().message("three or more");
        assert_eq!(
            first(decoder.decode(&json!("ab"))).message(),
            "three or more"
        );
        let decoder = string().trim().message("give a name");
        assert_eq!(first(decoder.decode(&Value::Null)).message(), "give a name");
    }

    #[test]
    fn conversions_give_their_message_to_either_issue() {
        let decoder = string().max_length(3).to_int().message("bad");
        assert_eq!(first(decoder.decode(&json!("abc"))).message(), "bad");
        assert_eq!(
            first(decoder.decode(&json!("1234"))).message(),
            "must be at most 3 characters"
        );
        let issue = first(string().to_int().decode(&json!("99999999999")));
        assert_eq!(issue.message_key(), "type_mismatch.numeric_range");
        let issue = first(string().to_int().decode(&json!("1.0")));
        assert!(!issue.meta().contains_key("actual"));
    }
}
