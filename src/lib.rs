#![feature(portable_simd, new_zeroed_alloc)]

extern crate alloc;

pub mod lender;
pub mod buffer;
pub mod delay;
pub mod processor;

use alloc::sync::Arc;
use core::{iter, mem, num::NonZeroUsize};