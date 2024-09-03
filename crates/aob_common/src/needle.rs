use crate::Error;
use chumsky::{
    primitive::end,
    Parser,
};

pub trait Index: Copy + Eq {
    const MAX: Self;
    const ZERO: Self;

    #[must_use]
    fn increment(&mut self) -> usize;

    #[must_use]
    fn into_usize(self) -> usize;
}

macro_rules! impl_index {
    ($t:ty) => {
        // the needle is at most as large as usize
        #[allow(clippy::cast_possible_truncation)]
        impl Index for $t {
            const MAX: Self = Self::MAX;
            const ZERO: Self = 0;

            fn increment(&mut self) -> usize {
                *self += 1;
                *self as usize
            }

            fn into_usize(self) -> usize {
                self as usize
            }
        }
    };
}

impl_index!(u8);
impl_index!(u16);
impl_index!(u32);
impl_index!(u64);
impl_index!(usize);

struct Wildcards<'a>(&'a [u8]);

impl<'a> Wildcards<'a> {
    #[must_use]
    fn is_wildcard(&self, pos: usize) -> bool {
        (self.0[pos / 8] >> (pos % 8)) & 0b1 != 0
    }
}

struct FindIter<'haystack, 'needle, I: Index> {
    haystack: &'haystack [u8],
    table: &'needle [I],
    word: &'needle [u8],
    wildcards: Wildcards<'needle>,
    j: usize,
    k: I,
}

impl<'needle, 'haystack, I: Index> Iterator for FindIter<'haystack, 'needle, I> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            haystack,
            table,
            word,
            wildcards,
            ref mut j,
            ref mut k,
        } = self;

        while *j < haystack.len() {
            let pos = k.into_usize();
            if word[pos] == haystack[*j] || wildcards.is_wildcard(pos) {
                *j += 1;
                let pos = k.increment();
                if pos == word.len() {
                    *k = table[pos];
                    return Some(*j - pos);
                }
            } else {
                *k = table[pos];
                if *k == I::MAX {
                    *j += 1;
                    *k = I::ZERO;
                }
            }
        }

        None
    }
}

pub trait Needle {
    #[must_use]
    fn find(&self, haystack: &[u8]) -> Option<usize> {
        self.find_iter(haystack).next()
    }

    #[must_use]
    fn find_iter<'iter, 'needle: 'iter, 'haystack: 'iter>(
        &'needle self,
        haystack: &'haystack [u8],
    ) -> impl Iterator<Item = usize> + 'iter;
}

pub struct StaticNeedle<I: Index, const X: usize, const Y: usize, const Z: usize> {
    table: [I; X],
    word: [u8; Y],
    wildcards: [u8; Z],
}

impl<I: Index, const X: usize, const Y: usize, const Z: usize> StaticNeedle<I, X, Y, Z> {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(table: [I; X], word: [u8; Y], wildcards: [u8; Z]) -> Self {
        Self {
            table,
            word,
            wildcards,
        }
    }
}

impl<I: Index, const X: usize, const Y: usize, const Z: usize> Needle for StaticNeedle<I, X, Y, Z> {
    fn find_iter<'iter, 'needle: 'iter, 'haystack: 'iter>(
        &'needle self,
        haystack: &'haystack [u8],
    ) -> impl Iterator<Item = usize> + 'iter {
        FindIter {
            haystack,
            table: &self.table,
            word: &self.word,
            wildcards: Wildcards(&self.wildcards),
            j: 0,
            k: I::ZERO,
        }
    }
}

pub struct DynamicNeedle {
    table: Box<[usize]>,
    buffer: Box<[u8]>,
    wildcards_offset: usize,
}

impl DynamicNeedle {
    pub fn from_ida(pattern: &str) -> Result<Self, Error<'_>> {
        let parser = crate::ida_pattern().then_ignore(end());
        match parser.parse(pattern) {
            Ok(ok) => Ok(Self::from_bytes(&ok)),
            Err(errors) => {
                let error = errors.first().unwrap();
                Err(Error {
                    source: pattern,
                    span: error.span().into(),
                })
            }
        }
    }

    #[must_use]
    pub fn from_bytes(bytes: &[Option<u8>]) -> Self {
        let num_bytes = bytes.len();
        let wildcards_offset = num_bytes;
        Self {
            table: Self::build_table(bytes),
            buffer: {
                let len = num_bytes + (num_bytes + 7) / 8;
                let mut v = Vec::with_capacity(len);
                v.resize_with(len, Default::default);
                for (l, r) in v[..wildcards_offset].iter_mut().zip(bytes) {
                    *l = r.unwrap_or_default();
                }
                Self::build_wildcards(bytes, &mut v[wildcards_offset..]);
                v.into_boxed_slice()
            },
            wildcards_offset,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn table_slice(&self) -> &[usize] {
        &self.table
    }

    #[doc(hidden)]
    #[must_use]
    pub fn word_slice(&self) -> &[u8] {
        &self.buffer[..self.wildcards_offset]
    }

    #[doc(hidden)]
    #[must_use]
    pub fn wildcards_slice(&self) -> &[u8] {
        &self.buffer[self.wildcards_offset..]
    }

    /// <https://en.wikipedia.org/wiki/Knuth–Morris–Pratt_algorithm#Description_of_pseudocode_for_the_table-building_algorithm>
    #[must_use]
    fn build_table(word: &[Option<u8>]) -> Box<[usize]> {
        let mut pos: usize = 1;
        let mut cnd: usize = 0;
        let mut table = {
            let len = word.len() + 1;
            let mut v = Vec::with_capacity(len);
            v.resize_with(len, Default::default);
            v.into_boxed_slice()
        };
        #[allow(clippy::unnested_or_patterns)]
        let compare_eq = |left: Option<u8>, right: Option<u8>| match (left, right) {
            (None, None) | (None, Some(_)) | (Some(_), None) => true,
            (Some(left), Some(right)) => left == right,
        };

        table[0] = usize::MAX;
        while pos < word.len() {
            if compare_eq(word[pos], word[cnd]) {
                table[pos] = table[cnd];
            } else {
                table[pos] = cnd;
                while cnd != usize::MAX && !compare_eq(word[pos], word[cnd]) {
                    cnd = table[cnd];
                }
            }
            pos += 1;
            cnd = cnd.wrapping_add(1);
        }

        table[pos] = cnd;
        table
    }

    fn build_wildcards(bytes: &[Option<u8>], dst: &mut [u8]) {
        for (i, byte) in bytes.iter().enumerate() {
            let bit = match byte {
                Some(_) => 0,
                None => 1,
            };
            dst[i / 8] |= bit << (i % 8);
        }
    }
}

impl Needle for DynamicNeedle {
    fn find_iter<'iter, 'needle: 'iter, 'haystack: 'iter>(
        &'needle self,
        haystack: &'haystack [u8],
    ) -> impl Iterator<Item = usize> + 'iter {
        FindIter {
            haystack,
            table: self.table_slice(),
            word: self.word_slice(),
            wildcards: Wildcards(self.wildcards_slice()),
            j: 0,
            k: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DynamicNeedle;

    /// <https://en.wikipedia.org/wiki/Knuth–Morris–Pratt_algorithm#Working_example_of_the_table-building_algorithm>
    #[test]
    fn test_from_ida() {
        assert!(DynamicNeedle::from_ida("4_ 42 41 43 41 42 41 42 43").is_err());

        let needle = DynamicNeedle::from_ida("41 42 41 43 41 42 41 42 43").unwrap();
        assert_eq!(
            needle.table_slice(),
            [
                usize::MAX,
                0,
                usize::MAX,
                1,
                usize::MAX,
                0,
                usize::MAX,
                3,
                2,
                0
            ]
        );
        assert_eq!(needle.word_slice(), b"ABACABABC");
        assert_eq!(needle.wildcards_slice(), [0, 0]);

        let needle = DynamicNeedle::from_ida("41 42 41 43 41 42 41 42 41").unwrap();
        assert_eq!(
            needle.table_slice(),
            [
                usize::MAX,
                0,
                usize::MAX,
                1,
                usize::MAX,
                0,
                usize::MAX,
                3,
                usize::MAX,
                3
            ]
        );
        assert_eq!(needle.word_slice(), b"ABACABABA");
        assert_eq!(needle.wildcards_slice(), [0, 0]);

        let needle = DynamicNeedle::from_ida(
            "50 41 52 54 49 43 49 50 41 54 45 20 49 4E 20 50 41 52 41 43 48 55 54 45",
        )
        .unwrap();
        assert_eq!(
            needle.table_slice(),
            [
                usize::MAX,
                0,
                0,
                0,
                0,
                0,
                0,
                usize::MAX,
                0,
                2,
                0,
                0,
                0,
                0,
                0,
                usize::MAX,
                0,
                0,
                3,
                0,
                0,
                0,
                0,
                0,
                0
            ]
        );
        assert_eq!(needle.word_slice(), b"PARTICIPATE IN PARACHUTE");
        assert_eq!(needle.wildcards_slice(), [0, 0, 0]);

        let needle = DynamicNeedle::from_ida("11 ? ? 22 ? 33 44 ?").unwrap();
        assert_eq!(
            needle.word_slice(),
            [0x11, 0x00, 0x00, 0x22, 0x00, 0x33, 0x44, 0x00]
        );
        assert_eq!(needle.wildcards_slice(), [0b10010110]);
    }
}
