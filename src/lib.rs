#![feature(
    portable_simd,
    const_fn_floating_point_arithmetic,
    const_slice_index,
    new_uninit,
    const_float_bits_conv,
    array_chunks,
    float_next_up_down
)]

extern crate alloc;

mod params;
use alloc::sync::Arc;

use core::{cell::Cell, num::NonZeroUsize, num::NonZeroU32};
use wt_osc::WTOsc;
use crate::params::JadeParamValues;
use nih_plug::{buffer::SamplesIter, prelude::*};
use plugin_util::{simd_util::{enclosing_div, Float, FLOATS_PER_VECTOR, sum_to_stereo_sample}, simd::Simd};

use polygraph::VoiceManager;

pub const MAX_POLYPHONY: usize = 128;
pub const VOICES_PER_VECTOR: usize = FLOATS_PER_VECTOR / 2;
pub const NUM_VECTORS: usize = enclosing_div(MAX_POLYPHONY, VOICES_PER_VECTOR);

#[derive(Default)]
pub struct WaveTableOscillator {
    vm: VoiceManager<VOICES_PER_VECTOR, NUM_VECTORS>,
    processor: WTOsc<JadeParamValues>,
    buffer: Box<[Cell<Float>]>,
}

impl WaveTableOscillator {
    fn handle_event(&mut self, event: NoteEvent<<Self as Plugin>::SysExMessage>) {
        match event {
            NoteEvent::NoteOff { note, .. } => {
                if let Some((i, j)) = self.vm.remove_voice(note) {
                    self.processor.deactivate_voice(i, j);
                    if self.vm.num_voices_in_cluster(i) == 0 {
                        self.processor.deactivate_cluster(i);
                    }
                }
            }

            NoteEvent::NoteOn { note, .. } => {
                if let Some((i, j)) = self.vm.add_voice(note) {
                    if self.vm.num_voices_in_cluster(i) == 1 {
                        self.processor.activate_cluster(i);
                    }
                    self.processor.activate_voice(i, j, note);
                }
            }

            _ => (),
        }
    }

    fn process(&mut self, samples: &mut SamplesIter, take: NonZeroUsize) {
        let inc = Simd::splat(1. / take.get() as f32);
        self.processor.update_smoothers(inc);

        let active_cluster_idxs = self.vm.active_clusters();

        let buffer = unsafe { self.buffer.get_unchecked(..take.get()) };

        if self.processor.process_buffer(active_cluster_idxs, buffer) {
            for (mut frame, samples) in samples.zip(buffer) {
                let output = sum_to_stereo_sample(samples.get());

                // SAFETY: the only layout we support is stereo
                unsafe {
                    *frame.get_unchecked_mut(0) = output[0];
                    *frame.get_unchecked_mut(1) = output[1];
                }
            }
        }
    }
}

impl Plugin for WaveTableOscillator {
    const NAME: &'static str = "Jade";

    const VENDOR: &'static str = "AquaEBM";

    const URL: &'static str = "https://github.com/AquaEBM";

    const EMAIL: &'static str = "AquaEBM@gmail.com";

    const VERSION: &'static str = "0.0.1";

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;

    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();

    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.processor.params().params().clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {

        self.processor.initialize(
            buffer_config.sample_rate,
            buffer_config.max_buffer_size as usize,
        );
        let buffer = Box::new_zeroed_slice(buffer_config.max_buffer_size as usize);

        self.buffer = unsafe { buffer.assume_init() };
        true
    }

    #[inline]
    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let Some(buffer_size) = NonZeroUsize::new(buffer.samples()) else {
            return ProcessStatus::Normal;
        };
        self.processor.update_param_smoothers(buffer_size);

        let mut current_sample = 0;

        let mut samples_iter = buffer.iter_samples();
        let samples_iter = samples_iter.by_ref();

        while let Some(event) = context.next_event() {
            let timing = event.timing() as usize;

            if let Some(block_len) = NonZeroUsize::new(timing - current_sample) {
                self.process(samples_iter, block_len);
            }

            self.handle_event(event);

            current_sample = timing;
        }

        if let Some(last_block_len) = NonZeroUsize::new(buffer_size.get() - current_sample) {
            self.process(samples_iter, last_block_len);
        }

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

    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::Instrument, ClapFeature::Stereo];
}

impl Vst3Plugin for WaveTableOscillator {
    const VST3_CLASS_ID: [u8; 16] = *b"bananananananana";

    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
        Vst3SubCategory::Stereo,
    ];
}

nih_export_clap!(WaveTableOscillator);
nih_export_vst3!(WaveTableOscillator);
