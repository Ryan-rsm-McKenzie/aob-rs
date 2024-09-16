use std::{
    marker::PhantomData,
    mem,
    ops::RangeFrom,
    ptr::NonNull,
};

#[derive(Clone, Copy)]
pub(crate) struct ThinSlice<'a, T> {
    pub(crate) start: NonNull<T>,
    pub(crate) end: NonNull<T>,
    _phantom: PhantomData<&'a T>,
}

impl<'a, T> ThinSlice<'a, T> {
    #[must_use]
    pub(crate) fn cast<U>(&self) -> ThinSlice<'a, U> {
        ThinSlice {
            start: self.start.cast(),
            end: self.end.cast(),
            _phantom: PhantomData,
        }
    }

    /// SAFETY: `range` must be a subset of the range [`start`, `end`).
    #[must_use]
    pub(crate) unsafe fn get_unchecked(&self, range: RangeFrom<usize>) -> Self {
        Self {
            start: self.start.add(range.start),
            end: self.end,
            _phantom: PhantomData,
        }
    }

    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// SAFETY: The byte distance between `start` and `end` must be an exact multiple of `T`.
    #[must_use]
    pub(crate) unsafe fn len(&self) -> usize {
        self.end.offset_from(self.start) as usize
    }

    #[must_use]
    pub(crate) fn trim_end_to_nearest_multiple_of<U>(
        self,
    ) -> (ThinSlice<'a, U>, ThinSlice<'a, u8>) {
        // SAFETY: `len` is always valid for a slice of `u8`.
        let cur_len = unsafe { self.cast::<u8>().len() };
        if cur_len < mem::size_of::<U>() {
            return (ThinSlice::default(), self.cast());
        }

        let offset = cur_len % mem::size_of::<U>();
        if offset == 0 {
            return (self.cast(), ThinSlice::default());
        }

        // SAFETY: `offset` is valid for self's pointer range
        let boundary = unsafe { self.end.cast::<u8>().sub(offset) };
        let trimmed = ThinSlice {
            start: self.start.cast(),
            end: boundary.cast(),
            _phantom: PhantomData,
        };
        let extra = ThinSlice {
            start: boundary,
            end: self.end.cast(),
            _phantom: PhantomData,
        };
        (trimmed, extra)
    }
}

impl<T> Default for ThinSlice<'_, T> {
    fn default() -> Self {
        Self {
            start: NonNull::dangling(),
            end: NonNull::dangling(),
            _phantom: PhantomData,
        }
    }
}

impl<'a, T> From<&'a [T]> for ThinSlice<'a, T> {
    fn from(value: &'a [T]) -> Self {
        let range = value.as_ptr_range();
        // SAFETY: ptrs from a slice are always valid
        let (start, end) = unsafe {
            let start = NonNull::new_unchecked(range.start.cast_mut());
            let end = NonNull::new_unchecked(range.end.cast_mut());
            (start, end)
        };
        Self {
            start,
            end,
            _phantom: PhantomData,
        }
    }
}
