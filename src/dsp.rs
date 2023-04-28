pub mod wavetable;
pub mod wt_osc;

use std::{simd::*, array, arch::x86_64::*, mem::transmute, sync::Arc};

use super::params;

const MAX_VECTOR_WIDTH: usize = 16;
type MaskType = <Mask<i32, MAX_VECTOR_WIDTH> as ToBitMask>::BitMask;
const VOICES_PER_VECTOR: usize = MAX_VECTOR_WIDTH / 2;

pub const MAX_POLYPHONY: usize = 128;
pub const NUM_VECTORS: usize = MAX_POLYPHONY / VOICES_PER_VECTOR;

type Float = Simd<f32, MAX_VECTOR_WIDTH>;
type UInt = Simd<u32, MAX_VECTOR_WIDTH>;
type Int = Simd<i32, MAX_VECTOR_WIDTH>;

#[inline]
pub const fn splat<T: SimdElement, const N: usize>(item: T) -> Simd<T, N>
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

pub fn as_stereo_samples_ref(vector_ref: &mut Float) -> &mut [f32x2 ; VOICES_PER_VECTOR] {
    // SAFETY:
    //  - VECTOR_WIDTH is a power of two greater than or equal to 2
    //  - VOICES_PER_VECTOR = VECTOR_WIDTH / 2
    //  - So Float always has greater than or equal alignment then f32x2
    // so the f32x2 values are properly aligned
    unsafe { transmute(vector_ref) }
}

#[inline]
fn to_fixed_point(x: Float) -> UInt {
    const MAX: Float = splat(u32::MAX as f32);
    unsafe { (x * MAX).to_int_unchecked() }
}

pub const fn zero_one<const N: usize>() -> [usize ; N] {
    let mut array = [0 ; N];
    let mut i = 0;
    while i < N {
        array[i] = i & 1;
        i += 1;
    }
    array
}

#[inline]
fn alternating<T: SimdElement>(pair: Simd<T, 2>) -> Simd<T, MAX_VECTOR_WIDTH> {

    const ZERO_ONE: [usize ; MAX_VECTOR_WIDTH] = zero_one();

    simd_swizzle!(pair, ZERO_ONE)
}

#[inline]
fn fxp_to_flp(x: UInt) -> Float {
    const RATIO: Float = splat(1. / u32::MAX as f32);
    x.cast() * RATIO
}

#[inline]
/// we're using intel intrinsics for now because u32 gathers aren't in std::simd yet
fn gather_select(slice: &[f32], index: UInt, bitmask: MaskType) -> Float {
    unsafe {
        // _mm_mask_i32gather_ps(
        //     splat(0.).into(),
        //     slice.as_ptr(),
        //     index.cast::<i32>.into(),
        //     std::mem::transmute(Mask::<i32, VECTOR_WIDTH>::from_bitmask(bitmask)),
        //     4
        // ) // 4

        // _mm256_mask_i32gather_ps(
        //     splat(0.).into(),
        //     slice.as_ptr(),
        //     index.cast::<i32>.into(),
        //     std::mem::transmute(Mask::<i32, VECTOR_WIDTH>::from_bitmask(bitmask)),
        //     4
        // ) // 8

        _mm512_mask_i32gather_ps(
            splat(0.).into(),
            bitmask,
            index.cast::<i32>().into(),
            slice.as_ptr().cast(),
            4,
        ) // 16
    }.into()
}

pub fn gather(slice: &[f32], index: UInt) -> Float {
    unsafe {
        // _mm_i32gather_ps(slice.as_ptr(), index.cast::<i32>.into(), 4) // 4
        // _mm256_i32gather_ps(slice.as_ptr(), index.cast::<i32>.into(), 4) // 8
        _mm512_i32gather_ps(index.cast::<i32>().into(), slice.as_ptr().cast(), 4) // 16
    }.into()
}

#[inline]
fn lerp(a: Float, b: Float, t: Float) -> Float {
    (b - a).mul_add(t, a)
}

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