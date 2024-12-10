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

    fn process(
        &mut self,
        buffers: Buffers<Self::Sample>,
        cluster_idx: usize,
    ) -> <Self::Sample as SimdFloat>::Mask;

    fn parameters(&self) -> Arc<dyn Parameters>;

    fn initialize(&mut self, sr: f32, max_buffer_size: usize, max_num_clusters: usize) -> usize;

    fn reset(&mut self, index: (usize, usize));
}

impl<T: ?Sized + Processor> Processor for Box<T> {
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
    fn initialize(&mut self, sr: f32, max_buffer_size: usize, max_num_clusters: usize) -> usize {
        self.as_mut()
            .initialize(sr, max_buffer_size, max_num_clusters)
    }

    #[inline]
    fn parameters(&self) -> Arc<dyn Parameters> {
        self.as_ref().parameters()
    }

    #[inline]
    fn reset(&mut self, index: (usize, usize)) {
        self.as_mut().reset(index);
    }
}
