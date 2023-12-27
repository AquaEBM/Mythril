use super::*;
use voice::WTOscVoice;

#[derive(Default)]
pub struct WTOscVoiceCluster {
    active_voices_mask: u64,
    voices: [WTOscVoice ; STEREO_VOICES_PER_VECTOR],
    normal_weights: LinearSmoother,
    flipped_weights: LinearSmoother,
}

impl WTOscVoiceCluster {

    fn get_sample_weights(param_values: &WTOscParamValues, _cluster_idx: usize) -> (Float, Float) {
        let level = param_values.level.get_current();
        let stereo = param_values.stereo.get_current();

        let pan = param_values.pan.get_current();
        let unison_normalisation = param_values.num_unison_voices.cast::<f32>().recip();
        let pan_weights = triangular_pan_weights(pan) * unison_normalisation;

        (
            pan_weights.mul_add(stereo, pan_weights).sqrt() * level,
            pan_weights.mul_add(-stereo, pan_weights).sqrt() * level,
        )
    }

    pub fn set_params_instantly(
        &mut self,
        param_values: &WTOscParamValues,
        cluster_idx: usize
    ) {
        let (normal_weights, flipped_weights) = Self::get_sample_weights(param_values, cluster_idx);

        self.normal_weights.set_instantly(normal_weights);
        self.flipped_weights.set_instantly(flipped_weights);
    }

    pub fn set_params_smoothed(
        &mut self,
        param_values: &WTOscParamValues,
        cluster_idx: usize,
        num_samples: usize
    ) {
        let (normal_weights, flipped_weights) = Self::get_sample_weights(param_values, cluster_idx);

        self.normal_weights.set_target(normal_weights, num_samples);
        self.flipped_weights.set_target(flipped_weights, num_samples);

        self.voices
            .iter_mut()
            .enumerate()
            .for_each(|(i, voice)| voice.set_params_smoothed(param_values, cluster_idx, i, num_samples));
    }
    
    #[inline]
    pub fn process(&mut self, table: &BandLimitedWaveTables) -> Float {

        let mut output = Simd::splat(0.);

        let mut active = self.active_voices_mask;
        let output_samples = as_mut_stereo_sample_array(&mut output);
        let mut output_samples_iter = output_samples.iter_mut().zip(&mut self.voices);

        while active != 0 {

            let n = active.trailing_zeros() as usize;
            
            let (sample, voice) = unsafe { output_samples_iter.nth(n).unwrap_unchecked() };
            
            *sample = voice.process(table);
            active >>= n + 1;
        }

        let flipped = swap_stereo(output);

        self.normal_weights.tick();
        self.flipped_weights.tick();

        output = self.normal_weights.get_current() * output + self.flipped_weights.get_current() * flipped;

        output
    }

    pub fn reset(&mut self) {
        self.voices.iter_mut().for_each(WTOscVoice::reset)
    }

    pub fn activate_voice(
        &mut self,
        param_values: &WTOscParamValues,
        cluster_idx: usize,
        voice_idx: usize,
        note: u8
    ) -> bool {
        if let Some(voice) = self.voices.get_mut(voice_idx) {
            voice.activate(param_values, cluster_idx, voice_idx, note);
            self.active_voices_mask |= 1 << voice_idx;
            return true;
        }
        false
    }

    pub fn deactivate_voice(&mut self, index: usize) -> bool {
        if let Some(voice) = self.voices.get_mut(index) {
            voice.deactivate();
            self.active_voices_mask &= !(1 << index);
            return true;
        }
        false
    }

    pub fn activate(&mut self, param_values: &WTOscParamValues, index: usize) {
        self.set_params_instantly(param_values, index);
    }

    pub fn deactivate(&mut self) {}
}