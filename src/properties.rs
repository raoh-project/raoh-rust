//! Reading a `.properties` file as Java's `Properties.load` does, so that a catalogue of Raoh for
//! Java, or of the Raoh Specification, reads here as it reads there. No Rust crate reads one with
//! the same escapes and continuation lines.

/// Where [`load_properties`] stopped reading.
#[derive(Debug)]
pub(crate) struct PropertiesError {
    pub(crate) line: usize,
    pub(crate) reason: &'static str,
}

/// `Properties.load(Reader)`: the key and value pairs of a `.properties` text, in order.
///
/// A logical line continues onto the next when it ends in an odd number of backslashes, and the
/// next line's leading whitespace is skipped. Lines that are blank or whose first character other
/// than whitespace is `#` or `!` are comments. A key ends at the first `=`, `:` or whitespace not
/// escaped; whitespace around the separator is skipped. Both key and value undo the escapes `\t`,
/// `\n`, `\r`, `\f` and `\uXXXX`, and a backslash before any other character stands for that
/// character. A `\u` not followed by four hexadecimal digits is an error, as in the JDK.
pub(crate) fn load_properties(text: &str) -> Result<Vec<(String, String)>, PropertiesError> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let is_space = |c: char| matches!(c, ' ' | '\t' | '\u{000C}');
    let mut pairs = Vec::new();
    let mut logical = String::new();
    let mut start = 0;
    let mut continuing = false;
    for (index, line) in text.split('\n').enumerate() {
        let line = if continuing {
            line.trim_start_matches(is_space)
        } else {
            let content = line.trim_start_matches(is_space);
            if content.is_empty() || content.starts_with('#') || content.starts_with('!') {
                continue;
            }
            start = index + 1;
            content
        };
        let trailing = line.len() - line.trim_end_matches('\\').len();
        if trailing % 2 == 1 {
            logical.push_str(&line[..line.len() - 1]);
            continuing = true;
        } else {
            logical.push_str(line);
            continuing = false;
            pairs.push(split_entry(&logical, start)?);
            logical.clear();
        }
    }
    if continuing {
        pairs.push(split_entry(&logical, start)?);
    }
    Ok(pairs)
}

fn split_entry(line: &str, number: usize) -> Result<(String, String), PropertiesError> {
    let is_space = |c: char| matches!(c, ' ' | '\t' | '\u{000C}');
    let mut key_end = line.len();
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '=' || c == ':' || is_space(c) {
            key_end = i;
            break;
        }
    }
    let rest = line[key_end..].trim_start_matches(is_space);
    let rest = rest
        .strip_prefix('=')
        .or_else(|| rest.strip_prefix(':'))
        .unwrap_or(rest)
        .trim_start_matches(is_space);
    Ok((unescape(&line[..key_end], number)?, unescape(rest, number)?))
}

fn unescape(raw: &str, number: usize) -> Result<String, PropertiesError> {
    let mut units: Vec<u16> = Vec::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buffer = [0; 2];
            units.extend_from_slice(c.encode_utf16(&mut buffer));
            continue;
        }
        match chars.next() {
            Some('t') => units.push(u16::from(b'\t')),
            Some('n') => units.push(u16::from(b'\n')),
            Some('r') => units.push(u16::from(b'\r')),
            Some('f') => units.push(0x0C),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                let unit = (hex.len() == 4)
                    .then(|| u16::from_str_radix(&hex, 16).ok())
                    .flatten()
                    .ok_or(PropertiesError {
                        line: number,
                        reason: "malformed \\uXXXX encoding",
                    })?;
                units.push(unit);
            }
            Some(other) => {
                let mut buffer = [0; 2];
                units.extend_from_slice(other.encode_utf16(&mut buffer));
            }
            None => {}
        }
    }
    Ok(String::from_utf16_lossy(&units))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_follow_properties_load() {
        let text = "# comment\n  ! also a comment\n\
                    a=1\n\
                    b : 2\n\
                    c 3\n\
                    d=\\u5fc5\\u9808\n\
                    e=one \\\n     two\n\
                    f\\=g=h\\:i\n\
                    emoji=\\ud83d\\ude00\n\
                    empty\n";
        let pairs = load_properties(text).unwrap();
        let get = |k: &str| {
            pairs
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("a"), Some("1"));
        assert_eq!(get("b"), Some("2"));
        assert_eq!(get("c"), Some("3"));
        assert_eq!(get("d"), Some("必須"));
        assert_eq!(get("e"), Some("one two"));
        assert_eq!(get("f=g"), Some("h:i"));
        assert_eq!(get("emoji"), Some("😀"));
        assert_eq!(get("empty"), Some(""));
        assert_eq!(pairs.len(), 8);
    }

    #[test]
    fn a_malformed_unicode_escape_is_an_error() {
        let error = load_properties("ok=1\nbad=\\u12").unwrap_err();
        assert_eq!(error.line, 2);
    }
}
