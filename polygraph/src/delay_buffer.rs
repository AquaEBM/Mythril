use core::{iter, mem, num::NonZeroUsize};

#[derive(Clone, Debug, Default)]
pub struct FixedDelayBuffer<T> {
    buf: Box<[T]>,
    current: usize,
}

impl<T> FixedDelayBuffer<T> {
    #[inline]
    pub fn new(num_samples: NonZeroUsize) -> Self
    where
        T: Default,
    {
        Self {
            buf: iter::repeat_with(T::default)
                .take(num_samples.get())
                .collect(),
            current: 0,
        }
    }

    #[inline]
    pub fn get_current(&self) -> &T {
        // SAFETY: `self.current` always starts at `0` and wraps around
        // at `self.buf.len()` so it remains in the correct range,
        // and `Self::new` garantees `self.buf` isn't empty
        unsafe { self.buf.get_unchecked(self.current) }
    }

    #[inline]
    fn get_current_mut(&mut self) -> &mut T {
        // SAFETY: same as `Self::get_current`
        unsafe { self.buf.get_unchecked_mut(self.current) }
    }

    #[inline]
    fn wrap_index(&mut self) {
        self.current += 1;
        if self.current == self.buf.len() {
            self.current = 0;
        }
    }

    #[inline]
    pub fn push_sample(&mut self, sample: T) -> T {
        let tmp = mem::replace(self.get_current_mut(), sample);
        self.wrap_index();
        tmp
    }

    #[inline]
    pub fn push_sample_ref(&mut self, sample: &mut T) {
        mem::swap(self.get_current_mut(), sample);
        self.wrap_index();
    }

    #[inline]
    fn delay_maybe_opt(&mut self, buf: &mut [T]) {
        // SAFETY: same as `Self::get_current`
        let current = unsafe { self.buf.get_unchecked_mut(self.current..) };

        if let Some((start, rem)) = buf.split_at_mut_checked(current.len()) {
            // hopefully the checks are optimized away
            current.swap_with_slice(start);

            let mut iter = rem.chunks_exact_mut(self.buf.len());

            iter.by_ref()
                .for_each(|chunk| self.buf.swap_with_slice(chunk));

            let rem = iter.into_remainder();
            let rem_len = rem.len();

            self.buf[..rem_len].swap_with_slice(rem);
            self.current = rem_len
        } else {
            let len = buf.len();
            buf.swap_with_slice(&mut current[..len]);
            self.current += len;
        }
    }

    #[inline]
    pub fn delay(&mut self, buf: &mut [T]) {
        self.delay_maybe_opt(buf)
    }
}

#[cfg(test)]
pub mod tests {

    use super::*;

    #[test]
    fn nani() {
        let mut buf = Vec::from_iter((0..12).map(|i| i as f32));

        let mut delay = FixedDelayBuffer::new(NonZeroUsize::new(18).unwrap());

        delay.delay(&mut buf);

        println!("{buf:?}");
    }
}
