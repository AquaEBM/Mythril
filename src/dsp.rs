pub mod wavetable;
pub mod wt_osc;

use core_simd::simd::*;
use std_float::*;
use plugin_util::*;
use simd_util::*;
use math::*;

use super::params;

pub const MAX_POLYPHONY: usize = 128;
pub const NUM_VECTORS: usize = enclosing_div(MAX_POLYPHONY, VOICES_PER_VECTOR);