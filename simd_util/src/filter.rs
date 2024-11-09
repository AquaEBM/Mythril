#[cfg(feature = "transfer_funcs")]
use ::num::{Complex, Float, One};

#[cfg(feature = "nih_plug")]
use ::nih_plug::prelude::Enum;

use super::{math, simd::*, smoothing::*, VFloat, FLOATS_PER_VECTOR};

pub mod one_pole;
pub mod svf;

#[derive(Default, Clone, Copy)]
/// Transposed Direct Form II Trapezoidal Integrator, but without the `0.5` pre-gain.
///
/// Specifically, let `x[n]` be the input signal, `y[n]` be the output signal, and `v[n]`
/// be the internal state.
///
/// This system's difference equations are:
///
/// `y[n] = x[n] + v[n-1]`
///
/// `v[n] = y[n] + x[n]`
///
/// Transfer function:
///
/// `(z + 1) / (z - 1)`
pub struct Integrator<const N: usize = FLOATS_PER_VECTOR>
where
    LaneCount<N>: SupportedLaneCount,
{
    s: VFloat<N>,
}

impl<const N: usize> Integrator<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    /// Feed the provided input `sample` (`x[n]`),
    /// update the system's internal state (`v[n]`),
    /// and return the system's next output (`y[n]`)
    #[inline]
    pub fn tick(&mut self, x: VFloat<N>) -> VFloat<N> {
        let output = x + self.s;
        self.s = output + x;
        output
    }

    /// Set the internal `v[n]` state to `0.0`
    #[inline]
    pub fn reset(&mut self) {
        self.s = Simd::splat(0.);
    }

    /// Get the current `v[n]` state
    #[inline]
    pub fn get_current(&self) -> &VFloat<N> {
        &self.s
    }
}
