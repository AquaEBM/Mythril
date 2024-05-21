use super::simd::{prelude::*, *};
use std::f32::consts::LN_2;

#[inline]
pub fn lerp<const N: usize>(a: Simd<f32, N>, b: Simd<f32, N>, t: Simd<f32, N>) -> Simd<f32, N>
where
    LaneCount<N>: SupportedLaneCount,
{
    (b - a).mul_add(t, a)
}

// surprisingly efficient/accurate tan(x/2) approximation
// credit to my uni for the free matlab
#[inline]
pub fn tan_half_x<const N: usize>(x: Simd<f32, N>) -> Simd<f32, N>
where
    LaneCount<N>: SupportedLaneCount,
{
    // optimised into constants, hopefully
    let na = Simd::splat(1. / 15120.);
    let nb = Simd::splat(-1. / 36.);
    let nc = Simd::splat(1.);
    let da = Simd::splat(1. / 504.);
    let db = Simd::splat(-2. / 9.);
    let dc = Simd::splat(2.);

    let x2 = x * x;
    let num = x * x2.mul_add(x2.mul_add(na, nb), nc);
    let den = x2.mul_add(x2.mul_add(da, db), dc);

    num / den
}

/// Unspecified results for i not in [-126 ; 126]
#[inline]
pub fn fexp2i<const N: usize>(i: Simd<i32, N>) -> Simd<f32, N>
where
    LaneCount<N>: SupportedLaneCount,
{
    // optimised into constants, hopefully
    let mantissa_bits = Simd::splat(f32::MANTISSA_DIGITS as i32 - 1);
    let exponent_bias = Simd::splat(f32::MAX_EXP - 1);
    Simd::from_bits(((i + exponent_bias) << mantissa_bits).cast())
}

/// "cheap" 2 ^ x approximation, Unspecified results if v is
/// NAN, inf or subnormal. Taylor series already works pretty well here since
/// the polynomial approximation we need here is in the interval (-0.5, 0.5)
/// (which is small and centered at zero)
#[inline]
pub fn exp2<const N: usize>(v: Simd<f32, N>) -> Simd<f32, N>
where
    LaneCount<N>: SupportedLaneCount,
{
    // optimised into constants, hopefully
    // LN_2^n / n!
    let a = Simd::splat(1.);
    let b = Simd::splat(LN_2);
    let c = Simd::splat(0.240_226_5);
    let d = Simd::splat(0.005_550_411);
    let e = Simd::splat(0.009_618_129);
    let f = Simd::splat(0.001_333_355_8);

    let rounded = v.round();

    let int = fexp2i(unsafe { rounded.to_int_unchecked() }); // very cheap

    let x = v - rounded; // is always in [-0.5 ; 0.5]

    let y = x.mul_add(x.mul_add(x.mul_add(x.mul_add(x.mul_add(f, e), d), c), b), a);
    int * y
}

/// This returns 2^(`semitones`/12)
#[inline]
pub fn semitones_to_ratio<const N: usize>(semitones: Simd<f32, N>) -> Simd<f32, N>
where
    LaneCount<N>: SupportedLaneCount,
{
    // optimised into a constant, hopefully
    let ratio = Simd::splat(1. / 12.);
    exp2(semitones * ratio)
}

/// Compute floor(log2(x)) as an Int. Unspecified results
/// if x is NAN, inf, negative or subnormal
#[inline]
pub fn ilog2f<const N: usize>(x: Simd<f32, N>) -> Simd<i32, N>
where
    LaneCount<N>: SupportedLaneCount,
{
    // optimised into constants, hopefully
    let mantissa_bits = Simd::splat(f32::MANTISSA_DIGITS as i32 - 1);
    let exponent_bias = Simd::splat(f32::MAX_EXP - 1);
    (x.to_bits().cast() >> mantissa_bits) - exponent_bias
}

/// "cheap" log2 approximation. Unspecified results is v is
/// NAN, inf, negative, or subnormal.
#[inline]
pub fn log2<const N: usize>(v: Simd<f32, N>) -> Simd<f32, N>
where
    LaneCount<N>: SupportedLaneCount,
{
    // optimised into constants, hopefully
    let a = Simd::splat(-1819. / 651.);
    let b = Simd::splat(5.);
    let c = Simd::splat(-10. / 3.);
    let d = Simd::splat(10. / 7.);
    let e = Simd::splat(-1. / 3.);
    let f = Simd::splat(1. / 31.);
    let mantissa_mask = Simd::splat((1 << (f32::MANTISSA_DIGITS - 1)) - 1);
    let zero_exponent = Simd::splat(1f32.to_bits());

    let log_exponent = ilog2f(v).cast();
    let x = Simd::<f32, N>::from_bits(v.to_bits() & mantissa_mask | zero_exponent);

    let y = x.mul_add(x.mul_add(x.mul_add(x.mul_add(x.mul_add(f, e), d), c), b), a);
    log_exponent + y
}

#[inline]
pub fn pow<const N: usize>(base: Simd<f32, N>, exp: Simd<f32, N>) -> Simd<f32, N>
where
    LaneCount<N>: SupportedLaneCount,
{
    exp2(log2(base) * exp)
}

#[inline]
pub fn flp_to_fxp<const N: usize>(x: Simd<f32, N>) -> Simd<u32, N>
where
    LaneCount<N>: SupportedLaneCount,
{
    // optimised into a constant, hopefully
    let max = Simd::splat((1u64 << u32::BITS) as f32);
    unsafe { (x * max).to_int_unchecked() }
}

#[inline]
pub fn fxp_to_flp<const N: usize>(x: Simd<u32, N>) -> Simd<f32, N>
where
    LaneCount<N>: SupportedLaneCount,
{
    // optimised into a constant, hopefully
    let ratio = Simd::splat(1. / (1u64 << u32::BITS) as f32);
    x.cast() * ratio
}
