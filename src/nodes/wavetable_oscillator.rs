mod dsp;
mod gui;
mod wavetable;

use wavetable::wavetable_from_file;

use super::*;
use dsp::WTOsc;

const WAVETABLE_FOLDER_PATH: &str =
    "C:\\Users\\etulyon1\\Documents\\Coding\\Krynth\\wavetables";

const FRAMES_PER_WT: usize = 256;
const WAVE_FRAME_LEN: usize = 2048;

type WaveFrame = [f32; WAVE_FRAME_LEN + 1];
type WaveTable = [WaveFrame ; FRAMES_PER_WT];

#[derive(Params)]
pub struct WTOscParams {
    #[persist = "editor_state"]
    editor_state: Arc<EguiState>,
    #[id = "level"]
    level: FloatParam,
    #[id = "pan"]
    pan: FloatParam,
    #[id = "unison"]
    num_unison_voices: IntParam,
    #[id = "frame"]
    frame: IntParam,
    #[id = "det_range"]
    detune_range: FloatParam,
    #[id = "detune"]
    detune: FloatParam,
    #[persist = "wt_name"]
    wt_name: AtomicRefCell<String>,
    wavetable: AtomicRefCell<Vec<WaveFrame>>,
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
            )
            .with_value_to_string(v2s_f32_rounded(3)),

            pan: FloatParam::new(
                "Pan",
                0.5,
                FloatRange::Linear {
                    min: 0.,
                    max: 1.
                }
            )
            .with_value_to_string(v2s_f32_rounded(3)),

            num_unison_voices: IntParam::new(
                "Unison",
                1,
                IntRange::Linear { min: 1, max: 16 },
            ),

            frame:IntParam::new(
                "Frame",
                0,
                IntRange::Linear {
                    min: 0,
                    max: FRAMES_PER_WT as i32 - 1,
                },
            ),

            detune_range: FloatParam::new(
                "Spread",
                2.,
                FloatRange::Linear {
                    min: 0.,
                    max: 48.
                }
            )
            .with_value_to_string(v2s_f32_rounded(3)),

            detune: FloatParam::new(
                "Detune",
                0.2,
                FloatRange::Linear {
                    min: 0.,
                    max: 1.
                }
            )
            .with_value_to_string(v2s_f32_rounded(3)),

            wt_name: AtomicRefCell::new("Basic Shapes".into()),

            wavetable: AtomicRefCell::new(Vec::new()),
            editor_state: EguiState::from_size(500, 270),
        }
    }
}

impl WTOscParams {
    fn oscillator(self: Arc<Self>) -> WTOsc {
        WTOsc::new(self)
    }

    fn load_wavetable(&self) {
        let name = self.wt_name.borrow();
        let name = name.as_str();
        let mut wt = wavetable_from_file(
            format!("{WAVETABLE_FOLDER_PATH}\\{name}.WAV")
        );

        wt.iter_mut().flatten().for_each(|sample| *sample /= 2.);

        *self.wavetable.borrow_mut() = wt;
    }
}
