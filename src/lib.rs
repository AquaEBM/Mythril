#![feature(
    portable_simd,
    new_zeroed_alloc,
    slice_from_ptr_range,
    ptr_sub_ptr,
    box_vec_non_null
)]

extern crate alloc;

pub mod buffer;
pub mod delay;
pub mod lender;
pub mod processor;

use alloc::sync::Arc;
use core::{iter, mem, num::NonZeroUsize};
