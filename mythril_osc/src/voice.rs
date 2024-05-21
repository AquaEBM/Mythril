use super::*;

pub struct VoiceParams {
    pub base_norm_frame: Float,
    pub transpose: Float,
    pub detune: Float,
    pub num_voices: UInt,
    pub base_phase_delta: Float,
}

impl VoiceParams {
    #[inline]
    pub fn new(index: usize, params: &WTOscClusterNormParams) -> Option<(Self, NonZeroUsize)> {
        (index < STEREO_VOICES_PER_VECTOR)
            // SAFETY: i has just been bounds checked
            .then(|| unsafe { Self::new_unchecked(index, params) })
    }

    #[inline]
    pub unsafe fn new_unchecked(
        index: usize,
        params: &WTOscClusterNormParams,
    ) -> (Self, NonZeroUsize) {
        let i = index;

        let norm_detune = split_stereo(&params.detune.current).get_unchecked(i);
        let norm_detune_range = split_stereo(&params.detune_range.current).get_unchecked(i);

        let pitch_range_semitones = Simd::splat(PITCH_RANGE_SEMITONES);

        let detune = norm_detune_range * pitch_range_semitones * norm_detune;
        let norm_transpose = split_stereo(&params.transpose.current).get_unchecked(i);
        let transpose =
            (Simd::splat(2.0) * norm_transpose - Simd::splat(1.0)) * pitch_range_semitones;

        let num_voices = split_stereo(&params.num_voices_f()).get_unchecked(i).cast();

        let fpv = Simd::splat(FLOATS_PER_VECTOR as u32);
        let onex2 = Simd::splat(1);

        // (panic) SAFETY: FLOATS_PER_VECTOR is garanteed to be a non-zero multiple of 2
        let n = num_voices + (num_voices & onex2);
        let num_oscs_stereo = (n + fpv - onex2) / fpv;

        (
            Self {
                base_norm_frame: splat_stereo(
                    *split_stereo(&params.frame.current).get_unchecked(i),
                ),
                transpose: splat_stereo(transpose),
                detune: splat_stereo(detune),
                num_voices: splat_stereo(num_voices),
                base_phase_delta: splat_stereo(*split_stereo(&params.phase_delta).get_unchecked(i)),
            },
            // (panic) SAFETY: num_voices is garanteed to be nonzero
            NonZeroUsize::new(num_oscs_stereo.reduce_max() as usize).unwrap(),
        )
    }

    #[inline]
    pub fn get_params(&self, index: usize) -> (Float, Float, TMask) {
        let one_u = UInt::splat(1);
        let two_u = UInt::splat(2);
        let last_voice_pair_idx =
            UInt::splat((((MAX_UNISON + (MAX_UNISON & 1)) >> 1) - 1).max(1) as u32);
        let last_voice_pair_idx_f = last_voice_pair_idx.cast::<f32>();
        let max_float_bit_index = UInt::splat(mem::size_of::<f32>() as u32 * 8 - 1);
        let counting = UInt::from_array(array::from_fn(|i| i as u32));
        let counting_by2 = counting >> one_u;

        let v_osc_index = UInt::splat((index * FLOATS_PER_VECTOR) as u32);
        let voice_indices = v_osc_index + counting;
        let voice_pair_indices = v_osc_index + counting_by2;
        let sign_mask = (voice_indices ^ voice_pair_indices) << max_float_bit_index;

        let num_voices = self.num_voices;

        let detune_step = (num_voices.simd_max(two_u) - one_u).cast::<f32>().recip();
        let start = (num_voices + one_u) & one_u;
        let abs_norm_detunes = detune_step * (start + (voice_pair_indices << one_u)).cast::<f32>();
        let norm_detunes = Float::from_bits(abs_norm_detunes.to_bits() ^ sign_mask);

        let detune_semitones = self.detune.mul_add(norm_detunes, self.transpose);
        let detune_ratio = semitones_to_ratio(detune_semitones);
        let phase_delta = self.unison_stack_mult(index) * detune_ratio;

        let norm_voice_spread = voice_pair_indices.cast::<f32>() / last_voice_pair_idx_f;

        let norm_frame = norm_voice_spread.mul_add(self.frame_spread(index), self.base_norm_frame);

        let norm_frame_clamped = norm_frame.simd_clamp(Simd::splat(0.0001), Simd::splat(0.9999));

        let mask = Self::get_gather_mask(num_voices + (num_voices & one_u), voice_indices);

        (phase_delta, norm_frame_clamped, mask)
    }

    #[inline]
    fn get_gather_mask(num_voices: UInt, voice_indices: UInt) -> TMask {
        num_voices.simd_gt(voice_indices)
    }

    #[inline]
    fn unison_stack_mult(&self, _index: usize) -> Float {
        Float::splat(1.)
    }

    #[inline]
    fn frame_spread(&self, _index: usize) -> Float {
        Float::splat(0.)
    }
}

#[derive(Default, Clone, Copy)]
pub struct Oscillator {
    phase: UInt,
    frame: LinearSmoother,
    phase_delta: LogSmoother,
}

impl Oscillator {
    #[inline]
    pub fn scale_frame(&mut self, ratio: Float) {
        self.frame.scale(ratio);
    }

    #[inline]
    pub fn scale_phase_delta(&mut self, ratio: Float) {
        self.phase_delta.scale(ratio);
    }

    #[inline]
    pub fn set_phase_delta(&mut self, phase_delta: Float) {
        self.phase_delta.set_all_vals_instantly(phase_delta);
    }

    #[inline]
    pub fn set_phase_delta_smoothed(&mut self, phase_delta: Float, t_recip: Float) {
        self.phase_delta.set_target_recip(phase_delta, t_recip);
    }

    #[inline]
    pub fn set_frame(&mut self, frame: Float) {
        self.frame.set_all_vals_instantly(frame);
    }

    #[inline]
    pub fn set_frame_smoothed(&mut self, frame: Float, t_recip: Float) {
        self.frame.set_target_recip(frame, t_recip);
    }

    #[inline]
    pub fn set_params_smoothed(
        &mut self,
        voice_params: &VoiceParams,
        voice_params_index: usize,
        num_frames_f: Float,
        smooth_dt: Float,
    ) -> TMask {
        let (total_detune, norm_frame, mask) = voice_params.get_params(voice_params_index);

        self.set_frame_smoothed(num_frames_f * norm_frame, smooth_dt);
        self.set_phase_delta_smoothed(voice_params.base_phase_delta * total_detune, smooth_dt);

        mask
    }

    #[inline]
    pub fn set_params(
        &mut self,
        voice_params: &VoiceParams,
        voice_params_index: usize,
        num_frames_f: Float,
    ) {
        let (total_detune, norm_frame, _) = voice_params.get_params(voice_params_index);

        self.set_frame(num_frames_f * norm_frame);
        self.set_phase_delta(voice_params.base_phase_delta * total_detune);
    }

    #[inline]
    pub fn set_phase(&mut self, phase: UInt) {
        self.phase = phase;
    }

    #[inline]
    pub fn tick_smoothers(&mut self) {
        self.frame.tick1();
        self.phase_delta.tick1();
    }

    #[inline]
    pub unsafe fn tick_all(&mut self, table: &BandLimitedWaveTables, mask: TMask) -> Float {
        let w = flp_to_fxp(self.phase_delta.get_current());
        let frame = unsafe { self.frame.get_current().to_int_unchecked() };
        let out = table.resample_select(w, frame, self.phase, mask);
        self.phase += w;
        self.tick_smoothers();

        out
    }
}
