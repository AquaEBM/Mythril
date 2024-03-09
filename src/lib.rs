#![feature(portable_simd, new_uninit)]

mod params;
extern crate alloc;

use alloc::sync::Arc;
use nih_plug::prelude::*;
use params::MythrilParameters;
use polygraph::{
    buffer::OwnedBuffer,
    processor::{new_vfloat_buffer, Processor},
    simd_util::{enclosing_div, Float, STEREO_VOICES_PER_VECTOR},
};
use wt_osc::WTOsc;

pub const MAX_POLYPHONY: usize = 128;
pub const MAX_NUM_CLUSTERS: usize = enclosing_div(MAX_POLYPHONY, STEREO_VOICES_PER_VECTOR);

pub struct Mythril {
    buffer: OwnedBuffer<Float>,
    processor: WTOsc,
    params: Arc<MythrilParameters>,
}

impl Default for Mythril {
    fn default() -> Self {
        Self {
            buffer: new_vfloat_buffer(0),
            processor: Default::default(),
            params: Default::default(),
        }
    }
}

impl Mythril {}

impl Plugin for Mythril {
    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;

    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    const HARD_REALTIME_ONLY: bool = false;

    const NAME: &'static str = "Mythril";

    const VENDOR: &'static str = "AquaEBM";

    const URL: &'static str = "https://github.com/AquaEBM";

    const EMAIL: &'static str = "AquaEBM@gmail.com";

    const VERSION: &'static str = "0.0.0";

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(0),
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    }];

    type SysExMessage = ();

    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        ProcessStatus::Normal
    }

    fn initialize(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.processor.initialize(
            buffer_config.sample_rate,
            buffer_config.max_buffer_size as usize,
            MAX_NUM_CLUSTERS,
        );

        true
    }

    fn reset(&mut self) {}
}
