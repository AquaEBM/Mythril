use super::{buffer::Buffers, simd_util::simd::num::SimdFloat};

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

pub trait AGProcessor {
    type Sample: SimdFloat;

    fn process(
        &mut self,
        buffers: Buffers<Self::Sample>,
        cluster_idx: usize,
    ) -> <Self::Sample as SimdFloat>::Mask;

    fn audio_io_layout(&self) -> (usize, usize);

    fn parameters(&self) -> Arc<dyn Parameters>;

    fn initialize(&mut self, sr: f32, max_buffer_size: usize, max_num_clusters: usize) -> usize;

    // TODO: use vectors & masks in the followng three metods

    fn activate_voice(
        &mut self,
        index: (usize, usize),
        velocity: <Self::Sample as SimdFloat>::Scalar,
        note: u8,
    );

    fn deactivate_voice(
        &mut self,
        index: (usize, usize),
        velocity: <Self::Sample as SimdFloat>::Scalar,
    );

    fn reset(&mut self, index: (usize, usize));

    fn move_state(&mut self, from: (usize, usize), to: (usize, usize));
}

impl<T: ?Sized + AGProcessor> AGProcessor for Box<T> {
    type Sample = T::Sample;

    #[inline]
    fn process(
        &mut self,
        buffers: Buffers<Self::Sample>,
        cluster_idx: usize,
    ) -> <Self::Sample as SimdFloat>::Mask {
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
    fn activate_voice(
        &mut self,
        index: (usize, usize),
        velocity: <Self::Sample as SimdFloat>::Scalar,
        note: u8,
    ) {
        self.as_mut().activate_voice(index, velocity, note);
    }

    #[inline]
    fn deactivate_voice(
        &mut self,
        index: (usize, usize),
        velocity: <Self::Sample as SimdFloat>::Scalar,
    ) {
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
