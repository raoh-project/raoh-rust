use crate::decoder::Decoder;
use crate::issue::Issues;
use crate::path::Path;

/// Runs every decoder of the tuple on the same input and keeps every issue, in the order the
/// decoders are written.
macro_rules! tuple_decoder {
    ($($T:ident $v:ident $idx:tt),+) => {
        impl<I: ?Sized, $($T: Decoder<I>),+> Decoder<I> for ($($T,)+) {
            type Output = ($($T::Output,)+);

            fn decode_at(&self, input: &I, path: &Path<'_>) -> Result<Self::Output, Issues> {
                let mut issues = Issues::new();
                $(
                    let $v = match self.$idx.decode_at(input, path) {
                        Ok(value) => Some(value),
                        Err(found) => {
                            issues.merge(found);
                            None
                        }
                    };
                )+
                match ($($v,)+) {
                    ($(Some($v),)+) => Ok(($($v,)+)),
                    _ => Err(issues),
                }
            }
        }
    };
}

for_tuples!(tuple_decoder);
