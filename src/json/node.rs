//! A JSON value read from text with every number's lexeme, and the reader of that text.

use super::input::{Elements, Input, Json, Members, Number, View, is_number, number_end};
use crate::issue::Issue;
use crate::{codes, message_keys};
use arrayvec::ArrayString;
use std::str::FromStr;

/// A JSON value as the Raoh Specification's input model has it: every number keeps its lexeme,
/// and every object its members in the order they were written, with no name twice.
///
/// [`from_str`](super::from_str) reads JSON text into one; it is also what [`str::parse`] gives.
///
/// ```
/// use raoh::json::prelude::*;
/// use raoh::json::Node;
///
/// let node: Node = r#"{"price": 1.50, "delta": -0}"#.parse().unwrap();
/// let price = object((field("price", decimal()), field("delta", f64())));
/// let (price, delta) = price.decode(&node).unwrap();
/// assert_eq!((price.to_string(), delta.is_sign_negative()), ("1.50".to_owned(), true));
/// ```
#[derive(Clone, Debug)]
pub enum Node {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number.
    Number(Lexeme),
    /// A string.
    String(String),
    /// An array.
    Array(Vec<Node>),
    /// An object.
    Object(JsonObject),
}

impl Input for Node {
    fn view(&self) -> View<'_> {
        match self {
            Node::Null => View::Null,
            Node::Bool(b) => View::Bool(*b),
            Node::Number(n) => View::Number(Number::valid(n.as_str())),
            Node::String(s) => View::String(s),
            Node::Array(a) => View::Array(a),
            Node::Object(o) => View::Object(o),
        }
    }
}

impl Elements for Vec<Node> {
    fn len(&self) -> usize {
        self.as_slice().len()
    }

    fn get(&self, index: usize) -> Option<&Json> {
        self.as_slice().get(index).map(|v| v as &Json)
    }
}

impl FromStr for Node {
    type Err = Issue;

    /// Reads `text` as [`from_str`](super::from_str) does.
    fn from_str(text: &str) -> Result<Self, Issue> {
        Reader::new(text).document()
    }
}

/// The lexeme of a JSON number. One of up to 22 bytes, which almost every number is, is held
/// without a pointer to memory of its own.
#[derive(Clone)]
pub struct Lexeme(LexemeRepr);

const INLINE: usize = 22;

#[derive(Clone)]
enum LexemeRepr {
    Inline(ArrayString<INLINE>),
    Heap(Box<str>),
}

impl Lexeme {
    /// The lexeme `text`, if it is a JSON number.
    pub fn new(text: &str) -> Option<Self> {
        is_number(text).then(|| Self::valid(text))
    }

    fn valid(text: &str) -> Self {
        match ArrayString::from(text) {
            Ok(inline) => Self(LexemeRepr::Inline(inline)),
            Err(_) => Self(LexemeRepr::Heap(text.into())),
        }
    }

    /// The text of the number.
    pub fn as_str(&self) -> &str {
        match &self.0 {
            LexemeRepr::Inline(text) => text,
            LexemeRepr::Heap(text) => text,
        }
    }
}

impl std::fmt::Debug for Lexeme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::fmt::Display for Lexeme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The members of a JSON object, in the order they were written, with no name twice.
#[derive(Clone, Debug, Default)]
pub struct JsonObject {
    members: Vec<(String, Node)>,
    /// For an object of more than [`SCANNED`] members, the positions of the members sorted by
    /// name; empty otherwise.
    by_name: Box<[u32]>,
}

/// The most members an object is looked up in one by one, rather than through its sorted names.
const SCANNED: usize = 8;

impl JsonObject {
    /// An object of `members`, in that order, or the name that is there twice.
    pub fn new(members: Vec<(String, Node)>) -> Result<Self, String> {
        Self::build(members).map_err(|(members, twice)| members[twice].0.clone())
    }

    /// An object of `members`, or them back with the position of the first member whose name an
    /// earlier one has.
    #[allow(clippy::type_complexity)]
    fn build(members: Vec<(String, Node)>) -> Result<Self, (Vec<(String, Node)>, usize)> {
        let name = |i: u32| members[i as usize].0.as_str();
        let (twice, by_name) = if members.len() > SCANNED {
            let mut sorted: Vec<u32> = (0..members.len() as u32).collect();
            sorted.sort_unstable_by(|&a, &b| name(a).cmp(name(b)).then(a.cmp(&b)));
            let twice = sorted
                .windows(2)
                .filter(|p| name(p[0]) == name(p[1]))
                .map(|p| p[1] as usize)
                .min();
            (twice, sorted.into_boxed_slice())
        } else {
            let twice =
                (1..members.len()).find(|&i| members[..i].iter().any(|m| m.0 == members[i].0));
            (twice, Box::default())
        };
        match twice {
            Some(twice) => Err((members, twice)),
            None => Ok(Self { members, by_name }),
        }
    }

    /// The value of the member `name`.
    pub fn get(&self, name: &str) -> Option<&Node> {
        let at = if self.by_name.is_empty() {
            self.members.iter().position(|m| m.0 == name)?
        } else {
            let found = self
                .by_name
                .binary_search_by(|&i| self.members[i as usize].0.as_str().cmp(name))
                .ok()?;
            self.by_name[found] as usize
        };
        Some(&self.members[at].1)
    }

    /// The members in the order they were written.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Node)> {
        self.members
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }

    /// How many members there are.
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
}

impl Members for JsonObject {
    fn len(&self) -> usize {
        JsonObject::len(self)
    }

    fn get(&self, name: &str) -> Option<&Json> {
        JsonObject::get(self, name).map(|v| v as &Json)
    }

    fn each(&self, visit: &mut dyn FnMut(&str, &Json)) {
        for (name, value) in &self.members {
            visit(name, value);
        }
    }
}

/// How deeply arrays and objects may be nested, so that reading does not run out of stack.
const DEPTH: usize = 128;

/// Reads RFC 8259 JSON text: one value, with white space around it and nothing else.
struct Reader<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
    depth: usize,
}

/// Where the text stopped being JSON.
struct Stop(usize);

impl<'a> Reader<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            bytes: text.as_bytes(),
            at: 0,
            depth: 0,
        }
    }

    fn document(mut self) -> Result<Node, Issue> {
        let read = self.value().and_then(|node| {
            self.space();
            if self.at == self.bytes.len() {
                Ok(node)
            } else {
                Err(Stop(self.at))
            }
        });
        read.map_err(|Stop(at)| self.invalid(at))
    }

    /// `invalid_format` at the line and column, both from 1, of the character at byte `at`.
    fn invalid(&self, at: usize) -> Issue {
        let before = &self.text[..at];
        let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |n| n + 1);
        let column = before[line_start..].chars().count() + 1;
        Issue::new(codes::INVALID_FORMAT)
            .with_message_key(message_keys::INVALID_FORMAT_JSON)
            .with_meta("line", line)
            .with_meta("column", column)
    }

    fn space(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.bytes.get(self.at) {
            self.at += 1;
        }
    }

    fn value(&mut self) -> Result<Node, Stop> {
        self.space();
        match self.bytes.get(self.at) {
            Some(b'n') => self.word("null", Node::Null),
            Some(b't') => self.word("true", Node::Bool(true)),
            Some(b'f') => self.word("false", Node::Bool(false)),
            Some(b'"') => self.string().map(Node::String),
            Some(b'[') => self.nested(Self::array),
            Some(b'{') => self.nested(Self::object),
            Some(_) => {
                let end = number_end(self.bytes, self.at).ok_or(Stop(self.at))?;
                let lexeme = Lexeme::valid(&self.text[self.at..end]);
                self.at = end;
                Ok(Node::Number(lexeme))
            }
            None => Err(Stop(self.at)),
        }
    }

    fn word(&mut self, word: &str, node: Node) -> Result<Node, Stop> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(node)
        } else {
            Err(Stop(self.at))
        }
    }

    fn nested(&mut self, read: fn(&mut Self) -> Result<Node, Stop>) -> Result<Node, Stop> {
        if self.depth == DEPTH {
            return Err(Stop(self.at));
        }
        self.depth += 1;
        self.at += 1;
        let node = read(self);
        self.depth -= 1;
        node
    }

    /// The rest of an array, after its `[`.
    fn array(&mut self) -> Result<Node, Stop> {
        let mut elements = Vec::new();
        self.space();
        if self.bytes.get(self.at) == Some(&b']') {
            self.at += 1;
            return Ok(Node::Array(elements));
        }
        loop {
            elements.push(self.value()?);
            self.space();
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Node::Array(elements));
                }
                _ => return Err(Stop(self.at)),
            }
        }
    }

    /// The rest of an object, after its `{`.
    fn object(&mut self) -> Result<Node, Stop> {
        let mut members = Vec::new();
        let mut starts = Vec::new();
        self.space();
        if self.bytes.get(self.at) == Some(&b'}') {
            self.at += 1;
            return Ok(Node::Object(JsonObject::default()));
        }
        loop {
            self.space();
            if self.bytes.get(self.at) != Some(&b'"') {
                return Err(Stop(self.at));
            }
            starts.push(self.at);
            let name = self.string()?;
            self.space();
            if self.bytes.get(self.at) != Some(&b':') {
                return Err(Stop(self.at));
            }
            self.at += 1;
            members.push((name, self.value()?));
            self.space();
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b'}') => break,
                _ => return Err(Stop(self.at)),
            }
        }
        self.at += 1;
        JsonObject::build(members)
            .map(Node::Object)
            .map_err(|(_, twice)| Stop(starts[twice]))
    }

    /// A string, from its opening `"`.
    fn string(&mut self) -> Result<String, Stop> {
        self.at += 1;
        let start = self.at;
        let plain = self.bytes[start..]
            .iter()
            .position(|&b| b == b'"' || b == b'\\' || b < 0x20)
            .ok_or(Stop(self.bytes.len()))?;
        self.at = start + plain;
        if self.bytes[self.at] == b'"' {
            self.at += 1;
            return Ok(self.text[start..start + plain].to_owned());
        }
        let mut out = String::from(&self.text[start..start + plain]);
        loop {
            match self.bytes.get(self.at) {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    out.push(self.escape()?);
                }
                Some(&b) if b >= 0x20 => {
                    let run = self.bytes[self.at..]
                        .iter()
                        .position(|&b| b == b'"' || b == b'\\' || b < 0x20)
                        .ok_or(Stop(self.bytes.len()))?;
                    out.push_str(&self.text[self.at..self.at + run]);
                    self.at += run;
                }
                _ => return Err(Stop(self.at)),
            }
        }
    }

    /// The character an escape stands for, from after its `\`.
    fn escape(&mut self) -> Result<char, Stop> {
        let at = self.at;
        let c = match self.bytes.get(at) {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                self.at += 1;
                let unit = self.hex()?;
                return match unit {
                    0xD800..=0xDBFF => {
                        if !self.bytes[self.at..].starts_with(b"\\u") {
                            return Err(Stop(self.at));
                        }
                        self.at += 2;
                        let low = self.hex()?;
                        if !(0xDC00..=0xDFFF).contains(&low) {
                            return Err(Stop(self.at - 4));
                        }
                        let scalar = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                        char::from_u32(scalar).ok_or(Stop(at))
                    }
                    0xDC00..=0xDFFF => Err(Stop(at)),
                    _ => char::from_u32(unit).ok_or(Stop(at)),
                };
            }
            _ => return Err(Stop(at)),
        };
        self.at += 1;
        Ok(c)
    }

    /// Four hexadecimal digits.
    fn hex(&mut self) -> Result<u32, Stop> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or(Stop(self.bytes.len()))?;
        let mut unit = 0;
        for (i, &d) in digits.iter().enumerate() {
            let value = (d as char).to_digit(16).ok_or(Stop(self.at + i))?;
            unit = unit * 16 + value;
        }
        self.at += 4;
        Ok(unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Node {
        text.parse()
            .unwrap_or_else(|i: Issue| panic!("{text}: {:?}", i.meta()))
    }

    fn stop(text: &str) -> (String, String) {
        let issue = text.parse::<Node>().unwrap_err();
        assert_eq!(issue.code(), "invalid_format");
        (
            issue.meta()["line"].to_string(),
            issue.meta()["column"].to_string(),
        )
    }

    #[test]
    fn numbers_keep_their_lexemes() {
        let Node::Array(items) =
            read(" [-0, 1.50, 1e400, 0, -0.0e+0, 12345678901234567890123456789] ")
        else {
            panic!()
        };
        let lexemes: Vec<String> = items
            .iter()
            .map(|n| match n {
                Node::Number(l) => l.to_string(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(
            lexemes,
            [
                "-0",
                "1.50",
                "1e400",
                "0",
                "-0.0e+0",
                "12345678901234567890123456789"
            ]
        );
    }

    #[test]
    fn strings_read_their_escapes() {
        let Node::String(s) = read(r#""a\"\\\/\b\f\n\r\t\u00e9\ud83d\ude00z""#) else {
            panic!()
        };
        assert_eq!(s, "a\"\\/\u{8}\u{c}\n\r\t\u{e9}\u{1F600}z");
        let Node::String(s) = read("\"日本語\"") else {
            panic!()
        };
        assert_eq!(s, "日本語");
    }

    #[test]
    fn objects_keep_their_order_and_find_members_by_name() {
        let names: Vec<String> = (0..20).rev().map(|i| format!("\"m{i}\":{i}")).collect();
        for n in [3, 20] {
            let text = format!("{{{}}}", names[20 - n..].join(","));
            let Node::Object(object) = read(&text) else {
                panic!()
            };
            let order: Vec<&str> = object.iter().map(|(name, _)| name).collect();
            assert_eq!(order.first(), Some(&format!("m{}", n - 1).as_str()));
            for i in 0..n {
                assert!(
                    matches!(object.get(&format!("m{i}")), Some(Node::Number(l)) if l.as_str() == i.to_string())
                );
            }
            assert!(object.get("x").is_none());
        }
    }

    #[test]
    fn text_that_is_not_json_is_reported_where_it_stops() {
        assert_eq!(stop("{"), ("1".into(), "2".into()));
        assert_eq!(stop("[1,\n 2,,]"), ("2".into(), "4".into()));
        assert_eq!(stop("\"é\" x"), ("1".into(), "5".into()));
        assert_eq!(stop(r#"{"a":1,"b":2,"a":3}"#), ("1".into(), "14".into()));
        let many: Vec<String> = (0..20).map(|i| format!("\"m{i}\":{i}")).collect();
        let text = format!("{{{},\"m3\":0}}", many.join(","));
        assert_eq!(
            stop(&text).1,
            (text.rfind("\"m3\"").unwrap() + 1).to_string()
        );
        for bad in [
            "",
            "nul",
            "01",
            "1.",
            "[1,]",
            "{\"a\"}",
            "\"\\x\"",
            "\"\\ud800\"",
            "\"\\udc00\"",
            "\"a\u{1}\"",
            "\"open",
            "1 2",
            "{1:2}",
        ] {
            assert!(bad.parse::<Node>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn nesting_is_bounded() {
        let deep = "[".repeat(DEPTH) + &"]".repeat(DEPTH);
        read(&deep);
        let deeper = "[".repeat(DEPTH + 1) + &"]".repeat(DEPTH + 1);
        assert!(deeper.parse::<Node>().is_err());
    }

    #[test]
    fn lexemes_longer_than_the_inline_space_are_kept_whole() {
        let long = "1".repeat(INLINE + 1);
        assert_eq!(Lexeme::new(&long).unwrap().as_str(), long);
        assert_eq!(Lexeme::new("1.0").unwrap().as_str(), "1.0");
        assert!(Lexeme::new("1.").is_none());
    }
}
