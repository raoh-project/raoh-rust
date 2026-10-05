//! The arities tuples are given trait implementations for, written once.
//!
//! A tuple of decoders, of fields, of alternatives and of variants, and the product an object
//! gives, each have their implementations for these arities. They are listed here once, so that
//! what a tuple of fields can build, a product of that many values can also be compared and
//! decoded into: an implementation that stops at another arity would narrow what the others give.

/// Invokes `$m!` once for each arity from 1 to 16, with each element as a type parameter, a
/// binding name and an index: `$m!(A a 0, B b 1)`.
macro_rules! for_tuples {
    ($m:ident) => {
        $m!(A a 0);
        $m!(A a 0, B b 1);
        $m!(A a 0, B b 1, C c 2);
        $m!(A a 0, B b 1, C c 2, D d 3);
        $m!(A a 0, B b 1, C c 2, D d 3, E e 4);
        $m!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5);
        $m!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6);
        $m!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7);
        $m!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, J j 8);
        $m!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, J j 8, K k 9);
        $m!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, J j 8, K k 9, L l 10);
        $m!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, J j 8, K k 9, L l 10, M m 11);
        $m!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, J j 8, K k 9, L l 10, M m 11, N n 12);
        $m!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, J j 8, K k 9, L l 10, M m 11, N n 12, O o 13);
        $m!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, J j 8, K k 9, L l 10, M m 11, N n 12, O o 13, P p 14);
        $m!(A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, J j 8, K k 9, L l 10, M m 11, N n 12, O o 13, P p 14, Q q 15);
    };
}
