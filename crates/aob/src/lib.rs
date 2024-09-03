#![warn(clippy::pedantic)]

pub use aob_common::{
    DynamicNeedle,
    Error,
    Needle,
    StaticNeedle,
};
pub use aob_macros::aob;

#[cfg(test)]
mod tests {
    use crate::aob;

    #[test]
    fn test_aob() {
        aob! {
            const _1 = ida("11 ? 22");
        }
        aob! {
            pub const _2 = ida("11 ? 22");
        }
        aob! {
            pub(crate) const _3 = ida("11 ? 22");
        }
        aob! {
            pub(super) const _4 = ida("11 ? 22");
        }
    }
}
