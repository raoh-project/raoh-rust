//! What a decoder that reads a value out of a string, or out of a number's text, reads.

use super::string::StringDecoder;

/// Where a decoder of a scalar reads its value from: a JSON value of its own kind, or a string that
/// a [`StringDecoder`] reads first and the decoder then converts.
#[derive(Clone, Debug)]
pub(crate) enum Source {
    Json,
    Text(Box<StringDecoder>),
}

/// What an integer's text is.
pub(crate) enum Integral {
    /// An integer, of any size that fits an `i128`.
    Value(i128),
    /// An integer too large for an `i128`, and so for any integer type here.
    TooLarge,
    /// Not an integer: a fraction, an exponent, or anything else.
    Not,
}

/// Reads `[+-]?[0-9]+`: an optional sign and one or more ASCII digits, leading zeros allowed,
/// with nothing else. Every JSON number written as an integer is one.
pub(crate) fn read_integer(text: &str) -> Integral {
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Integral::Not;
    }
    // 19 digits are below 2^64, so they add up in a u64 without a check on each.
    if digits.len() <= 19 {
        let magnitude = digits
            .bytes()
            .fold(0u64, |v, b| v * 10 + u64::from(b - b'0'));
        let magnitude = i128::from(magnitude);
        return Integral::Value(if negative { -magnitude } else { magnitude });
    }
    let mut value: i128 = 0;
    for b in digits.bytes() {
        let digit = i128::from(b - b'0');
        let next = value.checked_mul(10).and_then(|v| {
            if negative {
                v.checked_sub(digit)
            } else {
                v.checked_add(digit)
            }
        });
        match next {
            Some(next) => value = next,
            None => return Integral::TooLarge,
        }
    }
    Integral::Value(value)
}

/// Reads `true`, `1`, `yes` or `on` as true and `false`, `0`, `no` or `off` as false, ASCII case
/// insensitively.
pub(crate) fn read_bool(text: &str) -> Option<bool> {
    const TRUE: [&str; 4] = ["true", "1", "yes", "on"];
    const FALSE: [&str; 4] = ["false", "0", "no", "off"];
    if TRUE.iter().any(|t| t.eq_ignore_ascii_case(text)) {
        Some(true)
    } else if FALSE.iter().any(|f| f.eq_ignore_ascii_case(text)) {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_are_signs_and_ascii_digits() {
        assert!(matches!(read_integer("+5"), Integral::Value(5)));
        assert!(matches!(read_integer("-0"), Integral::Value(0)));
        assert!(matches!(read_integer("007"), Integral::Value(7)));
        assert!(matches!(
            read_integer("-9999999999999999999"),
            Integral::Value(-9_999_999_999_999_999_999)
        ));
        assert!(matches!(
            read_integer("18446744073709551616"),
            Integral::Value(18_446_744_073_709_551_616)
        ));
        assert!(matches!(
            read_integer("-170141183460469231731687303715884105728"),
            Integral::Value(i128::MIN)
        ));
        assert!(matches!(
            read_integer("170141183460469231731687303715884105728"),
            Integral::TooLarge
        ));
        for not in [
            "",
            "-",
            "+",
            " 1",
            "1 ",
            "1_000",
            "1.0",
            "1e3",
            "１２３",
            "0x10",
        ] {
            assert!(matches!(read_integer(not), Integral::Not), "{not:?}");
        }
    }

    #[test]
    fn booleans_are_four_words_each() {
        assert_eq!(read_bool("tRuE"), Some(true));
        assert_eq!(read_bool("Off"), Some(false));
        assert_eq!(read_bool("y"), None);
        assert_eq!(read_bool(""), None);
    }
}
