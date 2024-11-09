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

pub trait SVFParams {
    type Sample;
    /// `g = tan(w_c / 2)`, potentially shifted when outuputting shelving filter shapes
    /// where `w_c` is the cutoff frequency, in radians per sample.
    fn g(&self) -> Self::Sample;
    /// Resonance value at the cutoff, must be greater than 0
    fn res(&self) -> Self::Sample;
    /// Square root of the shelving gain (if applicable), must be greater than 0
    fn root_gain(&self) -> Self::Sample;
}

pub struct SVFParamsSmoothed<const N: usize = FLOATS_PER_VECTOR>
where
    LaneCount<N>: SupportedLaneCount,
{
    g: LogSmoother<N>,
    r: LogSmoother<N>,
    k: LogSmoother<N>,
}

impl<const N: usize> SVFParams for SVFParamsSmoothed<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    type Sample = VFloat<N>;

    #[inline]
    fn g(&self) -> Self::Sample {
        self.g.value
    }

    #[inline]
    fn res(&self) -> Self::Sample {
        self.r.value
    }

    #[inline]
    fn root_gain(&self) -> Self::Sample {
        self.k.value
    }
}

impl<const N: usize> SVFParamsSmoothed<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    #[inline]
    fn g(w_c: VFloat<N>) -> VFloat<N> {
        math::tan_half_x(w_c)
    }

    #[inline]
    fn set_values(&mut self, g: VFloat<N>, res: VFloat<N>, gain: VFloat<N>) {
        self.k.set_all_vals_instantly(gain);
        self.g.set_all_vals_instantly(g);
        self.r.set_all_vals_instantly(res);
    }

    /// call this if you intend to use _only_ the low-shelving output
    #[inline]
    pub fn set_params_low_shelving(&mut self, w_c: VFloat<N>, res: VFloat<N>, gain: VFloat<N>) {
        let m2 = gain.sqrt();
        let g = Self::g(w_c);
        self.set_values(g / m2.sqrt(), res, m2);
    }

    /// call this if you intend to use _only_ the band-shelving output
    #[inline]
    pub fn set_params_band_shelving(&mut self, w_c: VFloat<N>, res: VFloat<N>, gain: VFloat<N>) {
        let g = Self::g(w_c);
        self.set_values(g, res / gain.sqrt(), gain);
    }

    /// call this if you intend to use _only_ the high-shelving output
    #[inline]
    pub fn set_params_high_shelving(&mut self, w_c: VFloat<N>, res: VFloat<N>, gain: VFloat<N>) {
        let m2 = gain.sqrt();
        let g = Self::g(w_c);
        self.set_values(g * m2.sqrt(), res, m2);
    }

    /// call this if you do not intend to use the shelving outputs
    #[inline]
    pub fn set_params(&mut self, w_c: VFloat<N>, res: VFloat<N>, gain: VFloat<N>) {
        self.set_values(Self::g(w_c), res, gain);
    }

    #[inline]
    fn set_values_smoothed(
        &mut self,
        g: VFloat<N>,
        res: VFloat<N>,
        gain: VFloat<N>,
        inc: VFloat<N>,
    ) {
        self.k.set_target(gain, inc);
        self.g.set_target(g, inc);
        self.r.set_target(res, inc);
    }

    /// Like `Self::set_params_low_shelving` but with smoothing
    #[inline]
    pub fn set_params_low_shelving_smoothed(
        &mut self,
        w_c: VFloat<N>,
        res: VFloat<N>,
        gain: VFloat<N>,
        inc: VFloat<N>,
    ) {
        let m2 = gain.sqrt();
        let g = Self::g(w_c);
        self.set_values_smoothed(g / m2.sqrt(), res, m2, inc);
    }

    /// Like `Self::set_params_band_shelving` but with smoothing
    #[inline]
    pub fn set_params_band_shelving_smoothed(
        &mut self,
        w_c: VFloat<N>,
        res: VFloat<N>,
        gain: VFloat<N>,
        inc: VFloat<N>,
    ) {
        let g = Self::g(w_c);
        self.set_values_smoothed(g, res / gain.sqrt(), gain, inc);
    }

    /// Like `Self::set_params_high_shelving` but with smoothing
    #[inline]
    pub fn set_params_high_shelving_smoothed(
        &mut self,
        w_c: VFloat<N>,
        res: VFloat<N>,
        gain: VFloat<N>,
        inc: VFloat<N>,
    ) {
        let m2 = gain.sqrt();
        let g = Self::g(w_c);
        self.set_values_smoothed(g * m2.sqrt(), res, m2, inc);
    }

    /// Like `Self::set_params_non_shelving` but with smoothing
    #[inline]
    pub fn set_params_smoothed(
        &mut self,
        w_c: VFloat<N>,
        res: VFloat<N>,
        gain: VFloat<N>,
        inc: VFloat<N>,
    ) {
        self.g.set_target(Self::g(w_c), inc);
        self.r.set_target(res, inc);
        self.k.set_all_vals_instantly(gain);
    }

    /// Update the filter's internal parameter smoothers.
    ///
    /// After calling `Self::set_params_<output_type>_smoothed(values, ..., num_samples)` this
    /// function should be called _up to_ `num_samples` times, until, that function is to be
    /// called again, calling this function more than `num_samples` times might result in
    /// the internal parameter states diverging away from the previously set values
    #[inline]
    pub fn tick_all_smoothers(&mut self) {
        self.k.tick1();
        self.r.tick1();
        self.g.tick1();
    }

    #[inline]
    pub fn update_function(mode: FilterMode) -> fn(&mut Self, VFloat<N>, VFloat<N>, VFloat<N>) {
        use FilterMode::*;

        match mode {
            LSH => Self::set_params_low_shelving,
            BSH => Self::set_params_band_shelving,
            HSH => Self::set_params_high_shelving,
            _ => Self::set_params,
        }
    }

    #[inline]
    pub fn smoothing_update_function(
        mode: FilterMode,
    ) -> fn(&mut Self, VFloat<N>, VFloat<N>, VFloat<N>, VFloat<N>) {
        use FilterMode::*;

        match mode {
            LSH => Self::set_params_low_shelving_smoothed,
            BSH => Self::set_params_band_shelving_smoothed,
            HSH => Self::set_params_high_shelving_smoothed,
            _ => Self::set_params_smoothed,
        }
    }
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
    s: [Integrator<N>; 2],
    x: VFloat<N>,
    hp: VFloat<N>,
    bp: VFloat<N>,
    lp: VFloat<N>,
}

impl<const N: usize> SVF<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    #[inline]
    pub fn reset(&mut self) {
        self.s.iter_mut().for_each(Integrator::reset)
    }

    /// Update the filter's internal state.
    ///
    /// This should be called _only once_ per sample, _every sample_
    ///
    /// After calling this, you can get different filter outputs
    /// using `Self::get_{highpass, bandpass, notch, ...}`
    #[inline]
    pub fn process(
        &mut self,
        x: VFloat<N>,
        g: VFloat<N>,
        res: VFloat<N>,
    ) {
        let [&s1, &s2] = self.s.each_ref().map(Integrator::get_current);

        let g1 = res + g;

        self.hp = g1.mul_add(-s1, x - s2) / g1.mul_add(g, Simd::splat(1.));

        let [s1, s2] = &mut self.s;

        self.x = x;
        self.bp = s1.tick(self.hp * g);
        self.lp = s2.tick(self.bp * g);
    }

    #[inline]
    pub fn get_passthrough(&self) -> VFloat<N> {
        self.x
    }

    #[inline]
    pub fn get_lowpass(
        &self,
    ) -> VFloat<N> {
        self.lp
    }

    #[inline]
    pub fn get_bandpass(
        &self,
    ) -> VFloat<N> {
        self.bp
    }

    #[inline]
    pub fn get_unit_bandpass(
        &self,
        res: VFloat<N>,
    ) -> VFloat<N> {
        res * self.bp
    }

    #[inline]
    pub fn get_highpass(
        &self,
    ) -> VFloat<N> {
        self.hp
    }

    #[inline]
    pub fn get_allpass(&self, res: VFloat<N>) -> VFloat<N> {
        // 2 * bp1 - x
        res.mul_add(self.bp + self.bp, -self.x)
    }

    #[inline]
    pub fn get_notch(&self, res: VFloat<N>) -> VFloat<N> {
        // x - bp1
        self.bp.mul_add(-res, self.x)
    }

    #[inline]
    pub fn get_high_shelf(
        &self,
        res: VFloat<N>,
        root_gain: VFloat<N>,
    ) -> VFloat<N> {
        let m2 = root_gain;
        let bp1 = self.get_unit_bandpass(res);
        m2.mul_add(m2.mul_add(self.hp, bp1), self.lp)
    }

    #[inline]
    pub fn get_band_shelf(
        &self,
        res: VFloat<N>,
        root_gain: VFloat<N>,
    ) -> VFloat<N> {
        let bp1 = self.get_unit_bandpass(res);
        bp1.mul_add(root_gain, self.x - bp1)
    }

    #[inline]
    pub fn get_low_shelf(
        &self,
        res: VFloat<N>,
        root_gain: VFloat<N>,
    ) -> VFloat<N> {
        let m2 = root_gain;
        let bp1 = self.get_unit_bandpass(res);
        m2.mul_add(m2.mul_add(self.lp, bp1), self.hp)
    }

    pub fn get_output_function(
        mode: FilterMode,
    ) -> fn(&Self, VFloat<N>, VFloat<N>) -> VFloat<N> {
        use FilterMode::*;

        match mode {
            LP => |f, _res, _g| f.get_lowpass(),
            BP => |f, _res, _g| f.get_bandpass(),
            BP1 => |f, res, _g| f.get_unit_bandpass(res),
            HP => |f, _res, _g| f.get_highpass(),
            AP => |f, res, _g| f.get_allpass(res),
            NCH => |f, res, _g| f.get_notch(res),
            LSH => Self::get_low_shelf,
            BSH => Self::get_band_shelf,
            HSH => Self::get_high_shelf,
        }
    }
}

#[cfg(feature = "transfer_funcs")]
pub mod impedence {

    use super::*;

    pub fn transfer_function<T: Float>(
        filter_mode: FilterMode,
    ) -> fn(Complex<T>, T, T) -> Complex<T> {
        use FilterMode::*;

        match filter_mode {
            // yay function pointer coercions
            LP => |s, r, _g| low_pass(s, r),
            BP => |s, r, _g| band_pass(s, r),
            BP1 => |s, r, _g| unit_band_pass(s, r),
            HP => |s, r, _g| high_pass(s, r),
            AP => |s, r, _g| all_pass(s, r),
            NCH => |s, r, _g| notch(s, r),
            LSH => low_shelf,
            BSH => band_shelf,
            HSH => high_shelf,
        }
    }

    fn two<T: Float>(res: T) -> T {
        res + res
    }

    fn h_denominator<T: Float>(s: Complex<T>, res: T) -> Complex<T> {
        s * (s + two(res)) + T::one()
    }

    pub fn low_pass<T: Float>(s: Complex<T>, res: T) -> Complex<T> {
        h_denominator(s, res).finv()
    }

    pub fn band_pass<T: Float>(s: Complex<T>, res: T) -> Complex<T> {
        s.fdiv(h_denominator(s, res))
    }

    pub fn unit_band_pass<T: Float>(s: Complex<T>, res: T) -> Complex<T> {
        band_pass(s, res).scale(two(res))
    }

    pub fn high_pass<T: Float>(s: Complex<T>, res: T) -> Complex<T> {
        (s * s).fdiv(h_denominator(s, res))
    }

    pub fn all_pass<T: Float>(s: Complex<T>, res: T) -> Complex<T> {
        let bp1 = unit_band_pass(s, res);
        bp1 + bp1 - Complex::one()
    }

    pub fn notch<T: Float>(s: Complex<T>, res: T) -> Complex<T> {
        Complex::<T>::one() - unit_band_pass(s, res)
    }

    pub fn tilting<T: Float>(s: Complex<T>, res: T, gain: T) -> Complex<T> {
        let m2 = gain.sqrt();
        let m = m2.sqrt();
        let sm = s.unscale(m);
        (s * s + sm.scale(two(res)) + m2.recip()).fdiv(h_denominator(sm, res))
    }

    pub fn low_shelf<T: Float>(s: Complex<T>, res: T, gain: T) -> Complex<T> {
        let m2 = gain.sqrt();
        tilting(s, res, gain.recip()).scale(m2)
    }

    pub fn band_shelf<T: Float>(s: Complex<T>, res: T, gain: T) -> Complex<T> {
        let m = gain.sqrt();
        (s * (s + two(res) * m) + T::one()).fdiv(h_denominator(s, res / m))
    }

    pub fn high_shelf<T: Float>(s: Complex<T>, res: T, gain: T) -> Complex<T> {
        let m2 = gain.sqrt();
        tilting(s, res, gain).scale(m2)
    }
}
