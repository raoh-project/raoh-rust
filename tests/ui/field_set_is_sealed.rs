use raoh::json::prelude::*;
use raoh::json::FieldSet;
use raoh::{Issues, Path};

struct Anything;

impl FieldSet for Anything {
    type Output = ();

    fn decode_fields(&self, _: &Json, _: &Path<'_>) -> Result<(), Issues> {
        Ok(())
    }

    fn member_names<'a>(&'a self, _: &mut Vec<&'a str>) -> bool {
        true
    }
}

fn main() {
    let _ = object(Anything).strict();
}
