use crate::pattern::PatternRef;
use memchr::arch::{
    all::packedpair::{
        Finder as GenericFinder,
        Pair as PackedPair,
    },
    x86_64::{
        avx2::packedpair::Finder as Avx2Finder,
        sse2::packedpair::Finder as Sse2Finder,
    },
};

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawPrefilter {
    Length {
        len: usize,
    },
    Prefix {
        prefix: u8,
    },
    PrefixPostfix {
        prefix: u8,
        prefix_offset: u8,
        postfix: u8,
        postfix_offset: u8,
    },
}

impl From<&CompiledPrefilter> for RawPrefilter {
    fn from(value: &CompiledPrefilter) -> Self {
        match value.inner {
            Inner::Length { len } => RawPrefilter::Length { len },
            Inner::Prefix { prefix } => RawPrefilter::Prefix { prefix },
            Inner::GenericPrefixPostfix {
                finder,
                prefix,
                postfix,
            } => RawPrefilter::PrefixPostfix {
                prefix,
                prefix_offset: finder.pair().index1(),
                postfix,
                postfix_offset: finder.pair().index2(),
            },
            Inner::Sse2PrefixPostfix {
                finder,
                prefix,
                postfix,
            } => RawPrefilter::PrefixPostfix {
                prefix,
                prefix_offset: finder.pair().index1(),
                postfix,
                postfix_offset: finder.pair().index2(),
            },
            Inner::Avx2PrefixPostfix {
                finder,
                prefix,
                postfix,
            } => RawPrefilter::PrefixPostfix {
                prefix,
                prefix_offset: finder.pair().index1(),
                postfix,
                postfix_offset: finder.pair().index2(),
            },
        }
    }
}

#[derive(Clone, Debug)]
enum Inner {
    Length {
        len: usize,
    },
    Prefix {
        prefix: u8,
    },
    GenericPrefixPostfix {
        finder: GenericFinder,
        prefix: u8,
        postfix: u8,
    },
    Sse2PrefixPostfix {
        finder: Sse2Finder,
        prefix: u8,
        postfix: u8,
    },
    Avx2PrefixPostfix {
        finder: Avx2Finder,
        prefix: u8,
        postfix: u8,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct CompiledPrefilter {
    inner: Inner,
}

impl CompiledPrefilter {
    #[must_use]
    pub(crate) fn from_bytes(pattern: PatternRef<'_>) -> Self {
        let word = pattern.word_slice();
        let mask = pattern.mask_slice();
        let Some(prefix_offset) = mask
            .iter()
            .enumerate()
            .find_map(|(offset, &mask)| mask.is_unmasked().then_some(offset))
        else {
            // no prefix? they're all wildcards (or empty)
            return Self::from_length(pattern.len());
        };

        let prefix = word[prefix_offset];
        let Some(postfix_offset) = mask
            .iter()
            .zip(word)
            .enumerate()
            .filter_map(|(offset, (&mask, &byte))| {
                (mask.is_unmasked() && byte != prefix).then_some(offset)
            })
            .last()
        else {
            return Self::from_prefix(prefix);
        };

        Self::from_prefix_postfix(word, prefix_offset, postfix_offset)
    }

    #[must_use]
    pub(crate) fn from_length(len: usize) -> Self {
        Self {
            inner: Inner::Length { len },
        }
    }

    #[must_use]
    pub(crate) fn from_prefix(prefix: u8) -> Self {
        Self {
            inner: Inner::Prefix { prefix },
        }
    }

    #[must_use]
    pub(crate) fn from_prefix_postfix(
        needle: &[u8],
        prefix_offset: usize,
        postfix_offset: usize,
    ) -> Self {
        let inner =
            if let Some(pair) = Self::try_make_packed_pair(needle, prefix_offset, postfix_offset) {
                let prefix = needle[prefix_offset];
                let postfix = needle[postfix_offset];
                if let Some(finder) = Avx2Finder::with_pair(needle, pair) {
                    Inner::Avx2PrefixPostfix {
                        finder,
                        prefix,
                        postfix,
                    }
                } else if let Some(finder) = Sse2Finder::with_pair(needle, pair) {
                    Inner::Sse2PrefixPostfix {
                        finder,
                        prefix,
                        postfix,
                    }
                } else if let Some(finder) = GenericFinder::with_pair(needle, pair) {
                    Inner::GenericPrefixPostfix {
                        finder,
                        prefix,
                        postfix,
                    }
                } else {
                    Inner::Prefix {
                        prefix: needle[prefix_offset],
                    }
                }
            } else {
                Inner::Prefix {
                    prefix: needle[prefix_offset],
                }
            };

        Self { inner }
    }

    #[must_use]
    pub(crate) fn find_iter<'haystack, 'prefilter>(
        &'prefilter self,
        haystack: &'haystack [u8],
    ) -> Iter<'haystack, 'prefilter> {
        Iter {
            haystack,
            prefilter: self,
            last_offset: 0,
        }
    }

    #[must_use]
    pub(crate) fn min_haystack_len(&self) -> usize {
        match self.inner {
            Inner::Length { len: _ }
            | Inner::Prefix { prefix: _ }
            | Inner::GenericPrefixPostfix {
                finder: _,
                prefix: _,
                postfix: _,
            } => 0,
            Inner::Sse2PrefixPostfix {
                finder,
                prefix: _,
                postfix: _,
            } => finder.min_haystack_len(),
            Inner::Avx2PrefixPostfix {
                finder,
                prefix: _,
                postfix: _,
            } => finder.min_haystack_len(),
        }
    }

    #[must_use]
    fn find(&self, haystack: &[u8]) -> Option<usize> {
        match self.inner {
            Inner::Length { len } => {
                if haystack.len() >= len {
                    Some(0)
                } else {
                    None
                }
            }
            Inner::Prefix { prefix } => memchr::memchr(prefix, haystack),
            Inner::GenericPrefixPostfix {
                finder,
                prefix: _,
                postfix: _,
            } => finder.find_prefilter(haystack),
            Inner::Sse2PrefixPostfix {
                finder,
                prefix: _,
                postfix: _,
            } => finder.find_prefilter(haystack),
            Inner::Avx2PrefixPostfix {
                finder,
                prefix: _,
                postfix: _,
            } => finder.find_prefilter(haystack),
        }
    }

    #[must_use]
    fn try_make_packed_pair(
        needle: &[u8],
        prefix_offset: usize,
        postfix_offset: usize,
    ) -> Option<PackedPair> {
        if let Ok(prefix_offset) = prefix_offset.try_into() {
            if let Ok(postfix_offset) = postfix_offset.try_into() {
                return PackedPair::with_indices(needle, prefix_offset, postfix_offset);
            }
        }
        None
    }
}

pub(crate) struct Iter<'haystack, 'prefilter> {
    haystack: &'haystack [u8],
    prefilter: &'prefilter CompiledPrefilter,
    last_offset: usize,
}

impl<'haystack, 'prefilter> Iterator for Iter<'haystack, 'prefilter> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(pos) = self.prefilter.find(&self.haystack[self.last_offset..]) {
            let pos = pos + self.last_offset;
            self.last_offset = pos + 1;
            Some(pos)
        } else {
            self.last_offset = self.haystack.len();
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::RawPrefilter;
    use crate::DynamicNeedle;

    #[test]
    fn test_prefilter() {
        macro_rules! make_prefilter {
            ($($bytes:tt)+) => {
                {
                    let x: RawPrefilter = DynamicNeedle::from_bytes(&[$($bytes)+]).prefilter().into();
                    x
                }
            };
        }

        let pre = make_prefilter![Some(0x11), Some(0x22), Some(0x33)];
        assert_eq!(
            pre,
            RawPrefilter::PrefixPostfix {
                prefix: 0x11,
                prefix_offset: 0,
                postfix: 0x33,
                postfix_offset: 2
            }
        );

        let pre = make_prefilter![Some(0x11), Some(0x11), Some(0x11)];
        assert_eq!(pre, RawPrefilter::Prefix { prefix: 0x11 });

        let pre = make_prefilter![Some(0x11), None, Some(0x33)];
        assert_eq!(
            pre,
            RawPrefilter::PrefixPostfix {
                prefix: 0x11,
                prefix_offset: 0,
                postfix: 0x33,
                postfix_offset: 2,
            }
        );

        let pre = make_prefilter![None, None, Some(0x33)];
        assert_eq!(pre, RawPrefilter::Prefix { prefix: 0x33 });

        let pre = make_prefilter![Some(0x11), Some(0x22), Some(0x11)];
        assert_eq!(
            pre,
            RawPrefilter::PrefixPostfix {
                prefix: 0x11,
                prefix_offset: 0,
                postfix: 0x22,
                postfix_offset: 1
            }
        );

        let pre = make_prefilter![None, Some(0x22), Some(0x33)];
        assert_eq!(
            pre,
            RawPrefilter::PrefixPostfix {
                prefix: 0x22,
                prefix_offset: 1,
                postfix: 0x33,
                postfix_offset: 2
            }
        );
    }
}
