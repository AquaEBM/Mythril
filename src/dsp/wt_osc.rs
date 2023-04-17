use std::ops::{Deref, DerefMut};

use super::{*, wavetable::BandLimitedWaveTables};
use arrayvec::ArrayVec;
use nih_plug::{prelude::Param, util};
use params::WTOscParams;
use rand::random;

pub const MAX_UNISON: usize = 16;
const NUM_UNISON_VECTORS: usize = MAX_UNISON / VECTOR_WIDTH;

const UNISON_DETUNE: [[Float ; NUM_UNISON_VECTORS] ; MAX_UNISON + 1] = {

    assert!(VECTOR_WIDTH >= 2);
    assert!(VECTOR_WIDTH.is_power_of_two());
    assert!(MAX_UNISON % VECTOR_WIDTH == 0);
    assert!(MAX_POLYPHONY % VECTOR_WIDTH == 0);
 
    let mut array = [0. ; MAX_UNISON];
    let mut blocks = [[splat(0.) ; NUM_UNISON_VECTORS] ; MAX_UNISON + 1];

    let mut i = 2;
    while i < MAX_UNISON + 1 {

        let mut j = 0;
        let step = 2. / (i - 1) as f32;
        let end = i + (i & 1); // next even number

        while j < end >> 1 {

            let dist = step * j as f32;

            array[MAX_UNISON - 1 - j] = 1. - dist;
            array[MAX_UNISON - 1 - (end - 1 - j)] = dist - 1.;

            j += 1;
        }
        blocks[i] = unsafe { transmute(array) };

        i += 1;
    }

    blocks
};

const UNISON_DETUNE_VALUES: [(MaskType, &[Float]) ; MAX_UNISON + 1] = {
    let mut slices: [(_, &[Float]) ; MAX_UNISON + 1] = [(0, &[]) ; MAX_UNISON + 1];

    let mut i = 1;
    while i < MAX_UNISON + 1 {

        let mut length = i / VECTOR_WIDTH;

        if i != MAX_UNISON {
            length += 1;
        }

        slices[i].1 = unsafe { UNISON_DETUNE
            .get_unchecked(i)
            .get_unchecked(NUM_UNISON_VECTORS - length..)
        };

        let mask_num_voices = (i + (i & 1)) % (VECTOR_WIDTH + 1);

        let mask = ((1 << mask_num_voices) - 1) << (VECTOR_WIDTH - mask_num_voices);

        slices[i].0 = mask as MaskType;
        i += 1;
    }

    slices
};

#[inline]
pub fn exp2(x: Float) -> Float {
    const C0: Float = splat(1.);
    const C1: Float = splat(16970. / 24483.);
    const C2: Float = splat(1960. / 8161.);
    const C3: Float = splat(1360. / 24483.);
    const C4: Float = splat(80. / 8161.);
    const C5: Float = splat(32. / 24483.);

    const TW3: Simd<i32, VECTOR_WIDTH> = splat(23);
    const ONE27: Simd<i32, VECTOR_WIDTH> = splat(127);

    let rounded = x.round();

    let t = x - rounded;
    let int = Float::from_bits((unsafe { rounded.to_int_unchecked() + ONE27 } << TW3).cast());

    let y = t.mul_add(t.mul_add(t.mul_add(t.mul_add(t.mul_add(C5, C4), C3), C2), C1), C0);
    int * y
}

#[inline]
pub fn semitones_to_ratio(semitones: Float) -> Float {
    const RATIO: Float = splat(1. / 12.);
    exp2(semitones * RATIO)
}

/// circular panning of a vector of stereo samples, pan: [-1 ; 1]
#[inline]
pub fn circular_pan_stereo(pan: Float, sample: Float) -> Float {
    const A: Float = splat(0.5);

    const B: Float = {
        let mut array = A.to_array();
        let mut i = 0;
        while i < VECTOR_WIDTH {
            array[i] = -0.5;
            i += 2;
        }

        Float::from_array(array)
    };

    sample * pan.mul_add(B, A).sqrt()
}

pub fn circular_pan_mono(pan: Float, sample: Float) -> [Float ; 2] {
    let (first_half, second_half) = stereo_unpack(sample);
    [circular_pan_stereo(pan, first_half), circular_pan_stereo(pan, second_half)]
}

#[derive(Default)]
pub struct WTOscParamValues {
    pub level: Float,
    pub pan: Float,
    pub num_unison_voices: [usize ; VOICES_PER_VECTOR],
    pub frame: Int,
    pub detune_range: Float,
    pub detune: Float,
}

impl WTOscParamValues {
    pub fn update(&mut self, params: &WTOscParams) {

        self.level = splat(params.level.unmodulated_plain_value());
        self.pan =  splat(params.pan.unmodulated_plain_value());
        self.num_unison_voices = [params.num_unison_voices.unmodulated_plain_value() as usize ; VOICES_PER_VECTOR];
        self.frame = splat(params.frame.unmodulated_plain_value() as u32);
        self.detune_range = splat(params.detune_range.unmodulated_plain_value());
        self.detune = splat(params.detune.unmodulated_plain_value());
    }
}

pub struct Phase(Int);

impl Deref for Phase {
    type Target = Int;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Phase {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Phase {
    #[inline]
    pub fn advance(&mut self, base_phase_delta: Float, detune: Float) -> Int {

        let detune_mult = semitones_to_ratio(detune);
        let flp_phase_delta = base_phase_delta * detune_mult;
        let fxp_phase_delta = flp_to_fxp(flp_phase_delta);

        self.0 += fxp_phase_delta;

        fxp_phase_delta
    }
}

pub struct WTOscVoice {
    note_id: u8,
    oscillators: [Phase ; NUM_UNISON_VECTORS],
    /// phase delta before unison detuning, pitch bend, transposition, phase_distortion...
    /// all lanes have the same value
    base_phase_delta: Float,
}

impl WTOscVoice {
    /// random: [0 ; 1]
    pub fn new(note_id: u8, phase_randomisation: f32x2, sample_rate: f32) -> Self {

        Self {
            note_id,
            oscillators: array::from_fn(|_| {

                let mut phases = Float::splat(0.);

                for phase in as_stereo_samples_ref(&mut phases) {
                    *phase = phase_randomisation * f32x2::splat(random());
                }

                Phase(flp_to_fxp(phases))
            }),
            base_phase_delta: Float::splat(util::midi_note_to_freq(note_id) / sample_rate)
        }
    }

    #[inline]
    pub fn update_phases_and_resample(
        &mut self,
        table: &BandLimitedWaveTables,
        detune_range: f32x2,
        detune_percentage: f32x2,
        frame: u32x2,
        num_unison_voices: usize,
    ) -> f32x2 {

        let detune_range = alternating(detune_range);
        let unison_detune = alternating(detune_percentage);
        let frame = alternating(frame);

        let global_detune = detune_range * unison_detune;

        // SAFETY: 1 <= num_unison_voices <= MAX_UNISON
        let &(mask, detune_values) = unsafe {
            UNISON_DETUNE_VALUES.get_unchecked(num_unison_voices)
        };

        let (first_phase, first_detune) = unsafe { 
            (self.oscillators.get_unchecked_mut(0), detune_values.get_unchecked(0))
        };

        let mut voice_samples = {
            let local_detune = global_detune * first_detune;
            let phase_delta = first_phase.advance(self.base_phase_delta, local_detune);

            let stereo_amount = unison_detune * first_detune;
            let sample = table.resample_select(phase_delta, frame, **first_phase, mask);

            let [first_half, second_half] = circular_pan_mono(stereo_amount, sample);

            first_half + second_half
        };

        let (phases, detunes) = unsafe {
            (self.oscillators.get_unchecked_mut(1..), detune_values.get_unchecked(1..))
        };

        for (phase, detune) in phases.iter_mut().zip(detunes.iter()) {

            let local_detune = global_detune * detune;
            let phase_delta = phase.advance(self.base_phase_delta, local_detune);

            let stereo_amount = unison_detune * detune;
            let sample = table.resample(phase_delta, frame, **phase);

            let [first_half, second_half] = circular_pan_mono(stereo_amount, sample);

            voice_samples += first_half + second_half;
        }

        let output_sample = sum_to_stereo_sample(voice_samples);

        output_sample
    }
}

#[derive(Default)]
pub struct WTOscVoiceBlock {
    param_values: WTOscParamValues,
    voices: ArrayVec<WTOscVoice, VOICES_PER_VECTOR>,
}

impl WTOscVoiceBlock {

    #[inline]
    pub fn process(&mut self, table: &BandLimitedWaveTables) -> Float {

        let mut output = splat(0.);

        let params = &self.param_values;

        type SampleArray<T> = [Simd<T, 2> ; VOICES_PER_VECTOR];

        // SAFETY: we are transmuting between types of the same size
        // and vectors over the same scalar type
        let (detune_range, detune, frame) = unsafe {(
            transmute::<_, SampleArray<f32>>(params.detune_range),
            transmute::<_, SampleArray<f32>>(params.detune),
            transmute::<_, SampleArray<u32>>(params.frame),
        )};

        for (i, (voice, sample)) in self.voices
            .iter_mut()
            .zip(as_stereo_samples_ref(&mut output).iter_mut())
            .enumerate()
        {
            *sample += voice.update_phases_and_resample(
                table,
                detune_range[i],
                detune[i],
                frame[i],
                params.num_unison_voices[i]
            );
        }

        circular_pan_stereo(params.pan, output * params.level)
    }

    pub fn is_full(&self) -> bool {
        self.voices.is_full()
    }

    pub fn is_empty(&self) -> bool {
        self.voices.is_empty()
    }

    pub fn add_voice(&mut self, note_id: u8, sample_rate: f32) {

        self.voices.push(WTOscVoice::new(note_id, splat(1.), sample_rate));
    }

    pub fn remove_voice(&mut self, note: u8) -> bool {
        for (i, voice) in self.voices.iter().enumerate() {
            if voice.note_id == note {
                self.voices.swap_remove(i);
                return true;
            }
        }

        false
    }

    pub fn update_smoothers(&mut self,params: &WTOscParams) {
        self.param_values.update(params);
    }
}