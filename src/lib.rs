#![feature(portable_simd, const_fn_floating_point_arithmetic, const_slice_index, new_uninit, const_float_bits_conv)]

use std::{sync::Arc, num::NonZeroU32};

use arrayvec::ArrayVec;
use dsp::wt_osc::WTOsc;
use plugin_util::simd_util::{MAX_VECTOR_WIDTH, enclosing_div};
use nih_plug::{prelude::*, buffer::SamplesIter};

pub mod dsp;
mod params;

pub const MAX_POLYPHONY: usize = 128;
pub const NUM_VECTORS: usize = enclosing_div(MAX_POLYPHONY, VOICES_PER_VECTOR);
pub const VOICES_PER_VECTOR: usize = MAX_VECTOR_WIDTH / 2;

#[derive(Default)]
pub struct WaveTableOscillator {
    voice_manager: ArrayVec<ArrayVec<u8, VOICES_PER_VECTOR>, NUM_VECTORS>,
    processor: WTOsc,
}

impl WaveTableOscillator {

    fn add_voice(&mut self, note: u8) {

        let vm = &mut self.voice_manager;
        let num_clusters = vm.len();

        if let Some((cluster_idx, ids)) = vm
            .iter_mut()
            .enumerate()
            .find(|(_, ids)| !ids.is_full())
        {
            self.processor.push_voice(note, cluster_idx);
            ids.push(note);

        } else if !vm.is_full() {

            self.processor.push_cluster();
            self.processor.push_voice(note, num_clusters);

            let mut cluster_ids = ArrayVec::default();
            cluster_ids.push(note);
            vm.push(cluster_ids);
        };
    }

    fn handle_event(&mut self, event: NoteEvent<<Self as Plugin>::SysExMessage>) {

        match event {

            NoteEvent::NoteOff { note, .. } => {

                'outer: for (i, ids) in self.voice_manager
                    .iter_mut()
                    .enumerate()
                {
                    for (j, id) in ids.iter().enumerate() {

                        if &note == id {

                            self.processor.remove_voice(i, j);
                            ids.swap_pop(j);

                            if ids.is_empty() {

                                self.processor.remove_cluster(i);
                                self.voice_manager.swap_pop(i);
                            }

                            break 'outer;
                        }
                    }
                }
            },

            NoteEvent::NoteOn { note, .. } => {

                self.add_voice(note);
            },

            _ => (),
        }
    }

    fn process(&mut self, samples: &mut SamplesIter, take: usize) {

        if take != 0 {
            self.processor.param_values.advance_n(take as u32);

            self.processor.update_smoothers(take);
        }

        for mut frame in samples.take(take) {

            let output = self.processor.process_all();

            // SAFETY: the only layout we support is stereo
            unsafe { 
                *frame.get_unchecked_mut(0) = output[0];
                *frame.get_unchecked_mut(1) = output[1];
            }
        }
    }
}

impl Plugin for WaveTableOscillator {
    const NAME: &'static str = "Wavetable Oscillator";

    const VENDOR: &'static str = "AquaEBM";

    const URL: &'static str = "banananaaaa.com";

    const EMAIL: &'static str = "monke@gmail.com";

    const VERSION: &'static str = "0.6.9";

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        }
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;

    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();

    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.processor.params()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {

        self.processor.initialize(buffer_config.sample_rate);
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {

        let block_len = buffer.samples();
        self.processor.update_param_smoothers(block_len.max(32));

        let mut current_sample = 0;

        let mut samples_iter = buffer.iter_samples();
        let samples_iter = samples_iter.by_ref();

        while let Some(event) = context.next_event() {

            let timing = event.timing() as usize;
            
            self.process(samples_iter, timing - current_sample);

            self.handle_event(event);

            current_sample = timing;
        }

        self.process(samples_iter, block_len - current_sample);

        ProcessStatus::Normal
    }

    fn reset(&mut self) {
        self.processor.reset()
    }
}

impl ClapPlugin for WaveTableOscillator {
    const CLAP_ID: &'static str = "com.AquaEBM.WTOSC";

    const CLAP_DESCRIPTION: Option<&'static str> = None;

    const CLAP_MANUAL_URL: Option<&'static str> = None;

    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Stereo
    ];
}

impl Vst3Plugin for WaveTableOscillator {
    const VST3_CLASS_ID: [u8; 16] = *b"bananananananana";

    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
        Vst3SubCategory::Stereo,
    ];
}

// nih_export_clap!(WaveTableOscillator);
nih_export_vst3!(WaveTableOscillator);