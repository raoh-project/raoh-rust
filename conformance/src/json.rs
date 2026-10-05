//! Reads a suite file.
//!
//! A case's input is a value of the input model, in which a number is its lexeme
//! (spec/input-model.md), and a runner has to give its decoder the exact text of the input. raoh
//! reads JSON text into a [`Node`] that keeps every lexeme, member order and name, so the file is
//! read once into nodes, and each case's input is handed to its decoder as the node it is.
//!
//! The rest of a case, its decoder form and expected value, is read from a [`Value`] made from the
//! same nodes, whose numbers hold the same lexemes.

use raoh::json::Node;
use serde_json::{Map, Number, Value};

/// A case of a suite file, and its input.
pub struct Case {
    pub case: Value,
    pub input: Node,
}

pub fn parse(text: &str) -> Result<Vec<Case>, String> {
    let node: Node = text
        .parse()
        .map_err(|issue: raoh::Issue| format!("not JSON: {:?}", issue.meta()))?;
    let Node::Array(cases) = node else {
        return Err("a suite file is an array".into());
    };
    Ok(cases
        .into_iter()
        .map(|case| {
            let input = match &case {
                Node::Object(members) => members.get("input").cloned(),
                _ => None,
            };
            Case {
                case: value(&case),
                input: input.unwrap_or(Node::Null),
            }
        })
        .collect())
}

/// `node` as a `Value`, every number holding its lexeme.
pub fn value(node: &Node) -> Value {
    match node {
        Node::Null => Value::Null,
        Node::Bool(b) => Value::Bool(*b),
        Node::Number(lexeme) => Value::Number(Number::from_string_unchecked(lexeme.to_string())),
        Node::String(s) => Value::String(s.clone()),
        Node::Array(items) => Value::Array(items.iter().map(value).collect()),
        Node::Object(members) => Value::Object(
            members
                .iter()
                .map(|(name, v)| (name.to_owned(), value(v)))
                .collect::<Map<_, _>>(),
        ),
    }
}
