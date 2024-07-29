use super::{buffer::BufferIOSliced, simd_util::simd::num::SimdFloat};

use alloc::sync::Arc;
use std::io::{Read, Write};

pub trait Parameters {
    fn serialize(&self, writer: &mut dyn Write);
    fn deserialize(&self, reader: &mut dyn Read);
}

impl Parameters for () {
    #[inline]
    fn serialize(&self, _writer: &mut dyn Write) {}
    #[inline]
    fn deserialize(&self, _reader: &mut dyn Read) {}
}

pub trait Processor {
    type Sample: SimdFloat;

    /// Process the provided buffers, and report the current state of the output ports.
    ///
    /// # Processing buffers
    ///
    /// All buffers have the same length: [`buffers.len( )`](BufferIOSliced::len).
    /// The values in output buffers should be considered garbage and must be overwritten.
    /// if output is silent, you must fill the buffer with `0.0` values.
    ///
    /// ## Buffer Aliasing
    ///
    /// - Input buffers may alias with eachother.
    /// - An output buffer may alias with any number of input buffers.
    /// - Output buffers are always distinct.
    ///
    /// # Output port state.
    ///
    /// When accessing the buffers, a reference to an integer value is also provided. For an
    /// input port, this is the state of the output port that sends data to it. For an output
    /// port this represents the current state of this processor's corresponding output port.
    /// The initial value should be considered as garbage and must be overwritten.
    ///
    /// The value at each lane indicates the processing state of the corresponding voice:
    ///
    /// - `MAX` (all bits set to 1) means that it must be kept alive, the processor has not
    ///   finished processing and requires another `process` call.
    ///
    /// - `n` < [`buffers.len( )`](BufferIOSliced::len), means that the processor
    ///   has finished processing at the `n`th sample, this allows the caller to assume that
    ///   all subsequent samples will be silent.
    ///
    /// - Any other value means that the processor doesn't necessarily require another processing
    ///   cycle
    fn process(&mut self, buffers: BufferIOSliced<Self::Sample>, cluster_idx: usize);

    fn audio_io_layout(&self) -> (usize, usize);

    fn parameters(&self) -> Arc<dyn Parameters>;

    fn initialize(&mut self, sr: f32, max_buffer_size: usize, max_num_clusters: usize) -> usize;

    // TODO: use vectors & masks in the followng three methods

    fn set_voice_note(&mut self, index: (usize, usize), velocity: f32, note: u8);

    fn deactivate_voice(&mut self, index: (usize, usize), velocity: f32);

    fn reset(&mut self, index: (usize, usize));

    fn move_state(&mut self, from: (usize, usize), to: (usize, usize));
}

impl<T: ?Sized + Processor> Processor for Box<T> {
    type Sample = T::Sample;

    #[inline]
    fn process(&mut self, buffers: BufferIOSliced<Self::Sample>, cluster_idx: usize) {
        self.as_mut().process(buffers, cluster_idx)
    }

    #[inline]
    fn audio_io_layout(&self) -> (usize, usize) {
        self.as_ref().audio_io_layout()
    }

    #[inline]
    fn initialize(&mut self, sr: f32, max_buffer_size: usize, max_num_clusters: usize) -> usize {
        self.as_mut()
            .initialize(sr, max_buffer_size, max_num_clusters)
    }

    #[inline]
    fn parameters(&self) -> Arc<dyn Parameters> {
        self.as_ref().parameters()
    }

    #[inline]
    fn set_voice_note(&mut self, index: (usize, usize), velocity: f32, note: u8) {
        self.as_mut().set_voice_note(index, velocity, note);
    }

    #[inline]
    fn deactivate_voice(&mut self, index: (usize, usize), velocity: f32) {
        self.as_mut().deactivate_voice(index, velocity)
    }

    #[inline]
    fn reset(&mut self, index: (usize, usize)) {
        self.as_mut().reset(index);
    }

    #[inline]
    fn move_state(&mut self, from: (usize, usize), to: (usize, usize)) {
        self.as_mut().move_state(from, to);
    }
}
