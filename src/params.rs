// use crate::wavetable::wavetable_from_file;
use atomic_refcell::AtomicRefCell;
use parking_lot::Mutex;
use std::{fs::read_dir, sync::Arc};

use nih_plug_egui::{egui::*, EguiState};
use plot::*;

use plugin_util::{parameter::ParamHandle, gui::widgets::*};

use nih_plug::{prelude::*, formatters::v2s_f32_rounded};

use crate::dsp::{wavetable::{SharedLender, BandLimitedWaveTables}, wt_osc::MAX_UNISON};

const WAVETABLE_FOLDER_PATH: &str = r"C:\Users\etulyon1\Documents\Coding\Krynth\wavetables";

#[derive(Params)]
pub struct WTOscParams {
    #[persist = "editor"]
    pub editor_state: Arc<EguiState>,
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
    #[persist = "wt"]
    pub wt_name: AtomicRefCell<Box<str>>,
    pub wavetable: Mutex<SharedLender<BandLimitedWaveTables>>,
}

impl WTOscParams {

    pub fn new(wavetable: SharedLender<BandLimitedWaveTables>) -> Self {
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
                0.,
                FloatRange::Linear {
                    min: -1.,
                    max: 1.
                }
            )
            .with_value_to_string(v2s_f32_rounded(3)),

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

            wavetable: Mutex::new(wavetable),
            editor_state: EguiState::from_size(500, 270),
        }
    }

    pub fn load_wavetable(&self) {
        let name = self.wt_name.borrow();
        let name = name.as_ref();
        let wt = BandLimitedWaveTables::from_file(
            format!("{WAVETABLE_FOLDER_PATH}\\{name}.WAV")
        );

        self.wavetable.lock().add(wt);
    }

    pub fn ui(&self, ui: &mut Ui, setter: &ParamSetter) -> Response {

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.add(ParamWidget::new(
                    Knob::new().radius(40.),
                    ParamHandle::from((&self.level, setter)),
                ));

                ui.horizontal(|ui| {
                    ui.add(ParamWidget::<Knob, ParamHandle<_>>::default(
                        (&self.num_unison_voices, setter).into(),
                    ));

                    ui.add(ParamWidget::<Knob, ParamHandle<_>>::default(
                        (&self.pan, setter).into(),
                    ));
                });

                ui.horizontal(|ui| {
                    ui.add(ParamWidget::<Knob, ParamHandle<_>>::default(
                        (&self.detune, setter).into(),
                    ));

                    ui.add(ParamWidget::<Knob, ParamHandle<_>>::default(
                        (&self.detune_range, setter).into(),
                    ));
                });
            });

            ui.vertical_centered_justified(|ui| {

                let wavetable_list = ui.memory().data.get_temp(
                    ui.id().with("wt_list"),
                ).or_else(|| {
                    Some(read_dir(WAVETABLE_FOLDER_PATH).unwrap().map(|name| name
                        .unwrap()
                        .file_name()
                        .to_str()
                        .unwrap()
                        .strip_suffix(".WAV")
                        .unwrap()
                        .into()
                    ).collect::<Arc<[Box<str>]>>())
                }).unwrap();

                let current_wt_name = self.wt_name.borrow().clone();

                ComboBox::from_id_source(ui.id().with("combobox"))
                    .width(ui.available_width())
                    .selected_text(current_wt_name.as_ref())
                    .show_ui(ui, |ui| {
                        for name in wavetable_list.iter() {

                            let name_ref = name.as_ref();

                            if ui
                                .selectable_label(name_ref == current_wt_name.as_ref(), name_ref)
                                .clicked()
                            {
                                *self.wt_name.borrow_mut() = name.clone();
                                self.load_wavetable();
                            }
                        }
                    });

                ui.horizontal_centered(|ui| {

                    ui.add(ParamWidget::<VSlider, ParamHandle<_>>::default(
                        (&self.frame, setter).into(),
                    ));

                    let mut wavetable = self.wavetable.lock();

                    let points = PlotPoints::from_ys_f32(
                        &wavetable.current().unwrap()[self.frame.unmodulated_plain_value() as usize],
                    );

                    wavetable.update_drop_queue();

                    plain_plot(
                        ui.id().with("Plot"),
                        0.0..points.points().len() as f64,
                        -1.0..1.0,
                    )
                    .show(ui, |plot_ui| plot_ui.line(Line::new(points).fill(0.)));
                });
            })
        })
        .response
    }
}