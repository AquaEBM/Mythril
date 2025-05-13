#![feature(
    portable_simd,
    new_zeroed_alloc,
    slice_ptr_get,
    slice_from_ptr_range,
    box_vec_non_null
)]

extern crate alloc;

pub mod delay;
pub mod lender;
pub mod buffer;

use core::{iter, num::NonZeroUsize, ptr::NonNull};