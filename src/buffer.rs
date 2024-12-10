use super::*;
use simd_util::simd::num::SimdFloat;

pub struct BufferList<T, U> {
    buffers: Box<[(Box<[T]>, U)]>,
    buf_len: NonZeroUsize,
}

impl<T, U> BufferList<T, U> {
    /// # Safety
    ///
    /// `T` must be safely zeroable
    #[inline]
    #[must_use]
    pub unsafe fn new_with(
        num_buffers: usize,
        buf_len: NonZeroUsize,
        mut f: impl FnMut() -> U,
    ) -> Self {
        Self {
            buffers: iter::repeat_with(|| {
                (Box::new_zeroed_slice(buf_len.get()).assume_init(), f())
            })
            .take(num_buffers)
            .collect(),
            buf_len,
        }
    }

    /// # Safety
    ///
    /// Same as [`Self::new_with`]
    #[inline]
    #[must_use]
    pub unsafe fn new_default(num_buffers: usize, buf_len: NonZeroUsize) -> Self
    where
        U: Default,
    {
        unsafe { Self::new_with(num_buffers, buf_len, U::default) }
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<(&[T], &U)> {
        self.buffers
            .get(index)
            .map(|(buf, mask)| (buf.as_ref(), mask))
    }

    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<(&mut [T], &mut U)> {
        self.buffers
            .get_mut(index)
            .map(|(buf, mask)| (buf.as_mut(), mask))
    }

    #[inline]
    pub fn range_mut(&mut self, start: usize, len: NonZeroUsize) -> Option<BufferListRefMut<T, U>> {
        (start + len.get() <= self.buf_len.get()).then_some(BufferListRefMut {
            buffers: self.buffers.as_mut(),
            start,
            len,
        })
    }
}

impl<T: SimdFloat, U> BufferList<T, U> {
    #[inline]
    #[must_use]
    pub fn new_vfloat_with(
        num_buffers: usize,
        buf_len: NonZeroUsize,
        f: impl FnMut() -> U,
    ) -> Self {
        // SAFETY: T: SimdFloat implies T is a vector of f32s or f64s, whicha re safely zeroable
        unsafe { Self::new_with(num_buffers, buf_len, f) }
    }

    #[inline]
    #[must_use]
    pub fn new_vfloat_default(num_buffers: usize, buf_len: NonZeroUsize) -> Self
    where
        U: Default,
    {
        Self::new_vfloat_with(num_buffers, buf_len, Default::default)
    }
}

pub struct BufferListRefMut<'a, T, U> {
    buffers: &'a mut [(Box<[T]>, U)],
    start: usize,
    len: NonZeroUsize,
}

impl<'a, T, U> From<&'a mut BufferList<T, U>> for BufferListRefMut<'a, T, U> {
    #[inline]
    fn from(value: &'a mut BufferList<T, U>) -> Self {
        value.range_mut(0, value.buf_len).unwrap()
    }
}

impl<T, U> BufferListRefMut<'_, T, U> {
    #[inline]
    pub fn len(&self) -> NonZeroUsize {
        self.len
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<(&[T], &U)> {
        self.buffers.get(index).map(|(buf, mask)| {
            let range = self.start..self.start + self.len.get();
            (unsafe { buf.get_unchecked(range) }, mask)
        })
    }

    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<(&mut [T], &mut U)> {
        self.buffers.get_mut(index).map(|(buf, mask)| {
            let range = self.start..self.start + self.len.get();
            (unsafe { buf.get_unchecked_mut(range) }, mask)
        })
    }

    #[inline]
    pub fn reborrow(&mut self) -> BufferListRefMut<T, U> {
        BufferListRefMut {
            buffers: self.buffers,
            start: self.start,
            len: self.len,
        }
    }
}

pub struct Buffers<'a, T: SimdFloat> {
    buffers: BufferListRefMut<'a, T, T::Bits>,
    inputs: &'a [usize],
    outputs: &'a [usize],
}

pub enum GetBufferError {
    OOB,
    Empty,
}

impl<T: SimdFloat> Buffers<'_, T> {
    #[inline]
    pub fn len(&self) -> NonZeroUsize {
        self.buffers.len()
    }

    #[inline]
    pub fn input(&self, index: usize) -> Result<(&[T], &T::Bits), GetBufferError> {
        let &index = self.inputs.get(index).ok_or(GetBufferError::OOB)?;
        if index == usize::MAX {
            return Err(GetBufferError::Empty);
        }
        Ok(self.buffers.get(index).unwrap())
    }

    #[inline]
    pub fn output(&mut self, index: usize) -> Result<&mut [T], GetBufferError> {
        let &index = self.outputs.get(index).ok_or(GetBufferError::OOB)?;
        if index == usize::MAX {
            return Err(GetBufferError::Empty);
        }
        Ok(self.buffers.get_mut(index).unwrap().0)
    }
}
