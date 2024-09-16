use std::{
    marker::PhantomData,
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
    /// SAFETY: `range` must be a subset of the range [`start`, `end`)
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

    /// SAFETY: `mid` must be a valid offset in the range [`start`, `end`)
    #[must_use]
    pub(crate) unsafe fn split_at_unchecked<L, R>(
        &self,
        mid: usize,
    ) -> (ThinSlice<'a, L>, ThinSlice<'a, R>) {
        let mid = self.start.cast::<u8>().add(mid);
        let left = ThinSlice {
            start: self.start.cast(),
            end: mid.cast(),
            _phantom: PhantomData,
        };
        let right = ThinSlice {
            start: mid.cast(),
            end: self.end.cast(),
            _phantom: PhantomData,
        };
        (left, right)
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
        // SAFETY: pointers from a slice are always valid
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
