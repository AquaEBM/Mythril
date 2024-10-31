use core::{cell::Cell, iter, mem, num::NonZeroUsize};

use simd_util::{
    simd::{num::SimdFloat, Simd, SimdElement},
    split_stereo_cell, FLOATS_PER_VECTOR, STEREO_VOICES_PER_VECTOR,
};

/// A wrapper around a `Cell<T>` that only allows for reading the contained value
#[repr(transparent)]
pub struct ReadOnly<T: ?Sized>(Cell<T>);

// SAFETY (for the `mem::transmute`s used in the implementations of `ReadOnly<T>`):
//  - `ReadOnly<T>` has the same layout as `Cell<T>` (which in turn has the same layout as `T`).
//  - `ReadOnly<T>`'s semantics and functionality (`ReadOnly::get`) are a subset of those of
// `Cell<T>`, which garantees that the invariants required by the inner `Cell<T>` aren't broken.
//
// - Through ellision rules, lifetimes are preserved and remain bounded.

impl<T: ?Sized> ReadOnly<T> {
    #[inline]
    pub fn from_ref(cell: &Cell<T>) -> &Self {
        unsafe { mem::transmute(cell) }
    }
}

impl<T> ReadOnly<[T]> {
    #[inline]
    pub fn as_slice(&self) -> &[ReadOnly<T>] {
        unsafe { mem::transmute(self) }
    }
}

impl<T, const N: usize> ReadOnly<[T; N]> {
    #[inline]
    pub fn as_array(&self) -> &[ReadOnly<T>; N] {
        unsafe { mem::transmute(self) }
    }
}

impl<T> ReadOnly<T> {
    #[inline]
    pub fn from_cell(cell: Cell<T>) -> Self {
        Self(cell)
    }

    #[inline]
    pub fn from_slice(cell_slice: &[Cell<T>]) -> &[Self] {
        unsafe { mem::transmute(cell_slice) }
    }

    #[inline]
    pub fn get(&self) -> T
    where
        T: Copy,
    {
        self.0.get()
    }
}

impl<T: SimdElement> ReadOnly<Simd<T, FLOATS_PER_VECTOR>> {
    #[inline]
    pub fn split_stereo(&self) -> &ReadOnly<[Simd<T, 2>; STEREO_VOICES_PER_VECTOR]> {
        ReadOnly::from_ref(split_stereo_cell(&self.0))
    }
}

impl<T: SimdElement> ReadOnly<[Simd<T, FLOATS_PER_VECTOR>]> {
    #[inline]
    pub fn split_stereo_slice(&self) -> &[[ReadOnly<Simd<T, 2>>; STEREO_VOICES_PER_VECTOR]] {
        unsafe { mem::transmute(self) }
    }
}

pub type Buffer<T> = Box<Cell<[T]>>;

/// # Safety
///
/// All bit patterns for type `T` must be valid,
#[inline]
#[must_use]
pub unsafe fn new_owned_buffer<T>(len: usize) -> Buffer<T> {
    // SAFETY: Cell<T> has the same layout as T, thus, by extension, Cell<[T]>
    // has the same layout as [T] + garantee specified in the doc
    mem::transmute(Box::<[T]>::new_uninit_slice(len).assume_init())
}

/// # Safety
///
/// `T` must be safely zeroable
#[inline]
#[must_use]
pub unsafe fn new_owned_buffer_zeroed<T>(len: usize) -> Buffer<T> {
    // SAFETY: Cell<T> has the same layout as T, thus, by extension, Cell<[T]>
    // has the same layout as [T] + garantee specified in the doc
    mem::transmute(Box::<[T]>::new_zeroed_slice(len).assume_init())
}

#[inline]
#[must_use]
pub fn new_vfloat_buffer<T: SimdFloat>(len: usize) -> Buffer<T> {
    // SAFETY: `f32`s and 'f64's (and thus `Simd<f32, N>`s and `Simd<f64, N>`s,
    // the only implementors of `SimdFloat`) can be initialized with any bit pattern
    unsafe { new_owned_buffer(len) }
}

pub struct BufferList<T, U> {
    buffers: Box<[(Buffer<T>, Cell<U>)]>,
    buf_len: NonZeroUsize,
}

impl<T, U> BufferList<T, U> {
    /// # Safety
    ///
    /// All bit patterns for type `T` must be valid
    #[inline]
    #[must_use]
    pub unsafe fn new_with(
        num_buffers: usize,
        buf_len: NonZeroUsize,
        mut f: impl FnMut() -> U,
    ) -> Self {
        Self {
            buffers: iter::repeat_with(|| (new_owned_buffer(buf_len.get()), Cell::new(f())))
                .take(num_buffers)
                .collect(),
            buf_len,
        }
    }

    /// # Safety
    ///
    /// `T` must be safely zeroable
    #[inline]
    #[must_use]
    pub unsafe fn new_zeroed_with(
        num_buffers: usize,
        buf_len: NonZeroUsize,
        mut f: impl FnMut() -> U,
    ) -> Self {
        Self {
            buffers: iter::repeat_with(|| (new_owned_buffer_zeroed(buf_len.get()), Cell::new(f())))
                .take(num_buffers)
                .collect(),
            buf_len,
        }
    }

    /// # Safety
    ///
    /// All bit patterns for type `T` must be valid
    #[inline]
    #[must_use]
    pub unsafe fn new_default(num_buffers: usize, buf_len: NonZeroUsize) -> Self
    where
        U: Default,
    {
        unsafe { Self::new_with(num_buffers, buf_len, U::default) }
    }

    /// # Safety
    ///
    /// `T` must be safely zeroable
    #[inline]
    #[must_use]
    pub unsafe fn new_default_zeroed(num_buffers: usize, buf_len: NonZeroUsize) -> Self
    where
        U: Default,
    {
        unsafe { Self::new_zeroed_with(num_buffers, buf_len, U::default) }
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<(&[Cell<T>], &Cell<U>)> {
        self.buffers
            .get(index)
            .map(|(buf, mask)| (buf.as_slice_of_cells(), mask))
    }

    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<(&mut [T], &mut U)> {
        self.buffers
            .get_mut(index)
            .map(|(buf, mask)| (buf.get_mut(), mask.get_mut()))
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
        // SAFETY: T: SimdFloat so all bit patterns for T are valid
        unsafe { Self::new_with(num_buffers, buf_len, f) }
    }

    #[inline]
    #[must_use]
    pub fn new_vfloat_zeroed_with(
        num_buffers: usize,
        buf_len: NonZeroUsize,
        f: impl FnMut() -> U,
    ) -> Self {
        // SAFETY: T: SimdFloat so T is safely zeroable
        unsafe { Self::new_zeroed_with(num_buffers, buf_len, f) }
    }

    #[inline]
    #[must_use]
    pub fn new_vfloat_default(num_buffers: usize, buf_len: NonZeroUsize) -> Self
    where
        U: Default,
    {
        // SAFETY: T: SimdFloat so all bit patterns for T are valid
        unsafe { Self::new_default(num_buffers, buf_len) }
    }

    pub fn new_vfloat_zeroed_default(num_buffers: usize, buf_len: NonZeroUsize) -> Self
    where
        U: Default,
    {
        // SAFETY: T: SimdFloat so T is safely zeroable
        unsafe { Self::new_default_zeroed(num_buffers, buf_len) }
    }
}

pub struct BufferListRefMut<'a, T, U> {
    buffers: &'a mut [(Buffer<T>, Cell<U>)],
    start: usize,
    len: NonZeroUsize,
}

impl<'a, T, U> From<&'a mut BufferList<T, U>> for BufferListRefMut<'a, T, U> {
    #[inline]
    fn from(value: &'a mut BufferList<T, U>) -> Self {
        value.range_mut(0, value.buf_len).unwrap()
    }
}

impl<'a, T, U> BufferListRefMut<'a, T, U> {
    #[inline]
    pub fn len(&self) -> NonZeroUsize {
        self.len
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<(&[Cell<T>], &Cell<U>)> {
        self.buffers.get(index).map(|(buf, mask)| {
            let range = self.start..self.start + self.len.get();
            (
                unsafe { buf.as_slice_of_cells().get_unchecked(range) },
                mask,
            )
        })
    }

    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<(&mut [T], &mut U)> {
        self.buffers.get_mut(index).map(|(buf, mask)| {
            let range = self.start..self.start + self.len.get();
            (
                unsafe { buf.get_mut().get_unchecked_mut(range) },
                mask.get_mut(),
            )
        })
    }

    #[inline]
    pub fn reborrow(&mut self) -> BufferListRefMut<T, U> {
        BufferListRefMut {
            buffers: &mut self.buffers,
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

impl<'a, T: SimdFloat> Buffers<'a, T> {
    #[inline]
    pub fn len(&self) -> NonZeroUsize {
        self.buffers.len()
    }

    #[inline]
    pub fn input(&mut self, index: usize) -> Result<(&[T], &T::Bits), GetBufferError> {
        let &index = self.inputs.get(index).ok_or(GetBufferError::OOB)?;
        if index == usize::MAX {
            return Err(GetBufferError::Empty);
        }
        Ok(self
            .buffers
            .get_mut(index)
            .map(|(buf, mask)| (&*buf, &*mask))
            .unwrap())
    }

    #[inline]
    pub fn output(&mut self, index: usize) -> Result<&mut [T], GetBufferError> {
        let &index = self.outputs.get(index).ok_or(GetBufferError::OOB)?;
        if index == usize::MAX {
            return Err(GetBufferError::Empty);
        }
        Ok(self.buffers.get_mut(index).unwrap().0)
    }

    #[inline]
    pub fn input_shared(
        &self,
        index: usize,
    ) -> Result<(&[ReadOnly<T>], &ReadOnly<T::Bits>), GetBufferError> {
        let &index = self.inputs.get(index).ok_or(GetBufferError::OOB)?;
        if index == usize::MAX {
            return Err(GetBufferError::Empty);
        }
        Ok(self
            .buffers
            .get(index)
            .map(|(buf, mask)| (ReadOnly::from_slice(buf), ReadOnly::from_ref(mask)))
            .unwrap())
    }

    #[inline]
    pub fn output_shared(&self, index: usize) -> Result<&[Cell<T>], GetBufferError> {
        let &index = self.outputs.get(index).ok_or(GetBufferError::OOB)?;
        if index == usize::MAX {
            return Err(GetBufferError::Empty);
        }
        Ok(self.buffers.get(index).unwrap().0)
    }
}
