use super::*;
use core::{marker::PhantomData, ptr::NonNull, num::NonZeroUsize, mem};

/// A delay buffer with a fixed, non-zero size
#[derive(Clone, Debug)]
pub struct Delay<T> {
    start: NonNull<T>,
    end: NonNull<T>,
    current: NonNull<T>,
    _marker: PhantomData<T>,
}

impl<T: Default> Delay<T> {
    #[inline]
    pub fn new(num_samples: NonZeroUsize) -> Self {
        let len = num_samples.get();
        let boxed_slice = iter::repeat_with(T::default).take(len).collect();
        let start = Box::into_non_null(boxed_slice).as_non_null_ptr();
        let end = unsafe { start.add(len) };

        Self {
            start,
            end,
            current: start,
            _marker: PhantomData,
        }
    }
}

impl<T> Delay<T> {

    #[inline]
    pub fn current_index(&self) -> usize {
        // SAFETY: self.current is always >= self.start
        unsafe { self.current.offset_from_unsigned(self.start) }
    }

    #[inline]
    pub fn into_boxed_slice(self) -> (Box<[T]>, usize) {
        (
            unsafe { Box::from_non_null(self.as_non_null_slice()) },
            self.current_index(),
        )
    }

    #[inline]
    pub fn get_current(&self) -> &T {
        // SAFETY: `self.current` always starts at self.start, and, in Self::process, wraps
        // around at self.end Self::new garantees that self.start != self.end
        unsafe { self.current.as_ref() }
    }

    #[inline]
    fn get_current_mut(&mut self) -> &mut T {
        // SAFETY: same as `Self::get_current`
        unsafe { self.current.as_mut() }
    }

    #[inline]
    fn wrap_current_ptr(&mut self) {
        // SAFETY: self.current + size_of::<T>() is within the
        // same allocated object so it never overflows isize.
        self.current = unsafe { self.current.add(1) };
        if self.current == self.end {
            self.current = self.start;
        }
    }

    #[inline]
    pub fn len(&self) -> NonZeroUsize {
        // SAFETY: self.start and self.end represent both edges of a NON EMPTY (boxed) slice
        unsafe { NonZeroUsize::new_unchecked(self.end.offset_from_unsigned(self.start)) }
    }

    #[inline]
    fn as_non_null_slice(&self) -> NonNull<[T]> {
        NonNull::slice_from_raw_parts(self.start, self.len().get())
    }

    #[inline]
    pub fn as_slice(&self) -> &[T] {

        let slice = self.as_non_null_slice();
        // SAFETY: see Self::len
        unsafe { slice.as_ref() }
    }

    #[inline]
    pub fn process_sample_in_place(&mut self, sample: &mut T) {
        
        mem::swap(self.get_current_mut(), sample);
        self.wrap_current_ptr();
    }

    #[inline]
    pub fn process_sample(&mut self, mut sample: T) -> T {

        sample = mem::replace(self.get_current_mut(), sample);
        self.wrap_current_ptr();
        sample
    }

    #[inline]
    pub fn process_buffer(&mut self, buf: &mut [T]) {
        for sample in buf {
            self.process_sample_in_place(sample)
        }
    }
}

impl<T> Drop for Delay<T> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: *self is dropped after this
        let _b = unsafe { Box::from_non_null(self.as_non_null_slice()) };
    }
}
