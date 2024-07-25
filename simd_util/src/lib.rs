#![feature(portable_simd, stdarch_x86_avx512)]

use cfg_if::cfg_if;

cfg_if! {

    if #[cfg(feature = "non_std_simd")] {

        pub mod simd {
            pub use core_simd::simd::*;
            pub use std_float::*;
        }

    } else if #[cfg(feature = "std_simd")] {
        pub use std::simd;
    }
}

cfg_if! {
    if #[cfg(any(feature = "std_simd", feature = "non_std_simd"))] {

pub mod filter;
pub mod math;
pub mod smoothing;
mod util;
pub use util::*;

    }
}
