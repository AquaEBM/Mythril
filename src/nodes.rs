use atomic_refcell::AtomicRefCell;
pub use nih_plug::{formatters::*, prelude::*};
pub use nih_plug_egui::{
    egui::*,
    EguiState,
    create_egui_editor
};

use parking_lot::Mutex;
use plugin_util::{
    gui::widgets::*,
    parameter::ParamHandle,
};

use rtrb::{Consumer, Producer};
pub use std::sync::Arc;

use std::{any::Any, simd::f32x2};

pub trait Processor {

    fn add_voice(&mut self, norm_freq: f32);

    fn remove_voice(&mut self, voice_idx: usize);

    fn process(&mut self, input: f32x2, voice_idx: usize, editor_open: bool) -> f32x2;

    fn initialize(&mut self, sample_rate: f32) -> (bool, u32);

    fn reset(&mut self);

    fn update_smoothers(&mut self);
}

pub type ProcessNode = dyn Processor + Send;

pub trait SeenthNode: Params + Any {
    fn type_name(&self) -> &'static str;

    fn ui(&self, ui: &mut Ui, setter: &ParamSetter) -> Response;

    fn processor_node(self: Arc<Self>) -> Box<ProcessNode>;
}

pub trait SeenthStandAlonePlugin: SeenthNode + Default {
    type Processor: Processor + Send;

    fn processor(self: Arc<Self>) -> Self::Processor;
    fn editor_state(&self) -> Arc<EguiState>;
}

pub const MAX_POLYPHONY: usize = 16;

pub mod audio_graph;
pub mod wavetable_oscillator;