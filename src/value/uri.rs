//! URIs as RFC 3986 section 3 derives them.

use crate::json::ip;
use std::fmt;
use std::str::FromStr;

/// A URI as the `URI` production of RFC 3986 section 3 derives it: a scheme, a colon, the
/// hierarchical part, an optional query and an optional fragment, in ASCII with every other
/// character percent-encoded.
///
/// The value is the text as written: nothing is normalized, so `HTTP://a` and `http://a` are
/// different URIs. A relative reference is not one, and neither is an IPv6 host with a zone
/// identifier, which RFC 9844 removed from the syntax.
///
/// ```
/// use raoh::Uri;
///
/// let uri: Uri = "https://example.com/a?b#c".parse().unwrap();
/// assert_eq!(uri.scheme(), "https");
/// assert!("a:".parse::<Uri>().is_ok());
/// assert!("/relative".parse::<Uri>().is_err());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uri(String);

/// Text [`Uri::from_str`] does not read as a URI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseUriError(());

impl fmt::Display for ParseUriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not an RFC 3986 URI")
    }
}

impl std::error::Error for ParseUriError {}

impl Uri {
    /// The text of the URI.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The scheme, as written.
    pub fn scheme(&self) -> &str {
        self.0.split_once(':').map_or("", |(scheme, _)| scheme)
    }

    /// The host, as written, when the URI has an authority: `None` for `mailto:a@b`, and
    /// `Some("")` for `file:///x`.
    pub fn host(&self) -> Option<&str> {
        Parts::of(&self.0).and_then(|parts| parts.host)
    }

    /// Whether the URI is one `url()` takes: its scheme is `http` or `https` in any case, and it
    /// has an authority with a host that is not empty.
    pub(crate) fn is_web_url(&self) -> bool {
        let scheme = self.scheme();
        let web = scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https");
        web && self.host().is_some_and(|host| !host.is_empty())
    }
}

impl FromStr for Uri {
    type Err = ParseUriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Parts::of(s)
            .map(|_| Uri(s.to_owned()))
            .ok_or(ParseUriError(()))
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Uri {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// What the parse found, of what a caller asks about.
struct Parts<'a> {
    host: Option<&'a str>,
}

impl<'a> Parts<'a> {
    /// The parts of `text`, if the `URI` production derives it.
    fn of(text: &'a str) -> Option<Self> {
        if !text.is_ascii() {
            return None;
        }
        let (scheme, rest) = text.split_once(':')?;
        if !is_scheme(scheme) {
            return None;
        }
        let (rest, fragment) = match rest.split_once('#') {
            Some((rest, fragment)) => (rest, Some(fragment)),
            None => (rest, None),
        };
        let (hier, query) = match rest.split_once('?') {
            Some((hier, query)) => (hier, Some(query)),
            None => (rest, None),
        };
        let query_ok = query.is_none_or(|q| all_of(q, |c| is_pchar(c) || c == b'/' || c == b'?'));
        let fragment_ok =
            fragment.is_none_or(|f| all_of(f, |c| is_pchar(c) || c == b'/' || c == b'?'));
        if !query_ok || !fragment_ok {
            return None;
        }
        let mut host = None;
        let path = match hier.strip_prefix("//") {
            Some(after) => {
                let end = after.find('/').unwrap_or(after.len());
                host = Some(authority_host(&after[..end])?);
                &after[end..]
            }
            // Without an authority the path cannot begin with "//", which was taken above; a
            // path-absolute, a path-rootless and a path-empty are otherwise the same characters.
            None => hier,
        };
        all_of(path, |c| is_pchar(c) || c == b'/').then_some(Self { host })
    }
}

/// The host of `authority = [ userinfo "@" ] host [ ":" port ]`, if it is one.
fn authority_host(authority: &str) -> Option<&str> {
    let (userinfo, host_port) = match authority.split_once('@') {
        Some((userinfo, rest)) => (Some(userinfo), rest),
        None => (None, authority),
    };
    let userinfo_ok = userinfo.is_none_or(|userinfo| {
        all_of(userinfo, |c| {
            is_unreserved(c) || is_sub_delim(c) || c == b':'
        })
    });
    if !userinfo_ok {
        return None;
    }
    let (host, port) = if host_port.starts_with('[') {
        let close = host_port.find(']')?;
        let literal = &host_port[1..close];
        if !is_ip_literal(literal) {
            return None;
        }
        let after = &host_port[close + 1..];
        let port = match after {
            "" => "",
            _ => after.strip_prefix(':')?,
        };
        (&host_port[..=close], port)
    } else {
        // A reg-name holds no ":", so the first one begins the port.
        match host_port.split_once(':') {
            Some((host, port)) => (host, port),
            None => (host_port, ""),
        }
    };
    if !port.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let reg_name = |host: &str| all_of(host, |c| is_unreserved(c) || is_sub_delim(c));
    if !host.starts_with('[') && !reg_name(host) {
        return None;
    }
    Some(host)
}

/// `IPv6address / IPvFuture`, the inside of an `IP-literal`.
fn is_ip_literal(literal: &str) -> bool {
    if let Some(future) = literal
        .strip_prefix('v')
        .or_else(|| literal.strip_prefix('V'))
    {
        let Some((version, rest)) = future.split_once('.') else {
            return false;
        };
        return !version.is_empty()
            && version.bytes().all(|c| c.is_ascii_hexdigit())
            && !rest.is_empty()
            && rest
                .bytes()
                .all(|c| is_unreserved(c) || is_sub_delim(c) || c == b':');
    }
    // An IPv6address of RFC 3986 has no zone: RFC 9844 removed RFC 6874's.
    !literal.contains('%') && ip::is_ipv6(literal)
}

fn is_scheme(scheme: &str) -> bool {
    let mut bytes = scheme.bytes();
    bytes.next().is_some_and(|c| c.is_ascii_alphabetic())
        && bytes.all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
}

fn is_unreserved(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.' | b'_' | b'~')
}

fn is_sub_delim(c: u8) -> bool {
    matches!(
        c,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

/// `pchar = unreserved / pct-encoded / sub-delims / ":" / "@"`, but for `pct-encoded`, which
/// [`all_of`] reads.
fn is_pchar(c: u8) -> bool {
    is_unreserved(c) || is_sub_delim(c) || matches!(c, b':' | b'@')
}

/// Whether every byte of `s` is `allowed` or begins a `pct-encoded`: `%` and two hexadecimal
/// digits.
fn all_of(s: &str, allowed: impl Fn(u8) -> bool) -> bool {
    s.bytes().all(|c| c == b'%' || allowed(c)) && percent_encodings_ok(s)
}

fn percent_encodings_ok(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.iter().enumerate().all(|(i, &c)| {
        c != b'%'
            || (bytes.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
                && bytes.get(i + 2).is_some_and(u8::is_ascii_hexdigit))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> bool {
        s.parse::<Uri>().is_ok()
    }

    #[test]
    fn every_uri_the_grammar_derives_is_read() {
        for uri in [
            "a:",
            "a://",
            "http://",
            "http://example.com",
            "https://user:pw@host:8080/p/a/t/h?q=1&r=2#frag",
            "mailto:a@example.com",
            "urn:isbn:0451450523",
            "file:///etc/hosts",
            "http://[::1]:80/",
            "http://[v7.fe80::a+en1]/",
            "http://h:99999999999999/",
            "a:b%20c",
            "a:/b//c",
            "x-y.z+w:",
        ] {
            assert!(ok(uri), "{uri}");
        }
    }

    #[test]
    fn what_the_grammar_does_not_derive_is_refused() {
        for uri in [
            "",
            "/relative",
            "//host",
            "1a:",
            ":",
            "a:b c",
            "a:%2",
            "a:%zz",
            "http://h:8a/",
            "http://[::1%25eth0]/",
            "http://[fe80::1%eth0]/",
            "http://[::1/",
            "http://[1:2]/",
            "http://a@b@c/",
            "http://exämple.com/",
            "a:#b#c",
            "a:[",
        ] {
            assert!(!ok(uri), "{uri}");
        }
    }

    #[test]
    fn a_web_url_needs_an_http_scheme_and_a_host() {
        let url = |s: &str| s.parse::<Uri>().unwrap().is_web_url();
        assert!(url("HTTP://example.com"));
        assert!(url("https://[::1]"));
        assert!(!url("http://"));
        assert!(!url("http:///p"));
        assert!(!url("ftp://example.com"));
        assert!(!url("http:example.com"));
    }
}
