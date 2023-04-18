#![feature(portable_simd, stdsimd, const_fn_floating_point_arithmetic, const_slice_index, new_uninit)]

use std::{sync::Arc, num::NonZeroU32, thread, time::Duration};

use arrayvec::ArrayVec;
use dsp::{NUM_VECTORS, wt_osc::WTOscVoiceBlock, sum_to_stereo_sample};
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, EguiState, egui::CentralPanel};
use params::WTOscParams;

mod dsp;
mod params;

use dsp::wavetable::*;

pub struct WaveTableOscillator {
    params: Arc<WTOscParams>,
    table: LenderReciever<BandLimitedWaveTables>,
    oscillators: ArrayVec<WTOscVoiceBlock, NUM_VECTORS>,
}

impl Default for WaveTableOscillator {
    fn default() -> Self {
        let (gui_thr_table, audio_thr_table) = SharedLender::new();

        Self {
            params: Arc::new(WTOscParams::new(gui_thr_table)),
            table: audio_thr_table,
            oscillators: Default::default()
        }
    }
}

impl Plugin for WaveTableOscillator {
    const NAME: &'static str = "AquaEBM";

    const VENDOR: &'static str = "your mom";

    const URL: &'static str = "rule34.com";

    const EMAIL: &'static str = "monke@gmail.com";

    const VERSION: &'static str = "0.6.9";

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(0),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        }
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;

    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    const HARD_REALTIME_ONLY: bool = false;

    type SysExMessage = ();

    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();

        create_egui_editor(EguiState::from_size(520, 250), (), |_,_| (), move |ctx, setter, _| {
            CentralPanel::default().show(ctx, |ui| {
                params.ui(ui, setter);
            });

            thread::sleep(Duration::from_secs_f64(1. / 64.));
        })
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {

        self.params.load_wavetable();
        self.table.update_item();

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {

        self.oscillators.iter_mut().for_each(|osc| osc.update_smoothers(self.params.as_ref()));
        self.table.update_item();

        let mut next_event = context.next_event();

        for (i, mut frame) in buffer.iter_samples().enumerate() {
            while let Some(event) = next_event {

                if event.timing() > i as u32 { break; }

                match event {

                    NoteEvent::NoteOn { note, .. } => {

                        if let Some(osc) = self.oscillators.last_mut().filter(|osc| !osc.is_full()) {
                            osc
                        } else {
                            let _ = self.oscillators.try_push(Default::default());
                            let osc = self.oscillators.last_mut().unwrap(); // garanteed to succeed
                            osc.update_smoothers(self.params.as_ref());

                            osc
                        }.add_voice(note, context.transport().sample_rate);
                    },

                    NoteEvent::NoteOff { note, .. } => {

                        let mut empty_osc = None;

                        for (i, osc) in self.oscillators.iter_mut().enumerate() {
                            if osc.remove_voice(note) {
                                if osc.is_empty() {
                                    empty_osc = Some(i);
                                }
                                break;
                            }
                        }

                        empty_osc.map(|empty_osc_index| self.oscillators.swap_remove(empty_osc_index));
                    },
                    _ => (),
                }

                next_event = context.next_event();
            }

            // SAFETY: the only layout we support is stereo
            unsafe { frame.from_simd_unchecked(
                sum_to_stereo_sample(self.oscillators
                    .iter_mut()
                    .map(|osc| osc.process(self.table.data()))
                    .sum()
                )
            ) };
        }

        ProcessStatus::Normal
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

nih_export_clap!(WaveTableOscillator);