//! Decimals of any precision that keep their scale.

use num_bigint::BigUint;
use num_traits::Zero;
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

/// A decimal number: an integer coefficient of any size and a scale, coefficient × 10^-scale.
///
/// A decimal keeps its scale, as the value model of the Raoh Specification says: `1.5` and `1.50`
/// are different decimals, and `==` tells them apart. [`numeric_cmp`](Self::numeric_cmp) compares
/// by value alone, as the bounds of a decimal decoder do.
///
/// The scale is a 32-bit integer and may be negative: `1E+3` has coefficient 1 and scale -3.
///
/// ```
/// use raoh::Decimal;
///
/// let a: Decimal = "1.50".parse().unwrap();
/// let b: Decimal = "1.5".parse().unwrap();
/// assert_ne!(a, b);
/// assert_eq!(a.numeric_cmp(&b), std::cmp::Ordering::Equal);
/// assert_eq!(a.scale(), 2);
/// assert_eq!("1e3".parse::<Decimal>().unwrap().to_string(), "1E+3");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Decimal {
    negative: bool,
    /// The coefficient's digits with no leading zero, `0` for zero. A coefficient is kept as its
    /// digits and not as a binary integer: reading a long one into binary takes time that grows
    /// with the square of its length, and comparing, writing and checking the scale need only the
    /// digits.
    digits: String,
    scale: i32,
}

/// Text [`Decimal::from_str`] does not read as a decimal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseDecimalError(());

impl fmt::Display for ParseDecimalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a decimal number, or one whose scale does not fit 32 bits")
    }
}

impl std::error::Error for ParseDecimalError {}

impl Decimal {
    /// Zero with scale 0.
    pub fn zero() -> Self {
        Self {
            negative: false,
            digits: "0".into(),
            scale: 0,
        }
    }

    /// `unscaled` × 10^-`scale`.
    pub fn new(unscaled: i128, scale: i32) -> Self {
        Self {
            negative: unscaled < 0,
            digits: unscaled.unsigned_abs().to_string(),
            scale,
        }
    }

    /// The number of digits after the point, or, when negative, the number of zeros the
    /// coefficient is followed by.
    pub fn scale(&self) -> i32 {
        self.scale
    }

    /// Whether the value is zero, whatever the scale.
    pub fn is_zero(&self) -> bool {
        self.digits == "0"
    }

    /// -1, 0 or 1, as the value is negative, zero or positive.
    pub fn signum(&self) -> i32 {
        if self.is_zero() {
            0
        } else if self.negative {
            -1
        } else {
            1
        }
    }

    /// The coefficient in decimal digits, with a minus sign when negative.
    pub fn unscaled_string(&self) -> String {
        if self.negative {
            format!("-{}", self.digits)
        } else {
            self.digits.clone()
        }
    }

    /// Compares by value alone: `1.5` and `1.50` are equal here.
    pub fn numeric_cmp(&self, other: &Self) -> Ordering {
        let (a, b) = (self.signum(), other.signum());
        if a != b || a == 0 {
            return a.cmp(&b);
        }
        let magnitude = self.magnitude_cmp(other);
        if a > 0 {
            magnitude
        } else {
            magnitude.reverse()
        }
    }

    /// Compares the absolute values of two non-zero decimals.
    ///
    /// The exponents of the first digits are compared first, so that two decimals whose scales
    /// are far apart are never brought to the same scale. When the first digits are at the same
    /// place, the digits are compared one by one from the first, the shorter coefficient read as
    /// followed by zeros.
    fn magnitude_cmp(&self, other: &Self) -> Ordering {
        let adjusted_a = self.digits.len() as i64 - 1 - i64::from(self.scale);
        let adjusted_b = other.digits.len() as i64 - 1 - i64::from(other.scale);
        if adjusted_a != adjusted_b {
            return adjusted_a.cmp(&adjusted_b);
        }
        let (a, b) = (self.digits.as_bytes(), other.digits.as_bytes());
        let longest = a.len().max(b.len());
        (0..longest)
            .map(|i| {
                let x = a.get(i).copied().unwrap_or(b'0');
                let y = b.get(i).copied().unwrap_or(b'0');
                x.cmp(&y)
            })
            .find(|o| o.is_ne())
            .unwrap_or(Ordering::Equal)
    }

    /// Whether this is an integer multiple of `divisor`, which is not zero.
    ///
    /// The quotient is (coefficient ÷ divisor's coefficient) × 10^(divisor's scale - scale).
    /// Where that power is positive it is taken modulo the divisor's coefficient, so that a
    /// divisor such as `7E-2147483647` costs no more than one of a small scale. Where it is
    /// negative, the coefficient has to end in that many zeros and what is before them has to be
    /// a multiple. The coefficient, which comes from the input, is read as digits a block at a
    /// time and only its remainder is kept, so the time grows with its length, not its square.
    pub fn is_multiple_of(&self, divisor: &Self) -> bool {
        assert!(!divisor.is_zero(), "divisor must not be zero");
        if self.is_zero() {
            return true;
        }
        let modulus = BigUint::from_str(&divisor.digits).expect("the digits are ASCII digits");
        let shift = i64::from(divisor.scale) - i64::from(self.scale);
        if shift >= 0 {
            let power = BigUint::from(10u8).modpow(&BigUint::from(shift.unsigned_abs()), &modulus);
            (digits_mod(&self.digits, &modulus) * power % &modulus).is_zero()
        } else {
            let zeros = shift.unsigned_abs();
            // A coefficient of n digits is below 10^n, so no multiple of 10^zeros with
            // zeros >= n divides it unless it is zero, which was answered above.
            if zeros >= self.digits.len() as u64 {
                return false;
            }
            let (before, after) = self.digits.split_at(self.digits.len() - zeros as usize);
            after.bytes().all(|b| b == b'0') && digits_mod(before, &modulus).is_zero()
        }
    }

    /// Reads a decimal number as a JSON number writes one, or as the wider grammar
    /// `[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?`, keeping the scale it is written
    /// with. `None` when the text is neither, or its scale does not fit 32 bits.
    pub(crate) fn read(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let mut at = 0;
        let negative = match bytes.first() {
            Some(b'-') => {
                at = 1;
                true
            }
            Some(b'+') => {
                at = 1;
                false
            }
            _ => false,
        };
        let integer_start = at;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        let integer = &text[integer_start..at];
        let mut fraction = "";
        if bytes.get(at) == Some(&b'.') {
            at += 1;
            let start = at;
            while at < bytes.len() && bytes[at].is_ascii_digit() {
                at += 1;
            }
            fraction = &text[start..at];
        }
        if integer.is_empty() && fraction.is_empty() {
            return None;
        }
        let mut exponent: i64 = 0;
        if matches!(bytes.get(at), Some(b'e' | b'E')) {
            at += 1;
            let minus = match bytes.get(at) {
                Some(b'-') => {
                    at += 1;
                    true
                }
                Some(b'+') => {
                    at += 1;
                    false
                }
                _ => false,
            };
            let start = at;
            while at < bytes.len() && bytes[at] == b'0' {
                at += 1;
            }
            let significant = at;
            while at < bytes.len() && bytes[at].is_ascii_digit() {
                // Past 18 significant digits the scale cannot fit 32 bits whatever the fraction.
                if at - significant >= 18 {
                    return None;
                }
                exponent = exponent * 10 + i64::from(bytes[at] - b'0');
                at += 1;
            }
            if at == start {
                return None;
            }
            if minus {
                exponent = -exponent;
            }
        }
        if at != bytes.len() {
            return None;
        }
        let scale = i64::try_from(fraction.len()).ok()? - exponent;
        let scale = i32::try_from(scale).ok()?;
        let written = format!("{integer}{fraction}");
        let digits = match written.trim_start_matches('0') {
            "" => "0".to_owned(),
            significant => significant.to_owned(),
        };
        Some(Self {
            negative: negative && digits != "0",
            digits,
            scale,
        })
    }
}

/// The integer `digits` writes, modulo `modulus`, read 18 digits at a time.
fn digits_mod(digits: &str, modulus: &BigUint) -> BigUint {
    digits
        .as_bytes()
        .chunks(18)
        .fold(BigUint::zero(), |remainder, block| {
            let value = block.iter().fold(0u64, |v, b| v * 10 + u64::from(b - b'0'));
            let shift = BigUint::from(10u64.pow(block.len() as u32));
            (remainder * shift + value) % modulus
        })
}

/// Reads the grammar `[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?`, keeping the scale.
impl FromStr for Decimal {
    type Err = ParseDecimalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::read(s).ok_or(ParseDecimalError(()))
    }
}

/// Written as Java's `BigDecimal.toString` writes it, which is how the Raoh Specification writes a
/// decimal: plainly when the scale is not negative and the first digit is at most six places after
/// the point (`0.00010`, `10`), and otherwise as the first digit, the others after a point, `E`, a
/// sign and the exponent of the first digit (`1E+3`, `1.5E-7`).
impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits = &self.digits;
        if self.negative {
            f.write_str("-")?;
        }
        let length = digits.len() as i64;
        let scale = i64::from(self.scale);
        let adjusted = length - 1 - scale;
        if scale >= 0 && adjusted >= -6 {
            if scale == 0 {
                return f.write_str(digits);
            }
            let point = length - scale;
            if point > 0 {
                let point = point as usize;
                write!(f, "{}.{}", &digits[..point], &digits[point..])
            } else {
                write!(f, "0.{}{digits}", "0".repeat((-point) as usize))
            }
        } else {
            let (first, rest) = digits.split_at(1);
            f.write_str(first)?;
            if !rest.is_empty() {
                write!(f, ".{rest}")?;
            }
            let sign = if adjusted < 0 { '-' } else { '+' };
            write!(f, "E{sign}{}", adjusted.unsigned_abs())
        }
    }
}

impl From<i64> for Decimal {
    fn from(value: i64) -> Self {
        Self::new(i128::from(value), 0)
    }
}

impl From<i32> for Decimal {
    fn from(value: i32) -> Self {
        Self::new(i128::from(value), 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    #[test]
    fn the_scale_is_the_one_written() {
        assert_eq!(d("1.50").scale(), 2);
        assert_eq!(d("1e3").scale(), -3);
        assert_eq!(d("2.5E-4").scale(), 5);
        assert_eq!(d("5.").scale(), 0);
        assert_eq!(d(".5").scale(), 1);
        assert_eq!(d("-0").signum(), 0);
    }

    #[test]
    fn decimals_are_written_as_big_decimal_writes_them() {
        for (text, written) in [
            ("12.5", "12.5"),
            ("0.0001", "0.0001"),
            ("1e3", "1E+3"),
            ("2.5E-4", "0.00025"),
            ("0e5", "0E+5"),
            ("1e999", "1E+999"),
            ("-1.5", "-1.5"),
            ("+1.50", "1.50"),
            ("-0", "0"),
            ("1.5E-7", "1.5E-7"),
            ("0.00000010", "1.0E-7"),
            ("123E-2", "1.23"),
            ("7E-2147483647", "7E-2147483647"),
        ] {
            assert_eq!(d(text).to_string(), written, "{text}");
        }
    }

    #[test]
    fn a_scale_past_32_bits_is_not_read() {
        assert!("1e-2147483648".parse::<Decimal>().is_err());
        assert!("1e2147483649".parse::<Decimal>().is_err());
        assert!("1e99999999999".parse::<Decimal>().is_err());
        assert!("1e2147483648".parse::<Decimal>().is_ok());
        assert_eq!(d("1e00000000000000000001").scale(), -1);
        assert_eq!(d("1.5e-0000000000000000000001").scale(), 2);
        for bad in [
            "", ".", "-", ".e1", "1e", "1e+", "NaN", "1,5", "--1", "1.2.3", " 1",
        ] {
            assert!(bad.parse::<Decimal>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn values_compare_whatever_their_scales() {
        assert_eq!(d("1.5").numeric_cmp(&d("1.50")), Ordering::Equal);
        assert_eq!(d("10").numeric_cmp(&d("1E+1")), Ordering::Equal);
        assert_eq!(d("-2").numeric_cmp(&d("-10")), Ordering::Greater);
        assert_eq!(
            d("1E+100").numeric_cmp(&d("7E-2147483647")),
            Ordering::Greater
        );
        assert_eq!(d("0.0005").numeric_cmp(&d("0.0001")), Ordering::Greater);
        assert_eq!(d("0").numeric_cmp(&d("0E+5")), Ordering::Equal);
    }

    #[test]
    fn multiples_are_found_at_any_scale() {
        assert!(d("1.5").is_multiple_of(&d("0.5")));
        assert!(!d("1.2").is_multiple_of(&d("0.5")));
        assert!(!d("1E+100").is_multiple_of(&d("7E-2147483647")));
        assert!(d("7E+100").is_multiple_of(&d("7E-2147483647")));
        assert!(d("100").is_multiple_of(&d("1E+2")));
        assert!(!d("10").is_multiple_of(&d("1E+2")));
        assert!(!d("10").is_multiple_of(&d("1E+2000000000")));
        assert!(d("0").is_multiple_of(&d("3")));
        assert!(d("-6").is_multiple_of(&d("-3")));
    }
}
