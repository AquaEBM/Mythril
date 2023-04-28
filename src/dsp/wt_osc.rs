use super::{*, wavetable::BandLimitedWaveTables};
use arrayvec::ArrayVec;
use nih_plug::{prelude::Param, util};
use params::WTOscParams;
use rand::random;

pub const MAX_UNISON: usize = 16;
const NUM_UNISON_VECTORS: usize = MAX_UNISON / MAX_VECTOR_WIDTH;

const UNISON_DETUNES: [[Float ; NUM_UNISON_VECTORS] ; MAX_UNISON + 1] = {

    assert!(MAX_VECTOR_WIDTH >= 2);
    assert!(MAX_VECTOR_WIDTH.is_power_of_two());
    assert!(MAX_UNISON % MAX_VECTOR_WIDTH == 0);
    assert!(MAX_POLYPHONY % MAX_VECTOR_WIDTH == 0);

    let mut blocks = [[splat(0.) ; NUM_UNISON_VECTORS] ; MAX_UNISON + 1];
    let mut array = [0. ; MAX_UNISON];

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

#[derive(Clone, Copy)]
struct UnisonLookupData {
    mask: MaskType,
    scale: f32x2,
    detune_values: &'static [Float],
}

impl UnisonLookupData {
    pub const fn new() -> Self {
        Self {
            mask: 0,
            scale: splat(1.),
            detune_values: &[],
        }
    }
}

const UNISON_DETUNE_VALUES: [UnisonLookupData ; MAX_UNISON + 1] = {
    let mut data = [UnisonLookupData::new() ; MAX_UNISON + 1];

    let mut i = 1;
    while i < MAX_UNISON + 1 {

        let mut length = i / MAX_VECTOR_WIDTH;

        if i != MAX_UNISON {
            length += 1;
        }

        data[i].detune_values = unsafe { UNISON_DETUNES
            .get_unchecked(i)
            .get_unchecked(NUM_UNISON_VECTORS - length..)
        };

        let mask_num_voices = (i + (i & 1)) % (MAX_VECTOR_WIDTH + 1);

        let mask = !(usize::MAX << mask_num_voices) << (MAX_VECTOR_WIDTH - mask_num_voices);

        data[i].mask = mask as MaskType;

        data[i].scale = splat(1. / i as f32);
        i += 1;
    }

    data
};

#[inline]
/// compute `2^i` as a Float
pub fn fexp2i(i: Int) -> Float {
    const TW3: Int = splat(23);
    const ONE27: Int = splat(127);

    Float::from_bits((i + ONE27 << TW3).cast())
}

#[inline]
/// 2 ^ x approximation, results in undefined behavior in case of
/// NAN, +inf or subnormal numbers
pub fn exp2(x: Float) -> Float {

    const A: Float = splat(1.);
    const B: Float = splat(16970. / 24483.);
    const C: Float = splat(1960. / 8161.);
    const D: Float = splat(1360. / 24483.);
    const E: Float = splat(80. / 8161.);
    const F: Float = splat(32. / 24483.);

    let rounded = x.round();

    let t = x - rounded;
    let int = fexp2i(unsafe { rounded.to_int_unchecked() });

    let y = t.mul_add(t.mul_add(t.mul_add(t.mul_add(t.mul_add(F, E), D), C), B), A);
    int * y
}

#[inline]
pub fn semitones_to_ratio(semitones: Float) -> Float {
    const RATIO: Float = splat(1. / 12.);
    exp2(semitones * RATIO)
}

/// circular panning of a vector of stereo samples, pan: [-1 ; 1]
#[inline]
pub fn circular_pan_weights(pan: Float) -> Float {
    const A: Float = splat(0.5);

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

#[inline]
pub fn circular_pan_stereo(pan: Float, sample: Float) -> Float {
    let weights = circular_pan_weights(pan);
    sample * weights
}

#[derive(Default)]
pub struct WTOscParamValues {
    pub level: Float,
    pub pan: Float,
    pub num_unison_voices: [usize ; VOICES_PER_VECTOR],
    pub frame: UInt,
    pub detune_range: Float,
    pub detune: Float,
    pub stereo_unison: Float,
}

impl WTOscParamValues {
    pub fn update(&mut self, params: &WTOscParams) {

        self.level = Float::splat(params.level.unmodulated_plain_value());
        self.stereo_unison = Float::splat(params.stereo_unison.unmodulated_normalized_value());
        self.pan = Float::splat(params.pan.unmodulated_plain_value());
        self.num_unison_voices = [params.num_unison_voices.unmodulated_plain_value() as usize ; VOICES_PER_VECTOR];
        self.frame = UInt::splat(params.frame.unmodulated_plain_value() as u32);
        self.detune_range = Float::splat(params.detune_range.unmodulated_plain_value());
        self.detune = Float::splat(params.detune.unmodulated_plain_value());
        self.stereo_unison = Float::splat(params.stereo_unison.unmodulated_normalized_value());
    }
}

#[inline]
pub fn advance(phase: &mut UInt, base_phase_delta: Float, detune: Float) -> UInt {

    let detune_mult = semitones_to_ratio(detune);
    let float_phase_delta = base_phase_delta * detune_mult;
    let fixed_phase_delta = to_fixed_point(float_phase_delta);

    *phase += fixed_phase_delta;

    fixed_phase_delta
}

pub struct WaveTableOscVoice {
    note_id: u8,
    oscillators: [UInt ; NUM_UNISON_VECTORS],
    /// phase delta before unison detuning, pitch bend, transposition, phase_distortion...
    /// (coming soon lol)
    base_phase_delta: Float,
}

impl WaveTableOscVoice {
    /// random: [0 ; 1]
    pub fn new(note_id: u8, phase_randomisation: f32x2, sample_rate: f32) -> Self {

        Self {
            note_id,
            oscillators: array::from_fn(|_| {

                let mut phases = Float::splat(0.);

                for phase in as_stereo_samples_ref(&mut phases) {
                    *phase = phase_randomisation * f32x2::splat(random());
                }

                to_fixed_point(phases)
            }),
            base_phase_delta: Float::splat(util::midi_note_to_freq(note_id) / sample_rate)
        }
    }

    #[inline]
    pub fn update_phases_and_resample(
        &mut self,
        table: &BandLimitedWaveTables,
        global_detune: f32x2,
        frame: u32x2,
        num_unison_voices: usize,
    ) -> f32x2 {

        let frame = alternating(frame);
        let global_detune = alternating(global_detune);

        // SAFETY: 1 <= num_unison_voices <= MAX_UNISON
        let data = unsafe {
            UNISON_DETUNE_VALUES.get_unchecked(num_unison_voices)
        };

        let (first_phase, first_detune) = unsafe { 
            (self.oscillators.get_unchecked_mut(0), data.detune_values.get_unchecked(0))
        };

        let mut voice_samples = {
            let local_detune = global_detune * first_detune;
            let phase_delta = advance(first_phase, self.base_phase_delta, local_detune);

            table.resample_select(phase_delta, frame, *first_phase, data.mask)
        };

        let (phases, detunes) = unsafe {
            (self.oscillators.get_unchecked_mut(1..), data.detune_values.get_unchecked(1..))
        };

        for (phase, detune) in phases.iter_mut().zip(detunes.iter()) {

            let local_detune = global_detune * detune;
            let phase_delta = advance(phase, self.base_phase_delta, local_detune);

            voice_samples += table.resample(phase_delta, frame, *phase);
        }

        sum_to_stereo_sample(voice_samples)
    }
}

#[derive(Default)]
pub struct WTOscVoiceBlock {
    param_values: WTOscParamValues,
    voices: ArrayVec<WaveTableOscVoice, VOICES_PER_VECTOR>,
}

impl WTOscVoiceBlock {

    #[inline]
    pub fn process(&mut self, table: &BandLimitedWaveTables) -> Float {

        let mut output = splat(0.);

        let params = &self.param_values;

        type SampleArray<T> = [Simd<T, 2> ; VOICES_PER_VECTOR];

        // SAFETY: transmutation from an array to a vector of the same scalar type
        // so values are valid
        let (detune, frame) = unsafe {(
            transmute::<_, SampleArray<f32>>(params.detune_range * params.detune),
            transmute::<_, SampleArray<u32>>(params.frame),
        )};

        for (i, (voice, sample)) in self.voices
            .iter_mut()
            .zip(as_stereo_samples_ref(&mut output).iter_mut())
            .enumerate()
        {
            *sample += voice.update_phases_and_resample(
                table,
                detune[i],
                frame[i],
                params.num_unison_voices[i],
            );
        }

        const FLIP_PAIRS: [usize ; MAX_VECTOR_WIDTH] = {
            let mut array = [0 ; MAX_VECTOR_WIDTH];

            let mut i = 0;

            while i < MAX_VECTOR_WIDTH {

                array[i] = i ^ 1;

                i += 1;
            }

            array
        };

        const ONE: Float = splat(1.);

        let flipped_output = simd_swizzle!(output, FLIP_PAIRS);
        
        let stereo_unison = params.stereo_unison;

        let mono = (ONE - stereo_unison).sqrt();
        let stereo = (ONE + stereo_unison).sqrt();

        output = output * stereo + flipped_output * mono;

        circular_pan_stereo(params.pan, output * params.level)
    }

    pub fn is_full(&self) -> bool {
        self.voices.is_full()
    }

    pub fn is_empty(&self) -> bool {
        self.voices.is_empty()
    }

    pub fn add_voice(&mut self, note_id: u8, sample_rate: f32) {

        self.voices.push(WaveTableOscVoice::new(note_id, splat(1.), sample_rate));
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