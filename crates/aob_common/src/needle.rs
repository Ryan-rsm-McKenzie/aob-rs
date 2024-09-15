use crate::{
    parsing,
    pattern::{
        DynamicPattern,
        Method,
        PatternRef,
        StaticPattern,
    },
    prefilter::{
        CompiledPrefilter,
        PrefilterError,
    },
    Error,
    RawPrefilter,
    Sealed,
};
use chumsky::{
    primitive::end,
    Parser as _,
};
use std::ops::Range;

/// Represents a matching [`Needle`] found in the haystack.
#[derive(Clone, Copy, Debug)]
pub struct Match<'haystack> {
    range: (usize, usize),
    haystack: &'haystack [u8],
}

impl<'haystack> Match<'haystack> {
    /// The position of the first byte in the matching needle, relative to the haystack.
    ///
    /// ```
    /// # use aob_common::{DynamicNeedle, Needle as _};
    /// let needle = DynamicNeedle::from_ida("63 ? 74").unwrap();
    /// let haystack = "a_cat_tries";
    /// let matched = needle.find(haystack.as_bytes()).unwrap();
    /// assert_eq!(matched.start(), 2);
    /// ```
    #[must_use]
    pub fn start(&self) -> usize {
        self.range.0
    }

    /// The position of the last byte past the end of the matching needle, relative to the haystack.
    ///
    /// ```
    /// # use aob_common::{DynamicNeedle, Needle as _};
    /// let needle = DynamicNeedle::from_ida("63 ? 74").unwrap();
    /// let haystack = "a_cat_tries";
    /// let matched = needle.find(haystack.as_bytes()).unwrap();
    /// assert_eq!(matched.end(), 5);
    /// ```
    #[must_use]
    pub fn end(&self) -> usize {
        self.range.1
    }

    /// The range of the matching needle, relative to the haystack.
    ///
    /// ```
    /// # use aob_common::{DynamicNeedle, Needle as _};
    /// let needle = DynamicNeedle::from_ida("63 ? 74").unwrap();
    /// let haystack = "a_cat_tries";
    /// let matched = needle.find(haystack.as_bytes()).unwrap();
    /// assert_eq!(matched.range(), 2..5);
    /// ```
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        self.start()..self.end()
    }

    /// The actual matched bytes, from the haystack.
    ///
    /// ```
    /// # use aob_common::{DynamicNeedle, Needle as _};
    /// let needle = DynamicNeedle::from_ida("63 ? 74").unwrap();
    /// let haystack = "a_cat_tries";
    /// let matched = needle.find(haystack.as_bytes()).unwrap();
    /// assert_eq!(matched.as_bytes(), &b"cat"[..]);
    /// ```
    #[must_use]
    pub fn as_bytes(&self) -> &'haystack [u8] {
        &self.haystack[self.range()]
    }
}

/// The common interface for searching haystacks with needles.
///
/// A successful search will yield a [`Match`] in the haystack, whose length is equal to the [length](Needle::len) of the needle. Matches may overlap.
///
/// ```
/// # use aob_common::{DynamicNeedle, Needle as _};
/// let needle = DynamicNeedle::from_ida("12 23 ? 12").unwrap();
/// let haystack = [0x32, 0x21, 0x12, 0x23, 0xAB, 0x12, 0x23, 0xCD, 0x12];
/// let mut iter = needle.find_iter(&haystack);
/// assert_eq!(&haystack[iter.next().unwrap().start()..], [0x12, 0x23, 0xAB, 0x12, 0x23, 0xCD, 0x12]);
/// assert_eq!(&haystack[iter.next().unwrap().start()..], [0x12, 0x23, 0xCD, 0x12]);
/// assert!(iter.next().is_none());
/// ```
#[allow(clippy::len_without_is_empty)]
pub trait Needle: Sealed {
    /// A convenience method for getting only the first match.
    #[must_use]
    fn find<'haystack>(&self, haystack: &'haystack [u8]) -> Option<Match<'haystack>> {
        self.find_iter(haystack).next()
    }

    /// Finds all matching subsequences, iteratively.
    #[must_use]
    fn find_iter<'needle, 'haystack>(
        &'needle self,
        haystack: &'haystack [u8],
    ) -> Find<'needle, 'haystack>;

    /// The length of the needle itself.
    ///
    /// ```
    /// # use aob_common::{DynamicNeedle, Needle as _};
    /// let needle = DynamicNeedle::from_ida("12 ? 56 ? 9A BC").unwrap();
    /// assert_eq!(needle.len(), 6);
    /// ```
    #[must_use]
    fn len(&self) -> usize;
}

pub struct Find<'needle, 'haystack> {
    prefilter: CompiledPrefilter,
    pattern: PatternRef<'needle>,
    haystack: &'haystack [u8],
    last_offset: usize,
}

impl Find<'_, '_> {
    #[must_use]
    pub fn search_method(&self) -> Method {
        self.pattern.method()
    }
}

impl<'haystack> Iterator for Find<'_, 'haystack> {
    type Item = Match<'haystack>;

    fn next(&mut self) -> Option<Self::Item> {
        macro_rules! failure {
            () => {{
                self.last_offset = self.haystack.len();
                return None;
            }};
        }

        macro_rules! success {
            ($start:ident, $end:ident) => {{
                self.last_offset = $start + 1;
                return Some(Match {
                    range: ($start, $end),
                    haystack: self.haystack,
                });
            }};
        }

        let mut prefilter_iter = self.prefilter.find_iter(&self.haystack[self.last_offset..]);
        loop {
            let prefilter_offset = match prefilter_iter.next() {
                Some(Ok(offset)) => offset,
                Some(Err(PrefilterError::HaystackTooSmall { offset })) => {
                    self.last_offset += offset;
                    break;
                }
                None => failure!(),
            };
            let start = self.last_offset + prefilter_offset;
            let end = start + self.pattern.len();
            let Some(haystack) = &self.haystack.get(start..end) else {
                failure!();
            };
            if self.pattern.compare_eq(haystack) {
                success!(start, end);
            }
        }

        for (window_offset, window) in self.haystack[self.last_offset..]
            .windows(self.pattern.len())
            .enumerate()
        {
            if self.pattern.compare_eq(window) {
                let start = self.last_offset + window_offset;
                let end = start + self.pattern.len();
                success!(start, end);
            }
        }

        failure!();
    }
}

/// The compile-time variant of a [`Needle`].
///
/// [`StaticNeedle`] is intended for embedding into executables at compile-time,
/// such that no allocations or validation is needed to perform a match on a
/// haystack at runtime.
///
/// You should never need to name this type directly:
/// * If you need to instantiate one, please use the `aob!` macro instead.
/// * If you need to use one in an api, please use the [`Needle`] trait instead.
#[derive(Clone, Debug)]
pub struct StaticNeedle<const NEEDLE_LEN: usize, const BUFFER_LEN: usize> {
    prefilter: RawPrefilter,
    pattern: StaticPattern<NEEDLE_LEN, BUFFER_LEN>,
}

impl<const NEEDLE_LEN: usize, const BUFFER_LEN: usize> StaticNeedle<NEEDLE_LEN, BUFFER_LEN> {
    /// I will german suplex you if you use this hidden method.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        prefilter: RawPrefilter,
        word: [u8; BUFFER_LEN],
        mask: [u8; BUFFER_LEN],
    ) -> Self {
        Self {
            prefilter,
            pattern: StaticPattern::from_components(word, mask),
        }
    }
}

impl<const NEEDLE_LEN: usize, const BUFFER_LEN: usize> Sealed
    for StaticNeedle<NEEDLE_LEN, BUFFER_LEN>
{
}

impl<const NEEDLE_LEN: usize, const BUFFER_LEN: usize> Needle
    for StaticNeedle<NEEDLE_LEN, BUFFER_LEN>
{
    fn find_iter<'needle, 'haystack>(
        &'needle self,
        haystack: &'haystack [u8],
    ) -> Find<'needle, 'haystack> {
        let pattern: PatternRef<'_> = (&self.pattern).into();
        let prefilter = match self.prefilter {
            RawPrefilter::Length { len } => CompiledPrefilter::from_length(len),
            RawPrefilter::Prefix { prefix } => CompiledPrefilter::from_prefix(prefix),
            RawPrefilter::PrefixPostfix {
                prefix: _,
                prefix_offset,
                postfix: _,
                postfix_offset,
            } => CompiledPrefilter::from_prefix_postfix(
                pattern.word_slice(),
                prefix_offset.into(),
                postfix_offset.into(),
            ),
        };
        Find {
            prefilter,
            pattern,
            haystack,
            last_offset: 0,
        }
    }

    fn len(&self) -> usize {
        NEEDLE_LEN
    }
}

/// The run-time variant of a [`Needle`].
#[derive(Clone, Debug)]
pub struct DynamicNeedle {
    prefilter: CompiledPrefilter,
    pattern: DynamicPattern,
}

impl DynamicNeedle {
    /// Construct a [`DynamicNeedle`] using an Ida style pattern.
    ///
    /// # Syntax
    /// Expects a sequence of `byte` or `wildcard` separated by whitespace, where:
    /// * `byte` is exactly 2 hexadecimals (uppercase or lowercase), indicating an exact match
    /// * `wildcard` is one or two `?` characters, indicating a fuzzy match
    ///
    /// # Example
    /// ```
    /// # use aob_common::{DynamicNeedle, Needle as _};
    /// let needle = DynamicNeedle::from_ida("78 ? BC").unwrap();
    /// let haystack = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE];
    /// let matched = needle.find(&haystack).unwrap();
    /// assert_eq!(&haystack[matched.start()..], [0x78, 0x9A, 0xBC, 0xDE]);
    /// ```
    pub fn from_ida(pattern: &str) -> Result<Self, Error<'_>> {
        let parser = parsing::ida_pattern().then_ignore(end());
        match parser.parse(pattern) {
            Ok(ok) => Ok(Self::from_bytes(&ok)),
            Err(mut errors) => {
                let error = errors
                    .drain(..)
                    .next()
                    .expect("failure to parse should produce at least one error");
                Err(Error {
                    source: pattern,
                    inner: error,
                })
            }
        }
    }

    /// Contruct a [`DynamicNeedle`] using raw bytes, in plain Rust.
    ///
    /// # Syntax
    /// Expects an array of `Option<u8>`, where:
    /// * `Some(_)` indicates an exact match
    /// * `None` indicates a fuzzy match
    ///
    /// # Example
    /// ```
    /// # use aob_common::{DynamicNeedle, Needle as _};
    /// let needle = DynamicNeedle::from_bytes(&[Some(0x78), None, Some(0xBC)]);
    /// let haystack = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE];
    /// let matched = needle.find(&haystack).unwrap();
    /// assert_eq!(&haystack[matched.start()..], [0x78, 0x9A, 0xBC, 0xDE]);
    /// ```
    #[must_use]
    pub fn from_bytes(bytes: &[Option<u8>]) -> Self {
        let pattern = DynamicPattern::from_bytes(bytes);
        Self {
            prefilter: CompiledPrefilter::from_bytes((&pattern).into()),
            pattern,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn serialize_word(&self) -> &[u8] {
        self.pattern.word_slice_padded()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn serialize_mask(&self) -> &[u8] {
        self.pattern.mask_slice_padded()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn serialize_prefilter(&self) -> RawPrefilter {
        (&self.prefilter).into()
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn prefilter(&self) -> &CompiledPrefilter {
        &self.prefilter
    }
}

impl Sealed for DynamicNeedle {}

impl Needle for DynamicNeedle {
    fn find_iter<'needle, 'haystack>(
        &'needle self,
        haystack: &'haystack [u8],
    ) -> Find<'needle, 'haystack> {
        Find {
            prefilter: self.prefilter.clone(),
            pattern: (&self.pattern).into(),
            haystack,
            last_offset: 0,
        }
    }

    fn len(&self) -> usize {
        self.pattern.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DynamicNeedle,
        Needle as _,
    };

    #[test]
    fn test_from_ida() {
        assert!(DynamicNeedle::from_ida("4_ 42 41 43 41 42 41 42 43").is_err());
        assert!(DynamicNeedle::from_ida("11 ??? 22").is_err());

        macro_rules! test_success {
            ($pattern:literal, $length:literal) => {
                let needle = DynamicNeedle::from_ida($pattern);
                assert!(needle.is_ok(), "\"{}\"", $pattern);
                let needle = needle.unwrap();
                assert_eq!(needle.len(), $length, "\"{}\"", $pattern);
            };
        }

        test_success!("41 42 41 43 41 42 41 42 43", 9);
        test_success!("41 42 41 43 41 42 41 42 41", 9);
        test_success!(
            "50 41 52 54 49 43 49 50 41 54 45 20 49 4E 20 50 41 52 41 43 48 55 54 45",
            24
        );
        test_success!("11 ? ? 22 ? 33 44 ?", 8);
        test_success!("aA Bb 1d", 3);
        test_success!("11 ? 33 ?? 55 ? ?? 88", 8);
    }
}
