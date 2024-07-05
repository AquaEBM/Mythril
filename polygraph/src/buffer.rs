use core::{cell::Cell, mem, num::NonZeroUsize};

use simd_util::{
    simd::{num::SimdFloat, Simd, SimdElement},
    split_stereo_cell, FLOATS_PER_VECTOR, STEREO_VOICES_PER_VECTOR,
};

/// This is a wrapper around a `Cell<T>` that only allows for reading the contained value
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
    pub fn from_cell_ref(cell: &Cell<T>) -> &Self {
        unsafe { mem::transmute(cell) }
    }
}

impl<T> ReadOnly<[T]> {
    #[inline]
    pub fn transpose(&self) -> &[ReadOnly<T>] {
        unsafe { mem::transmute(self) }
    }
}

impl<T, const N: usize> ReadOnly<[T; N]> {
    #[inline]
    pub fn transpose(&self) -> &[ReadOnly<T>; N] {
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
        ReadOnly::from_cell_ref(split_stereo_cell(&self.0))
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
/// All bit patterns for type T must be valid,
#[inline]
pub(crate) unsafe fn new_owned_buffer<T>(len: usize) -> Buffer<T> {
    // SAFETY: Cell<T> has the same layout as T, thus, by extension, Cell<[T]>
    // has the same layout as [T] + garantee specified in the doc
    mem::transmute(Box::<[T]>::new_uninit_slice(len).assume_init())
}

#[inline]
pub fn new_vfloat_buffer<T: SimdFloat>(len: usize) -> Buffer<T> {
    // SAFETY: `f32`s and 'f64's (and thus `Simd<f32, N>`s and `Simd<f64, N>`s,
    // the only implementors of `SimdFloat`) can be initialized with any bit pattern
    unsafe { new_owned_buffer(len) }
}

// TODO: name bikeshedding

// The following structs describe a linked list-like interface in order to allow
// audio graph nodes (and potentially other audio graphs nested in them) to (re)use buffers
// from their callers as master/global inputs/outputs
//
// the tricks described in this discussion are used:
// https://users.rust-lang.org/t/safe-interface-for-a-singly-linked-list-of-mutable-references/107401

pub struct BufferHandleLocal<'a, T> {
    // the most notable trick here is the usage of a trait object to represent a nested
    // `BufferHandle<'_, T>`. Since trait objects (dyn Trait + '_) are covariant over their
    // inner lifetime(s) ('_), this compiles (and is usable in practice),
    // in spite of &'a mut T being invariant over T.
    parent: Option<&'a mut dyn BufferHandleImpl<T>>,
    buffers: &'a mut [Buffer<T>],
}

impl<'a, T> Default for BufferHandleLocal<'a, T> {
    #[inline]
    fn default() -> Self {
        Self::toplevel(&mut [])
    }
}

impl<'a, T> BufferHandleLocal<'a, T> {
    #[inline]
    pub fn toplevel(buffers: &'a mut [Buffer<T>]) -> Self {
        Self {
            parent: None,
            buffers,
        }
    }

    #[inline]
    pub fn with_indices(
        self,
        inputs: &'a [Option<BufferIndex>],
        outputs: &'a [Option<OutputBufferIndex>],
    ) -> BufferHandle<'a, T> {
        BufferHandle {
            node: self,
            inputs,
            outputs,
        }
    }

    #[inline]
    pub fn with_buffer_pos(self, start: usize, len: NonZeroUsize) -> BuffersLocal<'a, T> {
        BuffersLocal {
            start,
            len,
            node: self,
        }
    }

    #[inline]
    pub fn get_input(&mut self, buf_index: BufferIndex) -> Option<&[T]> {
        match buf_index {
            BufferIndex::MasterInput(i) => self.parent.as_mut().unwrap().get_input(i),
            BufferIndex::Output(buf) => self.get_output(buf).map(|buf| &*buf),
        }
    }

    #[inline]
    pub fn get_input_shared(&self, buf_index: BufferIndex) -> Option<&[ReadOnly<T>]> {
        match buf_index {
            BufferIndex::MasterInput(i) => self.parent.as_ref().unwrap().get_input_shared(i),
            BufferIndex::Output(buf) => self.get_output_shared(buf).map(ReadOnly::from_slice),
        }
    }

    #[inline]
    pub fn get_output(&mut self, buf_index: OutputBufferIndex) -> Option<&mut [T]> {
        match buf_index {
            OutputBufferIndex::Master(i) => self.parent.as_mut().unwrap().get_output(i),
            OutputBufferIndex::Local(i) => Some(Cell::get_mut(&mut self.buffers[i])),
        }
    }

    #[inline]
    pub fn get_output_shared(&self, buf_index: OutputBufferIndex) -> Option<&[Cell<T>]> {
        match buf_index {
            OutputBufferIndex::Master(i) => self.parent.as_ref().unwrap().get_output_shared(i),
            OutputBufferIndex::Local(i) => Some(self.buffers[i].as_slice_of_cells()),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub enum OutputBufferIndex {
    Master(usize),
    Local(usize),
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub enum BufferIndex {
    MasterInput(usize),
    Output(OutputBufferIndex),
}

pub trait BufferHandleImpl<T> {
    fn get_input(&mut self, index: usize) -> Option<&[T]>;

    fn get_input_shared(&self, index: usize) -> Option<&[ReadOnly<T>]>;

    fn get_output(&mut self, index: usize) -> Option<&mut [T]>;

    fn get_output_shared(&self, index: usize) -> Option<&[Cell<T>]>;
}

pub struct BufferHandle<'a, T> {
    node: BufferHandleLocal<'a, T>,
    inputs: &'a [Option<BufferIndex>],
    outputs: &'a [Option<OutputBufferIndex>],
}

impl<'a, T> Default for BufferHandle<'a, T> {
    #[inline]
    fn default() -> Self {
        Self {
            node: Default::default(),
            inputs: Default::default(),
            outputs: Default::default(),
        }
    }
}

impl<'a, T> BufferHandle<'a, T> {
    #[inline]
    pub fn append<'b>(&'b mut self, buffers: &'b mut [Buffer<T>]) -> BufferHandleLocal<'b, T> {
        BufferHandleLocal {
            parent: Some(self),
            buffers,
        }
    }

    #[inline]
    pub fn with_buffer_pos(self, start: usize, len: NonZeroUsize) -> Buffers<'a, T> {
        Buffers {
            node: self,
            start,
            len,
        }
    }
}

impl<'a, T> BufferHandleImpl<T> for BufferHandle<'a, T> {
    #[inline]
    fn get_input(&mut self, index: usize) -> Option<&[T]> {
        self.inputs.get(index).and_then(|maybe_index| {
            maybe_index.and_then(|buf_index| self.node.get_input(buf_index))
        })
    }

    #[inline]
    fn get_input_shared(&self, index: usize) -> Option<&[ReadOnly<T>]> {
        self.inputs.get(index).and_then(|maybe_buf_index| {
            maybe_buf_index.and_then(|buf_index| self.node.get_input_shared(buf_index))
        })
    }

    #[inline]
    fn get_output(&mut self, index: usize) -> Option<&mut [T]> {
        self.outputs.get(index).and_then(|maybe_index| {
            maybe_index.and_then(|buf_index| self.node.get_output(buf_index))
        })
    }

    #[inline]
    fn get_output_shared(&self, index: usize) -> Option<&[Cell<T>]> {
        self.outputs.get(index).and_then(|maybe_buf_index| {
            maybe_buf_index.and_then(|buf_index| self.node.get_output_shared(buf_index))
        })
    }
}

pub struct BuffersLocal<'a, T> {
    start: usize,
    len: NonZeroUsize,
    node: BufferHandleLocal<'a, T>,
}

impl<'a, T> BuffersLocal<'a, T> {
    #[inline]
    pub fn buffer_size(&self) -> NonZeroUsize {
        self.len
    }

    #[inline]
    pub fn with_indices(
        self,
        inputs: &'a [Option<BufferIndex>],
        outputs: &'a [Option<OutputBufferIndex>],
    ) -> Buffers<'a, T> {
        Buffers {
            start: self.start,
            len: self.len,
            node: self.node.with_indices(inputs, outputs),
        }
    }

    #[inline]
    pub fn get_input(&mut self, index: BufferIndex) -> Option<&[T]> {
        self.node
            .get_input(index)
            .map(|buf| &buf[self.start..][..self.len.get()])
    }

    #[inline]
    pub fn get_input_shared(&self, index: BufferIndex) -> Option<&[ReadOnly<T>]> {
        self.node
            .get_input_shared(index)
            .map(|buf| &buf[self.start..][..self.len.get()])
    }

    #[inline]
    pub fn get_output(&mut self, index: OutputBufferIndex) -> Option<&mut [T]> {
        self.node
            .get_output(index)
            .map(|buf| &mut buf[self.start..][..self.len.get()])
    }

    #[inline]
    pub fn get_output_shared(&self, index: OutputBufferIndex) -> Option<&[Cell<T>]> {
        self.node
            .get_output_shared(index)
            .map(|buf| &buf[self.start..][..self.len.get()])
    }
}

pub struct Buffers<'a, T> {
    start: usize,
    len: NonZeroUsize,
    node: BufferHandle<'a, T>,
}

impl<'a, T> Buffers<'a, T> {
    #[inline]
    pub fn buffer_size(&self) -> NonZeroUsize {
        self.len
    }

    #[inline]
    pub fn append<'b>(&'b mut self, buffers: &'b mut [Buffer<T>]) -> BuffersLocal<'b, T> {
        BuffersLocal {
            node: self.node.append(buffers),
            start: self.start,
            len: self.len,
        }
    }

    #[inline]
    pub fn get_input(&mut self, index: usize) -> Option<&[T]> {
        self.node
            .get_input(index)
            .map(|buf| &buf[self.start..][..self.len.get()])
    }

    #[inline]
    pub fn get_input_shared(&self, index: usize) -> Option<&[ReadOnly<T>]> {
        self.node
            .get_input_shared(index)
            .map(|buf| &buf[self.start..][..self.len.get()])
    }

    #[inline]
    pub fn get_output(&mut self, index: usize) -> Option<&mut [T]> {
        self.node
            .get_output(index)
            .map(|buf| &mut buf[self.start..][..self.len.get()])
    }

    #[inline]
    pub fn get_output_shared(&self, index: usize) -> Option<&[Cell<T>]> {
        self.node
            .get_output_shared(index)
            .map(|buf| &buf[self.start..][..self.len.get()])
    }
}
