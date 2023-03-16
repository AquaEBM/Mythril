use super::*;
use plot::*;
use std::{fs::read_dir, sync::OnceLock};

static WT_LIST: OnceLock<Vec<String>> = OnceLock::new();

impl SeenthNode for WTOscParams {
    fn type_name(&self) -> &'static str {
        "Oscillator"
    }

    fn ui(&self, ui: &mut Ui, setter: &ParamSetter) -> Response {
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

                let wt_list = WT_LIST.get_or_init(|| {
                    read_dir(WAVETABLE_FOLDER_PATH)
                        .unwrap()
                        .map(|dir| {
                            dir.unwrap()
                                .file_name()
                                .to_string_lossy()
                                .trim_end_matches(".WAV")
                                .into()
                        })
                        .collect::<Vec<_>>()
                });

                let current_wt_name = self.wt_name.borrow().clone();

                ComboBox::from_id_source(ui.id().with("combobox"))
                    .width(ui.available_width())
                    .selected_text(current_wt_name.as_str())
                    .show_ui(ui, |ui| {
                        for name in wt_list.iter().map(String::as_str) {
                            if ui
                                .selectable_label(name == &current_wt_name, name)
                                .clicked()
                            {
                                *self.wt_name.borrow_mut() = name.to_string();
                                self.load_wavetable();
                            }
                        }
                    });

                ui.horizontal_centered(|ui| {

                    ui.add(ParamWidget::<VSlider, ParamHandle<_>>::default(
                        (&self.frame, setter).into(),
                    ));

                    let wavetable = self.wavetable.borrow();

                    let points = PlotPoints::from_ys_f32(
                        wavetable[self.frame.unmodulated_plain_value() as usize].split_last().unwrap().1,
                    );

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

    fn processor_node(self: Arc<Self>) -> Box<ProcessNode> {
        Box::new(self.oscillator())
    }
}