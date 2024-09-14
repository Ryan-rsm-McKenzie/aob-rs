#![warn(clippy::pedantic)]

pub use aob_common::{
    DynamicNeedle,
    Error,
    Match,
    Needle,
    Reason,
    StaticNeedle,
};
pub use aob_macros::aob;

#[cfg(test)]
mod tests {
    use crate::{
        aob,
        DynamicNeedle,
        Needle as _,
    };

    #[test]
    fn test_aob() {
        aob! {
            const _1 = ida("11 ? 22");
            pub const _2 = ida("11 ? 22");
            pub(crate) const _3 = ida("11 ? 22");
            pub(super) const _4 = ida("11 ? 22");
            const _5 = ida("11");
            const _6 = ida("?");
        }
    }

    #[test]
    fn test_matches() {
        let haystack = include_bytes!("../../../data/the_raven.txt");
        macro_rules! do_test {
            ($pattern:literal, $count:literal) => {{
                let needle = DynamicNeedle::from_ida($pattern).unwrap();
                let matches = needle.find_iter(haystack).count();
                assert_eq!(matches, $count, "dyn: {}", $pattern);

                aob! { const NEEDLE = ida($pattern); }
                let matches = NEEDLE.find_iter(haystack).count();
                assert_eq!(matches, $count, "const: {}", $pattern);
            }};
        }

        do_test!("52 61 76 65 6e", 11);
        do_test!("? 61 76 65 6e", 14);
        do_test!("4f 6e 63 65", 1);
        do_test!("? 6e 63 65", 7);
        do_test!("? 6e 63 ?", 15);
        do_test!("21", 19);
        do_test!("? 75 ? 74 ?", 31);
    }
}
