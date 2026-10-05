//! 128-bit UUIDs.

use std::fmt;
use std::str::FromStr;

/// A 128-bit UUID of any version and variant, the nil and max UUIDs included.
///
/// It is read from 32 hexadecimal digits in either case grouped 8-4-4-4-12 by hyphens, as RFC 9562
/// section 4 writes one, and written in lower case.
///
/// ```
/// use raoh::Uuid;
///
/// let id: Uuid = "123E4567-E89B-12D3-A456-426614174000".parse().unwrap();
/// assert_eq!(id.to_string(), "123e4567-e89b-12d3-a456-426614174000");
/// assert!("{123e4567-e89b-12d3-a456-426614174000}".parse::<Uuid>().is_err());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uuid([u8; 16]);

/// Text [`Uuid::from_str`] does not read as a UUID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseUuidError(());

impl fmt::Display for ParseUuidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a UUID of 32 hexadecimal digits grouped 8-4-4-4-12")
    }
}

impl std::error::Error for ParseUuidError {}

impl Uuid {
    /// The UUID of these bytes, most significant first.
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// The bytes, most significant first.
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl FromStr for Uuid {
    type Err = ParseUuidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let text = s.as_bytes();
        if text.len() != 36 {
            return Err(ParseUuidError(()));
        }
        let mut bytes = [0u8; 16];
        let mut nibble = 0;
        for (i, &c) in text.iter().enumerate() {
            if matches!(i, 8 | 13 | 18 | 23) {
                if c != b'-' {
                    return Err(ParseUuidError(()));
                }
                continue;
            }
            let value = char::from(c).to_digit(16).ok_or(ParseUuidError(()))? as u8;
            bytes[nibble / 2] |= if nibble % 2 == 0 { value << 4 } else { value };
            nibble += 1;
        }
        Ok(Self(bytes))
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, byte) in self.0.iter().enumerate() {
            if matches!(i, 4 | 6 | 8 | 10) {
                f.write_str("-")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_hyphenated_form_is_read() {
        for bad in [
            "123e4567e89b12d3a456426614174000",
            "1-1-1-1-1",
            "urn:uuid:123e4567-e89b-12d3-a456-426614174000",
            "123e4567-e89b-12d3-a456-42661417400g",
            "123e4567-e89b-12d3-a456_426614174000",
        ] {
            assert!(bad.parse::<Uuid>().is_err(), "{bad}");
        }
        let max: Uuid = "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF".parse().unwrap();
        assert_eq!(max.as_bytes(), &[0xff; 16]);
    }
}
