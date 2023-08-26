use super::*;
use arrayvec::ArrayVec;
use voice::WTOscVoice;
use util::{triangular_pan_weights, swap_stereo};

#[derive(Default)]
pub struct WTOscVoiceCluster {
    voices: ArrayVec<WTOscVoice, VOICES_PER_VECTOR>,
    normal_weights: LinearSmoother,
    flipped_weights: LinearSmoother,
}

impl WTOscVoiceCluster {

    fn get_sample_weights(param_values: &WTOscParamValues, cluster_idx: usize) -> (Float, Float) {
        let level = param_values.level.get_current() * param_values.num_unison_voices.cast().recip().sqrt();
        let stereo = param_values.stereo.get_current();

        let pan = param_values.pan.get_current();
        let pan_weights = triangular_pan_weights(pan);

        (
            pan_weights.mul_add(stereo, pan_weights).sqrt() * level,
            pan_weights.mul_add(-stereo, pan_weights).sqrt() * level,
        )
    }

    pub fn from_param_values(param_values: &WTOscParamValues, cluster_idx: usize) -> Self {

        let mut output = Self::default();

        let (normal_weights, flipped_weights) = Self::get_sample_weights(param_values, cluster_idx);

        output.normal_weights.set_instantly(normal_weights);
        output.flipped_weights.set_instantly(flipped_weights);

        output
    }

    pub fn update_smoothers(
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
            .for_each(|(i, voice)| voice.update_smoothers(param_values, cluster_idx, i, num_samples));
    }

    pub fn process(&mut self, table: &BandLimitedWaveTables) -> Float {

        let mut output = Simd::splat(0.);

        self.voices.iter_mut()
            .zip(as_mut_stereo_sample_array(&mut output))
            .for_each(|(voice, sample)| *sample = voice.process(table));

        let flipped = swap_stereo(output);

        self.normal_weights.tick();
        self.flipped_weights.tick();

        self.normal_weights.get_current() * output + self.flipped_weights.get_current() * flipped
    }

    pub fn reset(&mut self) {
        self.voices.iter_mut().for_each(WTOscVoice::reset)
    }

    pub fn push_voice(
        &mut self,
        param_values: &WTOscParamValues,
        cluster_idx: usize,
        note: u8
    ) -> Option<()> {
        (!self.voices.is_full()).then(|| {

            let voice = WTOscVoice::from_param_values(
                param_values,
                note,
                cluster_idx,
                self.voices.len()
            );
            self.voices.push(voice);
        })
    }

    pub fn remove_voice(&mut self, index: usize) -> Option<()> {
        self.voices.swap_pop(index).and(Some(()))
    }
}