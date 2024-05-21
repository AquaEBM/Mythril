use super::*;
use cell_project::cell_project as cp;
use voice::Oscillator;

/// # Safety
/// Both `from` and `to` must be `< STEREO_VOICES_PER_VECTOR`
#[inline]
unsafe fn swap_index_cell_unchecked<T>(
    this: &Cell<[T; STEREO_VOICES_PER_VECTOR]>,
    from: usize,
    other: &Cell<[T; STEREO_VOICES_PER_VECTOR]>,
    to: usize,
) {
    Cell::swap(
        this.as_array_of_cells().get_unchecked(to),
        other.as_array_of_cells().get_unchecked(from),
    );
}

unsafe fn permute_smoother_values(
    this: &Cell<GenericSmoother>,
    from: usize,
    other: &Cell<GenericSmoother>,
    to: usize,
) {
    type L = GenericSmoother;

    let this_current_vals = split_stereo_cell(cp!(L, this.current));
    let other_current_vals = split_stereo_cell(cp!(L, other.current));
    let this_target_vals = split_stereo_cell(cp!(L, this.target));
    let other_target_vals = split_stereo_cell(cp!(L, other.target));

    swap_index_cell_unchecked(this_current_vals, from, other_current_vals, to);
    swap_index_cell_unchecked(this_target_vals, from, other_target_vals, to);
}

pub struct WTOscClusterNormParams {
    level: GenericSmoother,
    pub frame: GenericSmoother,
    pub num_voices: GenericSmoother,
    pub detune: GenericSmoother,
    pan: GenericSmoother,
    pub transpose: GenericSmoother,
    stereo: GenericSmoother,
    pub detune_range: GenericSmoother,
    pub random: GenericSmoother,
    pub phase_delta: Float,
}

impl Default for WTOscClusterNormParams {
    fn default() -> Self {
        let mut out = Self {
            level: Default::default(),
            frame: Default::default(),
            num_voices: Default::default(),
            detune: Default::default(),
            pan: Default::default(),
            transpose: Default::default(),
            stereo: Default::default(),
            detune_range: Default::default(),
            random: Default::default(),
            phase_delta: Default::default(),
        };

        let all_voices = TMask::splat(true);

        for (i, value) in DEFAULT_PARAMS.iter().copied().map(splat_stereo).enumerate() {
            out.set_param_instantly(i as u64, value, all_voices);
        }

        out
    }
}

impl WTOscClusterNormParams {
    #[inline]
    pub fn tick_n(&mut self, log2_alpha: f32, n: usize) {
        let alpha = Simd::splat(exp2(Simd::from_array([log2_alpha * n as f32]))[0]);
        self.level.smooth_exp(alpha);
        self.frame.smooth_exp(alpha);
        self.num_voices.smooth_exp(alpha);
        self.detune.smooth_exp(alpha);
        self.pan.smooth_exp(alpha);
        self.transpose.smooth_exp(alpha);
        self.stereo.smooth_exp(alpha);
        self.detune_range.smooth_exp(alpha);
        self.random.smooth_exp(alpha);
    }

    #[inline]
    #[allow(dead_code)]
    pub fn move_state(this: &Cell<Self>, from: usize, other: &Cell<Self>, to: usize) {
        if from < STEREO_VOICES_PER_VECTOR && to < STEREO_VOICES_PER_VECTOR {
            // SAFETY: `from and `to` have just been bounds checked
            unsafe { Self::move_state_unchecked(this, from, other, to) }
        }
    }

    #[inline]
    pub unsafe fn move_state_unchecked(
        this: &Cell<Self>,
        from: usize,
        other: &Cell<Self>,
        to: usize,
    ) {
        for (input, output) in [
            (cp!(Self, this.level), cp!(Self, other.level)),
            (cp!(Self, this.frame), cp!(Self, other.frame)),
            (cp!(Self, this.num_voices), cp!(Self, other.num_voices)),
            (cp!(Self, this.detune), cp!(Self, other.detune)),
            (cp!(Self, this.pan), cp!(Self, other.pan)),
            (cp!(Self, this.transpose), cp!(Self, other.transpose)),
            (cp!(Self, this.stereo), cp!(Self, other.stereo)),
            (cp!(Self, this.detune_range), cp!(Self, other.detune_range)),
            (cp!(Self, this.random), cp!(Self, other.random)),
        ] {
            permute_smoother_values(input, from, output, to);
        }

        swap_index_cell_unchecked(
            split_stereo_cell(cp!(Self, this.phase_delta)),
            from,
            split_stereo_cell(cp!(Self, other.phase_delta)),
            to,
        );
    }

    #[inline]
    pub fn get_param_smoother_mut(&mut self, param_id: u64) -> &mut GenericSmoother {
        match param_id {
            0 => &mut self.level,
            1 => &mut self.frame,
            2 => &mut self.num_voices,
            3 => &mut self.detune,
            4 => &mut self.pan,
            5 => &mut self.transpose,
            6 => &mut self.stereo,
            7 => &mut self.detune_range,
            8 => &mut self.random,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn num_voices_from_norm(norm_val: Float) -> Float {
        norm_val.mul_add(Simd::splat(15.998), Simd::splat(1.001))
    }

    #[inline]
    pub fn num_voices_f(&self) -> Float {
        Self::num_voices_from_norm(self.num_voices.current)
    }

    #[inline]
    pub fn set_base_phase_delta(&mut self, w: Float, voice_mask: TMask) {
        self.phase_delta = voice_mask.select(w, self.phase_delta);
    }

    #[inline]
    pub fn set_param_target(&mut self, param_id: u64, norm_val: Float, voice_mask: TMask) {
        match param_id {
            0..=MAX_PARAM_INDEX => {
                let smoother = self.get_param_smoother_mut(param_id);
                smoother.set_target(norm_val, voice_mask);
            }
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn set_param_instantly(&mut self, param_id: u64, norm_val: Float, voice_mask: TMask) {
        match param_id {
            0..=MAX_PARAM_INDEX => {
                let smoother = self.get_param_smoother_mut(param_id);
                smoother.set_val_instantly(norm_val, voice_mask);
            }
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn get_sample_weights(&self) -> (Float, Float) {
        let norm_level = self.level.current;
        let level = norm_level * norm_level;

        let stereo = self.stereo.current;
        let pan = self.pan.current;

        let unison_normalisation = self.num_voices_f().recip();
        let pan_weights = triangular_pan_weights(pan) * unison_normalisation;

        (
            pan_weights.mul_add(stereo, pan_weights).sqrt() * level,
            pan_weights.mul_add(-stereo, pan_weights).sqrt() * level,
        )
    }
}

#[derive(Default, Clone, Copy)]
pub struct WTOscVoiceCluster {
    active_voice_mask: u8,
    voices: [[Oscillator; OSCS_PER_VOICE]; STEREO_VOICES_PER_VECTOR],
    normal_weights: LinearSmoother,
    flipped_weights: LinearSmoother,
}

impl WTOscVoiceCluster {
    #[inline]
    pub fn active_voices(&mut self) -> impl Iterator<Item = (usize, &mut [Oscillator ; OSCS_PER_VOICE])> {
        let mut voices = self.voices.iter_mut();
        let mut mask = self.active_voice_mask;
        let mut i = 0;
        iter::from_fn(move || {
            let n = mask.trailing_zeros() as usize;
            i += n;
            mask >>= n;
            voices.nth(n).map(|voice| (i, voice))
        })
    }

    #[inline]
    pub fn voices_mut(&mut self) -> &mut [[Oscillator; OSCS_PER_VOICE]; STEREO_VOICES_PER_VECTOR] {
        &mut self.voices
    }

    #[inline]
    pub fn get_sample_weights(&self) -> (Float, Float) {
        (
            self.normal_weights.get_current(),
            self.flipped_weights.get_current(),
        )
    }

    #[inline]
    pub fn tick_weight_smoothers(&mut self) {
        self.normal_weights.tick1();
        self.flipped_weights.tick1();
    }

    #[inline]
    pub fn set_weights(&mut self, params: &WTOscClusterNormParams, voice_mask: TMask) {
        let (normal, flipped) = params.get_sample_weights();
        self.normal_weights.set_val_instantly(normal, voice_mask);
        self.flipped_weights.set_val_instantly(flipped, voice_mask);
    }

    #[inline]
    pub fn set_weights_smoothed(&mut self, params: &WTOscClusterNormParams, smooth_dt: Float) {
        let (normal, flipped) = params.get_sample_weights();
        self.normal_weights.set_target_recip(normal, smooth_dt);
        self.flipped_weights.set_target_recip(flipped, smooth_dt);
    }

    #[inline]
    pub fn scale_frames(&mut self, ratio: Float) {
        for oscs in self.voices.iter_mut() {
            for osc in oscs {
                osc.scale_frame(ratio);
            }
        }
    }

    #[inline]
    pub fn scale_phase_deltas(&mut self, ratio: Float) {
        for oscs in self.voices.iter_mut() {
            for osc in oscs {
                osc.scale_phase_delta(ratio);
            }
        }
    }

    #[inline]
    pub fn set_params(
        &mut self,
        params: &WTOscClusterNormParams,
        num_frames_f: Float,
        voice_mask: TMask,
    ) {
        self.set_weights(params, voice_mask);
        for (i, oscs) in self
            .voices
            .iter_mut()
            .enumerate()
            .zip(voice_mask.to_array().into_iter().step_by(2))
            .filter_map(|(data, active)| active.then_some(data))
        {
            let (voice_params, num_oscs) = unsafe { VoiceParams::new_unchecked(i, params) };
            let active_oscs = unsafe { oscs.get_unchecked_mut(0..num_oscs.get()) };
            for (j, osc) in active_oscs.iter_mut().enumerate() {
                osc.set_params(&voice_params, j, num_frames_f);
            }
        }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn move_state(this: &Cell<Self>, from: usize, other: &Cell<Self>, to: usize) {
        if from < STEREO_VOICES_PER_VECTOR && to < STEREO_VOICES_PER_VECTOR {
            // SAFETY: `from and `to` have just been bounds checked
            unsafe { Self::move_state_unchecked(this, from, other, to) }
        }
    }

    #[inline]
    pub unsafe fn move_state_unchecked(
        this: &Cell<Self>,
        from: usize,
        other: &Cell<Self>,
        to: usize,
    ) {
        type L = LinearSmoother;

        let tf = cp!(Self, this.flipped_weights);
        let of = cp!(Self, other.flipped_weights);

        swap_index_cell_unchecked(
            split_stereo_cell(cp!(L, tf.value)),
            from,
            split_stereo_cell(cp!(L, of.value)),
            to,
        );

        swap_index_cell_unchecked(
            split_stereo_cell(cp!(L, tf.increment)),
            from,
            split_stereo_cell(cp!(L, of.increment)),
            to,
        );

        let tn = cp!(Self, this.normal_weights);
        let on = cp!(Self, other.normal_weights);

        swap_index_cell_unchecked(
            split_stereo_cell(cp!(L, tn.value)),
            from,
            split_stereo_cell(cp!(L, on.value)),
            to,
        );

        swap_index_cell_unchecked(
            split_stereo_cell(cp!(L, tn.increment)),
            from,
            split_stereo_cell(cp!(L, on.increment)),
            to,
        );

        let this_voice = cp!(Self, this.voices);
        let other_voice = cp!(Self, other.voices);

        swap_index_cell_unchecked(this_voice, from, other_voice, to);
    }

    #[inline]
    pub fn reset_phases(
        &mut self,
        voice_mask: TMask,
        randomisation: Float,
        starting_phases: &[Float; OSCS_PER_VOICE],
    ) {
        for (voice, &random) in self
            .voices
            .iter_mut()
            .zip(split_stereo(&randomisation))
            .zip(voice_mask.to_array().into_iter().step_by(2))
            .filter_map(|(data, active)| active.then_some(data))
        {
            let random = splat_stereo(random);
            for (osc, starting_phase) in voice.iter_mut().zip(starting_phases) {
                osc.set_phase(flp_to_fxp(starting_phase * random));
            }
        }
    }
}
