//! The input model a decoder reads, and the values that hold it.

use crate::decoder::Nullish;
use serde_json::Value;
use std::borrow::Cow;

/// A value of the Raoh Specification's input model: what a JSON text denotes, with every number
/// keeping its lexeme.
///
/// Every decoder of [`json`](super) reads a [`Json`], which is `dyn Input`, so it reads any value
/// that implements this: a [`Node`](super::Node), which [`from_str`](super::from_str) reads JSON
/// text into, or a [`serde_json::Value`].
///
/// # Laws
///
/// A decoder may look at a value more than once, so `view` has to give the same view each time it
/// is called on the same value. [`View::Missing`] stands for a member that is not there: a value
/// that is there never gives it. [`Elements`] and [`Members`] state what their views have to keep.
pub trait Input {
    /// What this value is.
    fn view(&self) -> View<'_>;
}

/// What every decoder of [`json`](super) reads: any value of the input model.
///
/// ```
/// use raoh::json::prelude::*;
///
/// fn age() -> impl Decoder<Json, Output = u32> {
///     u32().range(0..=150)
/// }
///
/// assert_eq!(age().decode(&json!(30)).unwrap(), 30);
/// assert_eq!(from_str(&age(), "30").unwrap(), 30);
/// ```
pub type Json = dyn Input;

/// A value of the input model as a decoder sees it.
#[derive(Clone, Copy)]
pub enum View<'a> {
    /// A member of an object that is not there, which [`missing()`](super::missing) stands for.
    Missing,
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number.
    Number(Number<'a>),
    /// A string.
    String(&'a str),
    /// An array.
    Array(&'a dyn Elements),
    /// An object.
    Object(&'a dyn Members),
}

impl View<'_> {
    /// The kind of the value, as `type_mismatch` names it in `actual`.
    pub fn kind(&self) -> &'static str {
        match self {
            View::Missing => "missing",
            View::Null => "null",
            View::Bool(_) => "boolean",
            View::Number(_) => "number",
            View::String(_) => "string",
            View::Array(_) => "array",
            View::Object(_) => "object",
        }
    }

    /// Whether the value is `null` or a missing member.
    pub fn is_null_or_missing(&self) -> bool {
        matches!(self, View::Missing | View::Null)
    }
}

/// The elements of an array, in order.
///
/// # Laws
///
/// `get` gives an element for every index below `len`, and nothing from `len` on.
pub trait Elements {
    /// How many there are.
    fn len(&self) -> usize;

    /// Whether there are none.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The element at `index`.
    fn get(&self, index: usize) -> Option<&Json>;
}

impl<'a> dyn Elements + 'a {
    /// The elements in order.
    pub fn iter(&self) -> impl Iterator<Item = &Json> + '_ {
        (0..self.len()).filter_map(|i| self.get(i))
    }
}

/// The members of an object, each a name and a value, with no name twice.
///
/// # Laws
///
/// `each` hands every member to its visitor once, `len` of them, with no name twice, in the same
/// order every time it is called. `get` gives the value `each` hands with that name, and nothing
/// for a name `each` does not hand. Field decoders find members with `get`, and `dict` and the
/// strict decoders go over them with `each`, so a value one sees and the other does not is
/// decoded by one and reported unknown, or left out, by the other.
pub trait Members {
    /// How many there are.
    fn len(&self) -> usize;

    /// Whether there are none.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The value of the member `name`.
    fn get(&self, name: &str) -> Option<&Json>;

    /// Hands every member to `visit`, in the order of the object.
    fn each(&self, visit: &mut dyn FnMut(&str, &Json));
}

/// A number of the input model: its lexeme, such as `-0`, `1.50` or `1e400`.
#[derive(Clone, Copy)]
pub struct Number<'a>(Repr<'a>);

#[derive(Clone, Copy)]
enum Repr<'a> {
    Lexeme(&'a str),
    Serde(&'a serde_json::Number),
}

impl<'a> Number<'a> {
    /// The number written `lexeme`, if it is a JSON number.
    pub fn new(lexeme: &'a str) -> Option<Self> {
        is_number(lexeme).then_some(Self(Repr::Lexeme(lexeme)))
    }

    /// A lexeme already known to be a JSON number.
    pub(crate) fn valid(lexeme: &'a str) -> Self {
        Self(Repr::Lexeme(lexeme))
    }

    /// The lexeme. A `serde_json::Number` keeps it only with `serde_json`'s
    /// `arbitrary_precision` feature, and is otherwise written as the value it was read as.
    pub fn lexeme(&self) -> Cow<'a, str> {
        match self.0 {
            Repr::Lexeme(text) => Cow::Borrowed(text),
            #[cfg(feature = "arbitrary_precision")]
            Repr::Serde(n) => Cow::Borrowed(n.as_str()),
            #[cfg(not(feature = "arbitrary_precision"))]
            Repr::Serde(n) => Cow::Owned(n.to_string()),
        }
    }

    /// The number rounded to the nearest `f64`, read from the `f64` a `serde_json::Number` holds
    /// without its lexeme rather than from the text that value is written as.
    pub(crate) fn f64(&self) -> Option<f64> {
        match self.0 {
            #[cfg(not(feature = "arbitrary_precision"))]
            Repr::Serde(n) => n.as_f64(),
            _ => crate::value::float::read(&self.lexeme()),
        }
    }

    /// The number as an integer, read without writing out the value of a `serde_json::Number`
    /// that holds an `i64` or a `u64`: it holds one only for a number written as an integer.
    pub(crate) fn integral(&self) -> super::text::Integral {
        use super::text::{Integral, read_integer};
        match self.0 {
            Repr::Lexeme(text) => read_integer(text),
            Repr::Serde(n) => match (n.as_i64(), n.as_u64()) {
                (Some(i), _) => Integral::Value(i128::from(i)),
                (_, Some(u)) => Integral::Value(i128::from(u)),
                _ => read_integer(&self.lexeme()),
            },
        }
    }
}

impl std::fmt::Debug for Number<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.lexeme())
    }
}

/// Whether `text` is `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`, a JSON number.
pub(crate) fn is_number(text: &str) -> bool {
    number_end(text.as_bytes(), 0) == Some(text.len())
}

/// Where the JSON number that starts at `at` in `bytes` ends, if one starts there.
pub(crate) fn number_end(bytes: &[u8], at: usize) -> Option<usize> {
    let digits = |mut i: usize| {
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    let mut i = at;
    if bytes.get(i) == Some(&b'-') {
        i += 1;
    }
    match bytes.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => i = digits(i + 1),
        _ => return None,
    }
    if bytes.get(i) == Some(&b'.') {
        let end = digits(i + 1);
        if end == i + 1 {
            return None;
        }
        i = end;
    }
    if let Some(b'e' | b'E') = bytes.get(i) {
        i += 1;
        if let Some(b'+' | b'-') = bytes.get(i) {
            i += 1;
        }
        let end = digits(i);
        if end == i {
            return None;
        }
        i = end;
    }
    Some(i)
}

/// The member of an object that is not there, which a field's decoder is handed.
struct Missing;

impl Input for Missing {
    fn view(&self) -> View<'_> {
        View::Missing
    }
}

static MISSING: Missing = Missing;

/// The value a missing member is decoded from, which [`is_missing`] tells apart from `null`.
pub fn missing() -> &'static Json {
    &MISSING
}

/// Whether `value` is [`missing()`] rather than a value of the input.
pub fn is_missing(value: &Json) -> bool {
    matches!(value.view(), View::Missing)
}

/// A JSON null, or a missing member.
impl Nullish for Json {
    fn is_null_or_missing(&self) -> bool {
        self.view().is_null_or_missing()
    }
}

impl Input for Value {
    fn view(&self) -> View<'_> {
        match self {
            Value::Null => View::Null,
            Value::Bool(b) => View::Bool(*b),
            Value::Number(n) => View::Number(Number(Repr::Serde(n))),
            Value::String(s) => View::String(s),
            Value::Array(a) => View::Array(a),
            Value::Object(o) => View::Object(o),
        }
    }
}

/// A JSON null.
impl Nullish for Value {
    fn is_null_or_missing(&self) -> bool {
        self.is_null()
    }
}

impl Elements for Vec<Value> {
    fn len(&self) -> usize {
        self.as_slice().len()
    }

    fn get(&self, index: usize) -> Option<&Json> {
        self.as_slice().get(index).map(|v| v as &Json)
    }
}

/// The members in the order the map keeps them: the order of the input with `serde_json`'s
/// `preserve_order` feature, and sorted by name without it.
impl Members for serde_json::Map<String, Value> {
    fn len(&self) -> usize {
        serde_json::Map::len(self)
    }

    fn get(&self, name: &str) -> Option<&Json> {
        serde_json::Map::get(self, name).map(|v| v as &Json)
    }

    fn each(&self, visit: &mut dyn FnMut(&str, &Json)) {
        for (name, value) in self {
            visit(name, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same value: `a` and `b` at the same address. Only the data part of a `&dyn` is
    /// compared, which two references to one value share.
    fn at_same_place(a: &Json, b: &Json) -> bool {
        std::ptr::eq(a as *const Json as *const u8, b as *const Json as *const u8)
    }

    /// Checks the laws of [`Input`], [`Elements`] and [`Members`] on `value` and on everything in it.
    fn laws(value: &Json) {
        match (value.view(), value.view()) {
            (View::Missing, _) => panic!("a value that is there gives Missing"),
            (View::Null, View::Null) => {}
            (View::Bool(a), View::Bool(b)) => assert_eq!(a, b),
            (View::Number(a), View::Number(b)) => {
                assert_eq!(a.lexeme(), b.lexeme());
                assert!(is_number(&a.lexeme()), "{a:?}");
            }
            (View::String(a), View::String(b)) => assert_eq!(a, b),
            (View::Array(a), View::Array(b)) => {
                assert_eq!(a.len(), b.len());
                for i in 0..a.len() {
                    laws(a.get(i).expect("an element below len"));
                }
                assert!(a.get(a.len()).is_none());
                assert_eq!(a.iter().count(), a.len());
            }
            (View::Object(a), View::Object(b)) => {
                let mut seen: Vec<(String, *const u8)> = Vec::new();
                a.each(&mut |name, member| {
                    let found = a.get(name).expect("get finds what each hands");
                    assert!(
                        at_same_place(found, member),
                        "get and each differ at {name}"
                    );
                    seen.push((name.to_owned(), member as *const Json as *const u8));
                    laws(member);
                });
                let mut again = Vec::new();
                b.each(&mut |name, member| {
                    again.push((name.to_owned(), member as *const Json as *const u8));
                });
                assert_eq!(seen, again, "each hands the members in another order");
                assert_eq!(seen.len(), a.len());
                let mut names: Vec<&str> = seen.iter().map(|(n, _)| n.as_str()).collect();
                names.sort_unstable();
                names.dedup();
                assert_eq!(names.len(), seen.len(), "a name twice");
                assert!(a.get("not a member of any object here").is_none());
            }
            (a, b) => panic!("{} then {}", a.kind(), b.kind()),
        }
    }

    #[test]
    fn every_input_here_keeps_the_laws() {
        let wide: Vec<String> = (0..20)
            .map(|i| format!("\"m{}\":[{i},-0]", 19 - i))
            .collect();
        for text in [
            r#"{"b":[1,2.50,-0,{"x":null}],"a":true,"c":"s","":{},"d":[]}"#.to_owned(),
            format!("{{{}}}", wide.join(",")),
            "[[],[[1e400]],false,null]".to_owned(),
        ] {
            let node: crate::json::Node = text.parse().unwrap();
            laws(&node);
            let value: Value = serde_json::from_str(&text).unwrap();
            laws(&value);
        }
        assert!(matches!(missing().view(), View::Missing));
    }

    #[test]
    fn json_numbers_are_told_from_other_text() {
        for ok in ["0", "-0", "1.50", "1e400", "-1E+2", "0.0e-0", "123"] {
            assert!(is_number(ok), "{ok}");
        }
        for bad in [
            "", "-", "01", "1.", ".5", "+1", "1e", "1e+", "0x1", "1.5.", "NaN", " 1",
        ] {
            assert!(!is_number(bad), "{bad}");
        }
    }
}
