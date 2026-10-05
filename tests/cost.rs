//! What decoding costs grows with the input once, and not with its square.
//!
//! Two kinds of cost are held here. Allocations are counted, which do not vary from run to run: a
//! decoder that reads a number by writing it out first allocates for each element. Time is
//! compared between an input and one eight times its size: a step that grows with the input
//! takes about eight times as long, and one that grows with its square, as a lookup that goes over
//! every member for each member does, about sixty-four times. The bound is set between the two, so
//! that the tests tell the kinds apart without depending on how fast the machine is.

use raoh::json::BoxFieldSet;
use raoh::json::FieldSet;
use raoh::json::prelude::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::{Duration, Instant};

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|n| n.set(n.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// How many allocations `f` makes on this thread.
fn allocations(f: impl FnOnce()) -> usize {
    let before = ALLOCATIONS.with(Cell::get);
    f();
    ALLOCATIONS.with(Cell::get) - before
}

fn numbers(n: usize, each: impl Fn(usize) -> String) -> Value {
    let text = format!("[{}]", (0..n).map(each).collect::<Vec<_>>().join(","));
    serde_json::from_str(&text).unwrap()
}

#[test]
fn reading_a_number_allocates_nothing() {
    let ints = numbers(1000, |i| i.to_string());
    let doubles = numbers(1000, |i| format!("{i}.25"));
    // The list's own vector grows a few times; no element allocates.
    let n = allocations(|| {
        i64().list().decode(&ints).unwrap();
    });
    assert!(n <= 20, "{n} allocations for 1000 integers");
    let n = allocations(|| {
        f64().list().decode(&doubles).unwrap();
    });
    assert!(n <= 20, "{n} allocations for 1000 doubles");
}

#[test]
fn reading_numbers_from_text_allocates_nothing_for_each() {
    // A lexeme is held inside its node; only the vectors of the array and the list grow.
    for text in [
        format!(
            "[{}]",
            (0..1000)
                .map(|i| format!("{i}.25"))
                .collect::<Vec<_>>()
                .join(",")
        ),
        format!(
            "[{}]",
            (0..1000)
                .map(|i| format!("-{i}e-3"))
                .collect::<Vec<_>>()
                .join(",")
        ),
    ] {
        let n = allocations(|| {
            from_str(&f64().list(), &text).unwrap();
        });
        assert!(n <= 40, "{n} allocations for 1000 doubles read from text");
    }
}

#[test]
fn matching_a_pattern_again_allocates_nothing() {
    // A matcher is kept between matches, with what it has worked out; one made for each match
    // would allocate its cache every time.
    let code = string().pattern("[a-z]+[0-9]+");
    let input = json!("abcdefghijklmnopqrstuvwxyz0123456789");
    code.decode(&input).unwrap();
    let n = allocations(|| {
        for _ in 0..100 {
            code.decode(&input).unwrap();
        }
    });
    // Each decode gives a String it allocates; the match itself allocates nothing.
    assert!(n <= 100, "{n} allocations for 100 matches");
}

/// The least time of a few runs of `f`, which the machine's other work adds the least to.
fn least(f: impl Fn()) -> Duration {
    (0..5)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .min()
        .unwrap()
}

/// Fails when `run` on an input built for `8 × n` takes more than 24 times as long as on one for
/// `n`.
fn grows_with_the_input<I>(name: &str, n: usize, build: impl Fn(usize) -> I, run: impl Fn(&I)) {
    let (small, large) = (build(n), build(8 * n));
    let (a, b) = (least(|| run(&small)), least(|| run(&large)));
    let ratio = b.as_secs_f64() / a.as_secs_f64().max(1e-9);
    assert!(ratio < 24.0, "{name}: {a:?} for {n}, {b:?} for {}", 8 * n);
}

fn members(n: usize) -> Value {
    let text = format!(
        "{{{}}}",
        (0..n)
            .map(|i| format!("\"m{i}\":{i}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    serde_json::from_str(&text).unwrap()
}

#[test]
fn strict_decoders_take_time_in_the_members() {
    let nested = strict(strict(object((field("m0", i64()),)), ["m0"]), ["m0"]);
    grows_with_the_input("nested strict", 2000, members, |input| {
        let _ = nested.decode(input);
    });
    grows_with_the_input(
        "strict of many names",
        2000,
        |n| {
            let known: Vec<String> = (0..n).map(|i| format!("m{i}")).collect();
            (strict(object((field("m0", i64()),)), known), members(n))
        },
        |(decoder, input)| {
            let _ = decoder.decode(input);
        },
    );
}

#[test]
fn objects_read_from_text_take_time_in_their_members() {
    let text = |n: usize| {
        let members: Vec<String> = (0..n).map(|i| format!("\"m{i}\":{i}")).collect();
        format!("{{{}}}", members.join(","))
    };
    // A name written twice is looked for among all the others.
    grows_with_the_input("reading", 2000, text, |text| {
        text.parse::<Node>().unwrap();
    });
    // Each field finds its member among all of them.
    grows_with_the_input(
        "finding",
        2000,
        |n| {
            let fields: Vec<BoxFieldSet<Option<i64>>> = (0..n)
                .map(|i| optional_field(format!("m{i}"), i64()).boxed())
                .collect();
            (object(fields), text(n).parse::<Node>().unwrap())
        },
        |(decoder, node)| {
            decoder.decode(node).unwrap();
        },
    );
}

#[test]
fn decimals_take_time_in_their_digits() {
    let digits = |n: usize| -> Value { serde_json::from_str(&"7".repeat(n)).unwrap() };
    let multiple = decimal().multiple_of("3".parse().unwrap());
    grows_with_the_input("multipleOf", 20_000, digits, |input| {
        let _ = multiple.decode(input);
    });
    let bounded = decimal()
        .min("1".parse().unwrap())
        .max("1e100000000".parse().unwrap());
    grows_with_the_input("min and max", 20_000, digits, |input| {
        let _ = bounded.decode(input);
    });
}

#[test]
fn list_constraints_take_time_in_the_elements() {
    let ints = |n: usize| numbers(n, |i| i.to_string());
    let unique = i64().list().unique();
    grows_with_the_input("unique", 5000, ints, |input| {
        let _ = unique.decode(input);
    });
    let set = f64().list().to_set();
    grows_with_the_input("to_set", 5000, ints, |input| {
        let _ = set.decode(input);
    });
    grows_with_the_input(
        "contains_all",
        5000,
        |n| ((0..n as i64).collect::<Vec<_>>(), ints(n)),
        |(wanted, input)| {
            let _ = i64().list().contains_all(wanted.clone()).decode(input);
        },
    );
}
