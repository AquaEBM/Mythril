use super::*;

/// return a vector where values on the left channel
/// are on the right ones and vice-versa
pub fn swap_stereo(v: Float) -> Float {
    const FLIP_PAIRS: [usize ; MAX_VECTOR_WIDTH] = {

        let mut array = [0 ; MAX_VECTOR_WIDTH];

        let mut i = 0;
        while i < MAX_VECTOR_WIDTH {

            array[i] = i ^ 1;
            i += 1;
        }
        array
    };

    simd_swizzle!(v, FLIP_PAIRS)
}

pub fn semitones_to_ratio(semitones: Float) -> Float {
    const RATIO: Float = const_splat(1. / 12.);
    exp2(semitones * RATIO)
}

/// triangluar panning of a vector of stereo samples, 0 < pan <= 1
pub fn triangular_pan_weights(pan: Float) -> Float {

    const SIGN_MASK: Float = {
        let mut array = [0. ; MAX_VECTOR_WIDTH];
        let mut i = 0;
        while i < MAX_VECTOR_WIDTH {
            array[i] = -0.;
            i += 2;
        }
        Simd::from_array(array)
    };

    const ALT_ONE: Float = {
        let mut array = [0. ; MAX_VECTOR_WIDTH];
        let mut i = 0;
        while i < MAX_VECTOR_WIDTH {
            array[i] = 1.;
            i += 2;
        }
        Simd::from_array(array)
    };

    Float::from_bits(pan.to_bits() ^ SIGN_MASK.to_bits()) + ALT_ONE
}