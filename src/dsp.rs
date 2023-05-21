pub mod wavetable;
pub mod wt_osc;
pub mod utils;

use std::{array, mem::transmute, sync::Arc};
#[allow(unused_imports)]
use std::arch::x86_64::*;
use core_simd::simd::*;
use std_float::*;
use cfg_if::cfg_if;

use super::params;

const MAX_VECTOR_WIDTH: usize = {
    if cfg!(any(target_feature = "avx512f")) {
        16
    } else if cfg!(any(target_feature = "avx")) {
        8
    } else if cfg!(any(target_feature = "sse")) {
        4
    } else {
        2
    }
};

const VOICES_PER_VECTOR: usize = MAX_VECTOR_WIDTH / 2;

pub const MAX_POLYPHONY: usize = 128;
pub const NUM_VECTORS: usize = enclosing_div(MAX_POLYPHONY, VOICES_PER_VECTOR);

type Float = Simd<f32, MAX_VECTOR_WIDTH>;
type UInt = Simd<u32, MAX_VECTOR_WIDTH>;
type Int = Simd<i32, MAX_VECTOR_WIDTH>;

type TMask = Mask<i32, MAX_VECTOR_WIDTH>;

const ZERO_F: Float = const_splat(0.);
const ONE_F: Float = const_splat(1.);

pub const fn enclosing_div(n: usize, d: usize) -> usize {
    (n + d - 1) / d
}

pub const fn const_splat<T: SimdElement, const N: usize>(item: T) -> Simd<T, N>
where
    LaneCount<N>: SupportedLaneCount
{
    Simd::from_array([item ; N])
}

// convenience function on simd types when specialized functions aren't
// available in the standard library, hoping autovectorization compiles this
// into an simd instruction

fn map<T: SimdElement, U: SimdElement, const N: usize>(
    vector: Simd<T, N>,
    f: impl FnMut(T) -> U
) -> Simd<U, N>
where
    LaneCount<N>: SupportedLaneCount
{
    vector.to_array().map(f).into()
}

// safety argument for the following two functions:
// both referred to types have the same size 
// `vector` has greater alignment that the return type
// the output reference's lifetime is the same as that of the input
// so no unbounded lifetimes
// we are transmuting a vector to an array over the same scalar
// so values are valid

pub fn as_stereo_sample_array<T: SimdElement>(
    vector: &Simd<T, MAX_VECTOR_WIDTH>
) -> &[Simd<T, 2> ; VOICES_PER_VECTOR] {

    unsafe { transmute(vector) }
}

pub fn as_mut_stereo_sample_array<T: SimdElement>(
    vector: &mut Simd<T, MAX_VECTOR_WIDTH>
) -> &mut [Simd<T, 2> ; VOICES_PER_VECTOR] {

    unsafe { transmute(vector) }
}

fn flp_tp_fxp(x: Float) -> UInt {
    const MAX: Float = const_splat(u32::MAX as f32);
    unsafe { (x * MAX).to_int_unchecked() }
}

fn alternating<T: SimdElement>(pair: Simd<T, 2>) -> Simd<T, MAX_VECTOR_WIDTH> {

    const ZERO_ONE: [usize ; MAX_VECTOR_WIDTH] = {
        let mut array = [0 ; MAX_VECTOR_WIDTH];
        let mut i = 1;
        while i < MAX_VECTOR_WIDTH {
            array[i] = 1;
            i += 2;
        }
        array
    };

    simd_swizzle!(pair, ZERO_ONE)
}

fn fxp_to_flp(x: UInt) -> Float {
    const RATIO: Float = const_splat(1. / u32::MAX as f32);
    x.cast() * RATIO
}

/// we're using intrinsics for now because u32 gathers aren't in std::simd yet
unsafe fn gather_select_unchecked(slice: &[f32], index: UInt, mask: TMask, or: Float) -> Float {

    cfg_if! {

        if #[cfg(target_feature = "avx512f")] {
        
            _mm512_mask_i32gather_ps(
                or.into(),
                // Mask<T, 16> and __mmask16 (u16) are the same size on AVX-512
                transmute(mask),
                index.into(),
                slice.as_ptr().cast(),
                4,
            ).into()
        
        } else if #[cfg(target_feature = "avx2")] {
        
            _mm256_mask_i32gather_ps(
                or.into(),
                slice.as_ptr(),
                index.into(),
                mask.into(),
                4
            ).into()
        
        } else {
            Simd::gather_select_unchecked(slice, mask.cast(), index.cast(), or)
        }
    }
}

pub unsafe fn gather(slice: &[f32], index: UInt) -> Float {

    cfg_if! {
    
        if #[cfg(target_feature = "avx512f")] {

            _mm512_i32gather_ps(index.into(), slice.as_ptr().cast(), 4).into()
        
        } else if #[cfg(target_feature = "avx2")] {
        
            _mm256_i32gather_ps(slice.as_ptr(), index.into(), 4).into()
        
        } else {
            Simd::gather_select_unchecked(slice, Mask::splat(true), index.cast(), or)
        }
    }
}

fn lerp(a: Float, b: Float, t: Float) -> Float {
    (b - a).mul_add(t, a)
}

pub fn sum_to_stereo_sample(x: Float) -> f32x2 {

    unsafe { cfg_if! {

        if #[cfg(any(target_feature = "avx512f"))] {

            // MAX_VECTOR_WIDTH = 16
            let [left1, right1]: [Simd<f32, { MAX_VECTOR_WIDTH / 2 }> ; 2] = transmute(x);
            let [left2, right2]: [Simd<f32, { MAX_VECTOR_WIDTH / 4 }> ; 2] = transmute(left1 + right1);
            let [left3, right3]: [Simd<f32, { MAX_VECTOR_WIDTH / 8 }> ; 2] = transmute(left2 + right2);

            left3 + right3

        } else if #[cfg(any(target_feature = "avx"))] {

            // MAX_VECTOR_WIDTH = 8
            let [left1, right1]: [Simd<f32, { MAX_VECTOR_WIDTH / 2 }> ; 2] = transmute(x);
            let [left2, right2]: [Simd<f32, { MAX_VECTOR_WIDTH / 4 }> ; 2] = transmute(left1 + right1);
            left2 + right2
            
        } else if #[cfg(any(target_feature = "sse"))] {

            // MAX_VECTOR_WIDTH = 4
            let [left, right]: [Simd<f32, { MAX_VECTOR_WIDTH / 2 }> ; 2] = transmute(x);
            left + right

        } else {

            // MAX_VECTOR_WIDTH = 2
            x
        }
    } }
}