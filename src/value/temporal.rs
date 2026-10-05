//! Dates, times of day, date-times, date-times with an offset, and instants.
//!
//! Which text is one of these is decided by the grammar notation-199x shares between Raoh and
//! Souther, and the value is built from the fields its reading of the text gives, so the text is
//! read once and by that grammar alone. Years run from -999999999 to 999999999, beyond what the
//! usual date crates hold, so the types are this crate's own.

use notation199x::{
    TemporalDate, TemporalTime, read_date, read_date_time, read_instant, read_offset_date_time,
    read_time,
};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

const SECONDS_PER_DAY: i64 = 86_400;
const NANOS_PER_SECOND: u32 = 1_000_000_000;
/// The furthest an offset reaches from UTC, eighteen hours.
const MAX_OFFSET_SECONDS: i32 = 18 * 3600;

/// Text a temporal type's [`FromStr`] does not read as one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseTemporalError(&'static str);

impl fmt::Display for ParseTemporalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not an ISO 8601 {}", self.0)
    }
}

impl std::error::Error for ParseTemporalError {}

/// A day of the proleptic Gregorian calendar, from year -999999999 to 999999999.
///
/// ```
/// use raoh::Date;
///
/// let date: Date = "2024-02-29".parse().unwrap();
/// assert_eq!((date.year(), date.month(), date.day()), (2024, 2, 29));
/// assert!("2023-02-29".parse::<Date>().is_err());
/// assert_eq!("+10000-01-01".parse::<Date>().unwrap().to_string(), "+10000-01-01");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Date {
    year: i32,
    month: u8,
    day: u8,
}

/// A time of day to the nanosecond.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Time {
    hour: u8,
    minute: u8,
    second: u8,
    nano: u32,
}

/// A date and a time of day, with no offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DateTime {
    date: Date,
    time: Time,
}

/// A date-time and an offset from UTC.
///
/// Two of them at different offsets are different values even when they name the same instant,
/// so `==` compares all three parts. Chronology is another relation:
/// [`chronological_cmp`](Self::chronological_cmp) compares the instants alone, as `before`,
/// `after` and `between` do, and there `09:00Z` and `10:00+01:00` are neither before the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OffsetDateTime {
    date_time: DateTime,
    offset_seconds: i32,
}

/// A point on the UTC time-line to the nanosecond, counted from 1970-01-01T00:00:00Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Instant {
    seconds: i64,
    nano: u32,
}

impl Date {
    /// The date, if it exists and its year is from -999999999 to 999999999.
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        let in_range = (-999_999_999..=999_999_999).contains(&year)
            && (1..=12).contains(&month)
            && day >= 1
            && day <= days_in_month(i64::from(year), month);
        in_range.then_some(Self { year, month, day })
    }

    /// The year.
    pub fn year(&self) -> i32 {
        self.year
    }

    /// The month, from 1 to 12.
    pub fn month(&self) -> u8 {
        self.month
    }

    /// The day of the month, from 1.
    pub fn day(&self) -> u8 {
        self.day
    }

    /// Days from 1970-01-01.
    fn days_from_epoch(&self) -> i64 {
        days_from_civil(i64::from(self.year), self.month, self.day)
    }
}

impl Time {
    /// The time, if its parts are within a day: hours to 23, minutes and seconds to 59 and
    /// nanoseconds below a second.
    pub fn new(hour: u8, minute: u8, second: u8, nano: u32) -> Option<Self> {
        (hour < 24 && minute < 60 && second < 60 && nano < NANOS_PER_SECOND).then_some(Self {
            hour,
            minute,
            second,
            nano,
        })
    }

    /// The hour, from 0 to 23.
    pub fn hour(&self) -> u8 {
        self.hour
    }

    /// The minute, from 0 to 59.
    pub fn minute(&self) -> u8 {
        self.minute
    }

    /// The second, from 0 to 59.
    pub fn second(&self) -> u8 {
        self.second
    }

    /// The nanoseconds within the second.
    pub fn nanosecond(&self) -> u32 {
        self.nano
    }

    fn seconds_of_day(&self) -> i64 {
        i64::from(self.hour) * 3600 + i64::from(self.minute) * 60 + i64::from(self.second)
    }
}

impl DateTime {
    /// The date at the time of day.
    pub fn new(date: Date, time: Time) -> Self {
        Self { date, time }
    }

    /// The date.
    pub fn date(&self) -> Date {
        self.date
    }

    /// The time of day.
    pub fn time(&self) -> Time {
        self.time
    }

    fn seconds_from_epoch(&self) -> i64 {
        self.date.days_from_epoch() * SECONDS_PER_DAY + self.time.seconds_of_day()
    }
}

impl OffsetDateTime {
    /// The date-time at `offset_seconds` east of UTC, if the offset is at most 18 hours.
    pub fn new(date_time: DateTime, offset_seconds: i32) -> Option<Self> {
        (offset_seconds.abs() <= MAX_OFFSET_SECONDS).then_some(Self {
            date_time,
            offset_seconds,
        })
    }

    /// The local date-time.
    pub fn date_time(&self) -> DateTime {
        self.date_time
    }

    /// The offset from UTC in seconds, east positive.
    pub fn offset_seconds(&self) -> i32 {
        self.offset_seconds
    }

    /// Compares the instants the two name, whatever their offsets.
    pub fn chronological_cmp(&self, other: &Self) -> Ordering {
        self.moment().cmp(&other.moment())
    }

    fn moment(&self) -> (i64, u32) {
        (
            self.date_time.seconds_from_epoch() - i64::from(self.offset_seconds),
            self.date_time.time.nano,
        )
    }
}

impl Instant {
    /// The instant `seconds` and `nano` nanoseconds after 1970-01-01T00:00:00Z, if it is within
    /// the years -1000000000 to 1000000000 and `nano` is below a second.
    pub fn from_epoch(seconds: i64, nano: u32) -> Option<Self> {
        let in_range = (notation199x::INSTANT_MIN..=notation199x::INSTANT_MAX).contains(&seconds)
            && nano < NANOS_PER_SECOND;
        in_range.then_some(Self { seconds, nano })
    }

    /// Seconds from 1970-01-01T00:00:00Z.
    pub fn epoch_seconds(&self) -> i64 {
        self.seconds
    }

    /// The nanoseconds within the second.
    pub fn nanosecond(&self) -> u32 {
        self.nano
    }
}

/// A temporal type, compared chronologically by `before`, `after` and `between`. It cannot be
/// implemented outside this crate.
pub trait Chronological:
    sealed::Sealed + Copy + Into<crate::MetaValue> + Send + Sync + 'static
{
    /// Whether `self` is before, the same moment as, or after `other`.
    fn chronological_cmp(&self, other: &Self) -> Ordering;

    /// The value the text names, if it is one of this type.
    #[doc(hidden)]
    fn read(text: &str) -> Option<Self>;

    /// The message key of text that is not one.
    #[doc(hidden)]
    const KEY: &'static str;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::Date {}
    impl Sealed for super::Time {}
    impl Sealed for super::DateTime {}
    impl Sealed for super::OffsetDateTime {}
    impl Sealed for super::Instant {}
}

macro_rules! chronological_by_ord {
    ($t:ty, $key:expr, $read:expr) => {
        impl Chronological for $t {
            fn chronological_cmp(&self, other: &Self) -> Ordering {
                self.cmp(other)
            }

            fn read(text: &str) -> Option<Self> {
                $read(text).ok()
            }

            const KEY: &'static str = $key;
        }
    };
}

chronological_by_ord!(Date, "invalid_format.date", |text| read_date(text)
    .map(date));
chronological_by_ord!(Time, "invalid_format.time", |text| read_time(text)
    .map(time));
chronological_by_ord!(DateTime, "invalid_format.date_time", |text| read_date_time(
    text
)
.map(|read| DateTime {
    date: date(read.date),
    time: time(read.time),
}));
chronological_by_ord!(Instant, "invalid_format.instant", |text| read_instant(text)
    .map(|read| Instant {
        seconds: read.epoch_second,
        nano: read.nanosecond,
    }));

impl Chronological for OffsetDateTime {
    fn chronological_cmp(&self, other: &Self) -> Ordering {
        OffsetDateTime::chronological_cmp(self, other)
    }

    fn read(text: &str) -> Option<Self> {
        let read = read_offset_date_time(text).ok()?;
        Some(OffsetDateTime {
            date_time: DateTime {
                date: date(read.date_time.date),
                time: time(read.date_time.time),
            },
            offset_seconds: read.offset_seconds,
        })
    }

    const KEY: &'static str = "invalid_format.offset_date_time";
}

// The fields notation-199x gives are of text it admitted, within the ranges the types hold.

fn date(read: TemporalDate) -> Date {
    Date {
        year: read.year,
        month: read.month,
        day: read.day,
    }
}

/// A fraction written as `.000` and none are the same time of day here.
fn time(read: TemporalTime) -> Time {
    Time {
        hour: read.hour,
        minute: read.minute,
        second: read.second,
        nano: read.nanosecond.unwrap_or(0),
    }
}

macro_rules! from_str {
    ($t:ty, $what:expr) => {
        /// Reads the text form the decoder of this type reads.
        impl FromStr for $t {
            type Err = ParseTemporalError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                <$t as Chronological>::read(s).ok_or(ParseTemporalError($what))
            }
        }
    };
}

from_str!(Date, "date");
from_str!(Time, "local time");
from_str!(DateTime, "local date-time");
from_str!(OffsetDateTime, "offset date-time");
from_str!(Instant, "instant");

fn is_leap(year: i64) -> bool {
    year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0)
}

fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days from 1970-01-01 to the date, by Howard Hinnant's `days_from_civil`.
fn days_from_civil(year: i64, month: u8, day: u8) -> i64 {
    let (month, day) = (i64::from(month), i64::from(day));
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The date `days` after 1970-01-01, by Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn write_year(f: &mut fmt::Formatter<'_>, year: i64) -> fmt::Result {
    if (0..=9999).contains(&year) {
        write!(f, "{year:04}")
    } else if year < 0 {
        write!(f, "-{:04}", year.unsigned_abs())
    } else {
        write!(f, "+{year}")
    }
}

/// The fraction of a second in three, six or nine digits, with its point, or nothing when it is
/// zero.
fn write_fraction(f: &mut fmt::Formatter<'_>, nano: u32) -> fmt::Result {
    if nano == 0 {
        Ok(())
    } else if nano.is_multiple_of(1_000_000) {
        write!(f, ".{:03}", nano / 1_000_000)
    } else if nano.is_multiple_of(1_000) {
        write!(f, ".{:06}", nano / 1_000)
    } else {
        write!(f, ".{nano:09}")
    }
}

/// `yyyy-mm-dd`, with a year from 0000 to 9999 in four digits, a negative year as `-` and at least
/// four digits, and a later one as `+` and its digits.
impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_year(f, i64::from(self.year))?;
        write!(f, "-{:02}-{:02}", self.month, self.day)
    }
}

/// `hh:mm`, followed by `:ss` when the seconds or the fraction are not zero, followed by the
/// fraction in three, six or nine digits when it is not zero.
impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)?;
        if self.second != 0 || self.nano != 0 {
            write!(f, ":{:02}", self.second)?;
        }
        write_fraction(f, self.nano)
    }
}

/// The date, `T`, the time.
impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}T{}", self.date, self.time)
    }
}

/// The date-time, then `Z` for a zero offset and otherwise `±hh:mm`, followed by `:ss` when the
/// offset's seconds are not zero.
impl fmt::Display for OffsetDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.date_time)?;
        if self.offset_seconds == 0 {
            return f.write_str("Z");
        }
        let sign = if self.offset_seconds < 0 { '-' } else { '+' };
        let total = self.offset_seconds.unsigned_abs();
        write!(f, "{sign}{:02}:{:02}", total / 3600, total / 60 % 60)?;
        if !total.is_multiple_of(60) {
            write!(f, ":{:02}", total % 60)?;
        }
        Ok(())
    }
}

/// The date and time in UTC, the seconds always written, and `Z`.
impl fmt::Display for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let days = self.seconds.div_euclid(SECONDS_PER_DAY);
        let of_day = self.seconds.rem_euclid(SECONDS_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        write_year(f, year)?;
        write!(
            f,
            "-{month:02}-{day:02}T{:02}:{:02}:{:02}",
            of_day / 3600,
            of_day / 60 % 60,
            of_day % 60
        )?;
        write_fraction(f, self.nano)?;
        f.write_str("Z")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written<T: Chronological + fmt::Display>(text: &str) -> String {
        T::read(text)
            .unwrap_or_else(|| panic!("{text} is not read"))
            .to_string()
    }

    #[test]
    fn instants_are_written_in_utc() {
        for (text, expected) in [
            ("2024-01-15T10:30:00.5+09:00", "2024-01-15T01:30:00.500Z"),
            ("2024-01-15T24:00:00Z", "2024-01-16T00:00:00Z"),
            ("2024-01-15T10:30:00+05:30:15", "2024-01-15T04:59:45Z"),
            ("+10000-01-01T00:00:00Z", "+10000-01-01T00:00:00Z"),
            ("-0001-01-01T00:00:00Z", "-0001-01-01T00:00:00Z"),
            (
                "+1000000000-12-31T23:59:59.999999999Z",
                "+1000000000-12-31T23:59:59.999999999Z",
            ),
            ("-1000000000-01-01T00:00:00Z", "-1000000000-01-01T00:00:00Z"),
        ] {
            assert_eq!(written::<Instant>(text), expected);
        }
    }

    #[test]
    fn times_write_seconds_and_fractions_only_when_there_are_some() {
        for (text, expected) in [
            ("10:30:00", "10:30"),
            ("10:30:45.1", "10:30:45.100"),
            ("10:30:45.1234", "10:30:45.123400"),
            ("00:00:00.000000001", "00:00:00.000000001"),
        ] {
            assert_eq!(written::<Time>(text), expected);
        }
    }

    #[test]
    fn offsets_are_kept_and_compared_by_instant() {
        assert_eq!(
            written::<OffsetDateTime>("2024-01-15T10:30-00:00"),
            "2024-01-15T10:30Z"
        );
        assert_eq!(
            written::<OffsetDateTime>("2024-01-15T10:30:00+05:30:15"),
            "2024-01-15T10:30+05:30:15"
        );
        let a: OffsetDateTime = "2024-01-01T09:00Z".parse().unwrap();
        let b: OffsetDateTime = "2024-01-01T10:00+01:00".parse().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.chronological_cmp(&b), Ordering::Equal);
    }

    #[test]
    fn days_and_dates_go_both_ways() {
        for days in [-719_468, -1, 0, 1, 19_000, 2_932_896, -365_243_219_162] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }
}
