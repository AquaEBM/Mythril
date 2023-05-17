pub mod wavetable;
pub mod wt_osc;
pub mod utils;

use std::{simd::*, array, arch::x86_64::*, mem::transmute, sync::Arc};

use super::params;

const MAX_VECTOR_WIDTH: usize = 16;
const VOICES_PER_VECTOR: usize = MAX_VECTOR_WIDTH / 2;

pub const MAX_POLYPHONY: usize = 128;
pub const NUM_VECTORS: usize = enclosing_div(MAX_POLYPHONY, VOICES_PER_VECTOR);

type Float = Simd<f32, MAX_VECTOR_WIDTH>;
type UInt = Simd<u32, MAX_VECTOR_WIDTH>;
type Int = Simd<i32, MAX_VECTOR_WIDTH>;

type MaskType = < <Float as SimdFloat>::Mask as ToBitMask>::BitMask;

const ZERO_F: Float = const_splat(0.);
const ONE_F: Float = const_splat(1.);

pub const fn enclosing_div(n: usize, d: usize) -> usize {
    n / d + (n % d != 0) as usize
}

#[inline]
pub const fn const_splat<T: SimdElement, const N: usize>(item: T) -> Simd<T, N>
where
    LaneCount<N>: SupportedLaneCount
{
    Simd::from_array([item ; N])
}

// convenience function on simd types when specialized functions aren't
// available in the standard library, hoping autovectorization compiles this
// into an simd instruction

#[inline]
fn map<T: SimdElement, const N: usize>(
    vector: Simd<T, N>,
    f: impl FnMut(T) -> T
) -> Simd<T, N>
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

#[inline]
pub fn as_stereo_sample_array<T: SimdElement>(
    vector: &Simd<T, MAX_VECTOR_WIDTH>
) -> &[Simd<T, 2> ; VOICES_PER_VECTOR] {

    unsafe { transmute(vector) }
}

#[inline]
pub fn as_mut_stereo_sample_array<T: SimdElement>(
    vector: &mut Simd<T, MAX_VECTOR_WIDTH>
) -> &mut [Simd<T, 2> ; VOICES_PER_VECTOR] {

    unsafe { transmute(vector) }
}

#[inline]
fn to_fixed_point(x: Float) -> UInt {
    const MAX: Float = const_splat(u32::MAX as f32);
    unsafe { (x * MAX).to_int_unchecked() }
}

#[inline]
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

#[inline]
fn fxp_to_flp(x: UInt) -> Float {
    const RATIO: Float = const_splat(1. / u32::MAX as f32);
    x.cast() * RATIO
}

#[inline]
/// we're using intel intrinsics for now because u32 gathers aren't in std::simd yet
fn gather_select(slice: &[f32], index: UInt, bitmask: MaskType) -> Float {

    unsafe {
        // _mm_mask_i32gather_ps(
        //     ZEROF.into(),
        //     slice.as_ptr(),
        //     index.into(),
        //     std::mem::transmute(Mask::<i32, VECTOR_WIDTH>::from_bitmask(bitmask)),
        //     4
        // ) // 4

        // _mm256_mask_i32gather_ps(
        //     ZEROF.into(),
        //     slice.as_ptr(),
        //     index.into(),
        //     std::mem::transmute(Mask::<i32, VECTOR_WIDTH>::from_bitmask(bitmask)),
        //     4
        // ) // 8

        _mm512_mask_i32gather_ps(
            ZERO_F.into(),
            bitmask,
            index.into(),
            slice.as_ptr().cast(),
            4,
        ) // 16
    }.into()
}

#[inline]
pub fn gather(slice: &[f32], index: UInt) -> Float {
    unsafe {
        // _mm_i32gather_ps(slice.as_ptr(), index.cast::<i32>.into(), 4) // 4
        // _mm256_i32gather_ps(slice.as_ptr(), index.cast::<i32>.into(), 4) // 8
        _mm512_i32gather_ps(index.into(), slice.as_ptr().cast(), 4) // 16
    }.into()
}

#[inline]
fn lerp(a: Float, b: Float, t: Float) -> Float {
    (b - a).mul_add(t, a)
}

#[inline]
pub fn sum_to_stereo_sample(x: Float) -> f32x2 {
    let [left1, right1]: [Simd<f32, { MAX_VECTOR_WIDTH / 2 }> ; 2] = unsafe { transmute(x) };

    let out1 = left1 + right1;
    // out1 // 4

    let [left2, right2]: [Simd<f32, { MAX_VECTOR_WIDTH / 4 }> ; 2] = unsafe { transmute(out1) };

    let out2 = left2 + right2;
    // out2 // 8

    let [left3, right3]: [Simd<f32, { MAX_VECTOR_WIDTH / 8 }> ; 2] = unsafe { transmute(out2) };

    let out3 = left3 + right3;

    out3 // 16
}