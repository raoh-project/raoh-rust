//! Runs the cases of the Raoh Specification on raoh and writes a runner result
//! (spec/conformance.md), which raoh-verify checks against conformance/conformance.json.
//! scripts/conformance.sh runs both.

mod bind;
mod fixtures;
mod json;
mod value;

use bind::{Binder, Catalog};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Every feature the runner has a binding of, as the catalogue names it. A feature the catalogue
/// has and this does not is not bound, so the cases that need it are not run and the verifier
/// reports them.
const BINDS: &str = "
decoder.string decoder.int decoder.long decoder.float decoder.double decoder.decimal decoder.bool
decoder.list decoder.dict decoder.object decoder.strictObject decoder.strict decoder.nullable
decoder.enum decoder.enum.message decoder.literal decoder.literal.message decoder.discriminate
decoder.discriminateBy decoder.oneOf decoder.withDefault decoder.recover decoder.recoverWith
field.field field.optionalField field.optionalNullableField field.flat
encoder.string encoder.object property.propertyWithDefault
fixture.first fixture.square_side fixture.square fixture.area fixture.shift_add_10
fixture.shift_add_100 fixture.shift_add_1000 fixture.decimal_string fixture.even
fixture.ordered_period fixture.issue_count_plus_10 fixture.identity
operation.any.map operation.any.refine operation.any.flatMap
";

/// The operations bound on each kind of receiver; each with its `.message` facet where the
/// catalogue gives it one.
const OPERATIONS: &[(&str, &str)] = &[
    (
        "string",
        "trim toLowerCase toUpperCase normalize nonBlank minLength maxLength fixedLength oneOf \
         startsWith endsWith includes pattern email ipv4 ipv6 ip ulid cuid uuid url uri toInt \
         toLong toDecimal toBool iso8601 date time dateTime offsetDateTime",
    ),
    (
        "int32",
        "min max range positive negative nonNegative nonPositive oneOf multipleOf",
    ),
    (
        "int64",
        "min max range positive negative nonNegative nonPositive oneOf multipleOf",
    ),
    (
        "float32",
        "min max range positive negative nonNegative nonPositive oneOf",
    ),
    (
        "float64",
        "min max range positive negative nonNegative nonPositive oneOf",
    ),
    (
        "decimal",
        "min max range positive negative nonNegative nonPositive multipleOf scale",
    ),
    ("bool", "isTrue"),
    (
        "list",
        "nonempty minSize maxSize fixedSize unique contains containsAll toSet",
    ),
    ("map", "nonempty minSize maxSize fixedSize"),
    ("instant", "before after between"),
    ("date", "before after between"),
    ("time", "before after between"),
    ("datetime", "before after between"),
    ("offset_datetime", "before after between"),
];

fn binds() -> BTreeSet<String> {
    let mut out: BTreeSet<String> = BINDS.split_whitespace().map(str::to_owned).collect();
    for (kind, names) in OPERATIONS {
        for name in names.split_whitespace() {
            out.insert(format!("operation.{kind}.{name}"));
            out.insert(format!("operation.{kind}.{name}.message"));
        }
    }
    out
}

struct Args {
    spec: PathBuf,
    revision: String,
    manifest_digest: String,
    implementation_revision: String,
    messages: PathBuf,
    out: PathBuf,
}

fn args() -> Result<Args, String> {
    let mut spec = None;
    let mut revision = None;
    let mut digest = None;
    let mut implementation = None;
    let mut messages = None;
    let mut out = None;
    let mut given = std::env::args().skip(1);
    while let Some(flag) = given.next() {
        let value = given
            .next()
            .ok_or_else(|| format!("{flag} takes a value"))?;
        match flag.as_str() {
            "--spec" => spec = Some(PathBuf::from(value)),
            "--revision" => revision = Some(value),
            "--manifest-digest" => digest = Some(value),
            "--implementation-revision" => implementation = Some(value),
            "--messages" => messages = Some(PathBuf::from(value)),
            "--out" => out = Some(PathBuf::from(value)),
            other => return Err(format!("no flag {other}")),
        }
    }
    Ok(Args {
        spec: spec.ok_or("--spec is required")?,
        revision: revision.ok_or("--revision is required")?,
        manifest_digest: digest.ok_or("--manifest-digest is required")?,
        implementation_revision: implementation.ok_or("--implementation-revision is required")?,
        messages: messages.ok_or("--messages is required")?,
        out: out.ok_or("--out is required")?,
    })
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The features the catalogue has: its constructors, fields, operations on each kind of receiver,
/// encoders, properties and fixtures, with the `.message` facet of each form that takes one.
fn catalogue_features(operations: &Value, fixtures: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let takes_message = |args: &Value| {
        args.as_array()
            .is_some_and(|args| args.iter().any(|a| a["kind"] == "message"))
    };
    if let Some(constructors) = operations["constructors"].as_object() {
        for (name, c) in constructors {
            out.insert(format!("decoder.{name}"));
            if takes_message(&c["args"]) {
                out.insert(format!("decoder.{name}.message"));
            }
        }
    }
    for (section, prefix) in [
        ("fields", "field"),
        ("encoders", "encoder"),
        ("properties", "property"),
    ] {
        if let Some(entries) = operations[section].as_object() {
            for name in entries.keys() {
                out.insert(format!("{prefix}.{name}"));
            }
        }
    }
    for o in operations["operations"].as_array().into_iter().flatten() {
        let name = o["name"].as_str().unwrap_or_default();
        for receiver in o["receivers"].as_array().into_iter().flatten() {
            let receiver = receiver.as_str().unwrap_or_default();
            let kind = match receiver.split('<').next().unwrap_or_default() {
                "*" => "any",
                kind => kind,
            };
            out.insert(format!("operation.{kind}.{name}"));
            if takes_message(&o["args"]) {
                out.insert(format!("operation.{kind}.{name}.message"));
            }
        }
    }
    if let Some(fixtures) = fixtures.as_object() {
        for name in fixtures.keys() {
            out.insert(format!("fixture.{name}"));
        }
    }
    out
}

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_else(|| "the implementation panicked".into())
}

/// Runs a case whose features are all bound, and answers what it observed; `None` where the case
/// needs a feature the runner does not bind.
fn run_case(
    case: &Value,
    input: &raoh::json::Node,
    catalog: &Catalog,
    bound: &BTreeSet<String>,
) -> Option<Value> {
    let mut binder = Binder::new(catalog);
    let outcome = catch_unwind(AssertUnwindSafe(|| -> Result<Value, String> {
        if let Some(encoder) = case.get("encoder") {
            let ok = binder.encode(encoder, &case["value"])?;
            return Ok(json!({ "ok": ok }));
        }
        let (decoder, ty) = binder.decoder(&case["decoder"])?;
        Ok(match decoder.decode(input) {
            Ok(v) => json!({ "ok": value::observe(&ty, &v)? }),
            Err(issues) => json!({ "issues": value::write_issues(&issues) }),
        })
    }));
    if !binder.used.iter().all(|f| bound.contains(f)) {
        return None;
    }
    Some(match outcome {
        Ok(Ok(observed)) => observed,
        Ok(Err(error)) => json!({ "error": error }),
        Err(payload) => json!({ "error": panic_text(payload) }),
    })
}

fn catalogs(messages: &Path) -> Result<Value, String> {
    let mut out = Map::new();
    for locale in ["en", "ja"] {
        let path = messages.join(format!("{locale}.properties"));
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let catalogue = raoh::Messages::from_properties(&text)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let templates: Map<String, Value> = catalogue
            .templates()
            .map(|(k, v)| (k.to_owned(), Value::String(v.to_owned())))
            .collect();
        out.insert(locale.to_owned(), Value::Object(templates));
    }
    Ok(Value::Object(out))
}

fn run(args: Args) -> Result<(), String> {
    let version = read_json(&args.spec.join("specification.json"))?["version"]
        .as_str()
        .ok_or("specification.json has no version")?
        .to_owned();
    let operations = read_json(&args.spec.join("catalog/operations.json"))?;
    let fixtures = read_json(&args.spec.join("catalog/fixtures.json"))?;
    let catalog = Catalog::read(&operations)?;
    let binds = binds();
    let bound: BTreeSet<String> = catalogue_features(&operations, &fixtures)
        .into_iter()
        .filter(|f| binds.contains(f))
        .collect();

    // The runner's panics are reported as errors in the result, not on the terminal.
    std::panic::set_hook(Box::new(|_| {}));
    let mut results = Map::new();
    for profile in ["core", "encode"] {
        let dir = args.spec.join("suite").join(profile);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        for file in files {
            let text =
                std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let cases = json::parse(&text).map_err(|e| format!("{}: {e}", file.display()))?;
            for json::Case { case, input } in &cases {
                let id = case["id"].as_str().ok_or("a case has no id")?;
                if let Some(observed) = run_case(case, input, &catalog, &bound) {
                    results.insert(id.to_owned(), json!({ "observed": observed }));
                }
            }
        }
    }
    let _ = std::panic::take_hook();

    let result = json!({
        "format": "raoh-runner-result/v1",
        "specification": {
            "version": version,
            "revision": args.revision,
            "manifest_digest": args.manifest_digest,
        },
        "implementation": {
            "name": "raoh-rust",
            "version": raoh_version(),
            "revision": args.implementation_revision,
        },
        "environment": {
            "language": "rust",
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        },
        "bound_features": bound,
        "results": results,
        "catalogs": catalogs(&args.messages)?,
    });
    let text = serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?;
    std::fs::write(&args.out, text + "\n").map_err(|e| format!("{}: {e}", args.out.display()))
}

/// The version of raoh this runner was built with, from its Cargo.toml.
fn raoh_version() -> String {
    let manifest = include_str!("../../Cargo.toml");
    manifest
        .lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or("0.0.0")
        .to_owned()
}

fn main() -> ExitCode {
    match args().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
