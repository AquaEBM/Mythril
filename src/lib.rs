#![feature(portable_simd, new_uninit)]

extern crate alloc;
mod params;

use core::{cell::Cell, num::NonZeroUsize};

use alloc::sync::Arc;
use nih_plug::prelude::*;
use params::MythrilOscParams;

use polygraph::{
    simd_util::{enclosing_div, sum_to_stereo_sample, FLOATS_PER_VECTOR, STEREO_VOICES_PER_VECTOR},
    standalone_processor::StandaloneProcessor,
    voice::StackVoiceManager,
};
use wt_osc::WTOsc;

const MAX_POLYPHONY: usize = 128; // as many voices as there are midi notes
const MAX_NUM_CLUSTERS: usize = enclosing_div(MAX_POLYPHONY, STEREO_VOICES_PER_VECTOR);

#[derive(Default)]
pub struct MythrilOsc {
    processor: StandaloneProcessor<WTOsc, StackVoiceManager<FLOATS_PER_VECTOR>>,
    params: Arc<MythrilOscParams>,
}

impl Plugin for MythrilOsc {
    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;

    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    const HARD_REALTIME_ONLY: bool = false;

    const NAME: &'static str = "MythrilOsc";

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
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let mut current_sample = 0;

        let mut events_remain = true;

        let proc = &mut self.processor;

        let num_samples = buffer.samples();

        while events_remain {
            let event = context.next_event();
            let timing = event.map(|e| e.timing() as usize).unwrap_or_else(|| {
                events_remain = false;
                num_samples
            });

            if let Some(num_samples) = NonZeroUsize::new(timing - current_sample) {
                proc.process(current_sample, num_samples);
                current_sample = timing;
            }

            if let Some(e) = event {
                match e {
                    NoteEvent::NoteOn { note, velocity, .. } => proc.note_on(note, velocity),
                    NoteEvent::NoteOff { note, velocity, .. } => proc.note_off(note, velocity),
                    NoteEvent::Choke { note, .. } => proc.note_free(note),
                    _ => (),
                }
            }
        }

        if let Some(bufs) = proc.get_buffers() {
            let buf = Cell::get_mut(bufs.first_mut().unwrap());

            assert!(buf.len() >= num_samples);

            for (mut output, &mut sample) in buffer.iter_samples().zip(buf) {
                let [l, r] = sum_to_stereo_sample(sample).to_array();

                unsafe {
                    *output.get_unchecked_mut(0) = l;
                    *output.get_unchecked_mut(1) = r;
                }
            }
        }

        ProcessStatus::Normal
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.processor.initialize(
            buffer_config.sample_rate,
            buffer_config.max_buffer_size as usize,
            MAX_NUM_CLUSTERS,
        );

        context.set_current_voice_capacity(MAX_POLYPHONY as u32);

        true
    }

    fn reset(&mut self) {}
}

impl Vst3Plugin for MythrilOsc {
    const VST3_CLASS_ID: [u8; 16] = *b"mythrilsynth_osc";

    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = {
        use Vst3SubCategory::*;

        &[Instrument, Synth, Stereo]
    };
}

impl ClapPlugin for MythrilOsc {
    const CLAP_ID: &'static str = "com.AquaEBM.MythrilOsc";

    const CLAP_DESCRIPTION: Option<&'static str> = None;

    const CLAP_MANUAL_URL: Option<&'static str> = None;

    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    const CLAP_FEATURES: &'static [ClapFeature] = {
        use ClapFeature::*;

        &[Instrument, Sampler, Synthesizer]
    };
}

nih_export_clap!(MythrilOsc);
nih_export_vst3!(MythrilOsc);