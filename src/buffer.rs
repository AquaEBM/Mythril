use core::marker;

use super::*;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum GetBufError {
    OOB,
    Empty,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum GetBufMutError {
    Overlapping,
    Other(GetBufError),
}

impl GetBufError {
    pub fn is_unused(self) -> bool {
        self == GetBufError::Empty
    }
}

pub trait Buffers {
    type Sample;

    fn num_inputs(&self) -> usize;

    fn num_outputs(&self) -> usize;

    fn num_samples(&self) -> NonZeroUsize;

    fn get_input(&self, index: usize) -> Result<&[Self::Sample], GetBufError>;

    fn get_output(&mut self, index: usize) -> Result<&mut [Self::Sample], GetBufError>;

    fn get_many<const N: usize, const M: usize>(
        &mut self,
        inputs: [usize; N],
        outputs: [usize; M],
    ) -> (
        [Result<&[Self::Sample], GetBufError>; N],
        Option<[Result<&mut [Self::Sample], GetBufError>; M]>,
    );
}

#[derive(Clone, Debug)]
struct BufferList<T> {
    list: Box<[NonNull<T>]>,
    len: NonZeroUsize,
    phantom: marker::PhantomData<T>,
}

impl<T: Default> BufferList<T> {
    fn new(num_bufs: usize, buf_size: NonZeroUsize) -> Self {
        Self {
            list: iter::repeat_with(|| {
                let boxed = Box::into_raw(Box::from_iter(
                    iter::repeat_with(T::default).take(buf_size.get()),
                ))
                .as_mut_ptr();
                unsafe { NonNull::new_unchecked(boxed) }
            })
            .take(num_bufs)
            .collect(),
            len: buf_size,
            phantom: marker::PhantomData,
        }
    }
}

impl<T> BufferList<T> {
    /// # Safety:
    ///
    /// before reading anything from the buffers, T should be properly initialized
    /// unless T accepts any bit pattern
    unsafe fn new_uninit(num_bufs: usize, buf_size: NonZeroUsize) -> Self {
        Self {
            list: iter::repeat_with(|| {
                let boxed = unsafe { Box::new_uninit_slice(buf_size.get()).assume_init() };
                let pointer = Box::into_raw(boxed).as_mut_ptr();
                unsafe { NonNull::new_unchecked(pointer) }
            })
            .take(num_bufs)
            .collect(),
            len: buf_size,
            phantom: marker::PhantomData,
        }
    }

    /// Validate an IO configuration with respect to the given buffer list.
    fn validate(&mut self, inputs: &[u32], outputs: &[u32]) -> bool {
        let num_bufs = self.list.len();

        for in_idx in inputs {
            if in_idx != &u32::MAX && *in_idx as usize > num_bufs {
                return false;
            }
        }

        for (i, out_idx) in outputs.iter().enumerate() {
            if out_idx != &u32::MAX
                && (*out_idx as usize > num_bufs
                    || inputs.contains(out_idx)
                    || outputs[..i].contains(out_idx))
            {
                return false;
            }
        }

        true
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BufferListRef<'a, T> {
    inputs: &'a [u32],
    outputs: &'a [u32],
    size: NonZeroUsize,
    bufs: NonNull<NonNull<T>>,
    phantom: marker::PhantomData<&'a mut T>,
}

fn get_buf_index(slice: &[u32], index: usize) -> Result<usize, GetBufError> {
    slice.get(index).ok_or(GetBufError::OOB).and_then(|&i| {
        (i == u32::MAX)
            .then_some(i as usize)
            .ok_or(GetBufError::Empty)
    })
}

unsafe fn get_buf<T>(bufs: NonNull<NonNull<T>>, index: usize, size: usize) -> NonNull<[T]> {
    NonNull::slice_from_raw_parts(unsafe { bufs.add(index).read() }, size)
}

impl<'a, T> BufferListRef<'a, T> {
    /// Create a new `BufferListRef`
    unsafe fn new_unchecked(
        bufs: &'a mut BufferList<T>,
        inputs: &'a [u32],
        outputs: &'a [u32],
    ) -> Self {
        Self {
            inputs,
            outputs,
            size: bufs.len,
            // SAFETY: it bufs.list is a valid box so non-null
            bufs: unsafe { NonNull::new_unchecked(bufs.list.as_mut_ptr()) },
            phantom: marker::PhantomData,
        }
    }

    /// Create a new `BufferListRef`.
    ///
    /// Returns `None` if `output` has elements (excluding those `== u32::MAX`),
    /// that are greater than bufs.size.get() or aren't unique or are contained in `inputs`.
    ///
    /// This function is ~ `O(n^2)`. So, be careful when passing in large slices
    fn new(bufs: &'a mut BufferList<T>, inputs: &'a [u32], outputs: &'a [u32]) -> Option<Self> {
        bufs.validate(inputs, outputs)
            .then(|| unsafe { Self::new_unchecked(bufs, inputs, outputs) })
    }

    fn get_input_buf(&self, index: usize) -> Result<&[T], GetBufError> {
        get_buf_index(self.outputs, index)
            .map(|i| unsafe { get_buf(self.bufs, i, self.size.get()).as_ref() })
    }

    fn get_output_buf(&mut self, index: usize) -> Result<&mut [T], GetBufError> {
        get_buf_index(self.inputs, index)
            .map(|i| unsafe { get_buf(self.bufs, i, self.size.get()).as_mut() })
    }
}

impl<'a, T> Buffers for BufferListRef<'a, T> {
    type Sample = T;

    fn num_inputs(&self) -> usize {
        self.inputs.len()
    }

    fn num_outputs(&self) -> usize {
        self.outputs.len()
    }

    fn num_samples(&self) -> NonZeroUsize {
        self.size
    }

    fn get_input(&self, index: usize) -> Result<&[Self::Sample], GetBufError> {
        self.get_input_buf(index)
    }

    fn get_output(&mut self, index: usize) -> Result<&mut [Self::Sample], GetBufError> {
        self.get_output_buf(index)
    }

    fn get_many<const N: usize, const M: usize>(
        &mut self,
        inputs: [usize; N],
        outputs: [usize; M],
    ) -> (
        [Result<&[Self::Sample], GetBufError>; N],
        Option<[Result<&mut [Self::Sample], GetBufError>; M]>,
    ) {
        let input_ptrs = inputs.map(|i| {
            let index = get_buf_index(self.inputs, i);
            index.map(|i| unsafe { get_buf(self.bufs, i, self.size.get()) })
        });

        fn check_disgoint(slice: &[usize], mut filter: impl FnMut(&usize) -> bool) -> bool {
            for (i, e) in slice.iter().enumerate() {
                if !filter(e) {
                    continue;
                }
                for b in &slice[..i] {
                    if b == e {
                        return false;
                    }
                }
            }

            true
        }

        let output_ptrs = check_disgoint(&outputs, |&e| {
            self.inputs.get(e).is_some_and(|&i| i != u32::MAX)
        })
        .then(|| {
            outputs.map(|i| {
                let index = get_buf_index(self.outputs, i);
                index.map(|i| unsafe { get_buf(self.bufs, i, self.size.get()) })
            })
        });

        (
            input_ptrs.map(|r| r.map(|p| unsafe { p.as_ref() })),
            output_ptrs.map(|ptrs| ptrs.map(|r| r.map(|mut p| unsafe { p.as_mut() }))),
        )
    }
}
