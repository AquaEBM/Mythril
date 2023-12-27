pub mod wavetable;
pub mod wt_osc;

use plugin_util::{
    simd_util::*,
    math::*,
    simd::{SimdElement, Simd, simd_swizzle, f32x2, SimdOrd, SimdFloat, StdFloat, SimdUint},
    smoothing::*,
};
use rtrb::{RingBuffer, Consumer, Producer};
use core::iter;
use std::mem::transmute;
use cfg_if::cfg_if;
use super::*;

use self::wt_osc::voice::NUM_DETUNED_OSCILLATORS;

use super::params;

pub const STEREO_VOICES_PER_VECTOR: usize = FLOATS_PER_VECTOR / 2;

// Safety argument for the following two functions:
//  - both referred to types have the same size, more specifically, 2 * STEREO_VOICES_PER_VECTOR
// is always equal to FLOATS_PER_VECTOR, because it is always a multiple (in fact, a power) of 2
//  - the type of `vector` has greater alignment that of the return type
//  - the output reference's lifetime is the same as that of the input, so no unbounded lifetimes
//  - we are transmuting a vector to an array over the same scalar, so values are valid

#[inline]
pub fn as_stereo_sample_array<'a, T: SimdElement>(
    vector: &'a Simd<T, FLOATS_PER_VECTOR>
) -> &'a [Simd<T, 2> ; STEREO_VOICES_PER_VECTOR] {

    unsafe { transmute(vector) }
}

#[inline]
pub fn as_mut_stereo_sample_array<'a, T: SimdElement>(
    vector: &'a mut Simd<T, FLOATS_PER_VECTOR>
) -> &mut [Simd<T, 2> ; STEREO_VOICES_PER_VECTOR] {

    unsafe { transmute(vector) }
}

#[inline]
pub fn splat_stereo<T: SimdElement>(pair: Simd<T, 2>) -> Simd<T, FLOATS_PER_VECTOR> {

    const ZERO_ONE: [usize ; FLOATS_PER_VECTOR] = {
        let mut array = [0 ; FLOATS_PER_VECTOR];
        let mut i = 1;
        while i < FLOATS_PER_VECTOR {
            array[i] = 1;
            i += 2;
        }
        array
    };

    simd_swizzle!(pair, ZERO_ONE)
}

#[inline]
pub fn sum_to_stereo_sample(x: Float) -> f32x2 {

    unsafe { cfg_if! {

        if #[cfg(any(target_feature = "avx512f"))] {

            // FLOATS_PER_VECTOR = 16
            let [left1, right1]: [Simd<f32, { FLOATS_PER_VECTOR / 2 }> ; 2] = transmute(x);
            let [left2, right2]: [Simd<f32, { FLOATS_PER_VECTOR / 4 }> ; 2] = transmute(left1 + right1);
            let [left3, right3]: [Simd<f32, { FLOATS_PER_VECTOR / 8 }> ; 2] = transmute(left2 + right2);

            left3 + right3

        } else if #[cfg(any(target_feature = "avx"))] {

            // FLOATS_PER_VECTOR = 8
            let [left1, right1]: [Simd<f32, { FLOATS_PER_VECTOR / 2 }> ; 2] = transmute(x);
            let [left2, right2]: [Simd<f32, { FLOATS_PER_VECTOR / 4 }> ; 2] = transmute(left1 + right1);
            left2 + right2
            
        } else if #[cfg(any(target_feature = "sse", target_feature = "neon"))] {

            // FLOATS_PER_VECTOR = 4
            let [left, right]: [Simd<f32, { FLOATS_PER_VECTOR / 2 }> ; 2] = transmute(x);
            left + right

        } else {

            // FLOATS_PER_VECTOR = 2
            x
        }
    } }
}

/// return a vector where values on the left channel
/// are on the right ones and vice-versa
#[inline]
pub fn swap_stereo(v: Float) -> Float {
    const FLIP_PAIRS: [usize ; FLOATS_PER_VECTOR] = {

        let mut array = [0 ; FLOATS_PER_VECTOR];

        let mut i = 0;
        while i < FLOATS_PER_VECTOR {

            array[i] = i ^ 1;
            i += 1;
        }
        array
    };

    simd_swizzle!(v, FLIP_PAIRS)
}

#[inline]
pub fn semitones_to_ratio(semitones: Float) -> Float {
    const RATIO: Float = const_splat(1. / 12.);
    exp2(semitones * RATIO)
}

/// triangluar panning of a vector of stereo samples, 0 < pan <= 1
#[inline]
pub fn triangular_pan_weights(pan: Float) -> Float {

    const SIGN_MASK: Float = {
        let mut array = [0. ; FLOATS_PER_VECTOR];
        let mut i = 0;
        while i < FLOATS_PER_VECTOR {
            array[i] = -0.;
            i += 2;
        }
        Simd::from_array(array)
    };

    const ALT_ONE: Float = {
        let mut array = [0. ; FLOATS_PER_VECTOR];
        let mut i = 0;
        while i < FLOATS_PER_VECTOR {
            array[i] = 1.;
            i += 2;
        }
        Simd::from_array(array)
    };

    Float::from_bits(pan.to_bits() ^ SIGN_MASK.to_bits()) + ALT_ONE
}

#[inline]
fn splat_slot<T: SimdElement>(
    vector: &Simd<T, FLOATS_PER_VECTOR>,
    index: usize
) -> Simd<T, FLOATS_PER_VECTOR> {

    let array = as_stereo_sample_array(vector);

    let slot = array[index];

    splat_stereo(slot)
}

#[derive(Default)]
pub struct CenterDetuned<T> {
    pub center: T,
    pub detuned: [T ; NUM_DETUNED_OSCILLATORS]
}

impl<T> CenterDetuned<T> {

    #[inline]
    pub fn all(&self) -> impl Iterator<Item = &T> {
        iter::once(
            &self.center
        ).chain(&self.detuned)
    }

    #[inline]
    pub fn all_mut(&mut self) -> impl Iterator<Item = &mut T> {
        iter::once(
            &mut self.center
        ).chain(&mut self.detuned)
    }

    #[inline]
    pub fn all_range(&self, max: usize) -> impl Iterator<Item = &T> {
        self.detuned[..max].iter()
    }

    #[inline]
    pub fn all_mut_range(&mut self, max: usize) -> impl Iterator<Item = &mut T> {
        self.detuned[..max].iter_mut()
    }

    #[inline]
    pub unsafe fn all_range_unchecked(&self, max: usize) -> impl Iterator<Item = &T> {
        self.detuned.get_unchecked(..max).iter()
    }

    #[inline]
    pub unsafe fn all_range_mut_unchecked(&mut self, max: usize) -> impl Iterator<Item = &mut T> {
        self.detuned.get_unchecked_mut(..max).iter_mut()
    }
}

pub struct SharedLender<T: ?Sized, const BUFFER_SIZE: usize = 128> {
    ring_buffers: Vec<Producer<Arc<T>>>,
    drop_queue: Vec<Arc<T>>,
}

impl<T: ?Sized> Default for SharedLender<T> {
    fn default() -> Self {
        Self {
            ring_buffers: Vec::new(),
            drop_queue: Vec::new()
        }
    }
}

impl<T: ?Sized, const BUFFER_SIZE: usize> SharedLender<T, BUFFER_SIZE> {

    pub fn send(&mut self, item: Arc<T>) {

        for producer in &mut self.ring_buffers {
            producer.push(item.clone()).unwrap();
        }

        self.drop_queue.push(item);
    }

    pub fn update_drop_queue(&mut self) {
        self.drop_queue.retain(|item| Arc::strong_count(item) != 1);
        self.ring_buffers.retain(|producer| !producer.is_abandoned());
    }

    pub fn create_new_reciever(&mut self) -> LenderReciever<T> {

        let (producer, reciever) = RingBuffer::new(BUFFER_SIZE);
        self.ring_buffers.push(producer);

        LenderReciever {
            ring_buffer: reciever
        }
    }
}

pub struct LenderReciever<T: ?Sized> {
    ring_buffer: Consumer<Arc<T>>,
}

impl<T: ?Sized> LenderReciever<T> {

    pub fn recv_next(&mut self) -> Option<Arc<T>> {
        self.ring_buffer.pop().ok()
    }

    pub fn recv_latest(&mut self) -> Option<Arc<T>> {
        let mut output = None;
        while let Some(item) = self.recv_next() {
            output = Some(item);
        }

        output
    }
}