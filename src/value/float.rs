//! Floats as the value model holds them: one NaN, two zeros, and a total order.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// `f32` or `f64`. It cannot be implemented outside this crate.
pub trait Float:
    sealed::Sealed
    + Copy
    + PartialEq
    + fmt::Display
    + fmt::LowerExp
    + FromStr
    + Into<crate::MetaValue>
    + Send
    + Sync
    + 'static
{
    /// What an issue names the type in `expected`.
    const EXPECTED: &'static str;
    /// +0.
    const ZERO: Self;

    /// Whether this is NaN.
    #[doc(hidden)]
    fn nan(self) -> bool;

    /// Whether this is +∞ or -∞.
    #[doc(hidden)]
    fn infinite(self) -> bool;

    /// Whether the sign bit is set.
    #[doc(hidden)]
    fn negative_sign(self) -> bool;

    /// Compares as [`f64::total_cmp`] does.
    #[doc(hidden)]
    fn total(self, other: Self) -> Ordering;

    /// The bits of the value, with every NaN given the same bits.
    #[doc(hidden)]
    fn canonical_bits(self) -> u64;

    /// The JSON number rounded to this type once, or `None` when it cannot be read.
    #[doc(hidden)]
    fn from_number(n: &crate::json::Number<'_>) -> Option<Self>;
}

impl Float for f32 {
    const EXPECTED: &'static str = "float";
    const ZERO: Self = 0.0;

    fn nan(self) -> bool {
        self.is_nan()
    }

    fn infinite(self) -> bool {
        self.is_infinite()
    }

    fn negative_sign(self) -> bool {
        self.is_sign_negative()
    }

    fn total(self, other: Self) -> Ordering {
        self.total_cmp(&other)
    }

    fn canonical_bits(self) -> u64 {
        if self.is_nan() {
            u64::from(f32::NAN.to_bits())
        } else {
            u64::from(self.to_bits())
        }
    }

    /// From the lexeme, so that it is rounded once, to binary32, and not first to binary64.
    fn from_number(n: &crate::json::Number<'_>) -> Option<Self> {
        read(&n.lexeme())
    }
}

impl Float for f64 {
    const EXPECTED: &'static str = "double";
    const ZERO: Self = 0.0;

    fn nan(self) -> bool {
        self.is_nan()
    }

    fn infinite(self) -> bool {
        self.is_infinite()
    }

    fn negative_sign(self) -> bool {
        self.is_sign_negative()
    }

    fn total(self, other: Self) -> Ordering {
        self.total_cmp(&other)
    }

    fn canonical_bits(self) -> u64 {
        if self.is_nan() {
            f64::NAN.to_bits()
        } else {
            self.to_bits()
        }
    }

    fn from_number(n: &crate::json::Number<'_>) -> Option<Self> {
        n.f64()
    }
}

/// A float that a JSON number's digits and power of ten can be exact in.
pub(crate) trait Exact:
    'static
    + FromStr
    + Copy
    + std::ops::Mul<Output = Self>
    + std::ops::Div<Output = Self>
    + std::ops::Neg<Output = Self>
{
    /// The largest integer every one up to which is exact.
    const MANTISSA: u64;
    /// The powers of ten that are exact, from 10^0.
    const POWERS: &'static [Self];

    fn from_u64(m: u64) -> Self;
}

impl Exact for f32 {
    const MANTISSA: u64 = 1 << 24;
    const POWERS: &'static [Self] = &[1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10];

    fn from_u64(m: u64) -> Self {
        m as f32
    }
}

impl Exact for f64 {
    const MANTISSA: u64 = 1 << 53;
    const POWERS: &'static [Self] = &[
        1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
        1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
    ];

    fn from_u64(m: u64) -> Self {
        m as f64
    }
}

/// The JSON number written `lexeme` rounded once to `F`, or `None` when it cannot be read.
///
/// When the digits, read as one integer, and the power of ten they are scaled by are both exact
/// in `F`, one multiplication or division rounds the value once, correctly (W. D. Clinger, "How
/// to read floating point numbers accurately", 1990); [`str::parse`] reads the rest.
pub(crate) fn read<F: Exact>(lexeme: &str) -> Option<F> {
    exact(lexeme).or_else(|| lexeme.parse().ok())
}

fn exact<F: Exact>(lexeme: &str) -> Option<F> {
    let bytes = lexeme.as_bytes();
    let (negative, mut i) = match bytes.first() {
        Some(b'-') => (true, 1),
        _ => (false, 0),
    };
    let mut mantissa: u64 = 0;
    let mut digits = 0;
    let mut scale: i32 = 0;
    let mut fraction = false;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'0'..=b'9' => {
                digits += 1;
                if digits > 19 {
                    return None;
                }
                mantissa = mantissa * 10 + u64::from(b - b'0');
                scale -= i32::from(fraction);
            }
            b'.' => fraction = true,
            _ => break,
        }
        i += 1;
    }
    if let Some(b'e' | b'E') = bytes.get(i) {
        let rest = &lexeme[i + 1..];
        let exponent: i32 = rest.strip_prefix('+').unwrap_or(rest).parse().ok()?;
        scale = scale.checked_add(exponent)?;
    }
    if mantissa > F::MANTISSA {
        return None;
    }
    let power = *F::POWERS.get(scale.unsigned_abs() as usize)?;
    let magnitude = if scale < 0 {
        F::from_u64(mantissa) / power
    } else {
        F::from_u64(mantissa) * power
    };
    Some(if negative { -magnitude } else { magnitude })
}

/// The float order of the value model: -∞, the negative values, -0, +0, the positive values, +∞
/// and last NaN, every NaN equal to every other.
///
/// `min`, `max`, `range`, `positive`, `negative`, `non_negative`, `non_positive` and the sorting of
/// `one_of`'s values compare with it, so -0 is below +0 and every value is comparable.
pub fn float_order<F: Float>(a: F, b: F) -> Ordering {
    match (a.nan(), b.nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => a.total(b),
    }
}

/// Whether two floats are the same value: +0 and -0 differ, and every NaN is the same.
pub fn float_same<F: Float>(a: F, b: F) -> bool {
    a.canonical_bits() == b.canonical_bits()
}

/// The canonical decimal of a finite non-zero float, as the significant digits, with no
/// trailing zero, and the exponent of the first digit: 0.25 is `("25", -1)`.
///
/// It is the closest of the shortest decimals that round to the float, except that where one
/// digit would do, two are allowed and the closer taken: the least positive `f64` is 4.9E-324,
/// not 5E-324.
fn canonical_decimal<F: Float>(v: F) -> (String, i32) {
    // `{:e}` gives the least length; of two decimals of that length equally close to the float it
    // takes either, so the decimal is written again at that length with `{:.*e}`, which rounds
    // the float's exact value to nearest, ties to even, and so gives the closest.
    let (shortest, _) = split_exponential(&format!("{v:e}"));
    let length = shortest.len().max(2);
    let closest = format!("{v:.*e}", length - 1);
    if closest.parse::<F>().is_ok_and(|back| float_same(back, v)) {
        return split_exponential(&closest);
    }
    split_exponential(&format!("{v:e}"))
}

/// The digits and exponent of `d.ddde±n` as Rust's `{:e}` writes a positive float, with trailing
/// zeros removed.
fn split_exponential(text: &str) -> (String, i32) {
    let text = text.trim_start_matches('-');
    let (mantissa, exponent) = text.split_once('e').expect("`{:e}` writes an exponent");
    let exponent: i32 = exponent.parse().expect("`{:e}` writes an integer exponent");
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    (digits.to_owned(), exponent)
}

/// A float as a message writes it: its canonical decimal, plainly with at least one digit after
/// the point when the exponent of its first digit is from -3 to 6 (`0.5`, `100.0`, `0.001`), and
/// otherwise as a mantissa with at least one digit after the point, `E` and that exponent
/// (`1.0E7`, `1.0E-4`). Zeros are `0.0` and `-0.0`, and the others `NaN`, `Infinity` and
/// `-Infinity`.
pub(crate) fn float_message<F: Float>(v: F) -> String {
    if v.nan() {
        return "NaN".into();
    }
    let sign = if v.negative_sign() { "-" } else { "" };
    if v.infinite() {
        return format!("{sign}Infinity");
    }
    // IEEE 754 equality, under which -0 is +0.
    if v == F::ZERO {
        return format!("{sign}0.0");
    }
    let (digits, exponent) = canonical_decimal(v);
    let body = if (-3..=6).contains(&exponent) {
        let point = exponent + 1;
        if point <= 0 {
            format!("0.{}{digits}", "0".repeat(point.unsigned_abs() as usize))
        } else {
            let point = point as usize;
            if point >= digits.len() {
                format!("{digits}{}.0", "0".repeat(point - digits.len()))
            } else {
                format!("{}.{}", &digits[..point], &digits[point..])
            }
        }
    } else {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        format!("{first}.{rest}E{exponent}")
    };
    format!("{sign}{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `read` gives what `str::parse` gives, bit for bit, for lexemes in and out of the exact
    /// range: digits up to 20, a point anywhere, exponents up to ±40, both signs.
    #[test]
    fn reading_a_lexeme_rounds_as_parse_does() {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = |bound: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % bound
        };
        for _ in 0..200_000 {
            let digits = 1 + next(20) as usize;
            let mut text = String::new();
            if next(2) == 0 {
                text.push('-');
            }
            let mut body: String = (0..digits)
                .map(|_| char::from(b'0' + next(10) as u8))
                .collect();
            if body.len() > 1 && body.starts_with('0') {
                body.replace_range(..1, "1");
            }
            text.push_str(&body);
            if next(2) == 0 {
                text.push('.');
                text.extend((0..1 + next(8)).map(|_| char::from(b'0' + next(10) as u8)));
            }
            if next(2) == 0 {
                text.push_str(&format!("e{}", next(81) as i64 - 40));
            }
            let f64_parsed: f64 = text.parse().unwrap();
            let f32_parsed: f32 = text.parse().unwrap();
            assert_eq!(
                read::<f64>(&text).unwrap().to_bits(),
                f64_parsed.to_bits(),
                "{text}"
            );
            assert_eq!(
                read::<f32>(&text).unwrap().to_bits(),
                f32_parsed.to_bits(),
                "{text}"
            );
        }
        assert!(read::<f64>("-0").unwrap().is_sign_negative());
        assert!(read::<f32>("-0.0e5").unwrap().is_sign_negative());
        assert_eq!(read::<f64>("1e400"), Some(f64::INFINITY));
    }

    #[test]
    fn floats_are_written_as_the_specification_writes_them() {
        let cases: [(f64, &str); 16] = [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (100.0, "100.0"),
            (0.5, "0.5"),
            (0.001, "0.001"),
            (0.0001, "1.0E-4"),
            (1234567.0, "1234567.0"),
            (1e7, "1.0E7"),
            (1.25e7, "1.25E7"),
            (-1.5e-5, "-1.5E-5"),
            (f64::MAX, "1.7976931348623157E308"),
            (5e-324, "4.9E-324"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
        ];
        for (v, written) in cases {
            assert_eq!(float_message(v), written, "{v:e}");
        }
    }

    #[test]
    fn a_float32_is_written_at_its_own_width() {
        assert_eq!(float_message(0.1f32), "0.1");
        assert_eq!(float_message(1.4e-45f32), "1.4E-45");
        assert_eq!(float_message(16777216f32), "1.6777216E7");
        assert_eq!(float_message(1.0000001f32), "1.0000001");
    }

    #[test]
    fn of_two_equally_close_decimals_the_even_one_is_taken() {
        assert_eq!(
            float_message("1765629.25".parse::<f32>().unwrap()),
            "1765629.2"
        );
        assert_eq!(
            float_message("-1749220892028010.25".parse::<f64>().unwrap()),
            "-1.7492208920280102E15"
        );
    }

    #[test]
    fn the_order_puts_negative_zero_below_zero_and_nan_last() {
        let mut values = [
            f64::NAN,
            1.0,
            0.0,
            -0.0,
            f64::NEG_INFINITY,
            f64::INFINITY,
            -1.0,
        ];
        values.sort_by(|a, b| float_order(*a, *b));
        let written: Vec<String> = values.iter().map(|v| float_message(*v)).collect();
        assert_eq!(
            written,
            ["-Infinity", "-1.0", "-0.0", "0.0", "1.0", "Infinity", "NaN"]
        );
        assert!(float_same(f64::NAN, -f64::NAN));
        assert!(!float_same(0.0, -0.0));
    }

    proptest::proptest! {
        #[test]
        fn a_canonical_decimal_is_shortest_and_reads_back(bits in proptest::prelude::any::<u32>()) {
            let v = f32::from_bits(bits);
            proptest::prop_assume!(v.is_finite() && v != 0.0);
            let (digits, _) = canonical_decimal(v);
            let (shortest, _) = split_exponential(&format!("{v:e}"));
            proptest::prop_assert_eq!(float_message(v).replace('E', "e").parse::<f32>().unwrap(), v);
            proptest::prop_assert!(digits.len() <= shortest.len().max(2));
        }
    }

    /// The canonical decimal of `v`, which is positive, as spec/observation.md defines it, worked
    /// out with exact integers
    /// and no formatting of the platform's: of the decimals of the least length that read back as
    /// `v`, the closest, the even one of two equally close; and where one digit would do, the
    /// closest of one or two digits.
    fn reference<F: Float>(v: F, widened: f64) -> (String, i32) {
        use num_bigint::BigInt;
        use num_traits::One;
        let bits = widened.abs().to_bits();
        let (mantissa, exponent) = if (bits >> 52) == 0 {
            (bits & ((1 << 52) - 1), -1074)
        } else {
            (
                (bits & ((1 << 52) - 1)) | (1 << 52),
                ((bits >> 52) as i32) - 1075,
            )
        };
        // |v| = num / den exactly.
        let (mut num, mut den) = (BigInt::from(mantissa), BigInt::one());
        if exponent >= 0 {
            num <<= exponent as usize;
        } else {
            den <<= (-exponent) as usize;
        }
        // The exponent of the first digit: 10^e <= |v| < 10^(e+1).
        let mut e = (widened.abs().log10().floor()) as i32;
        let pow = |k: i32| num_traits::pow(BigInt::from(10), k.unsigned_abs() as usize);
        let below = |e: i32, num: &BigInt, den: &BigInt| {
            if e >= 0 {
                *num < pow(e) * den
            } else {
                num * pow(e) < *den
            }
        };
        while below(e, &num, &den) {
            e -= 1;
        }
        while !below(e + 1, &num, &den) {
            e += 1;
        }
        // |v| rounded to `p` significant digits, half to even, as a coefficient.
        let round = |p: i32| -> BigInt {
            let k = p - 1 - e;
            let (n, d) = if k >= 0 {
                (&num * pow(k), den.clone())
            } else {
                (num.clone(), &den * pow(k))
            };
            let (q, r) = (&n / &d, &n % &d);
            let twice = r * 2;
            if twice > d || (twice == d && (&q % 2u8) == BigInt::one()) {
                q + 1
            } else {
                q
            }
        };
        let reads_back = |c: &BigInt, p: i32| -> bool {
            format!("{c}e{}", e - (p - 1))
                .parse::<F>()
                .is_ok_and(|back| float_same(back, v))
        };
        let mut p = 1;
        let mut c = round(1);
        while !reads_back(&c, p) {
            p += 1;
            c = round(p);
        }
        if p == 1 {
            let two = round(2);
            if reads_back(&two, 2) {
                c = two;
                p = 2;
            }
        }
        let text = c.to_string();
        // A coefficient that rounded up to a power of ten has one more digit.
        let exponent = e + (text.len() as i32 - p);
        let digits = text.trim_end_matches('0');
        let digits = if digits.is_empty() { "0" } else { digits };
        (digits.to_owned(), exponent)
    }

    /// Floats spread over every exponent, from a fixed sequence so that a failure repeats.
    fn sample(n: usize) -> impl Iterator<Item = u64> {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        (0..n).map(move |_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        })
    }

    #[test]
    fn the_canonical_decimal_is_the_one_the_specification_defines() {
        let mut ties = vec!["1765629.25".parse::<f32>().unwrap()];
        ties.extend(sample(3_000).map(|x| f32::from_bits(x as u32)));
        for v in ties
            .into_iter()
            .filter(|v| v.is_finite() && *v != 0.0)
            .map(f32::abs)
        {
            assert_eq!(canonical_decimal(v), reference(v, f64::from(v)), "{v:e}");
        }
        let mut doubles = vec![
            "-1749220892028010.25".parse::<f64>().unwrap(),
            5e-324,
            f64::MAX,
        ];
        doubles.extend(sample(3_000).map(f64::from_bits));
        for v in doubles
            .into_iter()
            .filter(|v| v.is_finite() && *v != 0.0)
            .map(f64::abs)
        {
            assert_eq!(canonical_decimal(v), reference(v, v), "{v:e}");
        }
    }
}
