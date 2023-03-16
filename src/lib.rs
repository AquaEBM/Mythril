#![feature(array_chunks, once_cell, portable_simd)]

pub mod nodes;
use std::{thread, time::Duration};

use arrayvec::ArrayVec;
use nodes::*;

pub struct SeenthPlugin<T: SeenthStandAlonePlugin, const VOICES: usize = MAX_POLYPHONY> {
    voice_handler: ArrayVec<u8, VOICES>,
    params: Arc<T>,
    processor: T::Processor,
}

impl<T: SeenthStandAlonePlugin, const N: usize> Default for SeenthPlugin<T, N> {
    fn default() -> Self {

        let params: Arc<T> = Default::default();

        Self {
            voice_handler: Default::default(),
            params: params.clone(),
            processor: params.processor()
        }
    }
}

impl<T: SeenthStandAlonePlugin, const VOICES: usize> Plugin for SeenthPlugin<T, VOICES> {
    const NAME: &'static str = "Seenth Plugin";

    const VENDOR: &'static str = "AquaEBM";

    const URL: &'static str = "google.com";

    const EMAIL: &'static str = "monke@monke.com";

    const VERSION: &'static str = "0.6.9";

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        }
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;

    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> { self.params.clone() }

    fn editor(&self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        create_egui_editor(params.editor_state(), (), |_, _| (), move |ctx, setter, _| {
            CentralPanel::default().show(ctx, |ui| {
                params.ui(ui, setter);
            });

            thread::sleep(Duration::from_secs_f32(1. / 72.))
        })
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {

        let (success, latency) = self.processor.initialize(buffer_config.sample_rate);
        context.set_latency_samples(latency);
        success
    }

    fn reset(&mut self) {

        self.processor.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let mut next_event = context.next_event();

        self.processor.update_smoothers();

        for (i, mut input_frame) in buffer.iter_samples().enumerate() {
            while let Some(event) = next_event {

                if event.timing() > i as u32 { break; }

                match event {

                    NoteEvent::NoteOn { note, .. } => {

                        if let Ok(()) = self.voice_handler.try_push(note) {
                            self.processor.add_voice(
                                nih_plug::util::midi_note_to_freq(
                                    note
                                ) / context.transport().sample_rate,
                            );
                        };
                    }

                    NoteEvent::NoteOff { note, .. } => {

                        for (i, &id) in self.voice_handler.iter().enumerate() {
                            if note == id {
                                self.voice_handler.swap_remove(i);
                                self.processor.remove_voice(i);
                                break;
                            }
                        }
                    }
                    _ => (),
                }
                next_event = context.next_event();
            }

            // the only audio layout we support is stereo
            let input_frame_simd = unsafe { input_frame.to_simd_unchecked() };

            input_frame.from_simd(
                (0..self.voice_handler.len()).map( |i|
                    self.processor.process(input_frame_simd, i, false)
                ).sum()
            );
        }
        ProcessStatus::Normal
    }
}

impl<T: SeenthStandAlonePlugin, const N: usize> Vst3Plugin for SeenthPlugin<T, N> {
    const VST3_CLASS_ID: [u8; 16] = *b"0123456789012345";

    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[];
}

impl<T: SeenthStandAlonePlugin, const N: usize> ClapPlugin for SeenthPlugin<T, N> {
    const CLAP_ID: &'static str = "lol";

    const CLAP_DESCRIPTION: Option<&'static str> = None;

    const CLAP_MANUAL_URL: Option<&'static str> = None;

    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    const CLAP_FEATURES: &'static [ClapFeature] = &[];    
}

nih_export_vst3!(SeenthPlugin<wavetable_oscillator::WTOscParams>);

// find a way to use SIMD generically over vector size
// build audio graph GUI
// add different voice prioritization algorithms
// support pitch wheel, mod wheel and MIDI CCs
