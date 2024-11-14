use super::*;

#[cfg_attr(feature = "nih_plug", derive(Enum))]
#[derive(PartialEq, Eq, Clone, Copy, Debug, Default, PartialOrd, Ord, Hash)]
pub enum FilterMode {
    #[cfg_attr(feature = "nih_plug", name = "Passthrough")]
    #[default]
    ID,
    #[cfg_attr(feature = "nih_plug", name = "Highpass")]
    HP,
    #[cfg_attr(feature = "nih_plug", name = "Lowpass")]
    LP,
    #[cfg_attr(feature = "nih_plug", name = "Allpass")]
    AP,
    #[cfg_attr(feature = "nih_plug", name = "Low Shelf")]
    LSH,
    #[cfg_attr(feature = "nih_plug", name = "High Shelf")]
    HSH,
}

/// Contains parameters for an analogue one-pole filter 
pub struct OnePoleParamsSmoothed<const N: usize = FLOATS_PER_VECTOR>
where
    LaneCount<N>: SupportedLaneCount,
{
    g1: LogSmoother<N>,
    k: LogSmoother<N>,
}

impl<const N: usize> OnePoleParamsSmoothed<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    #[inline]
    pub fn get_g1(&self) -> VFloat<N> {
        self.g1.value
    }

    #[inline]
    pub fn get_gain(&self) -> VFloat<N> {
        self.k.value
    }

    #[inline]
    fn g(w_c: VFloat<N>) -> VFloat<N> {
        math::tan_half_x(w_c)
    }

    #[inline]
    fn g1(g: VFloat<N>) -> VFloat<N> {
        g / (Simd::splat(1.) + g)
    }

    #[inline]
    fn set_values(&mut self, g: VFloat<N>, k: VFloat<N>) {
        self.g1.set_all_vals_instantly(Self::g1(g));
        self.k.set_all_vals_instantly(k);
    }

    /// Call this _only_ if you intend to
    /// output non-shelving filter shapes.
    #[inline]
    pub fn set_params(&mut self, w_c: VFloat<N>, gain: VFloat<N>) {
        self.set_values(Self::g(w_c), gain)
    }

    /// Call this _only_ if you intend to output low-shelving filter shapes.
    #[inline]
    pub fn set_params_low_shelving(&mut self, w_c: VFloat<N>, gain: VFloat<N>) {
        self.k.set_all_vals_instantly(gain);
        self.g1.set_all_vals_instantly(Self::g(w_c) / gain.sqrt());
    }

    /// Call this _only_ if you intend to output high-shelving filter shapes.
    #[inline]
    pub fn set_params_high_shelving(&mut self, w_c: VFloat<N>, gain: VFloat<N>) {
        self.k.set_all_vals_instantly(gain);
        self.g1.set_all_vals_instantly(Self::g(w_c) * gain.sqrt());
    }

    #[inline]
    fn set_values_smoothed(&mut self, g: VFloat<N>, k: VFloat<N>, inc: VFloat<N>) {
        self.g1.set_target(Self::g1(g), inc);
        self.k.set_target(k, inc);
    }

    /// Like `Self::set_params` but smoothed
    #[inline]
    pub fn set_params_smoothed(&mut self, w_c: VFloat<N>, gain: VFloat<N>, inc: VFloat<N>) {
        self.set_values_smoothed(Self::g(w_c), gain, inc)
    }

    /// Like `Self::set_params_low_shelving` but smoothed
    #[inline]
    pub fn set_params_low_shelving_smoothed(
        &mut self,
        w_c: VFloat<N>,
        gain: VFloat<N>,
        inc: VFloat<N>,
    ) {
        self.set_values_smoothed(Self::g(w_c) / gain.sqrt(), gain, inc)
    }

    /// Like `Self::set_params_high_shelving` but smoothed.
    #[inline]
    pub fn set_params_high_shelving_smoothed(
        &mut self,
        w_c: VFloat<N>,
        gain: VFloat<N>,
        inc: VFloat<N>,
    ) {
        self.set_values_smoothed(Self::g(w_c) * gain.sqrt(), gain, inc)
    }

    /// Update the filter's internal parameter smoothers.
    ///
    /// After calling `Self::set_params_smoothed([values, ...], num_samples)` this should
    /// be called only _once_ per sample, _up to_ `num_samples` times, until
    /// `Self::set_params_smoothed` is to be called again
    #[inline]
    pub fn update_smoothers(&mut self) {
        self.g1.tick1();
        self.k.tick1();
    }

    pub fn update_function(mode: FilterMode) -> fn(&mut Self, VFloat<N>, VFloat<N>) {
        use FilterMode::*;

        match mode {
            HSH => Self::set_params_high_shelving,
            LSH => Self::set_params_low_shelving,
            _ => Self::set_params,
        }
    }

    pub fn smoothing_update_function(
        mode: FilterMode,
    ) -> fn(&mut Self, VFloat<N>, VFloat<N>, VFloat<N>) {
        use FilterMode::*;

        match mode {
            LSH => Self::set_params_low_shelving_smoothed,
            HSH => Self::set_params_high_shelving_smoothed,
            _ => Self::set_params_smoothed,
        }
    }
}

#[derive(Default)]
pub struct OnePole<const N: usize = FLOATS_PER_VECTOR>
where
    LaneCount<N>: SupportedLaneCount,
{
    lp: Integrator<N>,
    x: VFloat<N>,
}

impl<const N: usize> OnePole<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    #[inline]
    pub fn reset(&mut self) {
        self.lp.reset()
    }

    /// The "`tick`" method, must be called _only once_ per sample, _every sample_.
    /// 
    /// Feeds `x` into the filter, which updates it's internal state accordingly.
    ///
    /// After calling this, you can get different filter outputs
    /// using `Self::get_{highpass, lowpass, allpass, ...}`
    #[inline]
    pub fn process(&mut self, x: VFloat<N>, g1: VFloat<N>) {

        self.x = x;
        self.lp.process((x - self.lp.state()) * g1);
    }

    #[inline]
    pub fn get_passthrough(&self) -> &VFloat<N> {
        &self.x
    }

    #[inline]
    pub fn get_lowpass(&self) -> &VFloat<N> {
        self.lp.output()
    }

    #[inline]
    pub fn get_allpass(&self) -> VFloat<N> {
        self.get_lowpass() - self.get_highpass()
    }

    #[inline]
    pub fn get_highpass(&self) -> VFloat<N> {
        self.x - self.get_lowpass()
    }

    #[inline]
    pub fn get_low_shelf(&self, gain: VFloat<N>) -> VFloat<N> {
        gain.mul_add(*self.get_lowpass(), self.get_highpass())
    }

    #[inline]
    pub fn get_high_shelf(&self, gain: VFloat<N>) -> VFloat<N> {
        gain.mul_add(self.get_highpass(), *self.get_lowpass())
    }

    pub fn output_function(
        mode: FilterMode,
    ) -> fn(&Self, VFloat<N>) -> VFloat<N> {
        use FilterMode::*;

        match mode {
            ID => |f, _g| *f.get_passthrough(),
            LP => |f, _g| *f.get_lowpass(),
            AP => |f, _g| f.get_allpass(),
            HP => |f, _g| f.get_highpass(),
            LSH => Self::get_low_shelf,
            HSH => Self::get_high_shelf,
        }
    }
}

#[cfg(feature = "transfer_funcs")]
pub mod transfer {

    use super::*;

    pub fn transfer_function<T: Float>(
        filter_mode: FilterMode,
    ) -> fn(Complex<T>, T) -> Complex<T> {
        use FilterMode::*;

        match filter_mode {
            ID => |s, _g| s,
            LP => |s, _g| low_pass(s),
            AP => |s, _g| all_pass(s),
            HP => |s, _g| high_pass(s),
            LSH => low_shelf,
            HSH => high_shelf,
        }
    }

    fn h_denominator<T: Float>(s: Complex<T>) -> Complex<T> {
        s + T::one()
    }

    pub fn low_pass<T: Float>(s: Complex<T>) -> Complex<T> {
        h_denominator(s).finv()
    }

    pub fn all_pass<T: Float>(s: Complex<T>) -> Complex<T> {
        (-s + T::one()).fdiv(h_denominator(s))
    }

    pub fn high_pass<T: Float>(s: Complex<T>) -> Complex<T> {
        s.fdiv(h_denominator(s))
    }

    pub fn low_shelf<T: Float>(s: Complex<T>, gain: T) -> Complex<T> {
        tilting(s, gain.recip()).scale(gain.sqrt())
    }

    pub fn tilting<T: Float>(s: Complex<T>, gain: T) -> Complex<T> {
        let m = gain.sqrt();
        (s.scale(m) + T::one()) / (s + m)
    }

    pub fn high_shelf<T: Float>(s: Complex<T>, gain: T) -> Complex<T> {
        tilting(s, gain).scale(gain.sqrt())
    }
}
