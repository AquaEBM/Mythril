use super::{*, wavetable::{BandLimitedWaveTables, LenderReciever}};
use std::{array, mem::transmute, cmp::Ordering, iter};
use arrayvec::ArrayVec;
use nih_plug::prelude::Param;
use params::WTOscParams;
use rand::random;
use smoothing::*;

// TODO: implement unison center/detuned voice blending

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

pub fn semitones_to_ratio(semitones: Float) -> Float {
    const RATIO: Float = const_splat(1. / 12.);
    exp2(semitones * RATIO)
}

pub fn splat_per_voice_samples<T: SimdElement>(
    data: &Simd<T, MAX_VECTOR_WIDTH>
) -> impl Iterator<Item = Simd<T, MAX_VECTOR_WIDTH>> + '_
{
    as_stereo_sample_array(data).iter().copied().map(splat_stereo)
}

/// circular panning of a vector of stereo samples, 0 < pan <= 1
pub fn triangular_pan_weights(pan: Float) -> Float {

    let sign_mask: Float = {
        let mut array = [0. ; MAX_VECTOR_WIDTH];
        let mut i = 0;
        while i < MAX_VECTOR_WIDTH {
            array[i] = -0.;
            i += 2;
        }
        Simd::from_array(array)
    };

    let alternating_onef: Float = {
        let mut array = [0. ; MAX_VECTOR_WIDTH];
        let mut i = 0;
        while i < MAX_VECTOR_WIDTH {
            array[i] = 1.;
            i += 2;
        }
        Simd::from_array(array)
    };

    Float::from_bits(pan.to_bits() ^ sign_mask.to_bits()) + alternating_onef
}

#[derive(Default)]
pub struct Oscillator {
    /// phase delta before unison detuning, pitch bend (coming soon lol), transposition
    base_phase_delta: Float,
    phase_delta: LogSmoother<MAX_VECTOR_WIDTH>,
    phase: UInt,
    old_frame: UInt,
    new_frame: UInt,
}

impl Oscillator {
    pub fn advance_phase(&mut self) -> UInt {

        let phase_delta_fixed_point = flp_to_fxp(*self.phase_delta);

        self.phase += phase_delta_fixed_point;

        phase_delta_fixed_point
    }

    /// 0 <= start <= end < MAX_VECTOR_WIDTH
    pub fn randomize_phase(&mut self, randomisation: Float, start: usize, end: usize) {

        let mut phase = Simd::splat(0.);
        unsafe { phase.as_mut_array().get_unchecked_mut(start..end) } 
            .iter_mut()
            .for_each( |value| *value = random());

        let fixed_phase = flp_to_fxp(phase * randomisation);

        unsafe { fixed_phase.as_array().get_unchecked(start..end) }
            .iter()
            .zip(unsafe { self.phase.as_mut_array().get_unchecked_mut(start..end) })
            .for_each( |(&input, output)| *output = input)
    }

    pub fn update_phase_delta_smoother(&mut self) {
        self.phase_delta.tick()
    }

    pub fn reset_phase(&mut self) {
        self.phase = Default::default();
    }

    pub fn set_detune_semitones_smoothed(&mut self, semitones: Float, block_len: usize) {
        let detune_ratio = semitones_to_ratio(semitones);
        self.phase_delta.set_target(self.base_phase_delta * detune_ratio, block_len);
    }

    pub fn set_frame(&mut self, frame: UInt) {
        self.old_frame = self.new_frame;
        self.new_frame = frame;
    }

    pub fn advance_and_resample_select(&mut self, table: &BandLimitedWaveTables, mask: TMask) -> Float {
        self.update_phase_delta_smoother();
        let phase_delta = self.advance_phase();
        table.resample_select(phase_delta, self.new_frame, self.phase, mask)
    }

    pub fn advance_and_resample(&mut self, table: &BandLimitedWaveTables) -> Float {
        self.update_phase_delta_smoother();
        let phase_delta = self.advance_phase();
        table.resample(phase_delta, self.new_frame, self.phase)
    }
}

#[derive(Default)]
pub struct WaveTableOscVoice {
    center_osc: Oscillator,
    detuned_oscs: ArrayVec<Oscillator, {NUM_UNISON_VECTORS - 1}>,
    mask: TMask,
    randomisation: Float,
    num_unison_voices: usize,
}

impl WaveTableOscVoice {

    pub fn process(&mut self, table: &BandLimitedWaveTables) -> f32x2 {

        let mut voice_samples = self.center_osc.advance_and_resample_select(table, self.mask);

        self.detuned_oscs.iter_mut().for_each(
            |osc| voice_samples += osc.advance_and_resample(table)
        );

        sum_to_stereo_sample(voice_samples)
    }

    pub fn reset(&mut self) {

        iter::once(&mut self.center_osc)
            .chain(self.detuned_oscs.iter_mut())
            .for_each(Oscillator::reset_phase);
    }

    /// num >= 1
    pub fn set_num_unison_voices(&mut self, num: usize) {
        let diff = self.num_unison_voices as isize - num as isize;
        
    }
}

pub fn flip_pairs(v: Float) -> Float {
    const FLIP_PAIRS: [usize ; MAX_VECTOR_WIDTH] = {

        let mut array = [0 ; MAX_VECTOR_WIDTH];

        let mut i = 0;
        while i < MAX_VECTOR_WIDTH {

            array[i] = i ^ 1;
            i += 1;
        }
        array
    };
 
    simd_swizzle!(v, FLIP_PAIRS)
}

pub struct WTOscVoice {

    table: LenderReciever<BandLimitedWaveTables>,
    normalize: Float,
    voices: ArrayVec<WaveTableOscVoice, VOICES_PER_VECTOR>,
    normal_weights: LinearSmoother<MAX_VECTOR_WIDTH>,
    flipped_weights: LinearSmoother<MAX_VECTOR_WIDTH>,
}

impl WTOscVoice {

    pub fn from_table_lender(table: LenderReciever<BandLimitedWaveTables>) -> Self {

        Self {

            voices: Default::default(),
            normalize: Simd::splat(1.),
            normal_weights: Default::default(),
            flipped_weights: Default::default(),
            table
        }
    }

    pub fn process(&mut self) -> Float {

        let mut output = Simd::splat(0.);

        self.voices
            .iter_mut()
            .zip(as_mut_stereo_sample_array(&mut output))
            .for_each(|(voice, sample)| *sample = voice.process(&self.table));

        let flipped = flip_pairs(output);

        self.normal_weights.tick();
        self.flipped_weights.tick();

        *self.normal_weights * output + *self.flipped_weights * flipped
    }

    pub fn is_full(&self) -> bool {
        self.voices.is_full()
    }

    pub fn is_empty(&self) -> bool {
        self.voices.is_empty()
    }

    pub fn add_voice(&mut self, note_id: u8, sample_rate: f32) {
    }

    pub fn remove_voice(&mut self, note: u8) -> bool {

        false
    }

    pub fn reset(&mut self) {
        self.voices.iter_mut().for_each(WaveTableOscVoice::reset)
    }

    pub fn update_smoothers(&mut self, params: &WTOscParams, block_len: usize) {

        self.table.update_item();
        
        let detune = Simd::splat(params.detune.unmodulated_plain_value()) * Simd::splat(params.detune_range.unmodulated_normalized_value());

        let transpose = Simd::splat(params.transpose.unmodulated_plain_value());

        let frame = Simd::splat(params.frame.unmodulated_plain_value() as u32);

        let random = Simd::splat(params.random.unmodulated_plain_value());

        let voices = params.num_unison_voices.unmodulated_plain_value() as usize;

        let n = voices + (voices & 1);
        self.normalize = ONE_F / Simd::splat(n as f32).sqrt();
        
        let remainder = (n - 1) % MAX_VECTOR_WIDTH + 1;
        let num_full_vectors = (n - 1) / MAX_VECTOR_WIDTH;

        let remainder_mask = TMask::from_array(array::from_fn(|i| i < remainder));
        let norm_detunes = &UNISON_DETUNES[voices][..num_full_vectors];

        self.voices.iter_mut()
            .zip(splat_per_voice_samples(&detune))
            .zip(splat_per_voice_samples(&transpose))
            .zip(splat_per_voice_samples(&frame))
            .zip(splat_per_voice_samples(&random))
            .for_each( |((((voice, detune), transpose), frame), random)| {

                voice.mask = remainder_mask;
                voice.randomisation = random;

                voice.detuned_oscs
                    .iter_mut()
                    .zip(norm_detunes.iter())
                    .for_each( |(osc, norm_detune)| {

                        let total_detune_semitones = norm_detune.mul_add(detune, transpose);
                        osc.set_detune_semitones_smoothed(total_detune_semitones, block_len);
                        osc.set_frame(frame);
                    })
        });

        let level = Simd::splat(params.level.unmodulated_plain_value()) * self.normalize;
        let stereo = Simd::splat(params.stereo_unison.unmodulated_plain_value());

        let pan = Simd::splat(params.pan.unmodulated_plain_value());
        let pan_weights = triangular_pan_weights(pan);

        self.normal_weights.set_target(
            pan_weights.mul_add(stereo, pan_weights).sqrt() * level,
            block_len
        );

        self.flipped_weights.set_target(
            pan_weights.mul_add(-stereo, pan_weights).sqrt() * level,
            block_len
        );
    }
}