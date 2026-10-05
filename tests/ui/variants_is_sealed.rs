use raoh::json::prelude::*;
use raoh::json::Variants;
use raoh::{Issues, Path};

struct Anything;

impl Variants for Anything {
    type Output = ();

    fn tags(&self) -> Vec<&str> {
        vec![]
    }

    fn decode_variant(&self, _: &str, _: &Json, _: &Path<'_>) -> Option<Result<(), Issues>> {
        None
    }
}

fn main() {
    let _ = discriminate("type", Anything);
}
