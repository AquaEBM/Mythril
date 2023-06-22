pub mod wavetable;
pub mod wt_osc;

use plugin_util::*;
use simd::*; 
use simd_util::*;
use math::*;

use super::params;

pub const MAX_POLYPHONY: usize = 128;
pub const NUM_VECTORS: usize = enclosing_div(MAX_POLYPHONY, VOICES_PER_VECTOR);