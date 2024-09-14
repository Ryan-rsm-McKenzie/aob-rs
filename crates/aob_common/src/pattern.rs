use std::{
    alloc,
    alloc::Layout,
    marker::PhantomData,
    mem,
    ptr,
    slice,
};

trait Integer: Eq + Sized {
    const MAX: Self;
}

macro_rules! make_integer {
    ($type:ty) => {
        impl Integer for $type {
            const MAX: Self = Self::MAX;
        }
    };
}

make_integer!(u32);
make_integer!(u16);

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
    #[cfg(target_arch = "x86")]
    pub(crate) use std::arch::x86::{
        __m128i,
        _mm_blendv_epi8,
        _mm_cmpeq_epi8,
        _mm_load_si128,
        _mm_loadu_si128,
        _mm_movemask_epi8,
        _mm_set1_epi8,
    };
    #[cfg(target_arch = "x86_64")]
    pub(crate) use std::arch::x86_64::{
        __m128i,
        _mm_blendv_epi8,
        _mm_cmpeq_epi8,
        _mm_load_si128,
        _mm_loadu_si128,
        _mm_movemask_epi8,
        _mm_set1_epi8,
    };

    impl super::Simd for __m128i {
        const LANE_COUNT: usize = 16;
        type Integer = u16;

        unsafe fn blendv_epi8(self, b: Self, mask: Self) -> Self {
            _mm_blendv_epi8(self, b, mask)
        }

        unsafe fn cmpeq_epi8(self, b: Self) -> Self {
            _mm_cmpeq_epi8(self, b)
        }

        unsafe fn load(mem_addr: *const Self) -> Self {
            _mm_load_si128(mem_addr)
        }

        unsafe fn loadu(mem_addr: *const Self) -> Self {
            _mm_loadu_si128(mem_addr)
        }

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        unsafe fn movemask_epi8(self) -> Self::Integer {
            _mm_movemask_epi8(self) as u32 as u16
        }

        unsafe fn set1_epi8(a: i8) -> Self {
            _mm_set1_epi8(a)
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod avx2 {
    #[cfg(target_arch = "x86")]
    pub(crate) use std::arch::x86::{
        __m256i,
        _mm256_blendv_epi8,
        _mm256_cmpeq_epi8,
        _mm256_load_si256,
        _mm256_loadu_si256,
        _mm256_movemask_epi8,
        _mm256_set1_epi8,
    };
    #[cfg(target_arch = "x86_64")]
    pub(crate) use std::arch::x86_64::{
        __m256i,
        _mm256_blendv_epi8,
        _mm256_cmpeq_epi8,
        _mm256_load_si256,
        _mm256_loadu_si256,
        _mm256_movemask_epi8,
        _mm256_set1_epi8,
    };

    impl super::Simd for __m256i {
        const LANE_COUNT: usize = 32;
        type Integer = u32;

        unsafe fn blendv_epi8(self, b: Self, mask: Self) -> Self {
            _mm256_blendv_epi8(self, b, mask)
        }

        unsafe fn cmpeq_epi8(self, b: Self) -> Self {
            _mm256_cmpeq_epi8(self, b)
        }

        unsafe fn load(mem_addr: *const Self) -> Self {
            _mm256_load_si256(mem_addr)
        }

        unsafe fn loadu(mem_addr: *const Self) -> Self {
            _mm256_loadu_si256(mem_addr)
        }

        #[allow(clippy::cast_sign_loss)]
        unsafe fn movemask_epi8(self) -> Self::Integer {
            _mm256_movemask_epi8(self) as u32
        }

        unsafe fn set1_epi8(a: i8) -> Self {
            _mm256_set1_epi8(a)
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Method {
    Scalar,
    Sse2,
    Avx2,
}

impl Method {
    #[must_use]
    fn from_size(size: usize) -> Self {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if size >= avx2::__m256i::LANE_COUNT && is_x86_feature_detected!("avx2") {
            return Self::Avx2;
        }

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if size >= sse2::__m128i::LANE_COUNT && is_x86_feature_detected!("sse2") {
            return Self::Sse2;
        }

        Self::Scalar
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
        let word = unsafe { alloc::alloc_zeroed(layout) };
        let mask = unsafe { alloc::alloc(layout).cast::<MaskedByte>() };
        unsafe { ptr::write_bytes(mask, 0xFF, layout.size()) };

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
                Method::Sse2 => self.compare_eq_sse2(other),
                Method::Avx2 => self.compare_eq_avx2(other),
            }
        } else {
            false
        }
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
            if mask[i].is_unmasked() && word[i] != other[i] {
                return false;
            }
        }
        true
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[must_use]
    fn compare_eq_simd<T: Simd>(&self, other: &[u8]) -> bool {
        assert!(self.size >= T::LANE_COUNT);

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
            if mask[i].is_unmasked() && word[i] != other[i] {
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
