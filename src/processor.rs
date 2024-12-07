use super::*;
use buffer::Buffers;
use simd_util::simd::num::SimdFloat;
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

    /// Process the provided buffers, and return the current processing state.
    ///
    /// # Processing buffers
    ///
    /// All buffers have the same length: [`buffers.len() `](Buffers::len).
    /// The values in output buffers should be considered garbage and must be overwritten.
    /// if output is silent, you must fill the buffer with `0.0` values.
    ///
    /// ## Buffer Aliasing
    ///
    /// - Input buffers may alias with eachother.
    /// - An output buffer may alias with any number of input buffers.
    /// - Output buffers are always distinct from eachother.
    ///
    /// # Return value
    ///
    /// The value at each lane indicates the processing state of the corresponding voice:
    ///
    /// - `MAX` (all bits set to 1) means that it must be kept alive, it has not
    ///   finished and requires another `process` call.
    ///
    /// - A value `n` < [`buffers.len() `](Buffers::len)means that it
    ///   has finished processing at the `n`th sample. Note that this allows the caller to
    ///   assume that all subsequent samples will be silent.
    ///
    /// - Any other value means that it doesn't necessarily require another processing
    ///   cycle, but will continue to produce coherent data when `process` is called, this is
    ///   useful for generators without envelopes, such as oscillators, LFOs, samplers etc.,
    ///   where whether the voice should be kept alive depends mainly on the nodes recieving
    ///   said generators' outputs.
    ///
    /// When accessing input buffers, a reference to the state
    /// of the node that has last written data to it is also provided.
    ///
    /// If different output ports have different states, the maximum (lane-wise) value should
    /// be returned, to avoid accidentally informing the caller that a voice has finished
    /// processing when it hasn't.
    fn process(&mut self, buffers: Buffers<Self::Sample>, cluster_idx: usize);

    fn parameters(&self) -> Arc<dyn Parameters>;

    fn initialize(&mut self, sr: f32, max_buffer_size: usize, max_num_clusters: usize) -> usize;

    fn set_voice_note(&mut self, index: (usize, usize), velocity: f32, note: u8);

    fn deactivate_voice(&mut self, index: (usize, usize), velocity: f32);

    fn reset(&mut self, index: (usize, usize));

    fn move_state(&mut self, from: (usize, usize), to: (usize, usize));
}

impl<T: ?Sized + Processor> Processor for Box<T> {
    type Sample = T::Sample;

    #[inline]
    fn process(&mut self, buffers: Buffers<Self::Sample>, cluster_idx: usize) {
        self.as_mut().process(buffers, cluster_idx)
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
