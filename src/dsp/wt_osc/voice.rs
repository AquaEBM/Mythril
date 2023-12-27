use super::*;
use std::{mem::transmute, array};
use oscillator::Oscillator;

pub const MAX_UNISON: usize = 16;
const NUM_VOICE_OSCILLATORS: usize = enclosing_div(MAX_UNISON, FLOATS_PER_VECTOR);
pub const NUM_DETUNED_OSCILLATORS: usize = NUM_VOICE_OSCILLATORS - 1;

static UNISON_DETUNES: [[Float ; NUM_VOICE_OSCILLATORS] ; MAX_UNISON + 1] = {

    assert!(FLOATS_PER_VECTOR >= 2);

    let mut blocks = [[0. ; NUM_VOICE_OSCILLATORS * FLOATS_PER_VECTOR] ; MAX_UNISON + 1];

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

        let remainder_voices = (num_voices - 1) % FLOATS_PER_VECTOR + 1;
        let empty_voices = FLOATS_PER_VECTOR - remainder_voices;

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
    oscs: CenterDetuned<Oscillator>,
    num_detuned_oscs: usize,
    remainder_mask: Mask,
}

impl WTOscVoice {

    fn get_voice_params(
        param_values: &WTOscParamValues,
        _cluster_idx: usize,
        voice_idx: usize,
    ) -> (usize, UInt, Float, Float) {

        // invariant: all values are in the range 1..=MAX_UNISON
        let voices_num_array = param_values.num_unison_voices.as_array();

        (
            *unsafe { voices_num_array.get_unchecked(voice_idx) } as usize ,
            splat_slot(&param_values.frame, voice_idx),
            splat_slot(&param_values.transpose.get_current(), voice_idx),
            splat_slot(&param_values.detune.get_current(), voice_idx),
        )
    }

    fn initialize(
        &mut self,
        base_phase_delta: Float,
        randomisation: Float,
        phases: &CenterDetuned<Float>
    ) {
        self.oscs
            .all_mut()
            .zip(phases.all())
            .for_each(|(osc, phase)| {
                osc.set_phase(flp_to_fxp(phase * randomisation));
                osc.base_phase_delta = base_phase_delta;
            } );
    }

    pub fn deactivate(&mut self) {}

    pub fn activate(
        &mut self,
        param_values: &WTOscParamValues,
        cluster_idx: usize,
        voice_idx: usize,
        note: u8,
    ) {
        let randomisation = splat_slot(&param_values.random.get_current(), voice_idx);
        let base_phase_delta = Float::splat(nih_plug::util::midi_note_to_freq(note) / param_values.sr);
        let phases = &param_values.starting_phases;

        self.initialize(base_phase_delta, randomisation, phases);

        self.set_params_instantly(param_values, cluster_idx, voice_idx);
    }

    pub fn set_params_instantly(
        &mut self,
        param_values: &WTOscParamValues,
        cluster_idx: usize,
        voice_idx: usize,
    ) {
        let (num_unison_voices, frame, transpose, detune) = Self::get_voice_params(
            param_values,
            cluster_idx,
            voice_idx
        );

        let norm_detunes = self.set_num_unison_voices(num_unison_voices);

        self.oscs.all_mut()
            .zip(norm_detunes.iter())
            .for_each(|(osc, norm_detune)| {
                osc.set_frame(frame);
                osc.set_detune_semitones(norm_detune.mul_add(detune, transpose));
            });
    }

    pub fn set_params_smoothed(
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

        self.oscs.all_mut().zip(norm_detunes.iter()).for_each(|(osc, norm_detune)| {
            osc.set_frame_for_smoothing(frame);
            osc.set_detune_semitones_smoothed(norm_detune.mul_add(detune, transpose), num_samples);
        });
    }

    #[inline]
    pub fn process(&mut self, table: &BandLimitedWaveTables) -> f32x2 {

        let mut samples = self.oscs.center.advance_and_resample_select(table, self.remainder_mask);

        self.oscs.detuned
            .iter_mut()
            .for_each(|osc| samples += osc.advance_and_resample(table));

        sum_to_stereo_sample(samples)
    }

    fn set_num_unison_voices(&mut self, num: usize) -> &'static [Float] {
        let n = num + (num & 1);

        let num_vectors = enclosing_div(n, FLOATS_PER_VECTOR);

        self.num_detuned_oscs = num_vectors - 1;

        let rem = (n - 1) % FLOATS_PER_VECTOR + 1;
        self.remainder_mask = Mask::from_array(array::from_fn(|i| i < rem));

        unsafe {
            UNISON_DETUNES
                .get_unchecked(num)
                .get_unchecked(..num_vectors)
        }
    }

    pub fn reset(&mut self) {

        self.oscs.all_mut().for_each(Oscillator::reset_phase)
    }
}