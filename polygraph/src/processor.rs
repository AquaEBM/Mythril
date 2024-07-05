use super::{
    simd_util::simd::num::SimdFloat,
    buffer::Buffers,
};

use alloc::sync::Arc;
use core::num::NonZeroUsize;
use std::io::{Read, Write};

pub struct ParameterMut<'a, T: SimdFloat> {
    value: &'a mut T,
    mod_state: &'a mut T::Mask,
}

impl<'a, T: SimdFloat> ParameterMut<'a, T> {
    #[inline]
    pub fn new(value: &'a mut T, mod_state: &'a mut T::Mask) -> Self {
        Self {
            value,
            mod_state,
        }
    }

    #[inline]
    pub fn value(&self) -> &T {
        self.value
    }

    #[inline]
    pub fn set_value(&mut self, val: T) {
        *self.value = val;
    }

    #[inline]
    pub fn mod_state_mut(&mut self) -> &mut T::Mask {
        self.mod_state
    }

    #[inline]
    pub fn mod_state(&self) -> &T::Mask {
        &self.mod_state
    }
}

pub struct Parameter<'a, T: SimdFloat> {
    value: &'a T,
    mod_state: &'a T::Mask,
}

impl<'a, T: SimdFloat> Parameter<'a, T> {
    #[inline]
    pub fn new(value: &'a T, mod_state: &'a <T as SimdFloat>::Mask) -> Self {
        Self {
            value,
            mod_state,
        }
    }

    #[inline]
    pub fn value(&self) -> &T {
        self.value
    }

    #[inline]
    pub fn mod_state(&self) -> &T::Mask {
        self.mod_state
    }
}

pub trait Parameters<T: SimdFloat> {
    fn get(&self, id: u64) -> Option<Parameter<T>>;
    fn get_mut(&mut self, id: u64) -> Option<ParameterMut<T>>;
}

impl<T: SimdFloat> Parameters<T> for () {
    #[inline]
    fn get(&self, _id: u64) -> Option<Parameter<T>> { None }
    #[inline]
    fn get_mut(&mut self, _id: u64) -> Option<ParameterMut<T>> { None }
}

pub trait PersistentState {
    fn ser(&self, writer: &mut dyn Write);
    fn de(&self, reader: &mut dyn Read);
}

impl PersistentState for () {
    #[inline]
    fn ser(&self, _writer: &mut dyn Write) {}
    #[inline]
    fn de(&self, _reader: &mut dyn Read) {}
}

pub trait AGModule {
    type Sample: SimdFloat;

    fn persistent_state_handle(&self) -> Arc<dyn PersistentState>;

    fn initialize(&mut self, sr: f32, max_buffer_size: usize, max_num_clusters: usize) -> usize;

    fn set_voice_notes(
        &mut self,
        cluster_idx: usize,
        voice_mask: <Self::Sample as SimdFloat>::Mask,
        velocity: Self::Sample,
        note: <Self::Sample as SimdFloat>::Bits,
    );

    fn deactivate_voices(
        &mut self,
        cluster_idx: usize,
        voice_mask: <Self::Sample as SimdFloat>::Mask,
        velocity: Self::Sample,
    );

    fn reset(
        &mut self,
        cluster_idx: usize,
        voice_mask: <Self::Sample as SimdFloat>::Mask,
        params: &dyn Parameters<Self::Sample>,
    );

    fn move_state(&mut self, from: (usize, usize), to: (usize, usize));
}

impl<T: ?Sized + AGModule> AGModule for Box<T> {
    type Sample = T::Sample;

    #[inline]
    fn initialize(&mut self, sr: f32, max_buffer_size: usize, max_num_clusters: usize) -> usize {
        self.as_mut()
            .initialize(sr, max_buffer_size, max_num_clusters)
    }

    #[inline]
    fn persistent_state_handle(&self) -> Arc<dyn PersistentState> {
        self.as_ref().persistent_state_handle()
    }

    #[inline]
    fn reset(
        &mut self,
        cluster_idx: usize,
        voice_mask: <Self::Sample as SimdFloat>::Mask,
        params: &dyn Parameters<Self::Sample>,
    ) {
        self.as_mut().reset(cluster_idx, voice_mask, params);
    }

    #[inline]
    fn move_state(&mut self, from: (usize, usize), to: (usize, usize)) {
        self.as_mut().move_state(from, to);
    }

    #[inline]
    fn set_voice_notes(
        &mut self,
        cluster_idx: usize,
        voice_mask: <Self::Sample as SimdFloat>::Mask,
        velocity: Self::Sample,
        note: <Self::Sample as SimdFloat>::Bits,
    ) {
        self.as_mut()
            .set_voice_notes(cluster_idx, voice_mask, velocity, note);
    }

    #[inline]
    fn deactivate_voices(
        &mut self,
        cluster_idx: usize,
        voice_mask: <Self::Sample as SimdFloat>::Mask,
        velocity: Self::Sample,
    ) {
        self.as_mut()
            .deactivate_voices(cluster_idx, voice_mask, velocity)
    }
}

pub trait AGProcessor: AGModule {
    fn process(
        &mut self,
        buffers: Buffers<Self::Sample>,
        cluster_idx: usize,
        params: &dyn Parameters<Self::Sample>,
    ) -> <Self::Sample as SimdFloat>::Mask;

    fn audio_io_layout(&self) -> (usize, usize);
}

impl<T: AGProcessor + ?Sized> AGProcessor for Box<T> {
    #[inline]
    fn process(
        &mut self,
        buffers: Buffers<Self::Sample>,
        cluster_idx: usize,
        params: &dyn Parameters<Self::Sample>,
    ) -> <Self::Sample as SimdFloat>::Mask {
        self.as_mut().process(buffers, cluster_idx, params)
    }

    #[inline]
    fn audio_io_layout(&self) -> (usize, usize) {
        self.as_ref().audio_io_layout()
    }
}

pub trait AGModSource: AGModule {
    fn advance(
        &mut self,
        cluster_idx: usize,
        num_samples: NonZeroUsize,
        params: &dyn Parameters<Self::Sample>,
    ) -> (Self::Sample, <Self::Sample as SimdFloat>::Mask);
}
