use std::sync::{Mutex, Arc};

use atomic_refcell::AtomicRefCell;

use nih_plug::{prelude::*, formatters::*};

use crate::dsp::{wavetable::{SharedLender, BandLimitedWaveTables}, wt_osc::{MAX_UNISON, WTOscVoice}};

const WAVETABLE_FOLDER_PATH: &str = include_str!("wavetable_folder_path.txt");

#[derive(Params)]
pub struct WTOscParams {
    #[id = "level"]
    pub level: FloatParam,
    #[id = "pan"]
    pub pan: FloatParam,
    #[id = "unison"]
    pub num_unison_voices: IntParam,
    #[id = "frame"]
    pub frame: IntParam,
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
    #[persist = "wtname"]
    pub wt_name: AtomicRefCell<Box<str>>,
    pub wavetable: Mutex<SharedLender<BandLimitedWaveTables>>,
}

impl Default for WTOscParams {

    fn default() -> Self {

        Self {

            level: FloatParam::new(
                "Level",
                0.5,
                FloatRange::Skewed {
                    min: 0.,
                    max: 1.,
                    factor: 0.5,
                },
            ).with_value_to_string(v2s_f32_rounded(3)),

            pan: FloatParam::new(
                "Pan",
                0.,
                FloatRange::Linear {
                    min: f32::EPSILON,
                    max: 1.,
                }
            ).with_value_to_string(Arc::new( |value| {
                value.mul_add(2., -1.).to_string()
            })),

            num_unison_voices: IntParam::new(
                "Unison",
                1,
                IntRange::Linear { min: 1, max: MAX_UNISON as i32 },
            ),

            frame: IntParam::new(
                "Frame",
                0,
                IntRange::Linear {
                    min: 0,
                    max: BandLimitedWaveTables::NUM_FRAMES as i32 - 1,
                },
            ),

            detune_range: FloatParam::new(
                "Spread",
                2.,
                FloatRange::Linear {
                    min: 0.,
                    max: 48.
                }
            ).with_value_to_string(v2s_f32_rounded(3)),

            detune: FloatParam::new(
                "Detune",
                0.2,
                FloatRange::Linear {
                    min: 0.,
                    max: 1.
                }
            ).with_value_to_string(v2s_f32_rounded(3)),

            stereo_unison: FloatParam::new(
                "Unison Stereo Amount",
                1.,
                FloatRange::Linear {
                    min: 0.,
                    max: 1. - f32::EPSILON,
                }
            ).with_value_to_string(v2s_f32_percentage(3))
            .with_unit(" %"),

            blend: FloatParam::new(
                "Blend",
                0.,
                FloatRange::Linear {
                    min: -1.,
                    max: 1.,
                }
            ).with_value_to_string(v2s_f32_percentage(3))
            .with_unit(" %"),

            transpose: FloatParam::new(
                "Transpose",
                0.,
                FloatRange::Linear {
                    min: -48.,
                    max: 48.
                }
            ).with_value_to_string(v2s_f32_rounded(2)),

            random: FloatParam::new(
                "Phase Randomisation",
                1.,
                FloatRange::Linear {
                    min: 0.,
                    max: 1.
                }
            ).with_value_to_string(v2s_f32_percentage(3))
            .with_unit(" %"),

            wt_name: AtomicRefCell::new("Basic Shapes".into()),

            wavetable: Default::default(),
        }
    }
}

impl WTOscParams {

    pub fn load_wavetable(&self) {
        let name = self.wt_name.borrow();
        let name = name.as_ref();
        let wt = BandLimitedWaveTables::from_file(
            format!("{WAVETABLE_FOLDER_PATH}\\{name}.WAV")
        );

        let mut lock = self.wavetable.lock().expect("Issue unlocking the lock");

        lock.add(wt);
    }

    pub fn create_processor(&self) -> WTOscVoice {
        let mut lock = self.wavetable.lock().expect("Issue unlocking the lock");
        WTOscVoice::from_table_lender(lock.create_new_reciever())
    }
}