use super::*;

#[cfg_attr(feature = "nih_plug", derive(Enum))]
#[derive(PartialEq, Eq, Clone, Copy, Debug, Default, PartialOrd, Ord, Hash)]
pub enum FilterMode {
    #[cfg_attr(feature = "nih_plug", name = "Lowpass")]
    LP,
    #[cfg_attr(feature = "nih_plug", name = "Bandpass")]
    BP,
    #[cfg_attr(feature = "nih_plug", name = "Unit Bandpass")]
    BP1,
    #[cfg_attr(feature = "nih_plug", name = "Highpass")]
    HP,
    #[cfg_attr(feature = "nih_plug", name = "Allpass")]
    #[default]
    AP,
    #[cfg_attr(feature = "nih_plug", name = "Notch")]
    NCH,
    #[cfg_attr(feature = "nih_plug", name = "Low shelf")]
    LSH,
    #[cfg_attr(feature = "nih_plug", name = "Band shelf")]
    BSH,
    #[cfg_attr(feature = "nih_plug", name = "High Shelf")]
    HSH,
}

/// Digital implementation of the analogue SVF Filter, with built-in
/// parameter smoothing. Based on the one in the book The Art of VA
/// Filter Design by Vadim Zavalishin
///
/// Capable of outputing many different filter types,
/// (highpass, lowpass, bandpass, notch, shelving....)
#[derive(Default)]
pub struct SVF<const N: usize = FLOATS_PER_VECTOR>
where
    LaneCount<N>: SupportedLaneCount,
{
    g: LogSmoother<N>,
    r: LogSmoother<N>,
    k: LogSmoother<N>,
    s: [Integrator<N>; 2],
    x: Float<N>,
    hp: Float<N>,
    bp: Float<N>,
    lp: Float<N>,
}

impl<const N: usize> SVF<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    pub fn reset(&mut self) {
        self.s.iter_mut().for_each(Integrator::reset)
    }

    #[inline]
    fn g(w_c: Float<N>) -> Float<N> {
        math::tan_half_x(w_c)
    }

    #[inline]
    fn set_values(&mut self, g: Float<N>, res: Float<N>, gain: Float<N>) {
        self.k.set_all_vals_instantly(gain);
        self.g.set_all_vals_instantly(g);
        self.r.set_all_vals_instantly(res);
    }

    /// call this if you intend to use _only_ the low-shelving output
    #[inline]
    pub fn set_params_low_shelving(&mut self, w_c: Float<N>, res: Float<N>, gain: Float<N>) {
        let m2 = gain.sqrt();
        let g = Self::g(w_c);
        self.set_values(g / m2.sqrt(), res, m2);
    }

    /// call this if you intend to use _only_ the band-shelving output
    #[inline]
    pub fn set_params_band_shelving(&mut self, w_c: Float<N>, res: Float<N>, gain: Float<N>) {
        let g = Self::g(w_c);
        self.set_values(g, res / gain.sqrt(), gain);
    }

    /// call this if you intend to use _only_ the high-shelving output
    #[inline]
    pub fn set_params_high_shelving(&mut self, w_c: Float<N>, res: Float<N>, gain: Float<N>) {
        let m2 = gain.sqrt();
        let g = Self::g(w_c);
        self.set_values(g * m2.sqrt(), res, m2);
    }

    /// call this if you do not intend to use the shelving outputs
    #[inline]
    pub fn set_params(&mut self, w_c: Float<N>, res: Float<N>, gain: Float<N>) {
        self.set_values(Self::g(w_c), res, gain);
    }

    #[inline]
    fn set_values_smoothed(&mut self, g: Float<N>, res: Float<N>, gain: Float<N>, inc: Float<N>) {
        self.k.set_target(gain, inc);
        self.g.set_target(g, inc);
        self.r.set_target(res, inc);
    }

    /// Like `Self::set_params_low_shelving` but with smoothing
    #[inline]
    pub fn set_params_low_shelving_smoothed(
        &mut self,
        w_c: Float<N>,
        res: Float<N>,
        gain: Float<N>,
        inc: Float<N>,
    ) {
        let m2 = gain.sqrt();
        let g = Self::g(w_c);
        self.set_values_smoothed(g / m2.sqrt(), res, m2, inc);
    }

    /// Like `Self::set_params_band_shelving` but with smoothing
    #[inline]
    pub fn set_params_band_shelving_smoothed(
        &mut self,
        w_c: Float<N>,
        res: Float<N>,
        gain: Float<N>,
        inc: Float<N>,
    ) {
        let g = Self::g(w_c);
        self.set_values_smoothed(g, res / gain.sqrt(), gain, inc);
    }

    /// Like `Self::set_params_high_shelving` but with smoothing
    #[inline]
    pub fn set_params_high_shelving_smoothed(
        &mut self,
        w_c: Float<N>,
        res: Float<N>,
        gain: Float<N>,
        inc: Float<N>,
    ) {
        let m2 = gain.sqrt();
        let g = Self::g(w_c);
        self.set_values_smoothed(g * m2.sqrt(), res, m2, inc);
    }

    /// Like `Self::set_params_non_shelving` but with smoothing
    #[inline]
    pub fn set_params_smoothed(
        &mut self,
        w_c: Float<N>,
        res: Float<N>,
        _gain: Float<N>,
        inc: Float<N>,
    ) {
        self.g.set_target(Self::g(w_c), inc);
        self.r.set_target(res, inc);
        self.k.set_all_vals_instantly(Simd::splat(1.));
    }

    /// Update the filter's internal parameter smoothers.
    ///
    /// After calling `Self::set_params_<output_type>_smoothed(values, ..., num_samples)` this
    /// function should be called _up to_ `num_samples` times, until, that function is to be
    /// called again, calling this function more than `num_samples` times might result in
    /// the internal parameter states diverging away from the previously set values
    #[inline]
    pub fn update_all_smoothers(&mut self) {
        self.k.tick1();
        self.r.tick1();
        self.g.tick1();
    }

    /// Update the filter's internal state.
    ///
    /// This should be called _only once_ per sample, _every sample_
    ///
    /// After calling this, you can get different filter outputs
    /// using `Self::get_{highpass, bandpass, notch, ...}`
    #[inline]
    pub fn process(&mut self, sample: Float<N>) {
        let g = self.g.current();
        let [&s1, s2] = self.s.each_ref().map(Integrator::get_current);

        let g1 = self.r.current() + g;

        self.hp = g1.mul_add(-s1, sample - s2) / g1.mul_add(g, Simd::splat(1.));

        let [s1, s2] = self.s.each_mut();

        self.bp = s1.tick(self.hp * g);
        self.lp = s2.tick(self.bp * g);
        self.x = sample;
    }

    #[inline]
    pub fn get_passthrough(&self) -> Float<N> {
        self.x
    }

    #[inline]
    pub fn get_lowpass(&self) -> Float<N> {
        self.lp
    }

    #[inline]
    pub fn get_bandpass(&self) -> Float<N> {
        self.bp
    }

    #[inline]
    pub fn get_unit_bandpass(&self) -> Float<N> {
        self.r.current() * self.bp
    }

    #[inline]
    pub fn get_highpass(&self) -> Float<N> {
        self.hp
    }

    #[inline]
    pub fn get_allpass(&self) -> Float<N> {
        // 2 * bp1 - x
        self.r.current().mul_add(self.bp + self.bp, -self.x)
    }

    #[inline]
    pub fn get_notch(&self) -> Float<N> {
        // x - bp1
        self.bp.mul_add(-self.r.current(), self.x)
    }

    #[inline]
    pub fn get_high_shelf(&self) -> Float<N> {
        let m2 = self.k.current();
        let bp1 = self.get_unit_bandpass();
        m2.mul_add(m2.mul_add(self.hp, bp1), self.lp)
    }

    #[inline]
    pub fn get_band_shelf(&self) -> Float<N> {
        let bp1 = self.get_unit_bandpass();
        bp1.mul_add(self.k.current(), self.x - bp1)
    }

    #[inline]
    pub fn get_low_shelf(&self) -> Float<N> {
        let m2 = self.k.current();
        let bp1 = self.get_unit_bandpass();
        m2.mul_add(m2.mul_add(self.lp, bp1), self.hp)
    }

    pub fn get_output_function(mode: FilterMode) -> fn(&Self) -> Float<N> {
        use FilterMode::*;

        match mode {
            LP => Self::get_lowpass,
            BP => Self::get_bandpass,
            BP1 => Self::get_unit_bandpass,
            HP => Self::get_highpass,
            AP => Self::get_allpass,
            NCH => Self::get_notch,
            LSH => Self::get_low_shelf,
            BSH => Self::get_band_shelf,
            HSH => Self::get_high_shelf,
        }
    }

    pub fn get_update_function(mode: FilterMode) -> fn(&mut Self, Float<N>, Float<N>, Float<N>) {
        use FilterMode::*;

        match mode {
            LSH => Self::set_params_low_shelving,
            BSH => Self::set_params_band_shelving,
            HSH => Self::set_params_high_shelving,
            _ => Self::set_params,
        }
    }

    pub fn get_smoothing_update_function(
        mode: FilterMode,
    ) -> fn(&mut Self, Float<N>, Float<N>, Float<N>, Float<N>) {
        use FilterMode::*;

        match mode {
            LSH => Self::set_params_low_shelving_smoothed,
            BSH => Self::set_params_band_shelving_smoothed,
            HSH => Self::set_params_high_shelving_smoothed,
            _ => Self::set_params_smoothed,
        }
    }
}

#[cfg(feature = "transfer_funcs")]
impl<const _N: usize> SVF<_N>
where
    LaneCount<_N>: SupportedLaneCount,
{
    pub fn get_transfer_function<T: Float>(
        filter_mode: FilterMode,
    ) -> fn(Complex<T>, T, T) -> Complex<T> {
        use FilterMode::*;

        match filter_mode {
            LP => Self::low_pass_impedance,
            BP => Self::band_pass_impedance,
            BP1 => Self::unit_band_pass_impedance,
            HP => Self::high_pass_impedance,
            AP => Self::all_pass_impedance,
            NCH => Self::notch_impedance,
            LSH => Self::low_shelf_impedance,
            BSH => Self::band_shelf_impedance,
            HSH => Self::high_shelf_impedance,
        }
    }

    fn two<T: Float>(res: T) -> T {
        T::from(2f32).unwrap() * res
    }

    fn h_denominator<T: Float>(s: Complex<T>, res: T) -> Complex<T> {
        s * (s + Self::two(res)) + T::one()
    }

    pub fn low_pass_impedance<T: Float>(s: Complex<T>, res: T, _gain: T) -> Complex<T> {
        Self::h_denominator(s, res).finv()
    }

    pub fn band_pass_impedance<T: Float>(s: Complex<T>, res: T, _gain: T) -> Complex<T> {
        s.fdiv(Self::h_denominator(s, res))
    }

    pub fn unit_band_pass_impedance<T: Float>(s: Complex<T>, res: T, _gain: T) -> Complex<T> {
        Self::band_pass_impedance(s, res, _gain).scale(Self::two(res))
    }

    pub fn high_pass_impedance<T: Float>(s: Complex<T>, res: T, _gain: T) -> Complex<T> {
        (s * s).fdiv(Self::h_denominator(s, res))
    }

    pub fn all_pass_impedance<T: Float>(s: Complex<T>, res: T, _gain: T) -> Complex<T> {
        let bp1 = Self::unit_band_pass_impedance(s, res, _gain);
        bp1 + bp1 - Complex::one()
    }

    pub fn notch_impedance<T: Float>(s: Complex<T>, res: T, _gain: T) -> Complex<T> {
        Complex::<T>::one() - Self::unit_band_pass_impedance(s, res, _gain)
    }

    pub fn tilting_impedance<T: Float>(s: Complex<T>, res: T, gain: T) -> Complex<T> {
        let m2 = gain.sqrt();
        let m = m2.sqrt();
        let sm = s.unscale(m);
        (s * s + sm.scale(Self::two(res)) + m2.recip()).fdiv(Self::h_denominator(sm, res))
    }

    pub fn low_shelf_impedance<T: Float>(s: Complex<T>, res: T, gain: T) -> Complex<T> {
        let m2 = gain.sqrt();
        Self::tilting_impedance(s, res, gain.recip()).scale(m2)
    }

    pub fn band_shelf_impedance<T: Float>(s: Complex<T>, res: T, gain: T) -> Complex<T> {
        let m = gain.sqrt();
        (s * (s + Self::two(res) * m) + T::one()).fdiv(Self::h_denominator(s, res / m))
    }

    pub fn high_shelf_impedance<T: Float>(s: Complex<T>, res: T, gain: T) -> Complex<T> {
        let m2 = gain.sqrt();
        Self::tilting_impedance(s, res, gain).scale(m2)
    }
}
