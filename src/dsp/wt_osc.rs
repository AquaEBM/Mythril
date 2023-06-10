use super::{*, wavetable::BandLimitedWaveTables};
use arrayvec::ArrayVec;
use nih_plug::{prelude::Param, util};
use params::WTOscParams;
use rand::random;
use plugin_util::math::*;

// TODO: implement unison center/detuned voice blending

pub const MAX_UNISON: usize = 16;
const NUM_UNISON_VECTORS: usize = enclosing_div(MAX_UNISON, MAX_VECTOR_WIDTH);

const UNISON_DETUNES: [[Float ; NUM_UNISON_VECTORS] ; MAX_UNISON + 1] = {

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

/// circular panning of a vector of stereo samples, pan: [-1 ; 1]
pub fn circular_pan_weights(pan: Float) -> Float {
    const A: Float = const_splat(0.5);

    const B: Float = {
        let mut array = A.to_array();
        let mut i = 0;
        while i < MAX_VECTOR_WIDTH {
            array[i] = -0.5;
            i += 2;
        }

        Float::from_array(array)
    };

    pan.mul_add(B, A).sqrt()
}

#[derive(Default)]
pub struct WTOscParamValues {
    pub level: Float,
    pub pan: Float,
    pub base_detunes: &'static [Float],
    pub remainder_mask: TMask,
    pub frame: UInt,
    pub detune: Float,
    pub stereo_unison: Float,
    pub blend: Float,
    pub scale: Float,
    pub transpose: Float,
    pub random: Float,
}

impl WTOscParamValues {
    pub fn update(&mut self, params: &WTOscParams) {

        self.level = Float::splat(params.level.unmodulated_plain_value());
        self.stereo_unison = Float::splat(params.stereo_unison.unmodulated_normalized_value());
        self.pan = Float::splat(params.pan.unmodulated_plain_value());

        let voices = params.num_unison_voices.unmodulated_plain_value() as usize;

        let mut n = voices + (voices & 1);
        self.scale = (ONE_F / Float::splat(n as f32)).sqrt();
        n -= 1;

        let num_full_vectors = n / MAX_VECTOR_WIDTH;
        let remainder = n % MAX_VECTOR_WIDTH + 1;

        let detunes = &UNISON_DETUNES[voices as usize][..num_full_vectors];

        self.remainder_mask = TMask::from_array(array::from_fn(|i| i < remainder));
        self.base_detunes = detunes;
        self.frame = UInt::splat(params.frame.unmodulated_plain_value() as u32);

        self.detune = Float::splat(
            params.detune.unmodulated_plain_value() *
            params.detune_range.unmodulated_plain_value()
        );

        self.stereo_unison = Float::splat(params.stereo_unison.unmodulated_plain_value());

        self.transpose = Float::splat(params.transpose.unmodulated_plain_value());

        self.random = Float::splat(params.random.unmodulated_plain_value());

        self.blend = Float::splat(params.blend.unmodulated_plain_value());
    }
}

struct Oscillator {
    /// phase delta before unison detuning, pitch bend, transposition
    /// (coming soon lol)
    base_phase_delta: Float,
    phase: UInt,
}

impl Oscillator {
    pub fn advance_phase_detuned(&mut self, detune: Float) -> UInt {

        let detune_mult = semitones_to_ratio(detune);
        let float_phase_delta = self.base_phase_delta * detune_mult;
        let fixed_phase_delta = flp_tp_fxp(float_phase_delta);

        self.phase += fixed_phase_delta;

        fixed_phase_delta
    }
}

pub struct WaveTableOscVoice {
    note_id: u8,
    oscillators: [Oscillator ; NUM_UNISON_VECTORS],
}

impl WaveTableOscVoice {
    /// random: [0 ; 1]
    pub fn new(note_id: u8, phase_randomisation: f32x2, sample_rate: f32) -> Self {

        let note_freq = Float::splat(util::midi_note_to_freq(note_id) / sample_rate);
        let randomisation = alternating(phase_randomisation);

        Self {
            note_id,
            oscillators: array::from_fn(|_| {

                let mut phases = Float::splat(0.);

                for phase in as_mut_stereo_sample_array(&mut phases) {
                    *phase = f32x2::splat(random());
                }

                phases *= randomisation;

                Oscillator {
                    phase: flp_tp_fxp(phases),
                    base_phase_delta: note_freq
                }
            }),
        }
    }

    pub fn update_phases_and_resample(
        &mut self,
        table: &BandLimitedWaveTables,
        detune: f32x2,
        transpose: f32x2,
        blend: f32x2,
        frame: u32x2,
        base_detunes: &[Float],
        mask: TMask,
    ) -> f32x2 {

        let frame = alternating(frame);
        let global_detune = alternating(detune);
        let transpose = alternating(transpose);
        let blend = alternating(blend);

        let (center_osc, center_osc_detune) = unsafe { (
            self.oscillators.get_unchecked_mut(0),
            base_detunes.get_unchecked(0)
        ) };

        let mut voice_samples = {

            let local_detune_semitones = center_osc_detune.mul_add(global_detune, transpose);
            let phase_delta = center_osc.advance_phase_detuned(local_detune_semitones);

            table.resample_select(phase_delta, frame, center_osc.phase, mask)
        };

        let (detuned_oscs, detuned_oscs_detune) = unsafe {
            (self.oscillators.get_unchecked_mut(1..), base_detunes.get_unchecked(1..))
        };

        for (osc, osc_detune) in detuned_oscs.iter_mut().zip(detuned_oscs_detune.iter()) {

            let local_detune_semitones = osc_detune.mul_add(global_detune, transpose);
            let phase_delta = osc.advance_phase_detuned(local_detune_semitones);

            voice_samples += table.resample(phase_delta, frame, osc.phase);
        }

        sum_to_stereo_sample(voice_samples)
    }
}

#[derive(Default)]
pub struct WTOscVoice {

    param_values: WTOscParamValues,
    voices: ArrayVec<WaveTableOscVoice, VOICES_PER_VECTOR>,
}

pub fn with_stereo_amount(x: Float, stereo_amount: Float) -> Float {

    const FLIP_PAIRS: [usize ; MAX_VECTOR_WIDTH] = {

        let mut array = [0 ; MAX_VECTOR_WIDTH];

        let mut i = 0;
        while i < MAX_VECTOR_WIDTH {

            array[i] = i ^ 1;
            i += 1;
        }
        array
    };

    let flipped = simd_swizzle!(x, FLIP_PAIRS);

    let mono = (ONE_F - stereo_amount).sqrt();
    let stereo = (ONE_F + stereo_amount).sqrt();

    x.mul_add(stereo, flipped * mono)
}

impl WTOscVoice {

    pub fn process(&mut self, table: &BandLimitedWaveTables) -> Float {

        let mut output = const_splat(0.);

        let output_samples_ref = as_mut_stereo_sample_array(&mut output);

        let params = &self.param_values;

        let detune = as_stereo_sample_array(&params.detune);
        let frame = as_stereo_sample_array(&params.frame);
        let transpose = as_stereo_sample_array(&params.transpose);
        let blend = as_stereo_sample_array(&params.blend);

        // VERY UNSAFE
        if self.voices.len() > VOICES_PER_VECTOR {
            unsafe { std::hint::unreachable_unchecked() }
        }

        for (i, voice) in self.voices.iter_mut().enumerate() {
            output_samples_ref[i] += voice.update_phases_and_resample(
                table,
                detune[i],
                transpose[i],
                blend[i],
                frame[i],
                params.base_detunes,
                params.remainder_mask,
            );
        }

        output = with_stereo_amount(output, params.stereo_unison);

        let panning_weights = circular_pan_weights(params.pan);

        (output * params.level) * (params.scale * panning_weights)
    }

    pub fn is_full(&self) -> bool {
        self.voices.is_full()
    }

    pub fn is_empty(&self) -> bool {
        self.voices.is_empty()
    }

    pub fn add_voice(&mut self, note_id: u8, sample_rate: f32) {

        let Some(&random) = as_stereo_sample_array(&self.param_values.random).get(self.voices.len()) else {
            return
        };

        self.voices.push(
            WaveTableOscVoice::new(
                note_id,
                random,
                sample_rate
            )
        );
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

    pub fn update_smoothers(&mut self, params: &WTOscParams) {
        self.param_values.update(params);
    }    
}