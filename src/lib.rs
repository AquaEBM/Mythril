#![feature(portable_simd, new_zeroed_alloc, slice_ptr_get, box_vec_non_null)]

// Quite a hefty bit of unsafe here. TODO: Write docs & safety comments

extern crate alloc;

pub mod buffer;
pub mod lender;

use alloc::sync::Arc;
use core::{iter, ptr::NonNull};
