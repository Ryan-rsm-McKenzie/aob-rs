use crate::{
    Error,
    Sealed,
};
use chumsky::{
    primitive::end,
    Parser as _,
};
use regex_automata::{
    dfa::{
        dense::{
            Builder,
            Config as DenseConfig,
            DFA,
        },
        Automaton as _,
    },
    util::syntax::Config as SyntaxConfig,
    Input,
};
use std::{
    borrow::Borrow,
    fmt::Write as _,
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

struct FindIter<'haystack, 'needle, D, T>
where
    D: Borrow<DFA<T>>,
    T: AsRef<[u32]>,
{
    haystack: &'haystack [u8],
    needle: D,
    length: usize,
    last_offset: usize,
    _phantom: PhantomData<&'needle T>,
}

impl<'haystack, 'needle, D, T> Iterator for FindIter<'haystack, 'needle, D, T>
where
    D: Borrow<DFA<T>>,
    T: AsRef<[u32]>,
{
    type Item = Match<'haystack>;

    fn next(&mut self) -> Option<Self::Item> {
        let span = self.last_offset..self.haystack.len();
        let input = Input::new(self.haystack).span(span);
        let end = self.needle.borrow().try_search_fwd(&input).ok()??.offset();
        let start = end - self.length;
        self.last_offset = start + 1;
        Some(Match {
            range: (start, end),
            haystack: self.haystack,
        })
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
#[repr(C, align(4))]
pub struct StaticNeedle<const N: usize> {
    // do NOT reorder these, dfa_bytes MUST have 32-bit alignment
    dfa_bytes: [u8; N],
    length: usize,
}

impl<const N: usize> StaticNeedle<N> {
    /// I will german suplex you if you use this hidden method.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(serialized_dfa: [u8; N], needle_len: usize) -> Self {
        Self {
            dfa_bytes: serialized_dfa,
            length: needle_len,
        }
    }
}

impl<const N: usize> Sealed for StaticNeedle<N> {}

impl<const N: usize> Needle for StaticNeedle<N> {
    fn find_iter<'iter, 'needle: 'iter, 'haystack: 'iter>(
        &'needle self,
        haystack: &'haystack [u8],
    ) -> impl Iterator<Item = Match<'haystack>> + 'iter {
        // SAFETY:
        // * These bytes come from DFA::to_bytes_*_endian
        // * The dfa was serialized at compile-time, and converted to the target (runtime) endianness using cfg(target_endian)
        // * Self is repr(C) with align of 4 bytes (same as u32)
        let needle = unsafe {
            DFA::from_bytes_unchecked(&self.dfa_bytes)
                .unwrap_unchecked()
                .0
        };
        FindIter {
            haystack,
            needle,
            length: self.length,
            last_offset: 0,
            _phantom: PhantomData,
        }
    }

    fn len(&self) -> usize {
        self.length
    }
}

/// The run-time variant of a [`Needle`].
pub struct DynamicNeedle {
    dfa: DFA<Vec<u32>>,
    length: usize,
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
        let pattern = {
            let mut string = String::with_capacity(bytes.len() * 4);
            for byte in bytes {
                // writing here should only ever fail if an allocation fails, which should panic instead
                let _ = match byte {
                    Some(b) => write!(string, "\\x{b:X}"),
                    None => write!(string, "."),
                };
            }
            string
        };
        let config = DenseConfig::new().minimize(true);
        let syntax = SyntaxConfig::new().unicode(false).utf8(false);
        let dfa = Builder::new()
            .configure(config)
            .syntax(syntax)
            .build(&pattern)
            .expect("a needle's syntax has already been verified at this point, and thus converting it into a dfa should never fail");
        Self {
            dfa,
            length: bytes.len(),
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
}

impl Sealed for DynamicNeedle {}

impl Needle for DynamicNeedle {
    fn find_iter<'iter, 'needle: 'iter, 'haystack: 'iter>(
        &'needle self,
        haystack: &'haystack [u8],
    ) -> impl Iterator<Item = Match<'haystack>> + 'iter {
        FindIter {
            haystack,
            needle: &self.dfa,
            length: self.length,
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

    const HAYSTACK: &str = "Once upon a midnight dreary, while I pondered, weak and weary,
Over many a quaint and curious volume of forgotten lore—
While I nodded, nearly napping, suddenly there came a tapping,
As of some one gently rapping, rapping at my chamber door.
\"'Tis some visiter,\" I muttered, \"tapping at my chamber door—
            Only this and nothing more.\"


Ah, distinctly I remember it was in the bleak December;
And each separate dying ember wrought its ghost upon the floor.
Eagerly I wished the morrow;—vainly I had sought to borrow
From my books surcease of sorrow—sorrow for the lost Lenore—
For the rare and radiant maiden whom the angels name Lenore—
            Nameless here for evermore.

And the silken, sad, uncertain rustling of each purple curtain
Thrilled me—filled me with fantastic terrors never felt before;
So that now, to still the beating of my heart, I stood repeating
\"'Tis some visiter entreating entrance at my chamber door—
Some late visiter entreating entrance at my chamber door;—
            This it is and nothing more.\"

Presently my soul grew stronger; hesitating then no longer,
\"Sir,\" said I, \"or Madam, truly your forgiveness I implore;
But the fact is I was napping, and so gently you came rapping,
And so faintly you came tapping, tapping at my chamber door,
That I scarce was sure I heard you\"—here I opened wide the door;—
            Darkness there and nothing more.

Deep into that darkness peering, long I stood there wondering, fearing,
Doubting, dreaming dreams no mortal ever dared to dream before;
But the silence was unbroken, and the stillness gave no token,
And the only word there spoken was the whispered word, \"Lenore?\"
This I whispered, and an echo murmured back the word, \"Lenore!\"—
            Merely this and nothing more.

Back into the chamber turning, all my soul within me burning,
Soon again I heard a tapping somewhat louder than before.
\"Surely,\" said I, \"surely that is something at my window lattice;
Let me see, then, what thereat is, and this mystery explore—
Let my heart be still a moment and this mystery explore;—
            'Tis the wind and nothing more!\"

Open here I flung the shutter, when, with many a flirt and flutter,
In there stepped a stately Raven of the saintly days of yore;
Not the least obeisance made he; not a minute stopped or stayed he;
But, with mien of lord or lady, perched above my chamber door—
Perched upon a bust of Pallas just above my chamber door—
            Perched, and sat, and nothing more.

Then this ebony bird beguiling my sad fancy into smiling,
By the grave and stern decorum of the countenance it wore,
\"Though thy crest be shorn and shaven, thou,\" I said, \"art sure no craven,
Ghastly grim and ancient Raven wandering from the Nightly shore—
Tell me what thy lordly name is on the Night's Plutonian shore!\"
            Quoth the Raven \"Nevermore.\"

Much I marvelled this ungainly fowl to hear discourse so plainly,
Though its answer little meaning—little relevancy bore;
For we cannot help agreeing that no living human being
Ever yet was blessed with seeing bird above his chamber door—
Bird or beast upon the sculptured bust above his chamber door,
            With such name as \"Nevermore.\"

But the Raven, sitting lonely on the placid bust, spoke only
That one word, as if his soul in that one word he did outpour.
Nothing farther then he uttered—not a feather then he fluttered—
Till I scarcely more than muttered \"Other friends have flown before—
On the morrow he will leave me, as my Hopes have flown before.\"
            Then the bird said \"Nevermore.\"

Startled at the stillness broken by reply so aptly spoken,
\"Doubtless,\" said I, \"what it utters is its only stock and store
Caught from some unhappy master whom unmerciful Disaster
Followed fast and followed faster till his songs one burden bore—
Till the dirges of his Hope that melancholy burden bore
            Of 'Never—nevermore'.\"

But the Raven still beguiling my sad fancy into smiling,
Straight I wheeled a cushioned seat in front of bird, and bust and door;
Then, upon the velvet sinking, I betook myself to linking
Fancy unto fancy, thinking what this ominous bird of yore—
What this grim, ungainly, ghastly, gaunt, and ominous bird of yore
            Meant in croaking \"Nevermore.\"

This I sat engaged in guessing, but no syllable expressing
To the fowl whose fiery eyes now burned into my bosom's core;
This and more I sat divining, with my head at ease reclining
On the cushion's velvet lining that the lamp-light gloated o'er,
But whose velvet-violet lining with the lamp-light gloating o'er,
            She shall press, ah, nevermore!

Then, methought, the air grew denser, perfumed from an unseen censer
Swung by seraphim whose foot-falls tinkled on the tufted floor.
\"Wretch,\" I cried, \"thy God hath lent thee—by these angels he hath sent thee
Respite—respite and nepenthe, from thy memories of Lenore;
Quaff, oh quaff this kind nepenthe and forget this lost Lenore!\"
            Quoth the Raven \"Nevermore.\"

\"Prophet!\" said I, \"thing of evil!—prophet still, if bird or devil!—
Whether Tempter sent, or whether tempest tossed thee here ashore,
Desolate yet all undaunted, on this desert land enchanted—
On this home by Horror haunted—tell me truly, I implore—
Is there—is there balm in Gilead?—tell me—tell me, I implore!\"
            Quoth the Raven \"Nevermore.\"

\"Prophet!\" said I, \"thing of evil!—prophet still, if bird or devil!
By that Heaven that bends above us—by that God we both adore—
Tell this soul with sorrow laden if, within the distant Aidenn,
It shall clasp a sainted maiden whom the angels name Lenore—
Clasp a rare and radiant maiden whom the angels name Lenore.\"
            Quoth the Raven \"Nevermore.\"

\"Be that word our sign of parting, bird or fiend!\" I shrieked, upstarting—
\"Get thee back into the tempest and the Night's Plutonian shore!
Leave no black plume as a token of that lie thy soul hath spoken!
Leave my loneliness unbroken!—quit the bust above my door!
Take thy beak from out my heart, and take thy form from off my door!\"
            Quoth the Raven \"Nevermore.\"

And the Raven, never flitting, still is sitting, still is sitting
On the pallid bust of Pallas just above my chamber door;
And his eyes have all the seeming of a demon's that is dreaming,
And the lamp-light o'er him streaming throws his shadow on the floor;
And my soul from out that shadow that lies floating on the floor
            Shall be lifted—nevermore!
";

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

    #[test]
    fn test_matches() {
        macro_rules! do_test {
            ($pattern:literal, $count:literal) => {
                let pattern: Vec<_> = $pattern
                    .as_bytes()
                    .iter()
                    .map(|&x| match x {
                        b'?' => None,
                        _ => Some(x),
                    })
                    .collect();
                let needle = DynamicNeedle::from_bytes(&pattern);
                let matches = needle.find_iter(HAYSTACK.as_bytes()).count();
                assert_eq!(matches, $count, $pattern);
            };
        }

        do_test!("Raven", 10);
        do_test!("?aven", 13);
        do_test!("Once", 1);
        do_test!("?nce", 7);
        do_test!("?nc?", 16);
        do_test!("!", 19);
        do_test!("?u?t?", 31);
    }
}
