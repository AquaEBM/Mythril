#![feature(portable_simd, stdsimd, const_fn_floating_point_arithmetic, const_slice_index, new_uninit, const_float_bits_conv)]

use std::{sync::Arc, num::NonZeroU32};

use arrayvec::ArrayVec;
use dsp::wt_osc::WTOscVoice;
use plugin_util::simd_util::{sum_to_stereo_sample, MAX_VECTOR_WIDTH, enclosing_div};
use nih_plug::prelude::*;
use params::WTOscParams;

pub mod dsp;
mod params;

pub const MAX_POLYPHONY: usize = 128;
pub const NUM_VECTORS: usize = enclosing_div(MAX_POLYPHONY, VOICES_PER_VECTOR);
pub const VOICES_PER_VECTOR: usize = MAX_VECTOR_WIDTH / 2;

#[derive(Default)]
pub struct WaveTableOscillator {
    params: Arc<WTOscParams>,
    voice_manager: ArrayVec<ArrayVec<u8, VOICES_PER_VECTOR>, NUM_VECTORS>,
    oscillators: ArrayVec<WTOscVoice, NUM_VECTORS>,
}

impl WaveTableOscillator {
    pub fn add_voice(&mut self, note: u8, sr: f32) {

        if let Some(osc) = self.oscillators.last_mut().filter(|osc| !osc.is_full()) {

            osc.add_voice(note, sr);
        } else if !self.oscillators.is_full() {
            self.oscillators.push(self.params.create_processor());
            let osc = self.oscillators.last_mut().unwrap(); // never panics because we just pushed an element
            osc.add_voice(note, sr);
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
            main_input_channels: NonZeroU32::new(0),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        }
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;

    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();

    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {

        self.params.load_wavetable();
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {

        let block_len = buffer.samples().max(32);

        self.oscillators.iter_mut().for_each(|osc| osc.update_smoothers(self.params.as_ref(), block_len));

        let mut next_event = context.next_event();

        for (i, mut frame) in buffer.iter_samples().enumerate() {
            while let Some(event) = next_event {

                if event.timing() > i as u32 { break; }

                match event {

                    NoteEvent::NoteOff { note, .. } => {

                        'outer: for (ids, oscs) in self.voice_manager
                            .iter()
                            .zip(self.oscillators.iter_mut())
                        {
                            for (i, id) in ids.iter().enumerate() {
                                if &note == id {
                                    oscs.remove_voice(i);
                                    break 'outer;
                                }
                            }
                        }
                    },

                    NoteEvent::NoteOn { note, .. } => {

                        self.add_voice(note, context.transport().sample_rate);
                    },

                    _ => (),
                }

                next_event = context.next_event();
            }

            let output = sum_to_stereo_sample(self.oscillators
                .iter_mut()
                .map(WTOscVoice::process)
                .sum()
            );

            // SAFETY: the only layout we support is stereo
            unsafe { 
                *frame.get_unchecked_mut(0) = output[0];
                *frame.get_unchecked_mut(1) = output[1];
            }
        }

        ProcessStatus::Normal
    }

    fn reset(&mut self) {
        self.oscillators.clear()
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

nih_export_clap!(WaveTableOscillator);
nih_export_vst3!(WaveTableOscillator);