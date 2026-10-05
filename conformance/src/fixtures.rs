//! The fixtures of catalog/fixtures.json, written as a user of raoh writes such functions
//! (spec/fixtures.md).

use crate::value::{Ty, V};
use raoh::{Issue, Issues, MetaValue, Pointer};

/// A function a `map` fixture stands for.
pub type MapFn = fn(V) -> V;

/// A `map` fixture: its output type for the input type `input`, and the function.
pub fn map(name: &str, input: &Ty) -> Result<(Ty, MapFn), String> {
    Ok(match name {
        "first" => {
            let Ty::Product(parts) = input else {
                return Err(format!("first takes a product, not {input:?}"));
            };
            let [only] = parts.as_slice() else {
                return Err("first takes a product of one element".into());
            };
            (only.clone(), first)
        }
        "square_side" | "square" => (Ty::Int32, square),
        "area" => (Ty::Int32, area),
        "shift_add_10" => (Ty::Int32, shift_add_10),
        "shift_add_100" => (Ty::Int32, shift_add_100),
        "shift_add_1000" => (Ty::Int32, shift_add_1000),
        "decimal_string" => (Ty::String, decimal_string),
        other => return Err(format!("no map fixture {other}")),
    })
}

fn ints(v: V) -> Vec<i32> {
    v.into_product()
        .expect("the fixture's input is a product")
        .iter()
        .map(|p| p.as_i32().expect("the fixture's input holds int32s"))
        .collect()
}

fn first(v: V) -> V {
    v.into_product()
        .expect("the fixture's input is a product")
        .remove(0)
}

fn square(v: V) -> V {
    let product = v.into_product().expect("the fixture's input is a product");
    let side = product[0].as_i32().expect("the side is an int32");
    V::scalar(side * side)
}

fn area(v: V) -> V {
    let p = ints(v);
    V::scalar(p[0] * p[1])
}

fn shift_add(v: V, by: i32) -> V {
    let p = ints(v);
    V::scalar(p[0] * by + p[1])
}

fn shift_add_10(v: V) -> V {
    shift_add(v, 10)
}

fn shift_add_100(v: V) -> V {
    shift_add(v, 100)
}

fn shift_add_1000(v: V) -> V {
    shift_add(v, 1000)
}

fn decimal_string(v: V) -> V {
    V::scalar(v.as_i32().expect("the input is an int32").to_string())
}

/// The `refine` fixture `even`, as a user writes a check with an issue of their own.
pub fn even(v: V) -> Result<V, Issue> {
    let n = v.as_i32().expect("even takes an int32");
    if n % 2 == 0 {
        Ok(v)
    } else {
        Err(Issue::new("must_be_even")
            .with_message("must be even")
            .with_meta("actual", MetaValue::from(n)))
    }
}

/// The `flatMap` fixture `ordered_period`: the product unchanged when start <= end, and otherwise
/// an issue at `end`, relative to where the decoder is.
pub fn ordered_period(v: V) -> Result<V, Issue> {
    let product = v.into_product().expect("the input is a product");
    let start = product[0].as_i32().expect("start is an int32");
    let end = product[1].as_i32().expect("end is an int32");
    if start <= end {
        Ok(V::Product(product))
    } else {
        Err(Issue::new("invalid_value")
            .with_message("end is before start")
            .at(Pointer::root().join("end")))
    }
}

/// The `recover` fixture `issue_count_plus_10`.
pub fn issue_count_plus_10(issues: &Issues) -> V {
    V::scalar(issues.len() as i32 + 10)
}
