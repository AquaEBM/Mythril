use std::{sync::Arc, f32::EPSILON};

use nih_plug::{prelude::*, formatters::*};

use crate::dsp::wt_osc::voice::MAX_UNISON;

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
                0.5,
                FloatRange::Linear {
                    min: 0.,
                    max: 1.,
                }
            ).with_value_to_string(Arc::new( |value| {
                let v = value.mul_add(2., -1.);
                format!("{v:.3}")
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
                    max: 255,
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
                    min: EPSILON,
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
        }
    }
}