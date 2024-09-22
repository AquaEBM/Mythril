use super::{
    math::{exp2, log2, pow},
    simd::{num::SimdFloat, *},
    Float, TMask, FLOATS_PER_VECTOR,
};

pub trait Smoother {
    type Value: SimdFloat;

    fn set_target(&mut self, target: Self::Value, t: Self::Value);
    #[inline]
    fn set_target_recip(&mut self, target: Self::Value, t_recip: Self::Value) {
        self.set_target(target, t_recip.recip());
    }
    fn set_val_instantly(&mut self, target: Self::Value, mask: <Self::Value as SimdFloat>::Mask);
    fn set_all_vals_instantly(&mut self, target: Self::Value);
    fn tick(&mut self, t: Self::Value);
    fn tick1(&mut self);
    fn current(&self) -> Self::Value;
}

#[derive(Clone, Copy)]
pub struct LogSmoother<const N: usize = FLOATS_PER_VECTOR>
where
    LaneCount<N>: SupportedLaneCount,
{
    pub factor: Float<N>,
    pub value: Float<N>,
}

impl<const N: usize> Default for LogSmoother<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    fn default() -> Self {
        Self {
            factor: Simd::splat(1.),
            value: Simd::splat(1.),
        }
    }
}

impl<const N: usize> LogSmoother<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    #[inline]
    pub fn scale(&mut self, scale: Float<N>) {
        self.value *= scale;
    }
}

impl<const N: usize> Smoother for LogSmoother<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    type Value = Float<N>;

    #[inline]
    fn set_target(&mut self, target: Self::Value, t: Self::Value) {
        self.factor = exp2(log2(target / self.value) / t);
    }

    #[inline]
    fn set_target_recip(&mut self, target: Self::Value, t_recip: Self::Value) {
        self.factor = pow(target / self.value, t_recip);
    }

    #[inline]
    fn set_val_instantly(&mut self, target: Self::Value, mask: TMask<N>) {
        self.factor = mask.select(Simd::splat(1.), self.factor);
        self.value = mask.select(target, self.value);
    }

    #[inline]
    fn set_all_vals_instantly(&mut self, target: Self::Value) {
        self.value = target;
        self.factor = Simd::splat(1.0);
    }

    #[inline]
    fn tick(&mut self, dt: Self::Value) {
        self.value *= pow(self.factor, dt);
    }

    #[inline]
    fn tick1(&mut self) {
        self.value *= self.factor;
    }

    #[inline]
    fn current(&self) -> Self::Value {
        self.value
    }
}

#[derive(Default, Clone, Copy)]
pub struct LinearSmoother<const N: usize = FLOATS_PER_VECTOR>
where
    LaneCount<N>: SupportedLaneCount,
{
    pub increment: Float<N>,
    pub value: Float<N>,
}

impl<const N: usize> LinearSmoother<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    #[inline]
    pub fn scale(&mut self, scale: Float<N>) {
        self.value *= scale;
        self.increment *= scale;
    }
}

impl<const N: usize> Smoother for LinearSmoother<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    type Value = Float<N>;

    #[inline]
    fn set_target(&mut self, target: Self::Value, t: Self::Value) {
        self.increment = (target - self.value) / t;
    }

    #[inline]
    fn set_target_recip(&mut self, target: Self::Value, t_recip: Self::Value) {
        self.increment = (target - self.value) * t_recip;
    }

    #[inline]
    fn set_val_instantly(&mut self, target: Self::Value, mask: TMask<N>) {
        self.increment = mask.select(Simd::splat(0.), self.increment);
        self.value = mask.select(target, self.value);
    }

    #[inline]
    fn set_all_vals_instantly(&mut self, target: Self::Value) {
        self.increment = Simd::splat(0.0);
        self.value = target;
    }

    #[inline]
    fn tick(&mut self, t: Self::Value) {
        self.value += self.increment * t;
    }

    #[inline]
    fn tick1(&mut self) {
        self.value += self.increment;
    }

    #[inline]
    fn current(&self) -> Self::Value {
        self.value
    }
}

#[derive(Default, Clone, Copy)]
pub struct GenericSmoother<const N: usize = FLOATS_PER_VECTOR>
where
    LaneCount<N>: SupportedLaneCount,
{
    pub current: Float<N>,
    pub target: Float<N>,
}

impl<const N: usize> GenericSmoother<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    #[inline]
    pub fn smooth_exp(&mut self, alpha: Float<N>) {
        let y = &mut self.current;
        let x = self.target;
        *y = alpha.mul_add(*y - x, x);
    }

    #[inline]
    pub fn set_val_instantly(&mut self, target: Float<N>, mask: TMask<N>) {
        self.target = mask.select(target, self.target);
        self.current = mask.select(target, self.current);
    }

    #[inline]
    pub fn set_target(&mut self, target: Float<N>, mask: TMask<N>) {
        self.target = mask.select(target, self.target);
    }
}
