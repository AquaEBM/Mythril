use super::*;
use std::{mem::transmute, iter, array};
use oscillator::Oscillator;

pub const MAX_UNISON: usize = 16;
const NUM_UNISON_VECTORS: usize = enclosing_div(MAX_UNISON, MAX_VECTOR_WIDTH);

static UNISON_DETUNES: [[Float ; NUM_UNISON_VECTORS] ; MAX_UNISON + 1] = {

    assert!(MAX_VECTOR_WIDTH >= 2);

    let mut blocks = [[0. ; NUM_UNISON_VECTORS * MAX_VECTOR_WIDTH] ; MAX_UNISON + 1];

    /// sign_mask: 0. or -0.
    const fn const_copysign(x: f32, sign_mask: f32) -> f32 {
        f32::from_bits(x.to_bits() | sign_mask.to_bits())
    }

    let mut i = 2;
    while i < MAX_UNISON + 1 {

        let mut j = 0;
        let mut sign_mask = 0.;

        let step = 2. / (i - 1) as f32;
        let num_voices = i + i % 2; // next even number

        let remainder_voices = (num_voices - 1) % MAX_VECTOR_WIDTH + 1;
        let empty_voices = MAX_VECTOR_WIDTH - remainder_voices;

        while j < num_voices / 2 {

            let detune = const_copysign(1. - step * j as f32, sign_mask);

            let offset = if j + remainder_voices / 2 < num_voices / 2 {
                empty_voices
            } else {
                0
            };

            blocks[i][num_voices - j * 2 - 1 + offset] = detune;
            blocks[i][num_voices - j * 2 - 2 + offset] = -detune;

            j += 1;
            sign_mask = -sign_mask;
        }

        i += 1;
    }

    // SAFETY: we're transmuting f32s to Simd<f32, N>s so values are valid
    unsafe { transmute(blocks) }
};


#[derive(Default)]
pub struct WTOscVoice {
    center_osc: Oscillator,
    detuned_oscs: [Oscillator ; NUM_UNISON_VECTORS - 1],
    num_detuned_oscs: usize,
    remainder_mask: TMask,
}

impl WTOscVoice {

    fn get_and_splat_slot<T: SimdElement>(
        vector: &Simd<T, MAX_VECTOR_WIDTH>,
        index: usize
    ) -> Simd<T, MAX_VECTOR_WIDTH> {

        let array = as_stereo_sample_array(vector);

        let slot = unsafe { array.get_unchecked(index) };

        splat_stereo(*slot)
    }

    fn get_voice_params(
        param_values: &WTOscParamValues,
        cluster_idx: usize,
        voice_idx: usize
    ) -> (usize, UInt, Float, Float) {

        // invariant: all values are in the range 1..=MAX_UNISON
        let voices_num_array = param_values.num_unison_voices.as_array();

        let num = *unsafe { voices_num_array.get_unchecked(2 * voice_idx) } as usize;

        (
            num,
            Self::get_and_splat_slot(&param_values.frame, voice_idx),
            Self::get_and_splat_slot(&param_values.transpose.get_current(), voice_idx),
            Self::get_and_splat_slot(&param_values.detune.get_current(), voice_idx),
        )
    }

    pub fn from_param_values(
        param_values: &WTOscParamValues,
        note: u8,
        cluster_idx: usize,
        voice_idx: usize
    ) -> Self {

        let mut output = Self::default();

        let (num_unison_voices, frame, transpose, detune) = Self::get_voice_params(
            param_values,
            cluster_idx,
            voice_idx
        );

        let randomisation_factor = Self::get_and_splat_slot(&param_values.random.get_current(), voice_idx);
        let base_phase_delta = Float::splat(nih_plug::util::midi_note_to_freq(note) / param_values.sr);

        let norm_detunes = output.set_num_unison_voices(num_unison_voices);

        output.all_oscillators().zip(norm_detunes.iter()).for_each(|(osc, norm_detune)| {
            osc.base_phase_delta = base_phase_delta;
            osc.set_frame(frame);
            osc.set_detune_semitones(norm_detune.mul_add(detune, transpose));
            osc.randomize_phase(randomisation_factor);
        });

        output
    }

    pub fn update_smoothers(
        &mut self,
        param_values: &WTOscParamValues,
        cluster_idx: usize,
        voice_idx: usize,
        num_samples: usize
    ) {
        
        let (num_unison_voices, frame, transpose, detune) = Self::get_voice_params(
            param_values,
            cluster_idx,
            voice_idx
        );

        let norm_detunes = self.set_num_unison_voices(num_unison_voices);

        self.all_oscillators().zip(norm_detunes.iter()).for_each(|(osc, norm_detune)| {
            osc.set_frame_for_smoothing(frame);
            osc.set_detune_semitones_smoothed(norm_detune.mul_add(detune, transpose), num_samples);
        });
    }

    fn all_oscillators(&mut self) -> impl Iterator<Item = &mut Oscillator> {
        let (center, detuned) = (&mut self.center_osc, &mut self.detuned_oscs);
        iter::once(center).chain(detuned.iter_mut())
    }

    pub fn process(&mut self, table: &BandLimitedWaveTables) -> f32x2 {

        let mut samples = self.center_osc.advance_and_resample_select(table, self.remainder_mask);

        self.detuned_oscillators()
            .iter_mut()
            .for_each(|osc| samples += osc.advance_and_resample(table));

        sum_to_stereo_sample(samples)
    }

    fn set_num_unison_voices(&mut self, num: usize) -> &'static[Float] {
        let n = num + (num & 1);

        let num_vectors = enclosing_div(n, MAX_VECTOR_WIDTH);

        self.num_detuned_oscs = num_vectors - 1;

        let rem = (n - 1) % MAX_VECTOR_WIDTH + 1;
        self.remainder_mask = TMask::from_array(array::from_fn(|i| i < rem));

        unsafe {
            UNISON_DETUNES
                .get_unchecked(num)
                .get_unchecked(..num_vectors)
        }
    }

    pub fn reset(&mut self) {

        self.all_oscillators().for_each(Oscillator::reset_phase)
    }

    fn detuned_oscillators(&mut self) -> &mut [Oscillator] {
        unsafe { self.detuned_oscs.get_unchecked_mut(..self.num_detuned_oscs) }
    }
}