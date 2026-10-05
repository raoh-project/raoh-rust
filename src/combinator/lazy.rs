use crate::decoder::Decoder;
use crate::issue::Issues;
use crate::path::Path;
use std::sync::OnceLock;

/// A decoder built by `make` the first time it decodes, and kept, so a decoder can refer to itself.
///
/// What a recursive decoder builds is one decoder for each level of nesting it has decoded, each
/// once, so the matchers of a [`pattern`](crate::json::StringDecoder::pattern) inside it are kept
/// between decodes as anywhere else.
///
/// A recursive decoder names its own type, so it returns a [`BoxDecoder`](crate::BoxDecoder):
///
/// ```
/// use raoh::json::prelude::*;
/// use raoh::{lazy, BoxDecoder};
///
/// struct Category { name: String, children: Vec<Category> }
///
/// fn category() -> BoxDecoder<Json, Category> {
///     object((
///         field("name", string()),
///         field("children", lazy(category).list()),
///     ))
///     .map(|(name, children)| Category { name, children })
///     .boxed()
/// }
///
/// let tree = category()
///     .decode(&json!({"name": "a", "children": [{"name": "b", "children": []}]}))
///     .unwrap();
/// assert_eq!(tree.children[0].name, "b");
/// ```
pub fn lazy<F, D>(make: F) -> Lazy<F, D>
where
    F: Fn() -> D,
{
    Lazy {
        make,
        built: OnceLock::new(),
    }
}

/// The decoder [`lazy`] returns.
pub struct Lazy<F, D> {
    make: F,
    built: OnceLock<D>,
}

/// A clone builds its decoder again, the first time it decodes.
impl<F: Clone, D> Clone for Lazy<F, D> {
    fn clone(&self) -> Self {
        Self {
            make: self.make.clone(),
            built: OnceLock::new(),
        }
    }
}

impl<F, D> std::fmt::Debug for Lazy<F, D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lazy")
            .field("built", &self.built.get().is_some())
            .finish()
    }
}

impl<I: ?Sized, F, D> Decoder<I> for Lazy<F, D>
where
    F: Fn() -> D,
    D: Decoder<I>,
{
    type Output = D::Output;

    fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<D::Output, Issues> {
        self.built.get_or_init(&self.make).decode_at(input, path)
    }
}
