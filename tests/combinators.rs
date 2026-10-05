use raoh::json::prelude::*;
use raoh::{BoxDecoder, Issue, Messages, MetaValue, decoder_fn};

fn paths(issues: &Issues) -> Vec<String> {
    issues.iter().map(|i| i.path().to_string()).collect()
}

#[test]
fn a_tuple_reports_the_issues_of_every_part_in_the_order_written() {
    let decoder = object((field("c", i64()), field("a", string()), field("b", bool())));
    let issues = decoder
        .decode(&json!({"a": 1, "b": 2, "c": "3"}))
        .unwrap_err();
    assert_eq!(paths(&issues), ["/c", "/a", "/b"]);
}

#[test]
fn a_bare_tuple_is_a_decoder_too() {
    let both = (string(), string().lowercase());
    assert_eq!(
        both.decode(&json!("Ab")).unwrap(),
        ("Ab".into(), "ab".into())
    );
}

#[test]
fn and_then_runs_only_once_the_parts_decode() {
    let calls = std::cell::Cell::new(0);
    let decoder = object((field("a", i64()), field("b", i64()))).and_then(|(a, b)| {
        calls.set(calls.get() + 1);
        Ok::<_, Issue>(a + b)
    });
    assert!(decoder.decode(&json!({"a": 1})).is_err());
    assert_eq!(calls.get(), 0);
}

#[test]
fn and_then_moves_its_issues_to_where_the_decoder_is_exactly_once() {
    let period = object((field("start", i64()), field("end", i64()))).and_then(|(start, end)| {
        if start <= end {
            Ok((start, end))
        } else {
            Err(Issue::new("invalid_value")
                .with_message("end is before start")
                .at(["end"].into_iter().collect()))
        }
    });
    let decoder = object((field("trip", object((field("period", period),))),));
    let issues = decoder
        .decode(&json!({"trip": {"period": {"start": 2, "end": 1}}}))
        .unwrap_err();
    assert_eq!(paths(&issues), ["/trip/period/end"]);
}

#[test]
fn pipe_hands_the_output_on_at_the_same_path() {
    let even = decoder_fn(|n: &i64, path: &raoh::Path<'_>| {
        if n % 2 == 0 {
            Ok(*n)
        } else {
            Err(Issue::new("odd")
                .with_message("must be even")
                .at(path.to_pointer())
                .into())
        }
    });
    let decoder = object((field("n", i64().pipe(even)),));
    let issues = decoder.decode(&json!({"n": 3})).unwrap_err();
    assert_eq!(paths(&issues), ["/n"]);
}

#[test]
fn with_default_covers_only_missing_or_null() {
    let decoder = object((field("page", u32().with_default(1)),));
    assert_eq!(decoder.decode(&json!({})).unwrap(), (1,));
    assert_eq!(decoder.decode(&json!({"page": null})).unwrap(), (1,));
    let issues = decoder.decode(&json!({"page": "x"})).unwrap_err();
    assert_eq!(issues.iter().next().unwrap().code(), "type_mismatch");
}

#[test]
fn recover_covers_every_failure() {
    let decoder = object((field("page", u32().recover(1)),));
    assert_eq!(decoder.decode(&json!({"page": "x"})).unwrap(), (1,));
    assert_eq!(decoder.decode(&json!({})).unwrap(), (1,));
}

#[test]
fn refine_reports_its_message_as_a_custom_one() {
    let decoder = i64().refine(|n| n % 2 == 0, "odd", "must be even");
    let issues = decoder.decode(&json!(3)).unwrap_err();
    let issue = issues.iter().next().unwrap();
    assert_eq!(issue.code(), "odd");
    assert_eq!(issue.custom_message(), Some("must be even"));
}

#[test]
fn one_of_lists_each_candidate_issues() {
    let decoder = one_of((i64().map(|n| n.to_string()), string().min_length(3)));
    let issues = decoder.decode(&json!("ab")).unwrap_err();
    let issue = issues.iter().next().unwrap();
    assert_eq!(issue.code(), "one_of_failed");
    let candidates = issue.meta()["candidates"].as_list().unwrap();
    assert_eq!(candidates.len(), 2);
    let MetaValue::Record(second) = &candidates[1] else {
        panic!("a candidate is a record");
    };
    assert_eq!(second["candidate"], MetaValue::from(1));
    let MetaValue::Issues(found) = &second["issues"] else {
        panic!("a candidate's issues are issues");
    };
    assert_eq!(found.iter().next().unwrap().code(), "too_short");

    let japanese = issues.to_json_with(Messages::japanese());
    let written = &japanese[0]["meta"]["candidates"][1]["issues"][0];
    assert_eq!(written["code"], "too_short");
    assert_eq!(written["message"], "3文字以上で入力してください");
}

#[derive(Debug)]
struct Node {
    children: Vec<Node>,
}

fn node() -> BoxDecoder<Json, Node> {
    object((field("children", lazy(node).list()),))
        .map(|(children,)| Node { children })
        .boxed()
}

fn depth(node: &Node) -> usize {
    1 + node.children.iter().map(depth).max().unwrap_or(0)
}

/// serde_json stops reading text nested 128 arrays and objects deep, and each node here is
/// an object holding an array, so 63 nodes is as deep as an input read with `from_str` gets.
#[test]
fn lazy_decodes_input_as_deep_as_serde_json_reads() {
    let text = format!(
        "{}{}{}",
        r#"{"children":["#.repeat(62),
        r#"{"children":[]}"#,
        "]}".repeat(62)
    );
    let decoded = from_str(&node(), &text).unwrap();
    assert_eq!(depth(&decoded), 63);
}

#[test]
fn a_boxed_decoder_is_shared_across_threads() {
    let decoder: std::sync::Arc<BoxDecoder<Json, i64>> = std::sync::Arc::new(i64().boxed());
    let handles: Vec<_> = (0..4)
        .map(|n| {
            let decoder = std::sync::Arc::clone(&decoder);
            std::thread::spawn(move || decoder.decode(&json!(n)).unwrap())
        })
        .collect();
    let sum: i64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
    assert_eq!(sum, 6);
}

#[test]
fn from_str_reports_text_that_is_not_json() {
    let issues = from_str(&i64(), "[1,").unwrap_err();
    let issue = issues.iter().next().unwrap();
    assert_eq!(issue.code(), "invalid_format");
    assert!(issue.path().is_root());
    assert!(issue.meta().contains_key("line"));
}

#[test]
fn lazy_builds_its_decoder_once_for_each_level_of_nesting() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static BUILT: AtomicUsize = AtomicUsize::new(0);

    struct Tree(Vec<Tree>);
    fn tree() -> BoxDecoder<Json, Tree> {
        BUILT.fetch_add(1, Ordering::Relaxed);
        object((field("children", lazy(tree).list()),))
            .map(|(children,)| Tree(children))
            .boxed()
    }

    let leaf = r#"{"children":[]}"#;
    let text = format!(r#"{{"children":[{}]}}"#, vec![leaf; 100].join(","));
    let decoder = tree();
    for _ in 0..3 {
        let Tree(children) = from_str(&decoder, &text).unwrap();
        assert_eq!(children.len(), 100);
    }
    // The root's, and the one for the level of its children; the leaves have none to build.
    assert_eq!(BUILT.load(Ordering::Relaxed), 2);
}
