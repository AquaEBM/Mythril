use super::*;
use core::{marker::PhantomData, ptr::NonNull};

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
        let start =
            Box::into_non_null(Box::from_iter(iter::repeat_with(T::default).take(len))).cast();
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
    pub fn into_boxed_slice(self) -> (Box<[T]>, usize) {
        (
            unsafe { Box::from_non_null(self.as_slice().into()) },
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
    pub fn len(&self) -> NonZeroUsize {
        // SAFETY: self.start and self.end represent both edges of a NON EMPTY (boxed) slice
        unsafe { NonZeroUsize::new_unchecked(self.end.sub_ptr(self.start)) }
    }

    #[inline]
    pub fn as_slice(&self) -> &[T] {
        // SAFETY: see above
        let ptr = NonNull::slice_from_raw_parts(self.start, self.len().get());
        unsafe { ptr.as_ref() }
    }

    #[inline]
    pub fn current_index(&self) -> usize {
        // SAFETY: self.current is always >= self.start
        unsafe { self.current.sub_ptr(self.start) }
    }

    #[inline]
    pub fn process_sample_in_place(&mut self, sample: &mut T) {
        // SAFETY: same as `Self::get_current`
        mem::swap(unsafe { self.current.as_mut() }, sample);
        // SAFETY: self.current + size_of::<T>() is within the
        // same allocated object (or one size_of::<T>() after it), so it never overflows isize.
        self.current = unsafe { self.current.add(1) };
        if self.current == self.end {
            self.current = self.start;
        }
    }

    #[inline]
    pub fn process_sample(&mut self, mut sample: T) -> T {
        self.process_sample_in_place(&mut sample);
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
    fn drop(&mut self) {
        let _b = unsafe { Box::from_non_null(self.as_slice().into()) };
    }
}
