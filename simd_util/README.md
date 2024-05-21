# `simd_util`

Simple SIMD utilities I use throughout my other (audio related) projects.

- Miscellaneous, SIMD-compatible, fast approximations of common mathematical functions.
- Linear and logarithmic (EMA coming soon) SIMD-compatible parameter smoothers.
- Utilites for swizzling and operating on the layout of vector types.
- Implementations of simple, linear, analog filters, with built-in parameter smoothing.

Note that this crate depends on `[feaature(portable_simd)]`, which requires Nightly Rust.
