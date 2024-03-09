use wt_osc::{MAX_UNISON, PITCH_RANGE_SEMITONES};

use super::*;

#[derive(Params)]
pub struct MythrilParameters {
    #[id = "level"]
    pub level: FloatParam,
    #[id = "frame"]
    pub frame: FloatParam,
    #[id = "unison"]
    pub num_unison_voices: IntParam,
    #[id = "detune"]
    pub detune: FloatParam,
    #[id = "pan"]
    pub pan: FloatParam,
    #[id = "transp"]
    pub transpose: FloatParam,
    #[id = "stereo"]
    pub stereo: FloatParam,
    #[id = "drange"]
    pub detune_range: FloatParam,
}

impl Default for MythrilParameters {
    fn default() -> Self {
        Self {
            level: FloatParam::new(
                "Level",
                0.5,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 1.0,
                    factor: 2.0,
                },
            ),
            frame: FloatParam::new("Frame", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
            num_unison_voices: IntParam::new(
                "Unison Voices",
                1,
                IntRange::Linear {
                    min: 1,
                    max: MAX_UNISON as i32,
                },
            ),
            detune: FloatParam::new("Detune", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
            pan: FloatParam::new(
                "Pan",
                0.,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            ),
            transpose: FloatParam::new(
                "Transpose",
                0.0,
                FloatRange::Linear {
                    min: -PITCH_RANGE_SEMITONES,
                    max: PITCH_RANGE_SEMITONES,
                },
            ),
            stereo: FloatParam::new("Stereo", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
            detune_range: FloatParam::new(
                "Detune Range",
                1.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: PITCH_RANGE_SEMITONES,
                },
            ),
        }
    }
}
