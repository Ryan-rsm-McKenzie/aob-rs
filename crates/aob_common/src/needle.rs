use crate::{
    parsing,
    Error,
    PreparedPrefilter,
    Sealed,
    SerializablePrefilter,
};
use chumsky::{
    primitive::end,
    Parser as _,
};
use regex_automata::{
    dfa::{
        dense::DFA,
        Automaton as _,
    },
    nfa::thompson::{
        WhichCaptures,
        NFA,
    },
    Anchored,
    Input,
};
use regex_syntax::hir::{
    Dot,
    Hir,
};
use std::{
    borrow::Borrow,
    marker::PhantomData,
    ops::Range,
};

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

    // The range of the matching needle, relative to the haystack.
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

    // The actual matched bytes, from the haystack.
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
    fn find_iter<'iter, 'needle: 'iter, 'haystack: 'iter>(
        &'needle self,
        haystack: &'haystack [u8],
    ) -> impl Iterator<Item = Match<'haystack>> + 'iter;

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

struct FindIter<'haystack, 'needle, F, D, T>
where
    F: Borrow<PreparedPrefilter> + 'needle,
    D: Borrow<DFA<T>>,
    T: AsRef<[u32]>,
{
    prefilter: Option<F>,
    haystack: &'haystack [u8],
    needle: D,
    needle_len: usize,
    last_offset: usize,
    _phantom: PhantomData<&'needle T>,
}

impl<'haystack, 'needle, F, D, T> Iterator for FindIter<'haystack, 'needle, F, D, T>
where
    F: Borrow<PreparedPrefilter> + 'needle,
    D: Borrow<DFA<T>>,
    T: AsRef<[u32]>,
{
    type Item = Match<'haystack>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(prefilter) = self
            .prefilter
            .as_ref()
            .and_then(|x| (self.haystack.len() >= x.borrow().min_haystack_len()).then_some(x))
        {
            for prefilter_offset in prefilter
                .borrow()
                .find_iter(&self.haystack[self.last_offset..])
            {
                let start = self.last_offset + prefilter_offset;
                let input = Input::new(self.haystack)
                    .span(start..self.haystack.len())
                    .anchored(Anchored::Yes);
                if let Some(end) = self
                    .needle
                    .borrow()
                    .try_search_fwd(&input)
                    .ok()
                    .flatten()
                    .map(|x| x.offset())
                {
                    self.last_offset = start + 1;
                    return Some(Match {
                        range: (start, end),
                        haystack: self.haystack,
                    });
                }
            }
        } else {
            let input = Input::new(self.haystack)
                .span(self.last_offset..self.haystack.len())
                .anchored(Anchored::No);
            if let Some(end) = self
                .needle
                .borrow()
                .try_search_fwd(&input)
                .ok()
                .flatten()
                .map(|x| x.offset())
            {
                let start = end - self.needle_len;
                self.last_offset = start + 1;
                return Some(Match {
                    range: (start, end),
                    haystack: self.haystack,
                });
            }
        }

        self.last_offset = self.haystack.len();
        None
    }
}

#[derive(Clone, Debug)]
#[repr(C, align(4))]
struct DFAStorage<const N: usize>([u8; N]);

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
pub struct StaticNeedle<const DFA_LEN: usize, const NEEDLE_LEN: usize> {
    dfa_bytes: DFAStorage<DFA_LEN>,
    prefilter: Option<SerializablePrefilter>,
}

impl<const DFA_LEN: usize, const NEEDLE_LEN: usize> StaticNeedle<DFA_LEN, NEEDLE_LEN> {
    /// I will german suplex you if you use this hidden method.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        serialized_dfa: [u8; DFA_LEN],
        prefilter: Option<SerializablePrefilter>,
    ) -> Self {
        Self {
            dfa_bytes: DFAStorage(serialized_dfa),
            prefilter,
        }
    }
}

impl<const DFA_LEN: usize, const NEEDLE_LEN: usize> Sealed for StaticNeedle<DFA_LEN, NEEDLE_LEN> {}

impl<const DFA_LEN: usize, const NEEDLE_LEN: usize> Needle for StaticNeedle<DFA_LEN, NEEDLE_LEN> {
    fn find_iter<'iter, 'needle: 'iter, 'haystack: 'iter>(
        &'needle self,
        haystack: &'haystack [u8],
    ) -> impl Iterator<Item = Match<'haystack>> + 'iter {
        // SAFETY:
        // * These bytes come from DFA::to_bytes_*_endian
        // * The dfa was serialized at compile-time, and converted to the target (runtime) endianness using cfg(target_endian)
        // * dfa_bytes is repr(C) with align of 4 bytes (same as u32)
        let needle = unsafe {
            DFA::from_bytes_unchecked(&self.dfa_bytes.0)
                .unwrap_unchecked()
                .0
        };
        let prefilter = match self.prefilter {
            None => None,
            Some(SerializablePrefilter::Prefix { prefix }) => {
                Some(PreparedPrefilter::from_prefix(prefix))
            }
            Some(SerializablePrefilter::PrefixPostfix {
                prefix,
                prefix_offset,
                postfix,
                postfix_offset,
            }) => {
                let mut needle = [0u8; NEEDLE_LEN];
                needle[usize::from(prefix_offset)] = prefix;
                needle[usize::from(postfix_offset)] = postfix;
                Some(PreparedPrefilter::from_prefix_postfix(
                    &needle,
                    prefix_offset.into(),
                    postfix_offset.into(),
                ))
            }
        };
        FindIter {
            prefilter,
            haystack,
            needle,
            needle_len: NEEDLE_LEN,
            last_offset: 0,
            _phantom: PhantomData,
        }
    }

    fn len(&self) -> usize {
        DFA_LEN
    }
}

/// The run-time variant of a [`Needle`].
#[derive(Clone, Debug)]
pub struct DynamicNeedle {
    dfa: DFA<Vec<u32>>,
    length: usize,
    prefilter: Option<PreparedPrefilter>,
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
        let nfa = {
            let mut nodes = Vec::<Hir>::new();
            for &byte in bytes {
                let node = match byte {
                    Some(b) => Hir::literal([b]),
                    None => Hir::dot(Dot::AnyByte),
                };
                nodes.push(node);
            }
            let hir = Hir::concat(nodes);
            let config = NFA::config()
                .utf8(false)
                .shrink(true)
                .which_captures(WhichCaptures::None);
            NFA::compiler()
                .configure(config)
                .build_from_hir(&hir)
                .unwrap()
        };
        let config = DFA::config().minimize(true).accelerate(false);
        let dfa = DFA::builder()
            .configure(config)
            .build_from_nfa(&nfa)
            .expect("a needle's syntax has already been verified at this point, and thus converting it into a dfa should never fail");
        let prefilter = {
            let faux_needle = bytes
                .iter()
                .map(|x| x.unwrap_or_default())
                .collect::<Vec<_>>();
            let mut real_bytes = bytes
                .iter()
                .enumerate()
                .filter_map(|(offset, &byte)| byte.is_some().then_some(offset));
            let prefix = real_bytes.next();
            let postfix = prefix.and_then(|prefix_offset| {
                let prefix = faux_needle[prefix_offset];
                real_bytes.filter(|&i| faux_needle[i] != prefix).last()
            });
            match (prefix, postfix) {
                (Some(prefix), None) => Some(PreparedPrefilter::from_prefix(faux_needle[prefix])),
                (Some(prefix), Some(postfix)) => Some(PreparedPrefilter::from_prefix_postfix(
                    &faux_needle,
                    prefix,
                    postfix,
                )),
                _ => None,
            }
        };
        Self {
            dfa,
            length: bytes.len(),
            prefilter,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn serialize_dfa_with_target_endianness(&self) -> Vec<u8> {
        let (bytes, _) = if cfg!(target_endian = "little") {
            self.dfa.to_bytes_little_endian()
        } else {
            self.dfa.to_bytes_big_endian()
        };
        bytes
    }

    #[doc(hidden)]
    #[must_use]
    pub fn serialize_prefilter(&self) -> Option<SerializablePrefilter> {
        self.prefilter.as_ref().map(Into::into)
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn prefilter(&self) -> Option<&PreparedPrefilter> {
        self.prefilter.as_ref()
    }
}

impl Sealed for DynamicNeedle {}

impl Needle for DynamicNeedle {
    fn find_iter<'iter, 'needle: 'iter, 'haystack: 'iter>(
        &'needle self,
        haystack: &'haystack [u8],
    ) -> impl Iterator<Item = Match<'haystack>> + 'iter {
        FindIter {
            prefilter: self.prefilter.as_ref(),
            haystack,
            needle: &self.dfa,
            needle_len: self.length,
            last_offset: 0,
            _phantom: PhantomData,
        }
    }

    fn len(&self) -> usize {
        self.length
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
