use core::{iter, mem, num::NonZeroUsize};

/// A delay buffer with a fixed, non-zero size
#[derive(Clone, Debug, Default)]
pub struct Delay<T> {
    buf: Box<[T]>,
    current: usize,
}

impl<T: Default> Delay<T> {
    #[inline]
    pub fn new(num_samples: NonZeroUsize) -> Self {
        Self {
            buf: iter::repeat_with(T::default)
                .take(num_samples.get())
                .collect(),
            current: 0,
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.buf.fill_with(T::default)
    }
}

impl<T> Delay<T> {
    #[inline]
    pub fn get_current(&self) -> &T {
        // SAFETY: `self.current` always starts at `0` and wraps around
        // at `self.buf.len()` so it remains in the correct range,
        // and `Self::new` garantees `self.buf` isn't empty
        unsafe { self.buf.get_unchecked(self.current) }
    }

    #[inline]
    pub fn len(&self) -> NonZeroUsize {
        // SAFETY: self.buf has non-zero length
        unsafe { NonZeroUsize::new_unchecked(self.buf.len()) }
    }

    #[inline]
    pub fn process(&mut self, buf: &mut [T]) {
        for sample in buf {
            // SAFETY: same as `Self::get_current`
            mem::swap(unsafe { self.buf.get_unchecked_mut(self.current) }, sample);
            self.current += 1;
            if self.current == self.buf.len() {
                self.current = 0;
            }
        }
    }
}
