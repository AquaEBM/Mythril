use super::Arc;
use atomic_refcell::AtomicRefCell;
use core::{array, num::NonZeroUsize};
use nih_plug::{formatters::*, prelude::*};
use plugin_util::{
    simd::{Simd, SimdElement},
    simd_util::{Float, FLOATS_PER_VECTOR},
    smoothing::{LinearSmoother, Smoother},
};

use polygraph::stereo_util::STEREO_VOICES_PER_VECTOR;
use wt_osc::{WTOscParams, MAX_UNISON, NUM_VOICE_OSCILLATORS, wavetable::BandLimitedWaveTables};

#[derive(Params)]
pub struct JadeParams {
    #[persist = "starting_phases"]
    pub starting_phases: AtomicRefCell<[[f32; FLOATS_PER_VECTOR]; NUM_VOICE_OSCILLATORS]>,
    #[id = "level"]
    pub level: FloatParam,
    #[id = "pan"]
    pub pan: FloatParam,
    #[id = "unison"]
    pub num_unison_voices: IntParam,
    #[id = "frame"]
    pub frame: FloatParam,
    #[id = "spread"]
    pub detune_range: FloatParam,
    #[id = "detune"]
    pub detune: FloatParam,
    #[id = "steuni"]
    pub stereo_unison: FloatParam,
    #[id = "blend"]
    pub blend: FloatParam,
    #[id = "transp"]
    pub transpose: FloatParam,
    #[id = "random"]
    pub random: FloatParam,
    pub wavetable: AtomicRefCell<Arc<BandLimitedWaveTables>>,
}

impl Default for JadeParams {
    fn default() -> Self {
        let params = Self {
            starting_phases: AtomicRefCell::new([[0. ; FLOATS_PER_VECTOR]; NUM_VOICE_OSCILLATORS]),

            level: FloatParam::new(
                "Level",
                0.5,
                FloatRange::Skewed {
                    min: 0.,
                    max: 1.,
                    factor: 0.5,
                },
            )
            .with_value_to_string(v2s_f32_rounded(3)),

            pan: FloatParam::new("Pan", 0.5, FloatRange::Linear { min: 0., max: 1. })
                .with_value_to_string(Arc::new(|value| {
                    let v = value.mul_add(2., -1.);
                    format!("{v:.3}")
                })),

            num_unison_voices: IntParam::new(
                "Unison",
                1,
                IntRange::Linear {
                    min: 1,
                    max: MAX_UNISON as i32,
                },
            ),

            frame: FloatParam::new("Frame", 0., FloatRange::Linear { min: 0., max: 1f32.next_down().next_down() }),

            detune_range: FloatParam::new("Spread", 2., FloatRange::Linear { min: 0., max: 48. })
                .with_value_to_string(v2s_f32_rounded(3)),

            detune: FloatParam::new("Detune", 0.2, FloatRange::Linear { min: 0., max: 1. })
                .with_value_to_string(v2s_f32_rounded(3)),

            stereo_unison: FloatParam::new(
                "Unison Stereo Amount",
                1.,
                FloatRange::Linear { min: 0., max: 1. },
            )
            .with_value_to_string(v2s_f32_percentage(3))
            .with_unit(" %"),

            blend: FloatParam::new("Blend", 0., FloatRange::Linear { min: -1., max: 1. })
                .with_value_to_string(v2s_f32_percentage(3))
                .with_unit(" %"),

            transpose: FloatParam::new(
                "Transpose",
                0.,
                FloatRange::Linear {
                    min: -48.,
                    max: 48.,
                },
            )
            .with_value_to_string(v2s_f32_rounded(2)),

            random: FloatParam::new(
                "Phase Randomisation",
                1.,
                FloatRange::Linear { min: 0., max: 1. },
            )
            .with_value_to_string(v2s_f32_percentage(3))
            .with_unit(" %"),

            wavetable: AtomicRefCell::new(BandLimitedWaveTables::basic_shapes()),
        };

        params.set_starting_phases(array::from_fn(|i| {
            array::from_fn(|j| {
                let index = i * FLOATS_PER_VECTOR + j;
                const RATIO: f32 = 1. / (FLOATS_PER_VECTOR * NUM_VOICE_OSCILLATORS - 1) as f32;
                RATIO * index as f32
            })
        }));

        params
    }
}

impl JadeParams {
    fn set_starting_phases(&self, phases: [[f32; FLOATS_PER_VECTOR]; NUM_VOICE_OSCILLATORS]) {
        *self.starting_phases.borrow_mut() = phases;
    }
}

#[derive(Default)]
pub struct JadeParamValues {
    params: Arc<JadeParams>,
    detune: LinearSmoother,
    transpose: LinearSmoother,
    frame: LinearSmoother,
    random: LinearSmoother,
    starting_phases: [Float; NUM_VOICE_OSCILLATORS],
    level: LinearSmoother,
    stereo: LinearSmoother,
    pan: LinearSmoother,
    num_unison_voices: [usize; STEREO_VOICES_PER_VECTOR],
}

impl JadeParamValues {
    fn update_starting_phases(&mut self) {
        self.starting_phases = self.params.starting_phases.borrow().map(Float::from_array);
    }

    fn get<P: Param>(param: &P) -> P::Plain {
        param.unmodulated_plain_value()
    }

    fn get_splat<P: Param>(p: &P) -> Simd<P::Plain, FLOATS_PER_VECTOR>
    where
        P::Plain: SimdElement,
    {
        Simd::splat(Self::get(p))
    }

    pub fn update_values(&mut self) {
        let p = self.params.as_ref();

        self.detune.set_instantly(Simd::splat(
            Self::get(&p.detune) * Self::get(&p.detune_range),
        ));
        self.transpose.set_instantly(Self::get_splat(&p.transpose));
        self.frame.set_instantly(Self::get_splat(&p.frame));
        self.random.set_instantly(Self::get_splat(&p.random));
        self.level.set_instantly(Self::get_splat(&p.level));
        self.stereo.set_instantly(Self::get_splat(&p.stereo_unison));
        self.pan.set_instantly(Self::get_splat(&p.pan));
        self.num_unison_voices =
            [Self::get(&p.num_unison_voices) as usize; STEREO_VOICES_PER_VECTOR];
    }

    pub fn params(&self) -> &Arc<JadeParams> {
        &self.params
    }
}

impl WTOscParams for JadeParamValues {
    fn initialize(&mut self, _sr: f32, _max_buffer_size: usize) {
        self.update_starting_phases();
        self.update_values();
    }

    fn update_smoothers(&mut self, num_samples: NonZeroUsize) {
        let inc = Simd::splat(1. / num_samples.get() as f32);
        let p = self.params.as_ref();

        self.detune.set_increment(
            Simd::splat(Self::get(&p.detune) * Self::get(&p.detune_range)),
            inc,
        );
        self.transpose.set_increment(Self::get_splat(&p.transpose), inc);
        self.frame.set_increment(Self::get_splat(&p.frame), inc);
        self.random.set_increment(Self::get_splat(&p.random), inc);
        self.level.set_increment(Self::get_splat(&p.level), inc);
        self.stereo.set_increment(Self::get_splat(&p.stereo_unison), inc);
        self.pan.set_increment(Self::get_splat(&p.pan), inc);
        self.num_unison_voices =
            [Self::get(&p.num_unison_voices) as usize; STEREO_VOICES_PER_VECTOR];
    }

    fn tick_n(&mut self, inc: Float) {
        self.detune.tick_increments(inc);
        self.transpose.tick_increments(inc);
        self.random.tick_increments(inc);
        self.level.tick_increments(inc);
        self.stereo.tick_increments(inc);
        self.pan.tick_increments(inc);
    }

    fn get_detune(&self, _cluster_idx: usize) -> Float {
        self.detune.get_current()
    }

    fn get_transpose(&self, _cluster_idx: usize) -> Float {
        self.transpose.get_current()
    }

    fn get_norm_frame(&self, _cluster_idx: usize) -> Float {
        self.frame.get_current()
    }

    fn get_random(&self, _cluster_idx: usize) -> Float {
        self.random.get_current()
    }

    fn get_level(&self, _cluster_idx: usize) -> Float {
        self.level.get_current()
    }

    fn get_stereo_amount(&self, _cluster_idx: usize) -> Float {
        self.stereo.get_current()
    }

    fn get_norm_pan(&self, _cluster_idx: usize) -> Float {
        self.pan.get_current()
    }

    fn get_num_unison_voices(&self, _cluster_idx: usize) -> [usize; STEREO_VOICES_PER_VECTOR] {
        self.num_unison_voices
    }

    fn get_starting_phases(&self) -> &[Float; NUM_VOICE_OSCILLATORS] {
        &self.starting_phases
    }
}
