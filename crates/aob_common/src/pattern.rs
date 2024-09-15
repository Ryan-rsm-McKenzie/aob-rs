use std::{
    alloc,
    alloc::Layout,
    marker::PhantomData,
    mem,
    ops::{
        BitAnd,
        BitXor,
        Not,
    },
    ptr,
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
    unsafe fn blendv_epi8(self, b: Self, mask: Self) -> Self;
    #[must_use]
    unsafe fn cmpeq_epi8(self, b: Self) -> Self;
    #[must_use]
    unsafe fn load(mem_addr: *const Self) -> Self;
    #[must_use]
    unsafe fn loadu(mem_addr: *const Self) -> Self;
    #[must_use]
    unsafe fn movemask_epi8(self) -> Self::Integer;
    #[must_use]
    unsafe fn set1_epi8(a: i8) -> Self;
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod sse2 {
    pub(crate) use arch::__m128i;
    #[cfg(target_arch = "x86")]
    use std::arch::x86 as arch;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64 as arch;

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

        unsafe fn blendv_epi8(self, b: Self, mask: Self) -> Self {
            _mm_blendv_si128(
                self,
                b,
                arch::_mm_cmplt_epi8(mask, arch::_mm_setzero_si128()),
            )
        }

        unsafe fn cmpeq_epi8(self, b: Self) -> Self {
            arch::_mm_cmpeq_epi8(self, b)
        }

        unsafe fn load(mem_addr: *const Self) -> Self {
            arch::_mm_load_si128(mem_addr)
        }

        unsafe fn loadu(mem_addr: *const Self) -> Self {
            arch::_mm_loadu_si128(mem_addr)
        }

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        unsafe fn movemask_epi8(self) -> Self::Integer {
            arch::_mm_movemask_epi8(self) as u32 as u16
        }

        unsafe fn set1_epi8(a: i8) -> Self {
            arch::_mm_set1_epi8(a)
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

    impl super::Simd for __m256i {
        const LANE_COUNT: usize = 32;
        type Integer = u32;

        unsafe fn blendv_epi8(self, b: Self, mask: Self) -> Self {
            arch::_mm256_blendv_epi8(self, b, mask)
        }

        unsafe fn cmpeq_epi8(self, b: Self) -> Self {
            arch::_mm256_cmpeq_epi8(self, b)
        }

        unsafe fn load(mem_addr: *const Self) -> Self {
            arch::_mm256_load_si256(mem_addr)
        }

        unsafe fn loadu(mem_addr: *const Self) -> Self {
            arch::_mm256_loadu_si256(mem_addr)
        }

        #[allow(clippy::cast_sign_loss)]
        unsafe fn movemask_epi8(self) -> Self::Integer {
            arch::_mm256_movemask_epi8(self) as u32
        }

        unsafe fn set1_epi8(a: i8) -> Self {
            arch::_mm256_set1_epi8(a)
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

        if size >= mem::size_of::<u64>() && mem::size_of::<usize>() >= mem::size_of::<u64>() {
            Self::Swar64
        } else if size >= mem::size_of::<u32>() && mem::size_of::<usize>() >= mem::size_of::<u32>()
        {
            Self::Swar32
        } else {
            Self::Scalar
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
    word: *const u8,
    mask: *const MaskedByte,
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
        let word = unsafe {
            let x = alloc::alloc(layout);
            ptr::write_bytes(x, 0, layout.size());
            x
        };
        let mask = unsafe {
            let x = alloc::alloc(layout);
            ptr::write_bytes(x, MaskedByte::MASKED.into(), layout.size());
            x.cast::<MaskedByte>()
        };

        let word_slice = unsafe { slice::from_raw_parts_mut(word, layout.size()) };
        for (l, r) in word_slice.iter_mut().zip(bytes) {
            *l = match r {
                Some(byte) => *byte,
                None => 0,
            };
        }

        let mask_slice = unsafe { slice::from_raw_parts_mut(mask, layout.size()) };
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
        unsafe { slice::from_raw_parts(self.word, self.layout.size()) }
    }

    #[must_use]
    pub(crate) fn mask_slice_padded(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.mask.cast(), self.layout.size()) }
    }
}

impl Clone for DynamicPattern {
    fn clone(&self) -> Self {
        Self {
            word: unsafe {
                let ptr = alloc::alloc(self.layout);
                ptr::copy_nonoverlapping(self.word, ptr, self.layout.size());
                ptr
            },
            mask: unsafe {
                let ptr = alloc::alloc(self.layout).cast();
                ptr::copy_nonoverlapping(self.mask, ptr, self.layout.size());
                ptr
            },
            len: self.len,
            layout: self.layout,
        }
    }
}

impl Drop for DynamicPattern {
    fn drop(&mut self) {
        unsafe { alloc::dealloc(self.word.cast_mut(), self.layout) }
        unsafe { alloc::dealloc(self.mask.cast_mut().cast(), self.layout) }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PatternRef<'a> {
    word: *const u8,
    mask: *const MaskedByte,
    size: usize,
    capacity: usize,
    method: Method,
    _phantom: PhantomData<&'a u8>,
}

impl<'a> PatternRef<'a> {
    #[must_use]
    pub(crate) fn compare_eq(&self, other: &[u8]) -> bool {
        if self.size == other.len() {
            match self.method {
                Method::Scalar => self.compare_eq_scalar(other),
                Method::Swar32 => self.compare_eq_swar::<u32>(other),
                Method::Swar64 => self.compare_eq_swar::<u64>(other),
                Method::Sse2 => self.compare_eq_sse2(other),
                Method::Avx2 => self.compare_eq_avx2(other),
            }
        } else {
            false
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
        unsafe { slice::from_raw_parts(self.word, self.size) }
    }

    #[must_use]
    fn word_slice_padded(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.word, self.capacity) }
    }

    #[must_use]
    pub(crate) fn mask_slice(&self) -> &[MaskedByte] {
        unsafe { slice::from_raw_parts(self.mask, self.size) }
    }

    #[must_use]
    fn mask_slice_padded(&self) -> &[MaskedByte] {
        unsafe { slice::from_raw_parts(self.mask, self.capacity) }
    }

    #[must_use]
    fn compare_eq_scalar(&self, other: &[u8]) -> bool {
        let word = self.word_slice_padded();
        let mask = self.mask_slice_padded();
        for i in 0..self.size {
            if word[i] != other[i] && mask[i].is_unmasked() {
                return false;
            }
        }
        true
    }

    #[must_use]
    fn compare_eq_swar<Int: Integer>(&self, other: &[u8]) -> bool {
        let length = self
            .size
            .next_multiple_of(mem::size_of::<Int>())
            .min(other.len());
        let word = self.word_slice_padded();
        let mask = self.mask_slice_padded();

        let remainder = {
            let other_ptr = other.as_ptr().cast::<Int>();
            let word_ptr = word.as_ptr().cast::<Int>();
            let mask_ptr = mask.as_ptr().cast::<Int>();
            let mut i = 0;
            while i < length / mem::size_of::<Int>() {
                let other_int = unsafe { other_ptr.add(i).read_unaligned() };
                let word_int = unsafe { word_ptr.add(i).read() };
                let mask_int = unsafe { mask_ptr.add(i).read() };
                let comparison = !mask_int & (word_int ^ other_int);
                if comparison != Int::ZERO {
                    return false;
                }
                i += 1;
            }
            i
        };

        for i in remainder..length {
            if word[i] != other[i] && mask[i].is_unmasked() {
                return false;
            }
        }

        true
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[must_use]
    fn compare_eq_simd<T: Simd>(&self, other: &[u8]) -> bool {
        let length = self.size.next_multiple_of(T::LANE_COUNT).min(other.len());
        let all_set = unsafe { T::set1_epi8(-1) };
        let word = self.word_slice_padded();
        let mask = self.mask_slice_padded();

        let remainder = {
            let other_ptr = other.as_ptr().cast::<T>();
            let word_ptr = word.as_ptr().cast::<T>();
            let mask_ptr = mask.as_ptr().cast::<T>();
            let mut i = 0;
            while i < length / T::LANE_COUNT {
                let other_vec = unsafe { T::loadu(other_ptr.add(i)) };
                let word_vec = unsafe { T::load(word_ptr.add(i)) };
                let mask_vec = unsafe { T::load(mask_ptr.add(i)) };
                let comparison = unsafe {
                    other_vec
                        .cmpeq_epi8(word_vec)
                        .blendv_epi8(all_set, mask_vec)
                        .movemask_epi8()
                };
                if comparison != T::Integer::MAX {
                    return false;
                }
                i += 1;
            }
            i
        };

        for i in remainder..length {
            if word[i] != other[i] && mask[i].is_unmasked() {
                return false;
            }
        }

        true
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[must_use]
    fn compare_eq_sse2(&self, other: &[u8]) -> bool {
        self.compare_eq_simd::<sse2::__m128i>(other)
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    #[must_use]
    fn compare_eq_sse2(&self, other: &[u8]) -> bool {
        self.compare_eq_scalar(other)
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[must_use]
    fn compare_eq_avx2(&self, other: &[u8]) -> bool {
        self.compare_eq_simd::<avx2::__m256i>(other)
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    #[must_use]
    fn compare_eq_avx2(&self, other: &[u8]) -> bool {
        self.compare_eq_scalar(other)
    }
}

impl<'a, const SIZE: usize, const CAPACITY: usize> From<&'a StaticPattern<SIZE, CAPACITY>>
    for PatternRef<'a>
{
    fn from(value: &'a StaticPattern<SIZE, CAPACITY>) -> Self {
        Self {
            word: value.word.0.as_ptr(),
            mask: value.mask.0.as_ptr().cast(),
            size: SIZE,
            capacity: CAPACITY,
            method: Method::from_size(SIZE),
            _phantom: PhantomData,
        }
    }
}

impl<'a> From<&'a DynamicPattern> for PatternRef<'a> {
    fn from(value: &'a DynamicPattern) -> Self {
        let size = value.len;
        Self {
            word: value.word,
            mask: value.mask,
            size,
            capacity: value.layout.size(),
            method: Method::from_size(size),
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
        assert!(pattern.compare_eq(b"who"));
        assert!(!pattern.compare_eq(b"why"));
        assert!(!pattern.compare_eq(b"whose"));
        assert!(!pattern.compare_eq(b"wh"));
        assert!(!pattern.compare_eq(b""));

        make_pattern! { let pattern = "w?o"; }
        assert_eq!(pattern.method, Method::Scalar);
        assert!(pattern.compare_eq(b"who"));
        assert!(pattern.compare_eq(b"wao"));
        assert!(pattern.compare_eq(b"woo"));
        assert!(pattern.compare_eq(b"wto"));
        assert!(!pattern.compare_eq(b"aho"));
        assert!(!pattern.compare_eq(b"why"));
        assert!(!pattern.compare_eq(b"whoo"));
        assert!(!pattern.compare_eq(b"wh"));
        assert!(!pattern.compare_eq(b"hhh"));
        assert!(!pattern.compare_eq(b""));

        make_pattern! { let pattern = "???"; }
        assert_eq!(pattern.method, Method::Scalar);
        assert!(pattern.compare_eq(b"abc"));
        assert!(pattern.compare_eq(b"aaa"));
        assert!(pattern.compare_eq(b"dns"));
        assert!(pattern.compare_eq(b"jop"));
        assert!(!pattern.compare_eq(b"abcd"));
        assert!(!pattern.compare_eq(b"ab"));
        assert!(!pattern.compare_eq(b"a"));
        assert!(!pattern.compare_eq(b""));
    }

    #[test]
    fn test_swar32() {
        make_pattern! { let pattern = "nobody"; }
        assert_eq!(pattern.method, Method::Swar32);
        assert!(pattern.compare_eq(b"nobody"));
        assert!(!pattern.compare_eq(b"nobodys"));
        assert!(!pattern.compare_eq(b"nobod"));
        assert!(!pattern.compare_eq(b"n0b0dy"));
        assert!(!pattern.compare_eq(b"nobode"));
        assert!(!pattern.compare_eq(b""));

        make_pattern! { let pattern = "larc?ny"; }
        assert_eq!(pattern.method, Method::Swar32);
        assert!(pattern.compare_eq(b"larceny"));
        assert!(pattern.compare_eq(b"larcany"));
        assert!(pattern.compare_eq(b"larcuny"));
        assert!(pattern.compare_eq(b"larcony"));
        assert!(!pattern.compare_eq(b"lardeny"));
        assert!(!pattern.compare_eq(b"larcefy"));
        assert!(!pattern.compare_eq(b"larcenyy"));
        assert!(!pattern.compare_eq(b"larcen"));
        assert!(!pattern.compare_eq(b""));

        make_pattern! { let pattern = "????"; }
        assert_eq!(pattern.method, Method::Swar32);
        assert!(pattern.compare_eq(b"abcd"));
        assert!(pattern.compare_eq(b"aaaa"));
        assert!(pattern.compare_eq(b"dnla"));
        assert!(pattern.compare_eq(b"rt;l"));
        assert!(!pattern.compare_eq(b"abcde"));
        assert!(!pattern.compare_eq(b"abc"));
        assert!(!pattern.compare_eq(b"ab"));
        assert!(!pattern.compare_eq(b"a"));
        assert!(!pattern.compare_eq(b""));
    }

    #[test]
    fn test_swar64() {
        make_pattern! { let pattern = "how are you"; }
        assert_eq!(pattern.method, Method::Swar64);
        assert!(pattern.compare_eq(b"how are you"));
        assert!(!pattern.compare_eq(b"how arr you"));
        assert!(!pattern.compare_eq(b"how arr you"));
        assert!(!pattern.compare_eq(b"h0w are you"));
        assert!(!pattern.compare_eq(b"who are you"));
        assert!(!pattern.compare_eq(b"why am i"));
        assert!(!pattern.compare_eq(b"where are we"));
        assert!(!pattern.compare_eq(b""));

        make_pattern! { let pattern = "what ?im? i? it"; }
        assert_eq!(pattern.method, Method::Swar64);
        assert!(pattern.compare_eq(b"what time is it"));
        assert!(pattern.compare_eq(b"what lime is it"));
        assert!(pattern.compare_eq(b"what time if it"));
        assert!(pattern.compare_eq(b"what .im5 i? it"));
        assert!(!pattern.compare_eq(b"what time is it?"));
        assert!(!pattern.compare_eq(b"time is it?"));
        assert!(!pattern.compare_eq(b"asdnlasdlanldam"));
        assert!(!pattern.compare_eq(b""));

        make_pattern! { let pattern = "????????"; }
        assert_eq!(pattern.method, Method::Swar64);
        assert!(pattern.compare_eq(b"12345678"));
        assert!(pattern.compare_eq(b"asdnqnkm"));
        assert!(pattern.compare_eq(b"389u1jlm"));
        assert!(pattern.compare_eq(b"hdqi09uj"));
        assert!(!pattern.compare_eq(b"hdqi09uja"));
        assert!(!pattern.compare_eq(b"noqdkl"));
        assert!(!pattern.compare_eq(b"qoji"));
        assert!(!pattern.compare_eq(b"qdjpocomwkl"));
        assert!(!pattern.compare_eq(b""));
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn test_sse2() {
        make_pattern! { let pattern = "set the world aflame"; }
        assert_eq!(pattern.method, Method::Sse2);
        assert!(pattern.compare_eq(b"set the world aflame"));
        assert!(!pattern.compare_eq(b"set the world aflame?"));
        assert!(!pattern.compare_eq(b"set the world aflame!"));
        assert!(!pattern.compare_eq(b"set the world ablaze"));
        assert!(!pattern.compare_eq(b"set the house aflame"));
        assert!(!pattern.compare_eq(b""));

        make_pattern! { let pattern = "t?rn t?at li?ht around"; }
        assert_eq!(pattern.method, Method::Sse2);
        assert!(pattern.compare_eq(b"turn that light around"));
        assert!(pattern.compare_eq(b"turn t1at li8ht around"));
        assert!(pattern.compare_eq(b"t?rn t_at li;ht around"));
        assert!(!pattern.compare_eq(b"turn that light around?"));
        assert!(!pattern.compare_eq(b"turn that light aroun"));
        assert!(!pattern.compare_eq(b""));

        make_pattern! { let pattern = "?????????????????"; }
        assert_eq!(pattern.method, Method::Sse2);
        assert!(pattern.compare_eq(b"0123456789ABCDEF0"));
        assert!(pattern.compare_eq(b"asndkandlanldlalq"));
        assert!(pattern.compare_eq(b"2390ujondlasaasdh"));
        assert!(!pattern.compare_eq(b"nodqwndlam;[qk;"));
        assert!(!pattern.compare_eq(b"203hg1ftdvwhbjckcnvl"));
        assert!(!pattern.compare_eq(b""));
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn test_avx2() {
        make_pattern! { let pattern = "where the fear has gone there will be nothing"; }
        assert_eq!(pattern.method, Method::Avx2);
        assert!(pattern.compare_eq(b"where the fear has gone there will be nothing"));
        assert!(!pattern.compare_eq(b"where the fear has gone their will be nothing"));
        assert!(!pattern.compare_eq(b"where the fear has gone there will be nothin"));
        assert!(!pattern.compare_eq(b"where the fear has gone there will be nothing?"));
        assert!(!pattern.compare_eq(b"hfuqwom0293i2pk;,/.;'admpadbuyvqwgdiuhojfmcll"));
        assert!(!pattern.compare_eq(b""));

        make_pattern! { let pattern = "where the fear ??? gone there ???? be nothing"; }
        assert_eq!(pattern.method, Method::Avx2);
        assert!(pattern.compare_eq(b"where the fear has gone there will be nothing"));
        assert!(pattern.compare_eq(b"where the fear 988 gone there hoqa be nothing"));
        assert!(!pattern.compare_eq(b"where the fear has gone their will be nothing"));
        assert!(!pattern.compare_eq(b"where the fear has gone there will be nothing?"));
        assert!(!pattern.compare_eq(b"where the fear has gone there will be nothin"));
        assert!(!pattern.compare_eq(b""));

        make_pattern! { let pattern = "?????????????????????????????????????????????"; }
        assert_eq!(pattern.method, Method::Avx2);
        assert!(pattern.compare_eq(b"where the fear has gone there will be nothing"));
        assert!(pattern.compare_eq(b"where the fear 988 gone there hoqa be qjnnwkl"));
        assert!(pattern.compare_eq(b"qbhinldnklkdndabjkdbqoanlkmmwand,nd,andasnlda"));
        assert!(!pattern.compare_eq(b"where the fear has gone there will be nothing?"));
        assert!(!pattern.compare_eq(b"where the fear has gone there will be nothin"));
        assert!(!pattern.compare_eq(b""));
    }
}
