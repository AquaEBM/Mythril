#![feature(portable_simd, stdarch_x86_avx512)]

use cfg_if::cfg_if;

#[cfg(feature = "core_simd_crate")]
pub mod simd {
    pub use core_simd::simd::*;
    pub use std_float::*;
}
#[cfg(feature = "std_simd")]
pub use std::simd;

cfg_if! {
    if #[cfg(any(feature = "std_simd", feature = "core_simd_crate"))] {

        pub mod filter;
        pub mod math;
        pub mod smoothing;
        mod util;
        pub use util::*;

    }
}
