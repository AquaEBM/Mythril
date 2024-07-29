use core::{cell::Cell, mem, num::NonZeroUsize};

use simd_util::{
    simd::{num::SimdFloat, Simd, SimdElement},
    split_stereo_cell, FLOATS_PER_VECTOR, STEREO_VOICES_PER_VECTOR,
};

/// A _Read-Only_ wrapper around a `Cell<T>` that only allows for reading the contained value
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

pub struct BufferHandle<T: SimdFloat> {
    pub buffer: Buffer<T>,
    pub state: Cell<T::Bits>,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub enum OutBufIndex {
    Super(usize),
    Local(usize),
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub enum BufferIndex {
    SuperInput(usize),
    Output(OutBufIndex),
}

// TODO: Name bikeshedding
// TODO: implement some kind of safeguard against buffers of different lengths in the same handle
// TODO: squash some checks

// The following structs describe a linked list-like interface in order to allow
// audio graph nodes (and potentially other audio graphs nested in them) to (re)use buffers
// from their callers as master/global inputs/outputs
//
// the tricks described in this discussion are used:
// https://users.rust-lang.org/t/safe-interface-for-a-singly-linked-list-of-mutable-references/107401

pub struct Buffers<'a, T: SimdFloat> {
    buffers: &'a mut [BufferHandle<T>],
    // the trick here is the usage of a trait object to represent a nested `BufferIO<'_, T>`.
    parent: Option<&'a mut dyn BufferIOImpl<Sample = T>>,
}

impl<'a, T: SimdFloat> Default for Buffers<'a, T> {
    #[inline]
    fn default() -> Self {
        Self::toplevel(&mut []).unwrap()
    }
}

impl<'a, T: SimdFloat> Buffers<'a, T> {
    #[inline]
    fn new<'b>(
        parent: Option<&'a mut BufferIO<'b, T>>,
        buffers: &'a mut [BufferHandle<T>],
    ) -> Option<Self> {
        let mut bufs = buffers
            .iter()
            .map(|handle| handle.buffer.as_slice_of_cells().len());

        bufs.next()
            .map_or(true, |a| bufs.all(|x| x == a))
            .then_some(Self {
                parent: parent.map(|io| io as _),
                buffers,
            })
    }

    #[inline]
    pub fn toplevel(buffers: &'a mut [BufferHandle<T>]) -> Option<Self> {
        Self::new(None, buffers)
    }

    #[inline]
    pub const fn with_io(
        self,
        inputs: &'a [Option<BufferIndex>],
        outputs: &'a [Option<OutBufIndex>],
    ) -> BufferIO<'a, T> {
        BufferIO {
            node: self,
            inputs,
            outputs,
        }
    }

    #[inline]
    pub const fn slice(self, start: usize, len: NonZeroUsize) -> BuffersSliced<'a, T> {
        BuffersSliced {
            start,
            len,
            node: self,
        }
    }

    #[inline]
    pub fn get_handle_mut(&mut self, i: BufferIndex) -> Option<&mut BufferHandle<T>> {
        match i {
            BufferIndex::SuperInput(i) => {
                self.parent.as_mut().and_then(|p| p.input_buf_mut(i).ok())
            }
            BufferIndex::Output(i) => match i {
                OutBufIndex::Super(i) => {
                    self.parent.as_mut().and_then(|p| p.output_buf_mut(i).ok())
                }
                OutBufIndex::Local(i) => self.buffers.get_mut(i),
            },
        }
    }

    #[inline]
    pub fn get_handle(&self, i: BufferIndex) -> Option<&BufferHandle<T>> {
        match i {
            BufferIndex::SuperInput(i) => self.parent.as_ref().and_then(|p| p.input_buf(i).ok()),
            BufferIndex::Output(i) => match i {
                OutBufIndex::Super(i) => self.parent.as_deref().and_then(|p| p.output_buf(i).ok()),
                OutBufIndex::Local(i) => self.buffers.get(i),
            },
        }
    }
}
pub enum PortError {
    OOB,
    Empty,
}

enum BufferNodeIndexError {
    Current(PortError),
    Previous,
}

impl From<PortError> for BufferNodeIndexError {
    fn from(value: PortError) -> Self {
        BufferNodeIndexError::Current(value)
    }
}

impl TryFrom<BufferNodeIndexError> for PortError {
    type Error = ();

    fn try_from(value: BufferNodeIndexError) -> Result<Self, Self::Error> {
        if let BufferNodeIndexError::Current(e) = value {
            Ok(e)
        } else {
            Err(())
        }
    }
}

trait BufferIOImpl {
    type Sample: SimdFloat;

    fn input_buf_mut(
        &mut self,
        i: usize,
    ) -> Result<&mut BufferHandle<Self::Sample>, BufferNodeIndexError>;

    fn output_buf_mut(
        &mut self,
        i: usize,
    ) -> Result<&mut BufferHandle<Self::Sample>, BufferNodeIndexError>;

    fn input_buf(&self, i: usize) -> Result<&BufferHandle<Self::Sample>, BufferNodeIndexError>;

    fn output_buf(&self, i: usize) -> Result<&BufferHandle<Self::Sample>, BufferNodeIndexError>;
}

pub struct BufferIO<'a, T: SimdFloat> {
    node: Buffers<'a, T>,
    inputs: &'a [Option<BufferIndex>],
    outputs: &'a [Option<OutBufIndex>],
}

impl<'a, T: SimdFloat> Default for BufferIO<'a, T> {
    #[inline]
    fn default() -> Self {
        Self {
            node: Default::default(),
            inputs: &[],
            outputs: &[],
        }
    }
}

impl<T: SimdFloat> BufferIOImpl for BufferIO<'_, T> {
    type Sample = T;

    #[inline]
    fn input_buf_mut(&mut self, i: usize) -> Result<&mut BufferHandle<T>, BufferNodeIndexError> {
        let port_index = self.inputs.get(i).ok_or(PortError::OOB)?;
        let buf_index = port_index.ok_or(PortError::Empty)?;
        self.node
            .get_handle_mut(buf_index)
            .ok_or(BufferNodeIndexError::Previous)
    }

    #[inline]
    fn output_buf_mut(&mut self, i: usize) -> Result<&mut BufferHandle<T>, BufferNodeIndexError> {
        let port_index = self.outputs.get(i).ok_or(PortError::OOB)?;
        let buf_index = port_index.ok_or(PortError::Empty)?;
        self.node
            .get_handle_mut(BufferIndex::Output(buf_index))
            .ok_or(BufferNodeIndexError::Previous)
    }

    #[inline]
    fn input_buf(&self, i: usize) -> Result<&BufferHandle<T>, BufferNodeIndexError> {
        let port_index = self.inputs.get(i).ok_or(PortError::OOB)?;
        let buf_index = port_index.ok_or(PortError::Empty)?;
        self.node
            .get_handle(buf_index)
            .ok_or(BufferNodeIndexError::Previous)
    }

    #[inline]
    fn output_buf(&self, i: usize) -> Result<&BufferHandle<T>, BufferNodeIndexError> {
        let port_index = self.outputs.get(i).ok_or(PortError::OOB)?;
        let buf_index = port_index.ok_or(PortError::Empty)?;
        self.node
            .get_handle(BufferIndex::Output(buf_index))
            .ok_or(BufferNodeIndexError::Previous)
    }
}

impl<'a, T: SimdFloat> BufferIO<'a, T> {
    const ERR_MSG: &'static str = "Internal Error: Buffer index out of bounds";
    const MAP_ERR: fn(BufferNodeIndexError) -> PortError = |e| e.try_into().expect(Self::ERR_MSG);

    #[inline]
    pub fn append<'b>(&'b mut self, buffers: &'b mut [BufferHandle<T>]) -> Option<Buffers<'b, T>> {
        Buffers::new(Some(self), buffers)
    }

    #[inline]
    pub fn slice(self, start: usize, len: NonZeroUsize) -> BufferIOSliced<'a, T> {
        BufferIOSliced {
            io: self,
            start,
            len,
        }
    }

    #[inline]
    pub fn num_inputs(&self) -> usize {
        self.inputs.len()
    }

    #[inline]
    pub fn num_outputs(&self) -> usize {
        self.outputs.len()
    }

    #[inline]
    pub fn input(&mut self, i: usize) -> Result<(&[T], &T::Bits), PortError> {
        self.input_buf_mut(i)
            .map(|BufferHandle { buffer, state }| (&*buffer.get_mut(), &*state.get_mut()))
            .map_err(Self::MAP_ERR)
    }

    #[inline]
    pub fn input_shared(
        &self,
        i: usize,
    ) -> Result<(&[ReadOnly<T>], &ReadOnly<T::Bits>), PortError> {
        self.input_buf(i)
            .map(|BufferHandle { buffer, state }| {
                (
                    ReadOnly::from_slice(buffer.as_slice_of_cells()),
                    ReadOnly::from_cell_ref(state),
                )
            })
            .map_err(Self::MAP_ERR)
    }

    #[inline]
    pub fn output(&mut self, i: usize) -> Result<&mut [T], PortError> {
        self.output_buf_mut(i)
            .map(|handle| handle.buffer.get_mut())
            .map_err(Self::MAP_ERR)
    }

    #[inline]
    pub fn output_shared(&self, i: usize) -> Result<&[Cell<T>], PortError> {
        self.output_buf(i)
            .map(|handle| handle.buffer.as_slice_of_cells())
            .map_err(Self::MAP_ERR)
    }
}

pub struct BuffersSliced<'a, T: SimdFloat> {
    len: NonZeroUsize,
    start: usize,
    node: Buffers<'a, T>,
}

impl<'a, T: SimdFloat> BuffersSliced<'a, T> {
    #[inline]
    pub fn len(&self) -> NonZeroUsize {
        self.len
    }

    #[inline]
    pub fn with_io(
        self,
        inputs: &'a [Option<BufferIndex>],
        outputs: &'a [Option<OutBufIndex>],
    ) -> BufferIOSliced<'a, T> {
        BufferIOSliced {
            start: self.start,
            len: self.len,
            io: self.node.with_io(inputs, outputs),
        }
    }

    #[inline]
    pub fn get_handle_mut(&mut self, index: BufferIndex) -> Option<(&mut [T], &mut T::Bits)> {
        self.node
            .get_handle_mut(index)
            .map(|BufferHandle { buffer, state }| {
                (
                    &mut buffer.get_mut()[self.start..][..self.len.get()],
                    state.get_mut(),
                )
            })
    }

    #[inline]
    pub fn get_handle(&self, index: BufferIndex) -> Option<(&[Cell<T>], &Cell<T::Bits>)> {
        self.node
            .get_handle(index)
            .map(|BufferHandle { buffer, state }| {
                (
                    &buffer.as_slice_of_cells()[self.start..][..self.len.get()],
                    state,
                )
            })
    }
}

pub struct BufferIOSliced<'a, T: SimdFloat> {
    start: usize,
    len: NonZeroUsize,
    io: BufferIO<'a, T>,
}

impl<'a, T: SimdFloat> BufferIOSliced<'a, T> {
    #[inline]
    pub fn len(&self) -> NonZeroUsize {
        self.len
    }

    #[inline]
    pub fn num_inputs(&self) -> usize {
        self.io.num_inputs()
    }

    #[inline]
    pub fn num_outputs(&self) -> usize {
        self.io.num_outputs()
    }

    #[inline]
    pub fn append<'b>(
        &'b mut self,
        buffers: &'b mut [BufferHandle<T>],
    ) -> Option<BuffersSliced<'b, T>> {
        self.io.append(buffers).map(|node| BuffersSliced {
            node,
            start: self.start,
            len: self.len,
        })
    }

    #[inline]
    pub fn input(&mut self, index: usize) -> Result<(&[T], &T::Bits), PortError> {
        self.io
            .input(index)
            .map(|(buf, state)| (&buf[self.start..][..self.len.get()], state))
    }

    #[inline]
    pub fn input_shared(
        &self,
        index: usize,
    ) -> Result<(&[ReadOnly<T>], &ReadOnly<T::Bits>), PortError> {
        self.io
            .input_shared(index)
            .map(|(buf, state)| (&buf[self.start..][..self.len.get()], state))
    }

    #[inline]
    pub fn output(&mut self, index: usize) -> Result<&mut [T], PortError> {
        self.io
            .output(index)
            .map(|buf| &mut buf[self.start..][..self.len.get()])
    }

    #[inline]
    pub fn output_shared(&self, index: usize) -> Result<&[Cell<T>], PortError> {
        self.io
            .output_shared(index)
            .map(|buf| &buf[self.start..][..self.len.get()])
    }
}
