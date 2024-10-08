use crate::slice::ThinSlice;
use std::{
    alloc,
    alloc::Layout,
    marker::PhantomData,
    mem,
    ops::{
        BitAnd,
        BitXor,
        Not,
        RangeFrom,
    },
    ptr,
    ptr::NonNull,
    slice,
};

trait Integer: BitAnd<Output = Self> + BitXor<Output = Self> + Eq + Not<Output = Self> + Sized {
    const MAX: Self;
    const ZERO: Self;
}

macro_rules! make_integer {
    ($type:ty) => {
        impl Integer for $type {
            const MAX: Self = Self::MAX;
            const ZERO: Self = 0;
        }
    };
}

make_integer!(u16);
make_integer!(u32);
make_integer!(u64);

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
trait Simd: Clone + Copy + Sized {
    const LANE_COUNT: usize;
    type Integer: Integer;

    #[must_use]
    unsafe fn blendv_epi8(a: Self, b: Self, mask: Self) -> Self;
    #[must_use]
    unsafe fn cmpeq_epi8(a: Self, b: Self) -> Self;
    #[must_use]
    unsafe fn load(mem_addr: NonNull<Self>) -> Self;
    #[must_use]
    unsafe fn loadu(mem_addr: NonNull<Self>) -> Self;
    #[must_use]
    unsafe fn movemask_epi8(a: Self) -> Self::Integer;
    #[must_use]
    unsafe fn set1_epi8(a: u8) -> Self;
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod sse2 {
    pub(crate) use arch::__m128i;
    #[cfg(target_arch = "x86")]
    use std::arch::x86 as arch;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64 as arch;
    use std::ptr::NonNull;

    // https://github.com/aklomp/missing-sse-intrinsics
    unsafe fn _mm_blendv_si128(a: __m128i, b: __m128i, mask: __m128i) -> __m128i {
        arch::_mm_or_si128(
            arch::_mm_andnot_si128(mask, a),
            arch::_mm_and_si128(mask, b),
        )
    }

    impl super::Simd for __m128i {
        const LANE_COUNT: usize = 16;
        type Integer = u16;

        unsafe fn blendv_epi8(a: Self, b: Self, mask: Self) -> Self {
            _mm_blendv_si128(a, b, arch::_mm_cmplt_epi8(mask, arch::_mm_setzero_si128()))
        }

        unsafe fn cmpeq_epi8(a: Self, b: Self) -> Self {
            arch::_mm_cmpeq_epi8(a, b)
        }

        unsafe fn load(mem_addr: NonNull<Self>) -> Self {
            arch::_mm_load_si128(mem_addr.as_ptr())
        }

        unsafe fn loadu(mem_addr: NonNull<Self>) -> Self {
            arch::_mm_loadu_si128(mem_addr.as_ptr())
        }

        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        unsafe fn movemask_epi8(a: Self) -> Self::Integer {
            arch::_mm_movemask_epi8(a) as u32 as u16
        }

        #[expect(clippy::cast_possible_wrap)]
        unsafe fn set1_epi8(a: u8) -> Self {
            arch::_mm_set1_epi8(a as i8)
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod avx2 {
    pub(crate) use arch::__m256i;
    #[cfg(target_arch = "x86")]
    use std::arch::x86 as arch;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64 as arch;
    use std::ptr::NonNull;

    impl super::Simd for __m256i {
        const LANE_COUNT: usize = 32;
        type Integer = u32;

        unsafe fn blendv_epi8(a: Self, b: Self, mask: Self) -> Self {
            arch::_mm256_blendv_epi8(a, b, mask)
        }

        unsafe fn cmpeq_epi8(a: Self, b: Self) -> Self {
            arch::_mm256_cmpeq_epi8(a, b)
        }

        unsafe fn load(mem_addr: NonNull<Self>) -> Self {
            arch::_mm256_load_si256(mem_addr.as_ptr())
        }

        unsafe fn loadu(mem_addr: NonNull<Self>) -> Self {
            arch::_mm256_loadu_si256(mem_addr.as_ptr())
        }

        #[expect(clippy::cast_sign_loss)]
        unsafe fn movemask_epi8(a: Self) -> Self::Integer {
            arch::_mm256_movemask_epi8(a) as u32
        }

        #[expect(clippy::cast_possible_wrap)]
        unsafe fn set1_epi8(a: u8) -> Self {
            arch::_mm256_set1_epi8(a as i8)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// The method chosen to quickly compare strings for equality, in lieu of `strcmp`, since we need to account for wildcards.
pub enum Method {
    /// String comparison 1 byte at a time (arch independent).
    Scalar,
    /// String comparison 4 bytes at a time (32/64 bit systems only).
    Swar32,
    /// String comparison 8 bytes at a time (64 bit systems only).
    Swar64,
    /// String comparison 16 bytes at time (x86/x64 only).
    Sse2,
    /// String comparison 32 bytes at a time (x86/x64 only).
    Avx2,
}

impl Method {
    #[must_use]
    fn from_size(size: usize) -> Self {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if size >= avx2::__m256i::LANE_COUNT
            && is_x86_feature_detected!("avx")
            && is_x86_feature_detected!("avx2")
        {
            return Self::Avx2;
        }

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if size >= sse2::__m128i::LANE_COUNT && is_x86_feature_detected!("sse2") {
            return Self::Sse2;
        }

        #[cfg(target_pointer_width = "64")]
        if size >= mem::size_of::<u64>() {
            return Self::Swar64;
        }

        #[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
        if size >= mem::size_of::<u32>() {
            return Self::Swar32;
        }

        Self::Scalar
    }

    #[must_use]
    fn compute_vectorizable_boundary(self, len_bytes: usize) -> usize {
        match self {
            Self::Scalar => 0,
            Self::Swar32 => len_bytes - (len_bytes % 4),
            Self::Swar64 => len_bytes - (len_bytes % 8),
            Self::Sse2 => len_bytes - (len_bytes % 16),
            Self::Avx2 => len_bytes - (len_bytes % 32),
        }
    }
}

const BUFFER_ALIGNMENT: usize = 32;
const _: () = assert!(mem::align_of::<AlignedBytes<1>>() == BUFFER_ALIGNMENT);

#[derive(Clone, Debug)]
#[repr(C, align(32))]
struct AlignedBytes<const N: usize>([u8; N]);

#[derive(Clone, Debug)]
pub(crate) struct StaticPattern<const SIZE: usize, const CAPACITY: usize> {
    word: AlignedBytes<CAPACITY>,
    mask: AlignedBytes<CAPACITY>,
}

impl<const SIZE: usize, const CAPACITY: usize> StaticPattern<SIZE, CAPACITY> {
    #[must_use]
    pub(crate) const fn from_components(word: [u8; CAPACITY], mask: [u8; CAPACITY]) -> Self {
        Self {
            word: AlignedBytes(word),
            mask: AlignedBytes(mask),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct MaskedByte(u8);

impl MaskedByte {
    const MASKED: Self = Self(0xFF);
    const UNMASKED: Self = Self(0x00);

    #[must_use]
    pub(crate) fn is_unmasked(self) -> bool {
        self == Self::UNMASKED
    }
}

impl From<u8> for MaskedByte {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<MaskedByte> for u8 {
    fn from(value: MaskedByte) -> Self {
        value.0
    }
}

#[derive(Debug)]
pub(crate) struct DynamicPattern {
    word: NonNull<u8>,
    mask: NonNull<MaskedByte>,
    len: usize,
    layout: Layout,
}

impl DynamicPattern {
    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub(crate) fn from_bytes(bytes: &[Option<u8>]) -> Self {
        const _: () = assert!(BUFFER_ALIGNMENT != 0);
        const _: () = assert!(BUFFER_ALIGNMENT % 2 == 0);
        let layout = Layout::from_size_align(bytes.len().max(1), BUFFER_ALIGNMENT)
            .expect("creating the layout for an aligned buffer should be infallible")
            .pad_to_align();
        let word = unsafe { NonNull::new_unchecked(alloc::alloc_zeroed(layout)) };
        let mask = unsafe {
            let x = alloc::alloc(layout).cast();
            ptr::write_bytes(x, MaskedByte::MASKED.into(), layout.size());
            NonNull::new_unchecked(x)
        };

        let word_slice = unsafe { slice::from_raw_parts_mut(word.as_ptr(), layout.size()) };
        for (l, r) in word_slice.iter_mut().zip(bytes) {
            *l = match r {
                Some(byte) => *byte,
                None => 0,
            };
        }

        let mask_slice = unsafe { slice::from_raw_parts_mut(mask.as_ptr(), layout.size()) };
        for (l, r) in mask_slice.iter_mut().zip(bytes) {
            *l = match r {
                Some(_) => MaskedByte::UNMASKED,
                None => MaskedByte::MASKED,
            };
        }

        Self {
            word,
            mask,
            len: bytes.len(),
            layout,
        }
    }

    #[must_use]
    pub(crate) fn word_slice_padded(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.word.as_ptr(), self.layout.size()) }
    }

    #[must_use]
    pub(crate) fn mask_slice_padded(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.mask.as_ptr().cast(), self.layout.size()) }
    }
}

impl Clone for DynamicPattern {
    fn clone(&self) -> Self {
        Self {
            word: unsafe {
                let ptr = alloc::alloc(self.layout);
                ptr::copy_nonoverlapping(self.word.as_ptr(), ptr, self.layout.size());
                NonNull::new_unchecked(ptr)
            },
            mask: unsafe {
                let ptr = alloc::alloc(self.layout).cast();
                ptr::copy_nonoverlapping(self.mask.as_ptr(), ptr, self.layout.size());
                NonNull::new_unchecked(ptr)
            },
            len: self.len,
            layout: self.layout,
        }
    }
}

impl Drop for DynamicPattern {
    fn drop(&mut self) {
        unsafe { alloc::dealloc(self.word.as_ptr(), self.layout) }
        unsafe { alloc::dealloc(self.mask.as_ptr().cast(), self.layout) }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PatternRef<'a> {
    word: NonNull<u8>,
    mask: NonNull<MaskedByte>,
    size: usize,
    method: Method,
    vectorizable_boundary: usize,
    _phantom: PhantomData<&'a u8>,
}

impl<'a> PatternRef<'a> {
    #[cfg(test)]
    #[must_use]
    pub(crate) fn cmpeq(&self, other: &[u8]) -> bool {
        if self.len() == other.len() {
            // SAFETY: we just verified the lengths are equal
            unsafe { self.cmpeq_unchecked(other) }
        } else {
            false
        }
    }

    /// SAFETY: `other` must be equal to `self` in length
    #[must_use]
    pub(crate) unsafe fn cmpeq_unchecked(&self, other: &[u8]) -> bool {
        debug_assert_eq!(self.len(), other.len());
        let other = other.into();
        // SAFETY: a method was chosen based on the cpu's supported features
        match self.method {
            Method::Scalar => self.cmpeq_scalar(other),
            Method::Swar32 => self.cmpeq_swar::<u32>(other),
            Method::Swar64 => self.cmpeq_swar::<u64>(other),
            Method::Sse2 => self.cmpeq_sse2(other),
            Method::Avx2 => self.cmpeq_avx2(other),
        }
    }

    #[must_use]
    pub(crate) fn method(&self) -> Method {
        self.method
    }

    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.size
    }

    #[must_use]
    pub(crate) fn word_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.word.as_ptr(), self.len()) }
    }

    #[must_use]
    pub(crate) fn mask_slice(&self) -> &[MaskedByte] {
        unsafe { slice::from_raw_parts(self.mask.as_ptr(), self.len()) }
    }

    #[must_use]
    unsafe fn cmpeq_scalar_range(&self, other: ThinSlice<u8>, range: RangeFrom<usize>) -> bool {
        let mut word = self.word.add(range.start);
        let mut mask = self.mask.add(range.start);
        let mut other = other.get_unchecked(range);

        while other.start != other.end {
            let word_val = word.read();
            let other_val = other.start.read();
            if word_val != other_val && mask.read().is_unmasked() {
                return false;
            }
            word = word.add(1);
            mask = mask.add(1);
            other.start = other.start.add(1);
        }

        true
    }

    #[must_use]
    unsafe fn cmpeq_scalar(&self, other: ThinSlice<u8>) -> bool {
        self.cmpeq_scalar_range(other, 0..)
    }

    #[must_use]
    unsafe fn cmpeq_swar<Int: Integer>(&self, other: ThinSlice<u8>) -> bool {
        let mut word = self.word.cast::<Int>();
        let mut mask = self.mask.cast::<Int>();
        let (mut trimmed, extra) = other.split_at_unchecked::<Int, u8>(self.vectorizable_boundary);

        while trimmed.start != trimmed.end {
            let word_int = word.read();
            let mask_int = mask.read();
            let trimmed_int = trimmed.start.read_unaligned();
            let comparison = !mask_int & (word_int ^ trimmed_int);
            if comparison != Int::ZERO {
                return false;
            }
            word = word.add(1);
            mask = mask.add(1);
            trimmed.start = trimmed.start.add(1);
        }

        if extra.is_empty() {
            true
        } else {
            self.cmpeq_scalar_range(other, self.vectorizable_boundary..)
        }
    }

    /// SAFETY:
    /// * `other` must be equal to `self` in length
    /// * the relevant simd features must be available on the target cpu
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[must_use]
    unsafe fn do_cmpeq_simd<T: Simd>(&self, other: ThinSlice<u8>) -> bool {
        let mut word = self.word.cast::<T>();
        let mut mask = self.mask.cast::<T>();
        let (mut trimmed, extra) = other.split_at_unchecked::<T, u8>(self.vectorizable_boundary);
        let all_ones = T::set1_epi8(0xFF);

        while trimmed.start != trimmed.end {
            let word_vec = T::load(word);
            let mask_vec = T::load(mask);
            let trimmed_vec = T::loadu(trimmed.start);

            let cmpeq = T::cmpeq_epi8(trimmed_vec, word_vec);
            let blendv = T::blendv_epi8(cmpeq, all_ones, mask_vec);
            let movemask = T::movemask_epi8(blendv);
            if movemask != T::Integer::MAX {
                return false;
            }

            word = word.add(1);
            mask = mask.add(1);
            trimmed.start = trimmed.start.add(1);
        }

        if extra.is_empty() {
            true
        } else {
            self.cmpeq_scalar_range(other, self.vectorizable_boundary..)
        }
    }

    /// SAFETY:
    /// * `other` must be equal to `self` in length
    /// * the cpu must support "sse2"
    #[allow(unreachable_code)]
    #[must_use]
    unsafe fn cmpeq_sse2(&self, other: ThinSlice<u8>) -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        return self.do_cmpeq_simd::<sse2::__m128i>(other);
        self.cmpeq_scalar(other)
    }

    /// SAFETY:
    /// * `other` must be equal to `self` in length
    /// * the cpu must support "avx" and "avx2"
    #[allow(unreachable_code)]
    #[must_use]
    unsafe fn cmpeq_avx2(&self, other: ThinSlice<u8>) -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        return self.do_cmpeq_simd::<avx2::__m256i>(other);
        self.cmpeq_scalar(other)
    }
}

impl<'a, const SIZE: usize, const CAPACITY: usize> From<&'a StaticPattern<SIZE, CAPACITY>>
    for PatternRef<'a>
{
    fn from(value: &'a StaticPattern<SIZE, CAPACITY>) -> Self {
        // SAFETY: pointers that come from an array are obviously valid
        let (word, mask) = unsafe {
            let word = NonNull::new_unchecked(value.word.0.as_ptr().cast_mut());
            let mask = NonNull::new_unchecked(value.mask.0.as_ptr().cast_mut().cast());
            (word, mask)
        };
        let size = SIZE;
        let method = Method::from_size(SIZE);
        let vectorizable_boundary = method.compute_vectorizable_boundary(size);
        Self {
            word,
            mask,
            size,
            method,
            vectorizable_boundary,
            _phantom: PhantomData,
        }
    }
}

impl<'a> From<&'a DynamicPattern> for PatternRef<'a> {
    fn from(value: &'a DynamicPattern) -> Self {
        let size = value.len;
        let method = Method::from_size(size);
        let vectorizable_boundary = method.compute_vectorizable_boundary(size);
        Self {
            word: value.word,
            mask: value.mask,
            size,
            method,
            vectorizable_boundary,
            _phantom: PhantomData,
        }
    }
}

#[cfg(test)]
mod test {
    use super::{
        DynamicPattern,
        Method,
        PatternRef,
    };

    macro_rules! make_pattern {
        (let $ident:ident = $bytes:literal ;) => {
            let bytes = $bytes
                .as_bytes()
                .iter()
                .map(|&x| match x {
                    b'?' => None,
                    _ => Some(x),
                })
                .collect::<Vec<_>>();
            let dynamic = DynamicPattern::from_bytes(&bytes);
            let $ident = PatternRef::from(&dynamic);
        };
    }

    #[test]
    fn test_scalar() {
        make_pattern! { let pattern = "who"; }
        assert_eq!(pattern.method, Method::Scalar);
        assert!(pattern.cmpeq(b"who"));
        assert!(!pattern.cmpeq(b"why"));
        assert!(!pattern.cmpeq(b"whose"));
        assert!(!pattern.cmpeq(b"wh"));
        assert!(!pattern.cmpeq(b""));

        make_pattern! { let pattern = "w?o"; }
        assert_eq!(pattern.method, Method::Scalar);
        assert!(pattern.cmpeq(b"who"));
        assert!(pattern.cmpeq(b"wao"));
        assert!(pattern.cmpeq(b"woo"));
        assert!(pattern.cmpeq(b"wto"));
        assert!(!pattern.cmpeq(b"aho"));
        assert!(!pattern.cmpeq(b"why"));
        assert!(!pattern.cmpeq(b"whoo"));
        assert!(!pattern.cmpeq(b"wh"));
        assert!(!pattern.cmpeq(b"hhh"));
        assert!(!pattern.cmpeq(b""));

        make_pattern! { let pattern = "???"; }
        assert_eq!(pattern.method, Method::Scalar);
        assert!(pattern.cmpeq(b"abc"));
        assert!(pattern.cmpeq(b"aaa"));
        assert!(pattern.cmpeq(b"dns"));
        assert!(pattern.cmpeq(b"jop"));
        assert!(!pattern.cmpeq(b"abcd"));
        assert!(!pattern.cmpeq(b"ab"));
        assert!(!pattern.cmpeq(b"a"));
        assert!(!pattern.cmpeq(b""));
    }

    #[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
    #[test]
    fn test_swar32() {
        make_pattern! { let pattern = "nobody"; }
        assert_eq!(pattern.method, Method::Swar32);
        assert!(pattern.cmpeq(b"nobody"));
        assert!(!pattern.cmpeq(b"nobodys"));
        assert!(!pattern.cmpeq(b"nobod"));
        assert!(!pattern.cmpeq(b"n0b0dy"));
        assert!(!pattern.cmpeq(b"nobode"));
        assert!(!pattern.cmpeq(b""));

        make_pattern! { let pattern = "larc?ny"; }
        assert_eq!(pattern.method, Method::Swar32);
        assert!(pattern.cmpeq(b"larceny"));
        assert!(pattern.cmpeq(b"larcany"));
        assert!(pattern.cmpeq(b"larcuny"));
        assert!(pattern.cmpeq(b"larcony"));
        assert!(!pattern.cmpeq(b"lardeny"));
        assert!(!pattern.cmpeq(b"larcefy"));
        assert!(!pattern.cmpeq(b"larcenyy"));
        assert!(!pattern.cmpeq(b"larcen"));
        assert!(!pattern.cmpeq(b""));

        make_pattern! { let pattern = "????"; }
        assert_eq!(pattern.method, Method::Swar32);
        assert!(pattern.cmpeq(b"abcd"));
        assert!(pattern.cmpeq(b"aaaa"));
        assert!(pattern.cmpeq(b"dnla"));
        assert!(pattern.cmpeq(b"rt;l"));
        assert!(!pattern.cmpeq(b"abcde"));
        assert!(!pattern.cmpeq(b"abc"));
        assert!(!pattern.cmpeq(b"ab"));
        assert!(!pattern.cmpeq(b"a"));
        assert!(!pattern.cmpeq(b""));
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn test_swar64() {
        make_pattern! { let pattern = "how are you"; }
        assert_eq!(pattern.method, Method::Swar64);
        assert!(pattern.cmpeq(b"how are you"));
        assert!(!pattern.cmpeq(b"how arr you"));
        assert!(!pattern.cmpeq(b"how arr you"));
        assert!(!pattern.cmpeq(b"h0w are you"));
        assert!(!pattern.cmpeq(b"who are you"));
        assert!(!pattern.cmpeq(b"why am i"));
        assert!(!pattern.cmpeq(b"where are we"));
        assert!(!pattern.cmpeq(b""));

        make_pattern! { let pattern = "what ?im? i? it"; }
        assert_eq!(pattern.method, Method::Swar64);
        assert!(pattern.cmpeq(b"what time is it"));
        assert!(pattern.cmpeq(b"what lime is it"));
        assert!(pattern.cmpeq(b"what time if it"));
        assert!(pattern.cmpeq(b"what .im5 i? it"));
        assert!(!pattern.cmpeq(b"what time is it?"));
        assert!(!pattern.cmpeq(b"time is it?"));
        assert!(!pattern.cmpeq(b"asdnlasdlanldam"));
        assert!(!pattern.cmpeq(b""));

        make_pattern! { let pattern = "????????"; }
        assert_eq!(pattern.method, Method::Swar64);
        assert!(pattern.cmpeq(b"12345678"));
        assert!(pattern.cmpeq(b"asdnqnkm"));
        assert!(pattern.cmpeq(b"389u1jlm"));
        assert!(pattern.cmpeq(b"hdqi09uj"));
        assert!(!pattern.cmpeq(b"hdqi09uja"));
        assert!(!pattern.cmpeq(b"noqdkl"));
        assert!(!pattern.cmpeq(b"qoji"));
        assert!(!pattern.cmpeq(b"qdjpocomwkl"));
        assert!(!pattern.cmpeq(b""));
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn test_sse2() {
        if !is_x86_feature_detected!("sse2") {
            return;
        }

        make_pattern! { let pattern = "set the world aflame"; }
        assert_eq!(pattern.method, Method::Sse2);
        assert!(pattern.cmpeq(b"set the world aflame"));
        assert!(!pattern.cmpeq(b"set the world aflame?"));
        assert!(!pattern.cmpeq(b"set the world aflame!"));
        assert!(!pattern.cmpeq(b"set the world ablaze"));
        assert!(!pattern.cmpeq(b"set the house aflame"));
        assert!(!pattern.cmpeq(b""));

        make_pattern! { let pattern = "t?rn t?at li?ht around"; }
        assert_eq!(pattern.method, Method::Sse2);
        assert!(pattern.cmpeq(b"turn that light around"));
        assert!(pattern.cmpeq(b"turn t1at li8ht around"));
        assert!(pattern.cmpeq(b"t?rn t_at li;ht around"));
        assert!(!pattern.cmpeq(b"turn that light around?"));
        assert!(!pattern.cmpeq(b"turn that light aroun"));
        assert!(!pattern.cmpeq(b""));

        make_pattern! { let pattern = "?????????????????"; }
        assert_eq!(pattern.method, Method::Sse2);
        assert!(pattern.cmpeq(b"0123456789ABCDEF0"));
        assert!(pattern.cmpeq(b"asndkandlanldlalq"));
        assert!(pattern.cmpeq(b"2390ujondlasaasdh"));
        assert!(!pattern.cmpeq(b"nodqwndlam;[qk;"));
        assert!(!pattern.cmpeq(b"203hg1ftdvwhbjckcnvl"));
        assert!(!pattern.cmpeq(b""));
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn test_avx2() {
        if !is_x86_feature_detected!("avx2") || !is_x86_feature_detected!("avx2") {
            return;
        }

        make_pattern! { let pattern = "where the fear has gone there will be nothing"; }
        assert_eq!(pattern.method, Method::Avx2);
        assert!(pattern.cmpeq(b"where the fear has gone there will be nothing"));
        assert!(!pattern.cmpeq(b"where the fear has gone their will be nothing"));
        assert!(!pattern.cmpeq(b"where the fear has gone there will be nothin"));
        assert!(!pattern.cmpeq(b"where the fear has gone there will be nothing?"));
        assert!(!pattern.cmpeq(b"hfuqwom0293i2pk;,/.;'admpadbuyvqwgdiuhojfmcll"));
        assert!(!pattern.cmpeq(b""));

        make_pattern! { let pattern = "where the fear ??? gone there ???? be nothing"; }
        assert_eq!(pattern.method, Method::Avx2);
        assert!(pattern.cmpeq(b"where the fear has gone there will be nothing"));
        assert!(pattern.cmpeq(b"where the fear 988 gone there hoqa be nothing"));
        assert!(!pattern.cmpeq(b"where the fear has gone their will be nothing"));
        assert!(!pattern.cmpeq(b"where the fear has gone there will be nothing?"));
        assert!(!pattern.cmpeq(b"where the fear has gone there will be nothin"));
        assert!(!pattern.cmpeq(b""));

        make_pattern! { let pattern = "?????????????????????????????????????????????"; }
        assert_eq!(pattern.method, Method::Avx2);
        assert!(pattern.cmpeq(b"where the fear has gone there will be nothing"));
        assert!(pattern.cmpeq(b"where the fear 988 gone there hoqa be qjnnwkl"));
        assert!(pattern.cmpeq(b"qbhinldnklkdndabjkdbqoanlkmmwand,nd,andasnlda"));
        assert!(!pattern.cmpeq(b"where the fear has gone there will be nothing?"));
        assert!(!pattern.cmpeq(b"where the fear has gone there will be nothin"));
        assert!(!pattern.cmpeq(b""));
    }
}
