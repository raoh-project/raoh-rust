//! Translates the forms of a case (spec/decoder-language.md) into raoh's decoders and encoders.

use crate::fixtures;
use crate::value::{Ty, V, read};
use raoh::encode::{self, Encoder};
use raoh::json::{BoolDecoder, discriminate, discriminate_by, enum_of, literal, variant};
use raoh::json::{
    BoxFieldSet, DecimalDecoder, Dict, F32Decoder, F64Decoder, FieldSet, IntDecoder,
    JsonDecoderExt, ListDecoder, NormalizationForm, StringDecoder, TemporalDecoder, UriDecoder,
    UuidDecoder, Variant,
};
use raoh::{BoxDecoder, Date, DateTime, Decimal, Decoder, Instant, MetaValue, OffsetDateTime};
use raoh::{Time, one_of};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

type Dec = BoxDecoder<raoh::json::Json, V>;

/// An argument of a form, as catalog/operations.json declares it.
#[derive(Clone, Debug)]
pub struct ArgDef {
    pub kind: String,
    pub optional: bool,
}

/// What the runner reads from catalog/operations.json: the arguments of each form.
#[derive(Debug, Default)]
pub struct Catalog {
    pub constructors: BTreeMap<String, Vec<ArgDef>>,
    pub operations: BTreeMap<String, Vec<ArgDef>>,
}

impl Catalog {
    pub fn read(operations: &Value) -> Result<Self, String> {
        let defs = |args: Option<&Value>| -> Vec<ArgDef> {
            args.and_then(Value::as_array)
                .map(|args| {
                    args.iter()
                        .map(|a| ArgDef {
                            kind: a["kind"].as_str().unwrap_or_default().to_owned(),
                            optional: a["optional"].as_bool().unwrap_or(false),
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut catalog = Catalog::default();
        for (name, c) in operations["constructors"]
            .as_object()
            .ok_or("no constructors")?
        {
            catalog
                .constructors
                .insert(name.clone(), defs(c.get("args")));
        }
        for o in operations["operations"].as_array().ok_or("no operations")? {
            let name = o["name"].as_str().ok_or("an operation has no name")?;
            catalog
                .operations
                .entry(name.to_owned())
                .or_insert_with(|| defs(o.get("args")));
        }
        Ok(catalog)
    }
}

/// A decoder as a form builds it: one of raoh's typed decoders, to which the operations of its
/// type apply, or a decoder whose type has been erased.
enum Built {
    Str(StringDecoder),
    I32(IntDecoder<i32>),
    I64(IntDecoder<i64>),
    F32(F32Decoder),
    F64(F64Decoder),
    Dec(DecimalDecoder),
    Bool(BoolDecoder),
    Date(TemporalDecoder<Date>),
    Time(TemporalDecoder<Time>),
    DateTime(TemporalDecoder<DateTime>),
    OffsetDateTime(TemporalDecoder<OffsetDateTime>),
    Instant(TemporalDecoder<Instant>),
    Uuid(UuidDecoder),
    Uri(UriDecoder),
    List(ListDecoder<Dec>, Ty),
    Dict(Dict<Dec>, Ty),
    Any(Dec, Ty),
}

impl Built {
    fn ty(&self) -> Ty {
        match self {
            Built::Str(_) => Ty::String,
            Built::I32(_) => Ty::Int32,
            Built::I64(_) => Ty::Int64,
            Built::F32(_) => Ty::Float32,
            Built::F64(_) => Ty::Float64,
            Built::Dec(_) => Ty::Decimal,
            Built::Bool(_) => Ty::Bool,
            Built::Date(_) => Ty::Date,
            Built::Time(_) => Ty::Time,
            Built::DateTime(_) => Ty::DateTime,
            Built::OffsetDateTime(_) => Ty::OffsetDateTime,
            Built::Instant(_) => Ty::Instant,
            Built::Uuid(_) => Ty::Uuid,
            Built::Uri(_) => Ty::Uri,
            Built::List(_, element) => Ty::List(Box::new(element.clone())),
            Built::Dict(_, element) => Ty::Map(Box::new(element.clone())),
            Built::Any(_, ty) => ty.clone(),
        }
    }

    /// The decoder with its output held as a [`V`].
    fn erase(self) -> (Dec, Ty) {
        let ty = self.ty();
        let decoder = match self {
            Built::Str(d) => d.map(V::scalar).boxed(),
            Built::I32(d) => d.map(V::scalar).boxed(),
            Built::I64(d) => d.map(V::scalar).boxed(),
            Built::F32(d) => d.map(V::scalar).boxed(),
            Built::F64(d) => d.map(V::scalar).boxed(),
            Built::Dec(d) => d.map(V::scalar).boxed(),
            Built::Bool(d) => d.map(V::scalar).boxed(),
            Built::Date(d) => d.map(V::scalar).boxed(),
            Built::Time(d) => d.map(V::scalar).boxed(),
            Built::DateTime(d) => d.map(V::scalar).boxed(),
            Built::OffsetDateTime(d) => d.map(V::scalar).boxed(),
            Built::Instant(d) => d.map(V::scalar).boxed(),
            Built::Uuid(d) => d.map(V::scalar).boxed(),
            Built::Uri(d) => d.map(V::scalar).boxed(),
            Built::List(d, _) => d.map(V::List).boxed(),
            Built::Dict(d, _) => d.map(V::Map).boxed(),
            Built::Any(d, _) => d,
        };
        (decoder, ty)
    }

    fn with_message(self, message: String) -> Result<Built, String> {
        Ok(match self {
            Built::Str(d) => Built::Str(d.message(message)),
            Built::I32(d) => Built::I32(d.message(message)),
            Built::I64(d) => Built::I64(d.message(message)),
            Built::F32(d) => Built::F32(d.message(message)),
            Built::F64(d) => Built::F64(d.message(message)),
            Built::Dec(d) => Built::Dec(d.message(message)),
            Built::Bool(d) => Built::Bool(d.message(message)),
            Built::Date(d) => Built::Date(d.message(message)),
            Built::Time(d) => Built::Time(d.message(message)),
            Built::DateTime(d) => Built::DateTime(d.message(message)),
            Built::OffsetDateTime(d) => Built::OffsetDateTime(d.message(message)),
            Built::Instant(d) => Built::Instant(d.message(message)),
            Built::Uuid(d) => Built::Uuid(d.message(message)),
            Built::Uri(d) => Built::Uri(d.message(message)),
            Built::List(d, t) => Built::List(d.message(message), t),
            Built::Dict(d, t) => Built::Dict(d.message(message), t),
            Built::Any(_, ty) => return Err(format!("no message can be given to a {ty:?}")),
        })
    }
}

/// Builds the decoders and encoders of one case, recording every feature they use.
pub struct Binder<'a> {
    pub used: BTreeSet<String>,
    catalog: &'a Catalog,
}

fn array(form: &Value) -> Result<&[Value], String> {
    form.as_array()
        .map(Vec::as_slice)
        .filter(|f| f.first().is_some_and(Value::is_string))
        .ok_or_else(|| format!("{form} is not a form"))
}

fn text(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .ok_or_else(|| format!("{value} is not a string"))
}

fn scalar<T>(v: V, get: impl Fn(MetaValue) -> Option<T>) -> Result<T, String> {
    match v {
        V::Scalar(m) => get(m.clone()).ok_or_else(|| format!("{m:?} is not of the type needed")),
        other => Err(format!("{other:?} is not a scalar")),
    }
}

fn as_i32(m: MetaValue) -> Option<i32> {
    m.as_i64().and_then(|n| i32::try_from(n).ok())
}

fn as_i64(m: MetaValue) -> Option<i64> {
    m.as_i64()
}

fn as_f32(m: MetaValue) -> Option<f32> {
    match m {
        MetaValue::Float32(f) => Some(f),
        _ => None,
    }
}

fn as_f64(m: MetaValue) -> Option<f64> {
    match m {
        MetaValue::Float64(f) => Some(f),
        _ => None,
    }
}

fn as_decimal(m: MetaValue) -> Option<Decimal> {
    match m {
        MetaValue::Decimal(d) => Some(d),
        _ => None,
    }
}

fn as_string(m: MetaValue) -> Option<String> {
    match m {
        MetaValue::String(s) => Some(s),
        _ => None,
    }
}

macro_rules! temporal_getter {
    ($name:ident, $t:ty, $variant:ident) => {
        fn $name(m: MetaValue) -> Option<$t> {
            match m {
                MetaValue::$variant(v) => Some(v),
                _ => None,
            }
        }
    };
}

temporal_getter!(as_date, Date, Date);
temporal_getter!(as_time, Time, Time);
temporal_getter!(as_date_time, DateTime, DateTime);
temporal_getter!(as_offset_date_time, OffsetDateTime, OffsetDateTime);
temporal_getter!(as_instant, Instant, Instant);

impl<'a> Binder<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self {
            used: BTreeSet::new(),
            catalog,
        }
    }

    fn use_feature(&mut self, feature: String) {
        self.used.insert(feature);
    }

    /// The decoder `form` names, with its output held as a [`V`], and its result type.
    pub fn decoder(&mut self, form: &Value) -> Result<(Dec, Ty), String> {
        Ok(self.built(form)?.erase())
    }

    fn built(&mut self, form: &Value) -> Result<Built, String> {
        let form = array(form)?;
        let name = text(&form[0])?;
        self.use_feature(format!("decoder.{name}"));
        let defs = self
            .catalog
            .constructors
            .get(name)
            .ok_or_else(|| format!("no constructor {name}"))?;
        let required = defs.iter().filter(|d| !d.optional).count();
        if form.len() < 1 + required {
            return Err(format!("{name} takes {required} arguments"));
        }
        let args = &form[1..1 + required];
        let mut rest = &form[1 + required..];
        let mut message = None;
        if defs.iter().any(|d| d.kind == "message")
            && let Some(Value::String(m)) = rest.first()
        {
            self.use_feature(format!("decoder.{name}.message"));
            message = Some(m.clone());
            rest = &rest[1..];
        }
        let mut built = self.construct(name, args, message)?;
        for operation in rest {
            built = self.operation(built, operation)?;
        }
        Ok(built)
    }

    /// A string decoder, which `enum` and `literal` read their string with.
    fn string_decoder(&mut self, form: &Value) -> Result<StringDecoder, String> {
        match self.built(form)? {
            Built::Str(d) => Ok(d),
            other => Err(format!("{:?} is not a string decoder", other.ty())),
        }
    }

    fn construct(
        &mut self,
        name: &str,
        args: &[Value],
        message: Option<String>,
    ) -> Result<Built, String> {
        use raoh::json::{bool, decimal, dict, f32, f64, i32, i64, object, string};
        Ok(match name {
            "string" => Built::Str(string()),
            "int" => Built::I32(i32()),
            "long" => Built::I64(i64()),
            "float" => Built::F32(f32()),
            "double" => Built::F64(f64()),
            "decimal" => Built::Dec(decimal()),
            "bool" => Built::Bool(bool()),
            "list" => {
                let (element, ty) = self.decoder(&args[0])?;
                Built::List(element.list(), ty)
            }
            "dict" => {
                let (value, ty) = self.decoder(&args[0])?;
                Built::Dict(dict(value), ty)
            }
            "object" | "strictObject" => {
                let (fields, tys) = self.fields(&args[0])?;
                let ty = Ty::Product(tys);
                let decoder = if name == "object" {
                    object(fields).map(V::Product).boxed()
                } else {
                    object(fields).strict().map(V::Product).boxed()
                };
                Built::Any(decoder, ty)
            }
            "strict" => {
                let (inner, ty) = self.decoder(&args[0])?;
                let known = read(&Ty::List(Box::new(Ty::String)), &args[1])?;
                let V::List(known) = known else {
                    return Err("strict takes a list of names".into());
                };
                let known: Vec<String> = known
                    .into_iter()
                    .map(V::into_string)
                    .collect::<Result<_, _>>()?;
                Built::Any(raoh::json::strict(inner, known).boxed(), ty)
            }
            "nullable" => {
                let (inner, ty) = self.decoder(&args[0])?;
                Built::Any(
                    inner.nullable().map(|o| V::Opt(o.map(Box::new))).boxed(),
                    Ty::Nullable(Box::new(ty)),
                )
            }
            "enum" => {
                let symbols: Vec<String> = args[0]
                    .as_array()
                    .ok_or("enum takes a list of symbols")?
                    .iter()
                    .map(|s| text(s).map(str::to_owned))
                    .collect::<Result<_, _>>()?;
                let string = self.string_decoder(&args[1])?;
                let mut decoder =
                    enum_of(symbols.iter().map(|s| (s.as_str(), V::scalar(s.as_str()))))
                        .using(string);
                if let Some(message) = message {
                    decoder = decoder.message(message);
                }
                Built::Any(decoder.boxed(), Ty::Symbol(symbols))
            }
            "literal" => {
                let expected = text(&args[0])?.to_owned();
                let string = self.string_decoder(&args[1])?;
                let mut decoder = literal(expected).using(string);
                if let Some(message) = message {
                    decoder = decoder.message(message);
                }
                Built::Any(decoder.map(V::scalar).boxed(), Ty::String)
            }
            "discriminate" => {
                let field = text(&args[0])?.to_owned();
                let (variants, ty) = self.variants(&args[1])?;
                Built::Any(discriminate(field, variants).boxed(), ty)
            }
            "discriminateBy" => {
                let field = text(&args[0])?.to_owned();
                let (tag, tag_ty) = self.decoder(&args[1])?;
                if tag_ty != Ty::String {
                    return Err(format!("the tag decoder gives {tag_ty:?}, not a string"));
                }
                let tag = tag.map(|v: V| v.into_string().unwrap_or_default());
                let (variants, ty) = self.variants(&args[2])?;
                Built::Any(discriminate_by(field, tag, variants).boxed(), ty)
            }
            "oneOf" => {
                let mut candidates = Vec::new();
                let mut ty = None;
                for form in args[0].as_array().ok_or("oneOf takes a list of decoders")? {
                    let (decoder, t) = self.decoder(form)?;
                    ty.get_or_insert(t);
                    candidates.push(decoder);
                }
                Built::Any(
                    one_of(candidates).boxed(),
                    ty.ok_or("oneOf takes at least one decoder")?,
                )
            }
            "withDefault" => {
                let (inner, ty) = self.decoder(&args[0])?;
                let default = read(&ty, &args[1])?;
                Built::Any(inner.with_default(default).boxed(), ty)
            }
            "recover" => {
                let (inner, ty) = self.decoder(&args[0])?;
                let fallback = read(&ty, &args[1])?;
                Built::Any(inner.recover(fallback).boxed(), ty)
            }
            "recoverWith" => {
                let (inner, ty) = self.decoder(&args[0])?;
                let fixture = text(&args[1])?;
                self.use_feature(format!("fixture.{fixture}"));
                if fixture != "issue_count_plus_10" {
                    return Err(format!("no recover fixture {fixture}"));
                }
                Built::Any(
                    inner.recover_with(fixtures::issue_count_plus_10).boxed(),
                    ty,
                )
            }
            other => return Err(format!("no constructor {other}")),
        })
    }

    /// The fields of an `object`, each giving a [`V`], and their types.
    fn fields(&mut self, forms: &Value) -> Result<(Vec<BoxFieldSet<V>>, Vec<Ty>), String> {
        use raoh::json::{field, flat, optional_field, presence_field};
        let mut fields = Vec::new();
        let mut tys = Vec::new();
        for form in forms.as_array().ok_or("an object takes a list of fields")? {
            let form = array(form)?;
            let kind = text(&form[0])?;
            self.use_feature(format!("field.{kind}"));
            let (fields_of, ty) = match kind {
                "flat" => {
                    let (decoder, ty) = self.decoder(&form[1])?;
                    (flat(decoder).boxed(), ty)
                }
                _ => {
                    let name = text(&form[1])?.to_owned();
                    let (decoder, ty) = self.decoder(&form[2])?;
                    match kind {
                        "field" => (field(name, decoder).boxed(), ty),
                        "optionalField" => (
                            optional_field(name, decoder)
                                .map(|o| V::Opt(o.map(Box::new)))
                                .boxed(),
                            Ty::Optional(Box::new(ty)),
                        ),
                        "optionalNullableField" => (
                            presence_field(name, decoder)
                                .map(|p| V::Presence(p.map(Box::new)))
                                .boxed(),
                            Ty::Presence(Box::new(ty)),
                        ),
                        other => return Err(format!("no field kind {other}")),
                    }
                }
            };
            fields.push(fields_of);
            tys.push(ty);
        }
        Ok((fields, tys))
    }

    fn variants(&mut self, forms: &Value) -> Result<(Vec<Variant<Dec>>, Ty), String> {
        let mut variants = Vec::new();
        let mut ty = None;
        for (tag, form) in forms.as_object().ok_or("variants are an object")? {
            let (decoder, t) = self.decoder(form)?;
            ty.get_or_insert(t);
            variants.push(variant(tag.clone(), decoder));
        }
        Ok((variants, ty.ok_or("there is no variant")?))
    }

    /// Splits an operation's arguments into its value arguments and its message.
    fn split<'v>(
        &self,
        name: &str,
        given: &'v [Value],
    ) -> Result<(Vec<&'v Value>, Option<String>), String> {
        let defs = self
            .catalog
            .operations
            .get(name)
            .ok_or_else(|| format!("no operation {name}"))?;
        let mut values = Vec::new();
        let mut message = None;
        let mut given = given.iter();
        let mut next = given.next();
        for def in defs {
            match def.kind.as_str() {
                "message" => {
                    if let Some(Value::String(m)) = next {
                        message = Some(m.clone());
                        next = given.next();
                    }
                }
                _ => match next {
                    Some(v) => {
                        values.push(v);
                        next = given.next();
                    }
                    None if def.optional => {}
                    None => return Err(format!("{name} is missing an argument")),
                },
            }
        }
        if next.is_some() {
            return Err(format!("{name} has too many arguments"));
        }
        Ok((values, message))
    }

    fn operation(&mut self, built: Built, form: &Value) -> Result<Built, String> {
        let form = array(form)?;
        let name = text(&form[0])?;
        let generic = matches!(name, "map" | "refine" | "flatMap");
        let ty = built.ty();
        let kind = if generic { "any" } else { ty.kind() };
        self.use_feature(format!("operation.{kind}.{name}"));
        let (values, message) = self.split(name, &form[1..])?;
        if message.is_some() {
            self.use_feature(format!("operation.{kind}.{name}.message"));
        }
        let built = if generic {
            self.generic(built, name, text(values[0])?)?
        } else {
            self.typed(built, name, &values)?
        };
        match message {
            Some(message) => built.with_message(message),
            None => Ok(built),
        }
    }

    fn generic(&mut self, built: Built, name: &str, fixture: &str) -> Result<Built, String> {
        self.use_feature(format!("fixture.{fixture}"));
        let (decoder, ty) = built.erase();
        Ok(match (name, fixture) {
            ("map", _) => {
                let (out, f) = fixtures::map(fixture, &ty)?;
                Built::Any(decoder.map(f).boxed(), out)
            }
            ("refine", "even") => Built::Any(decoder.and_then(fixtures::even).boxed(), ty),
            ("flatMap", "ordered_period") => {
                Built::Any(decoder.and_then(fixtures::ordered_period).boxed(), ty)
            }
            _ => return Err(format!("no {name} fixture {fixture}")),
        })
    }

    fn typed(&mut self, built: Built, name: &str, args: &[&Value]) -> Result<Built, String> {
        let arg = |i: usize, ty: &Ty| -> Result<V, String> {
            read(
                ty,
                args.get(i)
                    .ok_or_else(|| format!("{name} needs argument {i}"))?,
            )
        };
        let usize_arg = |i: usize| -> Result<usize, String> {
            let n = scalar(arg(i, &Ty::Int32)?, as_i32)?;
            usize::try_from(n).map_err(|e| e.to_string())
        };
        let strings = |i: usize| -> Result<Vec<String>, String> {
            match arg(i, &Ty::List(Box::new(Ty::String)))? {
                V::List(items) => items.into_iter().map(V::into_string).collect(),
                _ => Err("not a list".into()),
            }
        };
        let receiver = built.ty();
        let no = || format!("no operation {name} on {receiver:?}");
        Ok(match built {
            Built::Str(d) => match name {
                "trim" => Built::Str(d.trim()),
                "toLowerCase" => Built::Str(d.lowercase()),
                "toUpperCase" => Built::Str(d.uppercase()),
                "normalize" => {
                    let form = match args.first() {
                        Some(form) => text(form)?,
                        None => "NFC",
                    };
                    let form = match form {
                        "NFC" => NormalizationForm::Nfc,
                        "NFD" => NormalizationForm::Nfd,
                        "NFKC" => NormalizationForm::Nfkc,
                        "NFKD" => NormalizationForm::Nfkd,
                        other => return Err(format!("no normalization form {other}")),
                    };
                    Built::Str(d.normalize(form))
                }
                "nonBlank" => Built::Str(d.non_blank()),
                "minLength" => Built::Str(d.min_length(usize_arg(0)?)),
                "maxLength" => Built::Str(d.max_length(usize_arg(0)?)),
                "fixedLength" => Built::Str(d.length(usize_arg(0)?)),
                "oneOf" => Built::Str(d.one_of(strings(0)?)),
                "startsWith" => Built::Str(d.starts_with(scalar(arg(0, &Ty::String)?, as_string)?)),
                "endsWith" => Built::Str(d.ends_with(scalar(arg(0, &Ty::String)?, as_string)?)),
                "includes" => Built::Str(d.contains(scalar(arg(0, &Ty::String)?, as_string)?)),
                "pattern" => Built::Str(d.pattern(&scalar(arg(0, &Ty::String)?, as_string)?)),
                "email" => Built::Str(d.email()),
                "ipv4" => Built::Str(d.ipv4()),
                "ipv6" => Built::Str(d.ipv6()),
                "ip" => Built::Str(d.ip()),
                "ulid" => Built::Str(d.ulid()),
                "cuid" => Built::Str(d.cuid()),
                "uuid" => Built::Uuid(d.uuid()),
                "url" => Built::Uri(d.url()),
                "uri" => Built::Uri(d.uri()),
                "toInt" => Built::I32(d.to_int()),
                "toLong" => Built::I64(d.to_long()),
                "toDecimal" => Built::Dec(d.to_decimal()),
                "toBool" => Built::Bool(d.to_bool()),
                "iso8601" => Built::Instant(d.instant()),
                "date" => Built::Date(d.date()),
                "time" => Built::Time(d.time()),
                "dateTime" => Built::DateTime(d.date_time()),
                "offsetDateTime" => Built::OffsetDateTime(d.offset_date_time()),
                _ => return Err(no()),
            },
            Built::I32(d) => Built::I32(integer(d, name, &arg, Ty::Int32, as_i32).ok_or_else(no)??),
            Built::I64(d) => Built::I64(integer(d, name, &arg, Ty::Int64, as_i64).ok_or_else(no)??),
            Built::F32(d) => match name {
                "min" => Built::F32(d.min(scalar(arg(0, &Ty::Float32)?, as_f32)?)),
                "max" => Built::F32(d.max(scalar(arg(0, &Ty::Float32)?, as_f32)?)),
                "range" => Built::F32(d.range(
                    scalar(arg(0, &Ty::Float32)?, as_f32)?..=scalar(arg(1, &Ty::Float32)?, as_f32)?,
                )),
                "positive" => Built::F32(d.positive()),
                "negative" => Built::F32(d.negative()),
                "nonNegative" => Built::F32(d.non_negative()),
                "nonPositive" => Built::F32(d.non_positive()),
                "oneOf" => {
                    Built::F32(d.one_of(floats(arg(0, &Ty::List(Box::new(Ty::Float32)))?, as_f32)?))
                }
                _ => return Err(no()),
            },
            Built::F64(d) => match name {
                "min" => Built::F64(d.min(scalar(arg(0, &Ty::Float64)?, as_f64)?)),
                "max" => Built::F64(d.max(scalar(arg(0, &Ty::Float64)?, as_f64)?)),
                "range" => Built::F64(d.range(
                    scalar(arg(0, &Ty::Float64)?, as_f64)?..=scalar(arg(1, &Ty::Float64)?, as_f64)?,
                )),
                "positive" => Built::F64(d.positive()),
                "negative" => Built::F64(d.negative()),
                "nonNegative" => Built::F64(d.non_negative()),
                "nonPositive" => Built::F64(d.non_positive()),
                "oneOf" => {
                    Built::F64(d.one_of(floats(arg(0, &Ty::List(Box::new(Ty::Float64)))?, as_f64)?))
                }
                _ => return Err(no()),
            },
            Built::Dec(d) => match name {
                "min" => Built::Dec(d.min(scalar(arg(0, &Ty::Decimal)?, as_decimal)?)),
                "max" => Built::Dec(d.max(scalar(arg(0, &Ty::Decimal)?, as_decimal)?)),
                "range" => Built::Dec(d.range(
                    scalar(arg(0, &Ty::Decimal)?, as_decimal)?
                        ..=scalar(arg(1, &Ty::Decimal)?, as_decimal)?,
                )),
                "positive" => Built::Dec(d.positive()),
                "negative" => Built::Dec(d.negative()),
                "nonNegative" => Built::Dec(d.non_negative()),
                "nonPositive" => Built::Dec(d.non_positive()),
                "multipleOf" => {
                    Built::Dec(d.multiple_of(scalar(arg(0, &Ty::Decimal)?, as_decimal)?))
                }
                "scale" => Built::Dec(d.scale(scalar(arg(0, &Ty::Int32)?, as_i32)?)),
                _ => return Err(no()),
            },
            Built::Bool(d) => match name {
                "isTrue" => Built::Bool(d.is_true()),
                _ => return Err(no()),
            },
            Built::Date(d) => Built::Date(temporal(d, name, &arg, Ty::Date, as_date)?),
            Built::Time(d) => Built::Time(temporal(d, name, &arg, Ty::Time, as_time)?),
            Built::DateTime(d) => {
                Built::DateTime(temporal(d, name, &arg, Ty::DateTime, as_date_time)?)
            }
            Built::OffsetDateTime(d) => Built::OffsetDateTime(temporal(
                d,
                name,
                &arg,
                Ty::OffsetDateTime,
                as_offset_date_time,
            )?),
            Built::Instant(d) => Built::Instant(temporal(d, name, &arg, Ty::Instant, as_instant)?),
            Built::List(d, element) => match name {
                "nonempty" => Built::List(d.non_empty(), element),
                "minSize" => Built::List(d.min_size(usize_arg(0)?), element),
                "maxSize" => Built::List(d.max_size(usize_arg(0)?), element),
                "fixedSize" => Built::List(d.size(usize_arg(0)?), element),
                "unique" => Built::List(d.unique(), element),
                "contains" => {
                    let wanted = arg(0, &element)?;
                    Built::List(d.contains(wanted), element)
                }
                "containsAll" => {
                    let V::List(wanted) = arg(0, &Ty::List(Box::new(element.clone())))? else {
                        return Err("containsAll takes a list".into());
                    };
                    Built::List(d.contains_all(wanted), element)
                }
                "toSet" => Built::Any(
                    d.to_set()
                        .map(|set| V::Set(set.into_iter().collect()))
                        .boxed(),
                    Ty::Set(Box::new(element)),
                ),
                _ => return Err(no()),
            },
            Built::Dict(d, element) => match name {
                "nonempty" => Built::Dict(d.non_empty(), element),
                "minSize" => Built::Dict(d.min_size(usize_arg(0)?), element),
                "maxSize" => Built::Dict(d.max_size(usize_arg(0)?), element),
                "fixedSize" => Built::Dict(d.size(usize_arg(0)?), element),
                _ => return Err(no()),
            },
            Built::Uuid(_) | Built::Uri(_) | Built::Any(..) => return Err(no()),
        })
    }

    /// The JSON the encoder `form` writes for the value `value` observes.
    pub fn encode(&mut self, form: &Value, value: &Value) -> Result<Value, String> {
        let form = array(form)?;
        let name = text(&form[0])?;
        self.use_feature(format!("encoder.{name}"));
        match name {
            "string" => {
                let s = read(&Ty::String, value)?.into_string()?;
                Ok(encode::string().encode(&s))
            }
            "object" => {
                let mut encoder = encode::object::<V>();
                let mut input = None;
                for property in form[1].as_array().ok_or("object takes properties")? {
                    let property = array(property)?;
                    let kind = text(&property[0])?;
                    self.use_feature(format!("property.{kind}"));
                    if kind != "propertyWithDefault" {
                        return Err(format!("no property {kind}"));
                    }
                    let member = text(&property[1])?.to_owned();
                    let getter = text(&property[2])?;
                    self.use_feature(format!("fixture.{getter}"));
                    if getter != "identity" {
                        return Err(format!("no getter fixture {getter}"));
                    }
                    let inner = array(&property[3])?;
                    let inner_name = text(&inner[0])?;
                    self.use_feature(format!("encoder.{inner_name}"));
                    if inner_name != "string" {
                        return Err(format!("no encoder {inner_name} in a property"));
                    }
                    let default = read(&Ty::String, &property[4])?;
                    // identity reads the value being encoded, a nullable<string>, as the
                    // property's nullable value.
                    input = Some(Ty::Nullable(Box::new(Ty::String)));
                    encoder = encoder.property_with_default(
                        member,
                        |v: &V| match v {
                            V::Opt(o) => o.as_deref().cloned(),
                            other => Some(other.clone()),
                        },
                        |v: &V| match v {
                            V::Scalar(MetaValue::String(s)) => encode::string().encode(s),
                            other => panic!("{other:?} is not a string"),
                        },
                        default,
                    );
                }
                let ty = input.ok_or("an object encoder takes a property")?;
                Ok(encoder.encode(&read(&ty, value)?))
            }
            other => Err(format!("no encoder {other}")),
        }
    }
}

fn floats<F>(v: V, get: fn(MetaValue) -> Option<F>) -> Result<Vec<F>, String> {
    match v {
        V::List(items) => items.into_iter().map(|i| scalar(i, get)).collect(),
        _ => Err("not a list".into()),
    }
}

/// An operation on an integer decoder, or `None` when there is no such operation.
fn integer<T: raoh::json::SignedInteger>(
    d: IntDecoder<T>,
    name: &str,
    arg: &dyn Fn(usize, &Ty) -> Result<V, String>,
    ty: Ty,
    get: fn(MetaValue) -> Option<T>,
) -> Option<Result<IntDecoder<T>, String>> {
    let value = |i: usize| -> Result<T, String> { scalar(arg(i, &ty)?, get) };
    let run = || -> Result<IntDecoder<T>, String> {
        Ok(match name {
            "min" => d.min(value(0)?),
            "max" => d.max(value(0)?),
            "range" => d.range(value(0)?..=value(1)?),
            "positive" => d.positive(),
            "negative" => d.negative(),
            "nonNegative" => d.non_negative(),
            "nonPositive" => d.non_positive(),
            "multipleOf" => d.multiple_of(value(0)?),
            "oneOf" => {
                let V::List(items) = arg(0, &Ty::List(Box::new(ty.clone())))? else {
                    return Err("oneOf takes a list".into());
                };
                let allowed: Vec<T> = items
                    .into_iter()
                    .map(|i| scalar(i, get))
                    .collect::<Result<_, _>>()?;
                d.one_of(allowed)
            }
            _ => unreachable!(),
        })
    };
    matches!(
        name,
        "min"
            | "max"
            | "range"
            | "positive"
            | "negative"
            | "nonNegative"
            | "nonPositive"
            | "multipleOf"
            | "oneOf"
    )
    .then(run)
}

fn temporal<T: raoh::Chronological>(
    d: TemporalDecoder<T>,
    name: &str,
    arg: &dyn Fn(usize, &Ty) -> Result<V, String>,
    ty: Ty,
    get: fn(MetaValue) -> Option<T>,
) -> Result<TemporalDecoder<T>, String> {
    let value = |i: usize| -> Result<T, String> { scalar(arg(i, &ty)?, get) };
    Ok(match name {
        "before" => d.before(value(0)?),
        "after" => d.after(value(0)?),
        "between" => d.between(value(0)?, value(1)?),
        other => return Err(format!("no operation {other} on {ty:?}")),
    })
}
