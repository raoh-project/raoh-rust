# raoh

Rust port of [Raoh](https://github.com/kawasima/raoh), a decoder library for turning untyped
boundary input into typed domain values.

It is built around a parse-don't-validate approach:

- decode at the boundary
- keep invalid states out of the domain model
- return failures as values instead of panicking
- attach structured errors to precise paths

raoh reads JSON text, or a `serde_json::Value` an application already has, into domain values,
and when the input is wrong it reports every problem it found, each with the JSON Pointer of where
it was, instead of stopping at the first one.

```text
JSON text ---raoh::json::from_str---> Node ---\
serde_json::Value ------------------------------+--raoh--> domain values
                                                     \--> Issues (path, code, message, meta)
```

A domain type does not derive `Deserialize` and its fields stay private. The only way to get a
value of it from outside is through its decoder, so a value that exists has been checked.

## Installation

```toml
[dependencies]
raoh = "0.9.0"
```

Optional features:

| Feature               | What it does                                                          |
|-----------------------|-----------------------------------------------------------------------|
| `arbitrary_precision` | Turns on `serde_json`'s feature of that name, so that a `serde_json::Value`'s number keeps the text it was written with (see [Input](#input)) |
| `preserve_order`      | Turns on `serde_json`'s feature of that name, so that a `serde_json::Value`'s object keeps its members in the order written |

Both matter only to an application that decodes a `serde_json::Value`. Text read with
`raoh::json::from_str` keeps every number's text and every object's order without them.

The minimum supported Rust version is 1.88.

## Quick start

```rust
use raoh::json::prelude::*;

#[derive(Debug)]
pub struct Email(String);

#[derive(Debug)]
pub struct Age(u32);

#[derive(Debug)]
pub struct User {
    email: Email,
    age: Age,
}

fn email() -> impl Decoder<Json, Output = Email> {
    string().trim().lowercase().email().map(Email)
}

fn age() -> impl Decoder<Json, Output = Age> {
    u32().range(0..=150).map(Age)
}

fn user() -> impl Decoder<Json, Output = User> {
    object((
        field("email", email()),
        field("age", age()),
    ))
    .map(|(email, age)| User { email, age })
}

let issues = from_str(&user(), r#"{"email": "not an email", "age": 200}"#).unwrap_err();
assert_eq!(
    issues.to_json(),
    json!([
        {"path": "/email", "code": "invalid_format", "message": "not a valid email", "meta": {}},
        {"path": "/age", "code": "out_of_range", "message": "must be between 0 and 150",
         "meta": {"min": 0, "max": 150, "actual": 200}}
    ])
);
```

`Issues` implements `std::error::Error` and `serde::Serialize`, so it can be returned from a
handler or written out as the response body as it is.

## The model

### `Decoder`

```rust,ignore
pub trait Decoder<I: ?Sized> {
    type Output;
    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<Self::Output, Issues>;
    fn decode(&self, input: &I) -> Result<Self::Output, Issues>;
    // map, and_then, pipe, refine, with_default, recover, boxed
}
```

A decoder is a value that describes how to read an input. What it gives depends on the input
alone, so it can be reused and shared between threads. It may keep what it has worked out, such as
the matchers of a `pattern`, but nothing that changes a result.
Decoders compose like iterator adapters, and the composed type is hidden behind
`impl Decoder<Json, Output = T>`. Where a type has to be named, such as a recursive decoder or a
decoder kept in a struct field or a `static`, `.boxed()` turns it into a
`BoxDecoder<Json, T>`, which is `Send + Sync`.

The walk down the input uses a `Path` borrowed from the stack, so a successful decode allocates
nothing for paths. A path is copied out into a `Pointer` only when an issue is recorded.

### `Issue` and `Issues`

Each issue has:

- `path`: a JSON Pointer (RFC 6901), such as `/items/0/name`
- `code`: what kind of problem it is, such as `required` or `out_of_range`
- `message_key`: the code, or a refinement of it such as `out_of_range.minimum`
- `meta`: what else the code says, such as `min`, `max` and `actual`

An issue carries no sentence of its own. `issue.message()` writes one from the English catalogue,
and `issue.message_with(Messages::japanese())` from another. The only sentence an issue carries is
one its creator gave with `with_message(...)`, which every language then shows as written:

```rust
use raoh::{Issue, Messages};

let built_in = Issue::new("too_short").with_meta("min", 3);
assert_eq!(built_in.message_with(Messages::japanese()), "3文字以上で入力してください");

let custom = Issue::new("checksum").with_message("the check digit does not match");
assert_eq!(custom.message_with(Messages::japanese()), "the check digit does not match");
```

The codes, message keys and meta keys are those of the
[Raoh Specification](https://github.com/raoh-project/raoh-specification), which Raoh for Java and
Go follow too, so the same client-side handling works for all of them, and a catalogue written for
Raoh for Java resolves these issues too.

`meta` holds each value with its type, as a `MetaValue`: a `float` bound of 0.1 is the `f32`
nearest 0.1 and appears in a message as `0.1`, a decimal keeps its scale, and a date is a date.

`Issues` keeps them in the order they were found. `flatten()` groups the English messages by
path, and `to_json()` gives the `[{"path", "code", "message", "meta"}]` form, which is also what
`serde::Serialize` writes. `flatten_with` and `to_json_with` take another catalogue.

## Combining decoders

### Independent parts: tuples

A tuple of decoders over the same input is a decoder. It runs every part and reports the issues
of all of them. `object` wraps a tuple of fields, which also allows `.strict()`.

```rust
use raoh::json::prelude::*;

let point = object((field("x", i64()), field("y", i64()))).strict();

let issues = point.decode(&json!({"x": "1", "y": 2, "z": 3})).unwrap_err();
let paths: Vec<String> = issues.iter().map(|i| i.path().to_string()).collect();
assert_eq!(paths, ["/x", "/z"]);
```

### Dependent rules: `and_then`

A rule that relates several parts runs once the parts have decoded. The function returns
`Result<T, E>` where `E: Into<Issues>`, so a domain constructor returning `Result<Self, Issue>`
can be passed directly. An issue it returns is read as relative to where the decoder is.

```rust
use raoh::json::prelude::*;
use raoh::{Issue, Pointer};

#[derive(Debug)]
pub struct Period { start: i64, end: i64 }

impl Period {
    pub fn new(start: i64, end: i64) -> Result<Self, Issue> {
        if start > end {
            let at: Pointer = ["end"].into_iter().collect();
            return Err(Issue::new("invalid_value").with_message("must not be before start").at(at));
        }
        Ok(Self { start, end })
    }
}

let period = object((field("start", i64()), field("end", i64())))
    .and_then(|(start, end)| Period::new(start, end));
let trip = object((field("period", period),));

let issues = trip.decode(&json!({"period": {"start": 5, "end": 1}})).unwrap_err();
assert_eq!(issues.iter().next().unwrap().path().to_string(), "/period/end");
```

Tuples accumulate issues; `and_then` stops at the first failure, because the rule cannot be
checked before its parts exist.

### Decoder into decoder: `pipe`

`pipe` hands one decoder's output to another decoder as its input, at the same path. It is how a
decoder written for a domain type is composed with the boundary checks in front of it.

### Constraints without a new type: `refine`

`refine(predicate, code, message)` keeps the output type and adds a check, reported with the
message as a custom one.

## Built-in decoders

All of these live in `raoh::json` and come with `use raoh::json::prelude::*`. Missing or `null`
input is `required` for every one of them, and a value of another JSON type is `type_mismatch`.
The constraints of one decoder run in the order written, and the first to fail is reported.

`string()`: the transformations `trim`, `lowercase`, `uppercase` and `normalize(form)`; the
constraints `non_blank`, `min_length`, `max_length`, `length`, `starts_with`, `ends_with`,
`contains`, `one_of`, `pattern`, `email`, `ip`, `ipv4`, `ipv6`, `ulid` and `cuid`; and the
conversions `to_int`, `to_long`, `to_decimal`, `to_bool`, `uuid`, `uri`, `url`, `instant`, `date`,
`time`, `date_time`, `offset_date_time` and `parse::<T: FromStr>()`.

`i32()`, `i64()`, `u32()`, `u64()`: `min`, `max`, `range(a..=b)`, `positive`, `multiple_of`,
`one_of`, and for the signed ones `negative`, `non_negative` and `non_positive`. A number with a
fraction or an exponent is `type_mismatch`, and one the type cannot hold is `type_mismatch` under
the message key `type_mismatch.numeric_range`.

`f32()`, `f64()`: `min`, `max`, `range`, `positive`, `negative`, `non_negative`, `non_positive`,
`one_of`. A number is rounded to the type once, from its text; one beyond the type's range is
`type_mismatch.numeric_range`. The bounds use the float order of the value model, in which -0 is
below +0, so `negative` takes -0 and `non_negative` refuses it.

`decimal()`: a `Decimal` of any precision that keeps its scale, with the numeric constraints plus
`multiple_of` and `scale`. The bounds compare by value.

`bool()`: `is_true`, `is_false`.

The temporal conversions give this crate's `Date`, `Time`, `DateTime`, `OffsetDateTime` and
`Instant`, with years from -999999999 to 999999999, and take `before`, `after` and `between`. An
offset date-time keeps its offset and is compared by its instant: `09:00Z` and `10:00+01:00` are
different values, and neither is before the other.

Every one of them takes `.message("...")`, which gives the most recent constraint written before
it a custom message, or the reading of the value when there is none. Transformations such as
`trim` cannot fail and are passed over, so `string().trim().message("...")` gives the message to
the type check, and `string().min_length(3).trim().message("...")` gives it to `min_length`. A
conversion's message is that of either issue it gives, so in
`string().max_length(3).to_int().message("bad")` a text that is not an integer, or one out of
range, reads `bad`, and a text longer than 3 still reads as `max_length` writes it.

Whitespace, case and normalization follow Unicode 18.0.0 whatever Rust release the crate is built
with, through [notation-199x](https://github.com/raoh-project/notation-199x), which Raoh for Java
and Go use too: `trim` and `non_blank` use Unicode's `White_Space` (so U+3000 and U+00A0 are
whitespace and control characters are not), `lowercase` writes a final sigma where Unicode's
`Final_Sigma` condition holds, and lengths count Unicode scalar values. `pattern` takes the pattern
language of the specification, the one Souther has, and matches a value in one pass over it:
`\d`, `\w` and `\s` are ASCII, and a back reference or a lookaround is refused. `one_of`,
`discriminate` and `enum_of` sort by code point, and `enum_of` folds ASCII case only. An issue's
`meta` iterates in key order.

## Objects, lists and maps

- `field(name, d)`: a member that must be there
- `optional_field(name, d)`: `Option<T>`, `None` when the member is missing
- `presence_field(name, d)`: `Presence<T>`, one of `Absent`, `Null` or `Present(T)`
- `flat(d)`: the whole input read by `d`, as one value of the object
- `d.nullable()`: `Option<T>`, `None` when the value is `null`
- `d.list()`: `Vec<T>`, with `non_empty`, `min_size`, `max_size`, `size`, `unique`, `contains`,
  `contains_all` and `to_set`, which gives a `Set<T>`
- `dict(d)`: an `IndexMap<String, T>` from an object used as a map, in the order the `Value`
  keeps its keys, with `non_empty`, `min_size`, `max_size` and `size`

Each field of an `object` checks for itself that the input is an object. A required field of
anything else, `null` and a missing member included, is `type_mismatch` with `expected` `object`
at the field's own path; an optional field reads it as not having the member. `object(...).strict()`
also reports every member no field names as `unknown_field`, and `strict(d, names)` does the same
around any decoder, such as a `discriminate`. Strict decoders one inside another report a member
once, by the innermost one that does not know it.

`unique`, `contains`, `contains_all` and `to_set` compare elements by `Same`, the value model's
sameness, not by Rust's `Eq`: for floats -0 and +0 are two values and every NaN one, and decimals
of different scales differ. So `f64().list().to_set()` is a `Set<f64>` of each value once, though
`f64` has no `Eq` or `Hash`. A type of your own, such as the enum `enum_of` decodes into, takes
`same_by_eq!` for `Same`, and `meta_by_display!` for the message form `unique` and `contains`
write an element in.

A missing member and a `null` one are different inputs. `field("note", string().nullable())`
accepts `null` but reports a missing member as `required`, while `optional_field` accepts a
missing member but not `null`. `presence_field` tells all three apart, which is what a PATCH
request needs:

```rust
use raoh::json::prelude::*;

let nickname = object((presence_field("nickname", string()),));

assert_eq!(nickname.decode(&json!({})).unwrap(), (Presence::Absent,));
assert_eq!(nickname.decode(&json!({"nickname": null})).unwrap(), (Presence::Null,));
assert_eq!(
    nickname.decode(&json!({"nickname": "Ken"})).unwrap(),
    (Presence::Present("Ken".to_string()),),
);
```

## Choices

- `enum_of([("red", Color::Red), ...])`: a string naming one of the values, ignoring ASCII case;
  `.using(string().trim())` reads the string with another decoder
- `literal("v1")`: exactly that string, with `.using(...)` too
- `one_of((a, b, ...))`: the first alternative that decodes, or `one_of_failed` with each
  alternative's issues in `meta.candidates`
- `discriminate("type", (variant("a", da), variant("b", db), ...))`: the variant the member
  `type` names
- `discriminate_by("type", tag, variants)`: the variant the decoder `tag` names, which reads the
  whole input, so the tag can be trimmed or lower-cased first

The alternatives of `one_of` and the variants of `discriminate` can also be a `Vec`, and the fields
of an `object` a `Vec` of boxed fields, for a decoder whose parts are decided at run time.

```rust
use raoh::json::prelude::*;

#[derive(Debug, PartialEq)]
pub enum Contact {
    Email(String),
    Phone(String),
}

fn contact() -> impl Decoder<Json, Output = Contact> {
    discriminate(
        "type",
        (
            variant("email", object((field("address", string().email()),)).map(|(a,)| Contact::Email(a))),
            variant("phone", object((field("number", string()),)).map(|(n,)| Contact::Phone(n))),
        ),
    )
}

let issues = contact().decode(&json!({"type": "fax"})).unwrap_err();
let issue = issues.iter().next().unwrap();
assert_eq!(issue.path().to_string(), "/type");
assert_eq!(issue.meta()["allowed"].to_string(), "[email, phone]");
```

## Defaults and recovery

`with_default(v)` gives `v` when the input is missing or `null`, looked at before the decoder
runs, and otherwise gives what the decoder gives: an object that is there but lacks a member is
reported, not defaulted. `recover(v)` gives `v` whatever the problem was, and `recover_with(f)`
what `f` makes of the issues.

## Recursive structures

A decoder that refers to itself names its own type, so it returns a `BoxDecoder` and refers to
itself through `lazy`:

```rust
use raoh::json::prelude::*;

#[derive(Debug)]
pub struct Category {
    name: String,
    children: Vec<Category>,
}

fn category() -> BoxDecoder<Json, Category> {
    object((
        field("name", string().non_blank()),
        field("children", lazy(category).list()),
    ))
    .map(|(name, children)| Category { name, children })
    .boxed()
}

let tree = category().decode(&json!({"name": "a", "children": [{"name": "b", "children": []}]}));
assert!(tree.is_ok());
```

## Messages in other languages

`Messages::english()` and `Messages::japanese()` hold the catalogues of the Raoh Specification,
word for word, under a layer of this crate's own with a template for `invalid_format.json`. A catalogue is a stack of layers, as a locale's
`.properties` file sits over its parent's: `japanese()` is a layer over `english()`,
`with_overrides` puts a layer of your own on top, and `falling_back_to` puts another catalogue
beneath. An issue is looked up one layer at a time, by message key and then by code, so a layer
that translates only `invalid_format` wins over the refined `invalid_format.email` beneath it, as
in Raoh for Java. `Messages::from_properties` reads a `.properties` file as Java's
`Properties.load` does, `\uXXXX` escapes included, so an existing Raoh for Java catalogue can be
used as it is. A template's `{name}` placeholders are filled with the message forms of `meta`,
such as `1.0E7` for a float and `1E+3` for a decimal; a placeholder with no entry stays as it is
written.

```rust
use raoh::json::prelude::*;
use raoh::Messages;

let issues = string().min_length(3).decode(&json!("ab")).unwrap_err();
assert_eq!(issues.flatten_with(Messages::japanese())[""], ["3文字以上で入力してください"]);

let ours = Messages::english().with_overrides([("too_short", "{min} characters or more")]);
assert_eq!(issues.flatten_with(&ours)[""], ["3 characters or more"]);
```

Any `Fn(&Issue) -> String` is a resolver too.

## Encoders

`raoh::encode` writes values back as JSON. `encode::object::<T>()` builds an object encoder member
by member, with `property(name, getter, encoder)` and `property_with_default(name, getter,
encoder, default)`, whose getter gives an `Option` and whose default is written for `None`. Any
`Fn(&T) -> Value` is an encoder too.

## Input

Every decoder reads a `Json`, which is `dyn raoh::json::Input`: a value of the specification's
input model, in which a number keeps the text it was written with. Two types are one.

`raoh::json::Node` is what `raoh::json::from_str` reads JSON text into, and what `str::parse`
gives. Every number keeps its text, held inside the node when it is up to 22 bytes long, which
almost every number is, so reading a number allocates nothing. Every object keeps its members in
the order written. A name written twice in one object, and arrays and objects nested more than
128 deep, are `invalid_format`, as is text that is not JSON.

`serde_json::Value` is one too, for an application that already has one, from a web framework
for instance. Its numbers keep their text only with `serde_json`'s `arbitrary_precision` feature,
which this crate's feature of the same name turns on, and then `serde_json` holds every number as
a `String` of its own. Without it a number is the `i64`, `u64` or `f64` that `serde_json` read,
and a decoder reads the text that value is written as: `1.50` as `1.5`, `-0` as `-0.0`. Even with
the feature, `serde_json`'s parser reads an integer as an `i64` or `u64` first, so the text `-0`
becomes `0`.

## Numbers

A decoder reads a number from its text, as the input model has it: `int` takes `1` and refuses
`1.0` and `1e2`, `decimal` reads `1.50` with scale 2, `double` reads `-0` and `-0.0` as -0, and
`float` rounds the text to `f32` once.

Reading JSON text with `from_str` takes about as long as `serde_json::from_str` without
`arbitrary_precision`, which loses that text, and less than half as long as with it. On an array
of 1000 doubles, reading and decoding took 35 µs with `from_str`, 29 µs with `serde_json` and the
feature off, and 91 µs with it on.

## The Raoh Specification

The major and minor version of this crate are the version of the specification it follows, and
the patch part is the crate's own: every 0.9.x follows the Raoh Specification 0.9. This crate is checked against the [Raoh Specification](https://github.com/raoh-project/raoh-specification)
by `scripts/conformance.sh`, which runs every case of the revision `conformance/spec.lock` pins
through the runner in `conformance/` and has the specification's `raoh-verify` compare what it
gave with what each case expects. At the pinned revision:

Raoh Specification 0.9 — core: conformant; encode: conformant; messages-en: conformant;
messages-ja: conformant.

The runner reads each suite file into `Node`s and gives each case's input to the decoder as the
node it is.

The specification does not cover the API. Where this crate's API differs from Raoh for Java's:

- Combining is done with tuples and `object`, not `combine`. There is no `nested`, because a
  member is handed to its decoder as a `Value` already.
- `flatMap` is `and_then`, and there are no `Result`, `Ok` or `Err` types of its own: decoding
  gives `std::result::Result<T, Issues>`.
- There is no domain construction guard (`raoh-gsh`). Private fields and module privacy stop a
  domain value from being built anywhere but its own module.

## Development

```sh
cargo test --all-features
scripts/conformance.sh
```

`scripts/conformance.sh` needs git, jq and Go, which builds the specification's `raoh-verify`. It
clones the specification at the pinned revision, or uses the checkout `RAOH_SPECIFICATION_DIR`
names, and writes `conformance/target/runner-result.json` and
`conformance/target/conformance-report.json`.

## License

Apache License 2.0
