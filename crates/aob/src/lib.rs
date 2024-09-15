//! If you're looking to construct a pattern:
//! * See [`aob!`] to construct a pattern at compile-time.
//! * See [`DynamicNeedle`] to construct a pattern at run-time.
//!
//! You'll need to `use` [`Needle`] before you can do anything with a pattern:
//! ```
//! use aob::Needle as _;
//! aob::aob! { const NEEDLE = ida("67 ? AB"); }
//! let haystack = [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF];
//! let found = NEEDLE.find(&haystack).unwrap();
//! assert_eq!(found.range(), 3..6);
//! ```

#![warn(clippy::pedantic)]

pub use aob_common::{
    DynamicNeedle,
    Error,
    Find,
    Match,
    Method,
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
        Method,
        Needle,
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

    fn collect_matching_positions<N: Needle>(
        haystack: &[u8],
        needle: N,
        method: Method,
        pattern: &str,
    ) -> Vec<usize> {
        let iter = needle.find_iter(haystack);
        assert_eq!(iter.search_method(), method, "{pattern}");
        iter.map(|x| x.start()).collect()
    }

    fn collect_matching_count<N: Needle>(
        haystack: &[u8],
        needle: N,
        method: Method,
        pattern: &str,
    ) -> usize {
        let iter = needle.find_iter(haystack);
        assert_eq!(iter.search_method(), method, "{pattern}");
        iter.count()
    }

    const MOBY_DICK: &[u8] = include_bytes!("../../../data/moby_dick.txt");
    const THE_RAVEN: &[u8] = include_bytes!("../../../data/the_raven.txt");

    macro_rules! do_test_pos {
        ($method:ident, $pattern:literal, [$($match_positions:tt)*], $haystack:ident) => {{
            let match_positions = &[$($match_positions)*];

            let needle = DynamicNeedle::from_ida($pattern).unwrap();
            let matches = collect_matching_positions($haystack, needle, Method::$method, $pattern);
            assert_eq!(matches, match_positions, $pattern);

            aob! { const NEEDLE = ida($pattern); }
            let matches = collect_matching_positions($haystack, NEEDLE, Method::$method, $pattern);
            assert_eq!(matches, match_positions, $pattern);
        }};
        ($method:ident, $pattern:literal, [$($match_positions:tt)*]) => {{
            do_test_pos!($method, $pattern, [$($match_positions)*], MOBY_DICK);
        }};
    }

    macro_rules! do_test_count {
        ($method:ident, $pattern:literal, $match_count:literal) => {{
            let needle = DynamicNeedle::from_ida($pattern).unwrap();
            let matches = collect_matching_count(THE_RAVEN, needle, Method::$method, $pattern);
            assert_eq!(matches, $match_count, $pattern);

            aob! { const NEEDLE = ida($pattern); }
            let matches = collect_matching_count(THE_RAVEN, NEEDLE, Method::$method, $pattern);
            assert_eq!(matches, $match_count, $pattern);
        }};
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn test_avx2() {
        do_test_pos!(Avx2, "67 20 74 68 ? ? 6D 69 64 64 6C 65 ? 66 ? 67 75 72 65 20 69 6E ? ? 74 68 65 20 70 69 63 ? 75 ? 65 20 6F 66 20 74 68 72 65 65 20 77 68 61 6C", [607662]);
        do_test_pos!(Avx2, "? 73 70 65 6E ? ? 69 6E 20 64 65 6C 69 62 65 72 ? 74 ? 6E 67 20 77 68 61 74 20 74 6F ? 73", [79560]);
        do_test_pos!(Avx2, "65 ? 20 61 73 20 63 ? 61 6D 6F 69 ? 20 68 75 6E 74 65 72 73 20 ? 6C ? ? 62 ? 74 68 65 20 ? ? 70 73 2E 20 46 6F 72 20 ? 65 61 72 73 0D 0A 68 65 ? 6B ? 6F 77", [164495]);
        do_test_pos!(Avx2, "61 73 ? 20 69 ? 20 77 69 74 68 6F 75 74 20 69 6D ? 65 ? 69 61 74 65 20 64 65 ? ? 68 2E ? 42 75 74 20 74 68 65 20 74 ? ? 74 68 ? 6F 66 ? 74 68 65 ? ? 61 74 74 65 ?", [1018186]);
        do_test_pos!(Avx2, "61 72 20 67 ? 6E 65 20 ? 6D 20 ? ? 69 6E 20 74 68 ? 20 64 61 72 ? 20 73 69 64 65 ? ? ? 20 65 ? 72 74", [1154977]);
        do_test_pos!(Avx2, "? 77 68 61 6C ? 0D 0A 73 6F ? 65 74 68 69 6E 67 20 6C 65 73 73 20 74 ? 61 6E 20 32 30 30 30 20 73 71 75 61 72 ? ? 66 65 65 74 E2 80 94 74 68 ?", [797429]);
        do_test_pos!(Avx2, "64 69 64 ? 65 73 73 2E 20 47 72 61 ? 74 ? 6E 67 20 74 68 61 74 20 74 ? 65 ? 57 68 69 ? 65 20", [484443]);
        do_test_pos!(Avx2, "? E2 ? ? 72 65 6D 6F 76 69 6E ? 20 68 69 73 20 68 ? ? ? 20 ? 6E 64 0D ? 62 72 75 73 68 69 6E 67 20 61 73 69 ? 65 20 68 ? 73 20 68 61 69 72 2C 20 61 6E ? 20 65 78 70", [981299]);
        do_test_pos!(Avx2, "72 20 69 6E 74 6F 20 ? 68 65 0D 0A 77 68 69 74 65 20 63 75 72 64 6C 69 ? ? 20 63 72 65 61 ? ? ? 66 20 74 ? 65 20 73 71 ? 61 6C 6C ? ? ? 71", [512884]);
        do_test_pos!(Avx2, "? 6E 67 2E 0D 0A 0D 0A ? ? ? 65 20 6D 61 69 6E ? 74 6F 70 2D ? 61 69 ? ? 79 61 72 64 5F 2E E2 80 94 5F ? 61 73 68 74 ? 67 6F 20 70 61 ? 73 69 6E 67 20 6E 65 ? 20 6C 61 73 68 69", [1122358]);
        do_test_pos!(Avx2, "74 6F ? ? 61 72 70 6F 6F 6E 20 ? 69 74 68 20 63 69 76 69 6C 69 7A 65 64 20 73 74 65 ? 6C 20 74 68 ? 20 67 ? 65 61 74 20 53 ? 65 72 6D 0D ? 57", [986640]);
        do_test_pos!(Avx2, "2D 72 69 62 62 6F 6E 65 64 ? 68 61 74 20 62 65 74 ? 6B 65 ? 20 68 69 73 0D 0A ? ? ? 6E 64 20 66 ? 61 74 75 72 65 73 2E", [569510]);
        do_test_pos!(Avx2, "73 20 64 69 61 ? 6F 6C 69 63 ? 6C 20 69 6E ? 6F 68 65 72 65 6E 63 65 73 ? ? 6E 69 ? 76 ? 74 65 64 6C 79 0D 0A 72 65 63 75 ? 72 69 ? 67 20 74 6F 20 ? 65 2C 20 77 69 74 68", [284628]);
        do_test_pos!(Avx2, "68 ? 20 74 61 ? 62 6F 75 72 69 6E 65 20 75 70 20 ? 68 65 ? 0A 73 63 75 ? ? 6C ? 5F 2E 29", [395862]);
        do_test_pos!(Avx2, "21 20 4F ? 21 ? 77 ? 65 6E 20 79 65 20 67 ? 74 20 74 ? ? 72 ? 2C 20 74 ? 6C ? 20 E2 80 99 65 6D ? 49 E2 80 99 76 ? 20 63 6F 6E ? 6C ? ? ? 64 20 6E 6F 74 20 74 6F 20 6D", [229720]);
        do_test_pos!(Avx2, "72 69 ? 67 73 20 68 65 61 76 65 6E 6C 79 20 76 6F 75 63 68 65 72 73 0D 0A ? 66 ? 61 6C 6C 20 6F 75 72 20 68 65 61 76 65 6E 6C 79 20 68 6F 6D 65 73 2E 20 57 68 ?", [1063744]);
        do_test_pos!(Avx2, "64 67 65 20 6F 66 20 74 68 65 ? ? ? 72 65 ? 75 6C 20 7A ? 6E 65 ? 20 77 68 6F 73 65 20 63 65 6E ? 72 65 20 68 61 64 ? 0A 6E", [1202437]);
        do_test_pos!(Avx2, "63 ? ? ? 6E 74 6F 20 74 68 65 20 62 6F 77 73 20 6F 66 20 6F 6E 65 20 6F 66 20 ? 68 65 20 77 ? 61 6C 65 2D 62 6F 61 74 ? 0D ? 68 61 6E 67 69 6E 67 20 74 6F", [220034]);
        do_test_pos!(Avx2, "6C 79 ? ? 74 72 65 6D ? 6C 69 6E 67 20 ? 76 65 ? 20 74 68 65 ? 73 69 64 65 3B ? 74 68 65 20 73 74 ? 77 61 72 ? 20 ? 64 ? 61 ? 63 ? 73 2C 20 61 6E ? 20 77 69 ? 68 20 61 20", [723781]);
        do_test_pos!(Avx2, "? 74 ? 61 6E 64 ? 63 6F ? 72 ? 67 65 6F 75 ? 0D ? 65 6E 6F ? 67 68 20 ? ? 20 6F 66 ? 65 ? 69 6E 67 20 62 ?", [407617]);
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn test_sse2() {
        do_test_pos!(
            Sse2,
            "? ? 6C 65 65 70 2C 20 ? 20 ? 61 73 0D 0A 68 6F ? 72 69 62 6C",
            [945074]
        );
        do_test_pos!(
            Sse2,
            "73 3B ? ? ? ? 74 20 6F 75 72 20 77 69 6C 64 20 77 ? 61 6C ? ? ? ? 73 68 65",
            [570112]
        );
        do_test_pos!(
            Sse2,
            "65 61 6B 61 62 6C 65 20 74 65 72 72 ? 72 ? 20 6F ? 20 74 68 ? ? 77 ? ? 6C 65 2C 20 77",
            [1016873]
        );
        do_test_pos!(
            Sse2,
            "20 6D 61 6E 79 ? 6F 66 20 69 ? 73 20 72 69 6D ? 65 ? 20 ? 61 72",
            [554295]
        );
        do_test_pos!(Sse2, "68 61 ? 74 73 ? ? 69 74 68 6F 75 74 20 6B 6E 6F 77 69 6E 67 20 74 ? 65 6D 0D 0A 74 6F 20", [622022]);
        do_test_pos!(Sse2, "73 68 65 64 20 75 70 6F ? ? 0A ? 65 ? ? 69 6D 6F ? 79 20 65 6E 74 69 72 65 6C 79 20 69", [468412]);
        do_test_pos!(
            Sse2,
            "48 ? 50 54 45 52 20 32 33 ? 20 54 ? ? 20 4C 65 65 20 53 68 6F 72 65 2E 0D",
            [734, 253040]
        );
        do_test_pos!(
            Sse2,
            "74 20 61 20 ? 69 74 ? 6C ? ? 6F 66 20 74 68 ? 73 20",
            [288918]
        );
        do_test_pos!(
            Sse2,
            "65 ? ? 20 74 68 65 ? 50 65 71 75 6F 64 2C 20 61 6E 64 20 ? ? 72 65 20 73 ? ?",
            [224814]
        );
        do_test_pos!(
            Sse2,
            "20 6C 6F 6E ? ? 68 61 6E 67 69 6E 67 20 61 ? 6C 20 72",
            [1150952]
        );
        do_test_pos!(
            Sse2,
            "? 64 20 ? 68 65 72 65 E2 80 99 73 20 61 20 6D ?",
            [301145]
        );
        do_test_pos!(
            Sse2,
            "65 ? 73 65 ? ? 65 64 ? ? 6F 20 68 61 76 65 20 ? 72 65",
            [1204983]
        );
        do_test_pos!(
            Sse2,
            "61 6E 64 73 20 68 69 ? ? 80 94 ? 68 61 74 3F ? 53 6F 6D 65 20 ?",
            [723876]
        );
        do_test_pos!(
            Sse2,
            "80 99 ? 20 ? 6F 75 6E 64 ? 68 ? 75 ? 65 20 61 ? 61 66 74 3B 20 61 6E 64 20 ? 6F",
            [906323]
        );
        do_test_pos!(
            Sse2,
            "? 65 72 20 70 69 ? 6F 74 65 ? 20 61 6E ? 20 6F 74 68 65 72 ? 63 72 61 66 ? E2",
            [246402]
        );
        do_test_pos!(
            Sse2,
            "73 20 73 61 69 ? 20 ? 6F 20 62 ? 20 61 20 6C 61 6B 65 20 ? ? 20 77 68 ? ? ? 20 74",
            [412127]
        );
        do_test_pos!(
            Sse2,
            "0A 62 ? 61 63 6B 20 68 ? ? ? 6B 65 72 63 68 69 ? 66 20",
            [225141]
        );
        do_test_pos!(
            Sse2,
            "74 77 ? ? 6C 61 72 67 65 2C 20 ? 6F 61 64 65",
            [757698]
        );
        do_test_pos!(
            Sse2,
            "72 65 73 2D 6C 69 6B ? 0D ? ? 70 ? 6C 69 65 73 ? 74 6F ? 68 69",
            [1164470]
        );
        do_test_pos!(
            Sse2,
            "? ? ? 74 68 65 20 64 65 63 6B 2E 0D 0A 0D 0A ? 80 9C ? 68",
            [1053831, 1134240]
        );
    }

    #[test]
    fn test_swar64() {
        do_test_pos!(
            Swar64,
            "65 73 ? 20 ? 76 65 6E",
            [129720, 443261, 484563, 554399, 588467, 877576]
        );
        do_test_pos!(Swar64, "69 74 68 20 74 6F ? 6E 61 64 6F", [508833]);
        do_test_pos!(Swar64, "? 69 6E 74 6F 20 69 74 2C 20 61", [834651, 838474]);
        do_test_pos!(
            Swar64,
            "20 74 68 65 20 ? 6E 74 65 ? 6C 69 6E ? 65",
            [257523]
        );
        do_test_pos!(Swar64, "68 6F ? 74 20 61 6C 74 ?", [764174]);
        do_test_pos!(Swar64, "72 20 74 ? 65 20 57 61 74 65 72 2D 62", [964720]);
        do_test_pos!(Swar64, "? 61 76 ? 20 65 79 65 73 20 61", [1234872]);
        do_test_pos!(Swar64, "6C 69 ? 74 6C 65 ? 66 ? 61 6B", [169039]);
        do_test_pos!(Swar64, "E2 ? 9C 49 20 ? 69 6C 6C ? 6A 75", [171587]);
        do_test_pos!(Swar64, "73 0D 0A ? 65 20 70 69 6E", [668312]);
        do_test_pos!(
            Swar64,
            "67 65 20 74 ? ? 6F 75 ? 68 20 ?",
            [25936, 759781, 1143628]
        );
        do_test_pos!(Swar64, "69 ? ? 65 6E 64 ? ? 20 61 73 20 ?", [832019]);
        do_test_pos!(
            Swar64,
            "2C 20 74 68 65 20 63 ? 72 70 ? 6E",
            [1033453, 1036124, 1036374, 1036755, 1042148, 1059876, 1060605]
        );
        do_test_pos!(
            Swar64,
            "79 ? 75 72 20 61 6E 73 77 ? 72 3F E2 80 9D",
            [672294]
        );
        do_test_pos!(Swar64, "68 6F 75 ? 65 0D 0A 61", [1072186]);
        do_test_pos!(Swar64, "? 20 73 68 65 ? ? 72 65 77 ? 6E 69", [1176885]);
        do_test_pos!(Swar64, "6F 20 ? 6E 20 77 69 74", [980423]);
        do_test_pos!(Swar64, "68 65 20 73 ? 6F 61 6C 20 77 68 69", [470506]);
        do_test_pos!(Swar64, "77 ? 79 73 20 ? 61 ? 20 73 75 72 70", [103139]);
        do_test_pos!(
            Swar64,
            "57 68 69 74 65 ? 57 68 61",
            [
                402817, 404737, 406840, 410495, 412578, 413849, 414890, 416314, 422829, 423920,
                424007, 456306, 466149, 481561, 482851, 484470, 499366, 499487, 501415, 519439,
                536759, 537229, 588014, 588032, 627660, 627677, 710216, 711332, 711425, 714131,
                732330, 786101, 903596, 903759, 966343, 969395, 970141, 970740, 974011, 974053,
                974644, 981888, 983074, 984073, 1069537, 1069920, 1080374, 1081364, 1090755,
                1112432, 1117046, 1146803, 1146823, 1156030, 1177399, 1178079, 1191465, 1194442,
                1198250, 1201970, 1208978, 1216105, 1216659, 1218229, 1218862, 1219985, 1221115,
                1237335, 1245334, 1246606
            ]
        );
    }

    #[test]
    fn test_swar32() {
        do_test_count!(Swar32, "72 65 2E E2 80 9D", 13);
        do_test_count!(Swar32, "64 20 74 ?", 15);
        do_test_count!(Swar32, "? 9D 0D 0A 0D 0A 20", 12);
        do_test_count!(Swar32, "? 74 68 20 6D", 3);
        do_test_count!(Swar32, "68 65 ? 66 72 6F 6D", 1);
        do_test_count!(Swar32, "80 9C ? 80 ?", 2);
        do_test_count!(Swar32, "6E 61 6D 65 20", 5);
        do_test_count!(Swar32, "? 20 ? ? 69", 64);
        do_test_count!(Swar32, "20 61 ? 64 ? 6F 6D", 1);
        do_test_count!(Swar32, "20 48 6F 72 72 6F 72", 1);
        do_test_count!(Swar32, "0A 20 ? 54 69 6C", 2);
        do_test_count!(Swar32, "67 68 74 6C 79 20 73", 1);
        do_test_count!(Swar32, "65 6E 2C 20 74 68 ?", 1);
        do_test_count!(Swar32, "6C 79 2C 20 67 61", 1);
        do_test_count!(Swar32, "20 69 66 2C", 1);
        do_test_count!(Swar32, "E2 80 9C 4E ? 76", 8);
        do_test_count!(Swar32, "65 20 66 6C 6F 77 6E", 2);
        do_test_count!(Swar32, "? 20 20 20 ? 20 20", 576);
        do_test_count!(Swar32, "20 ? ? 66 6F", 5);
        do_test_count!(Swar32, "72 ? ? 73", 10);
        do_test_count!(Swar32, "61 72 74 ?", 8);
        do_test_count!(Swar32, "80 9C 4C 65 6E 6F", 2);
        do_test_count!(Swar32, "20 69 66 20 69 74", 1);
        do_test_count!(Swar32, "? 20 73 61 ?", 15);
        do_test_count!(Swar32, "66 ? 20 77 69", 1);
        do_test_count!(Swar32, "65 6D 70 65 73 74", 2);
        do_test_count!(Swar32, "20 ? 20 51 75 6F 74", 5);
        do_test_count!(Swar32, "? 61 74 65 ? 76 69", 1);
        do_test_count!(Swar32, "2C ? 0A 20 20 4F", 1);
        do_test_count!(Swar32, "0A 20 20 E2 80 9C", 11);
        do_test_pos!(
            Swar32,
            "6E ? 20 ? ?",
            [
                97, 117, 126, 130, 250, 274, 347, 421, 429, 506, 526, 718, 726, 839, 873, 1002,
                1070, 1133, 1215, 1223, 1281, 1289, 1404, 1439, 1476, 1627, 1635, 1684, 1740, 1750,
                1819, 1823, 1845, 1859, 1949, 2052, 2060, 2165, 2175, 2248, 2291, 2311, 2369, 2373,
                2452, 2456, 2464, 2496, 2515, 2525, 2537, 2864, 2873, 2881, 2907, 2922, 2969, 3047,
                3055, 3087, 3115, 3123, 3139, 3292, 3472, 3480, 3487, 3535, 3725, 3733, 3784, 3817,
                3848, 4262, 4350, 4385, 4563, 4641, 4653, 4662, 4677, 4759, 4842, 4907, 4919, 4980,
                5079, 5167, 5237, 5266, 5356, 5428, 5533, 5569, 5599, 5666, 5679, 5762, 5816, 5984,
                6192, 6246, 6418, 6512, 6520, 6614, 6753, 6796, 6956, 7046, 7072, 7083, 7201, 7231,
                7270, 7306, 7346, 7394
            ],
            THE_RAVEN
        );
        do_test_pos!(Swar32, "70 61 72 74 ?", [6662], THE_RAVEN);
        do_test_pos!(Swar32, "6F 72 65 73 20", [], THE_RAVEN);
    }

    #[test]
    fn test_scalar() {
        do_test_count!(Scalar, "?", 7478);
        do_test_count!(Scalar, "20", 1810);
        do_test_count!(Scalar, "20", 1810);
        do_test_count!(Scalar, "20 20", 738);
        do_test_count!(Scalar, "3B ? 0A", 10);
        do_test_count!(Scalar, "20 64", 35);
        do_test_count!(Scalar, "61 6E 69", 1);
        do_test_count!(Scalar, "6D 65 20", 21);
        do_test_count!(Scalar, "20", 1810);
        do_test_count!(Scalar, "?", 7478);
        do_test_count!(Scalar, "45 61", 1);
        do_test_count!(Scalar, "63 68 61", 12);
        do_test_count!(Scalar, "4C", 12);
        do_test_count!(Scalar, "20 20 54", 18);
        do_test_count!(Scalar, "20 63", 27);
        do_test_count!(Scalar, "6E 67", 79);
        do_test_count!(Scalar, "20 20 20", 630);
        do_test_count!(Scalar, "61 73", 27);
        do_test_count!(Scalar, "68 6F", 20);
        do_test_count!(Scalar, "65 72 20", 31);
        do_test_count!(Scalar, "20 20 20", 630);
        do_test_count!(Scalar, "? 64", 188);
        do_test_count!(Scalar, "75 6E 64", 1);
        do_test_count!(Scalar, "74 68 65", 80);
        do_test_count!(Scalar, "6E 74 61", 1);
        do_test_count!(Scalar, "6D 6F", 26);
        do_test_count!(Scalar, "65 72", 93);
        do_test_count!(Scalar, "62", 84);
        do_test_count!(Scalar, "0A", 133);
        do_test_count!(Scalar, "? 80", 113);
        do_test_pos!(
            Scalar,
            "? 70",
            [
                49, 80, 196, 197, 227, 228, 261, 262, 270, 271, 343, 344, 515, 553, 883, 886, 1027,
                1361, 1395, 1396, 1427, 1428, 1463, 1464, 1472, 1473, 1549, 1651, 1672, 1882, 1900,
                1940, 2161, 2162, 2328, 2390, 2481, 2563, 2564, 2665, 2666, 2719, 2764, 3373, 3463,
                3586, 3598, 3750, 3764, 3834, 4032, 4175, 4183, 4189, 4295, 4296, 4430, 4680, 4993,
                5181, 5251, 5325, 5390, 5437, 5582, 5592, 5603, 5670, 5792, 5829, 5832, 5879, 5904,
                6054, 6129, 6222, 6259, 6262, 6444, 6501, 6661, 6701, 6746, 6804, 6847, 7146, 7279
            ],
            THE_RAVEN
        );
    }
}
