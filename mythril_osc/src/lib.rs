#![feature(portable_simd, new_uninit, as_array_of_cells, slice_flatten)]

extern crate alloc;

mod basic_shapes;
mod cluster;
mod oscillator;
mod voice;
pub mod wavetable;

use cluster::{WTOscClusterNormParams, WTOscVoiceCluster};
use core::{array, cell::Cell, f32::consts::FRAC_1_SQRT_2, iter, mem, num::NonZeroUsize};
use polygraph::{
    buffer::Buffers,
    processor::{Parameters, Processor},
    simd_util::{
        math::*,
        simd::{prelude::*, Simd, StdFloat},
        smoothing::*,
        *,
    },
};
use voice::VoiceParams;
use wavetable::BandLimitedWaveTables;

pub const MAX_UNISON: usize = 16;
pub const PITCH_RANGE_SEMITONES: f32 = 48.0;
const OSCS_PER_VOICE: usize = enclosing_div(MAX_UNISON, FLOATS_PER_VECTOR);
const NUM_PARAMS: u64 = 9;
const MAX_PARAM_INDEX: u64 = NUM_PARAMS - 1;
pub static DEFAULT_PARAMS: [f32x2; NUM_PARAMS as usize] = [
    f32x2::from_array([FRAC_1_SQRT_2; 2]), // level
    f32x2::from_array([0.0; 2]),           // frame
    f32x2::from_array([0.0; 2]),           // num_voices
    f32x2::from_array([0.5; 2]),           // detune
    f32x2::from_array([0.5; 2]),           // pan
    f32x2::from_array([0.5; 2]),           // transpose
    f32x2::from_array([1.0; 2]),           // stereo
    f32x2::from_array([1.0 / 48.0; 2]),    // detune range
    f32x2::from_array([1.0; 2]),           // random amount
];

#[derive(Default)]
pub struct WTOsc {
    table: Box<BandLimitedWaveTables>,
    starting_phases: [Float; OSCS_PER_VOICE],
    sr: f32,
    log2_alpha: f32,
    scratch_buffer: Box<[Float]>,
    clusters: Box<[WTOscVoiceCluster]>,
    params: Box<[WTOscClusterNormParams]>,
}

impl WTOsc {
    pub fn replace_table(
        &mut self,
        table: Box<BandLimitedWaveTables>,
    ) -> Box<BandLimitedWaveTables> {
        mem::replace(&mut self.table, table)
    }

    pub fn replace_starting_phases(
        &mut self,
        starting_phases: [f32; MAX_UNISON],
    ) -> [f32; MAX_UNISON] {
        unsafe {
            mem::transmute(mem::replace(
                &mut self.starting_phases,
                mem::transmute(starting_phases),
            ))
        }
    }
}

impl Processor for WTOsc {
    type Sample = Float;

    fn audio_io_layout(&self) -> (usize, usize) {
        (0, 1)
    }

    fn process(
        &mut self,
        mut buffers: Buffers<Self::Sample>,
        cluster_idx: usize,
        params: &dyn Parameters<Float>,
    ) -> TMask {
        let table = self.table.as_ref();

        if let Some((output_buf, num_frames)) = buffers
            .get_output(0)
            .zip(NonZeroUsize::new(table.num_frames()))
        {
            let buffer_size = output_buf.len();
            let smooth_dt = Float::splat(1.0 / buffer_size as f32);

            let cluster = &mut self.clusters[cluster_idx];
            let cluster_params = &mut self.params[cluster_idx];

            cluster_params.tick_n(self.log2_alpha, buffer_size);

            let num_frames_f = Float::splat(num_frames.get() as f32);

            for (voice_index, voice) in cluster.active_voices() {
                let (voice_params, num_oscs) =
                    VoiceParams::new(voice_index, cluster_params).unwrap();

                let (first_osc, other_oscs) = unsafe { voice.get_unchecked_mut(..num_oscs.get()) }
                    .split_first_mut()
                    .unwrap();

                let mask = first_osc.set_params_smoothed(&voice_params, 0, num_frames_f, smooth_dt);
                let voice_samples = split_stereo_slice_mut(output_buf)
                    .as_flattened_mut()
                    .iter_mut()
                    .skip(voice_index)
                    .step_by(STEREO_VOICES_PER_VECTOR);

                if OSCS_PER_VOICE > 1 {
                    let scratch_buffer = &mut self.scratch_buffer[..buffer_size];

                    for sample in scratch_buffer.iter_mut() {
                        *sample = unsafe { first_osc.tick_all(table, mask) };
                    }

                    for (osc, osc_index) in other_oscs.iter_mut().zip(1..) {
                        let mask = osc.set_params_smoothed(
                            &voice_params,
                            osc_index,
                            num_frames_f,
                            smooth_dt,
                        );

                        for sample in scratch_buffer.iter_mut() {
                            *sample += unsafe { osc.tick_all(table, mask) };
                        }
                    }

                    for (out_sample, &scratch) in voice_samples.zip(scratch_buffer.iter()) {
                        *out_sample = sum_to_stereo_sample(scratch);
                    }
                } else {
                    // On devices with vectors that can hold as many or more floats
                    // as there are unison voices (e. g. AVX-512 for 16 voices)
                    // a scratch buffer wouldn't be necessary
                    for out_sample in voice_samples {
                        let output = unsafe { first_osc.tick_all(table, mask) };
                        *out_sample = sum_to_stereo_sample(output);
                    }
                }
            }

            cluster.set_weights_smoothed(cluster_params, smooth_dt);

            for poly_sample in output_buf {
                let (normal, flipped) = cluster.get_sample_weights();
                cluster.tick_weight_smoothers();
                let sample = *poly_sample;
                let out = sample * normal + swap_stereo(sample) * flipped;
                *poly_sample = out;
            }
        }

        TMask::splat(false)
    }

    fn initialize(&mut self, sr: f32, max_buffer_size: usize, max_num_clusters: usize) -> usize {
        self.sr = sr;

        // reach the target value (0.999%) in approximately 20ms
        const BASE_LOG2_ALPHA: f32 = -500.0;

        self.log2_alpha = BASE_LOG2_ALPHA / sr;

        self.clusters = iter::repeat_with(Default::default)
            .take(max_num_clusters)
            .collect();

        self.params = iter::repeat_with(Default::default)
            .take(max_num_clusters)
            .collect();

        // On devices with vectors that can hold as many or more floats as there are unison voices
        // (e. g. AVX-512 for 16 voices) a scratch buffer wouldn't be necessary
        self.scratch_buffer = unsafe {
            Box::new_uninit_slice((OSCS_PER_VOICE > 1) as usize * max_buffer_size).assume_init()
        };

        0
    }

    fn reset(&mut self, cluster_idx: usize, voice_mask: TMask, params: &dyn Parameters<Float>) {
        let random = self.params[cluster_idx].random.current;
        self.clusters[cluster_idx].reset_phases(voice_mask, random, &self.starting_phases);
    }

    fn move_state(
        &mut self,
        (from_cluster, from_voice): (usize, usize),
        (to_cluster, to_voice): (usize, usize),
    ) {
        (from_voice < STEREO_VOICES_PER_VECTOR && to_voice < STEREO_VOICES_PER_VECTOR)
            .then(|| {
                let clusters = Cell::from_mut(self.clusters.as_mut()).as_slice_of_cells();
                let params = Cell::from_mut(self.params.as_mut()).as_slice_of_cells();

                unsafe {
                    WTOscVoiceCluster::move_state_unchecked(
                        &clusters[from_cluster],
                        from_voice,
                        &clusters[to_cluster],
                        to_voice,
                    );

                    WTOscClusterNormParams::move_state_unchecked(
                        &params[from_cluster],
                        from_voice,
                        &params[to_cluster],
                        to_voice,
                    );
                }
            })
            .expect("out of bounds voice indices")
    }

    fn set_voice_notes(
        &mut self,
        cluster_idx: usize,
        voice_mask: TMask,
        _velocity: Float,
        note: UInt,
    ) {
        let a4_phase_delta = Simd::splat(440. / self.sr);
        let nice = Simd::splat(69);
        let a4_detune_semitones = note.cast::<i32>() - nice;
        let new_phase_delta = a4_phase_delta * semitones_to_ratio(a4_detune_semitones.cast());

        let params = &mut self.params[cluster_idx];

        let ratio = voice_mask.select(new_phase_delta / params.phase_delta, Simd::splat(1.0));

        params.set_base_phase_delta(new_phase_delta, voice_mask);

        self.clusters[cluster_idx].scale_phase_deltas(ratio);
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use std::io::{self, Write};

    use polygraph::{
        buffer::new_vfloat_buffer,
        buffer::{BufferHandleLocal, OutputBufferIndex},
    };

    #[test]
    pub fn test() {
        const MAX_BUFFER_SIZE: usize = 256;
        const CLUSTER_IDX: usize = 0;

        let mut osc = WTOsc::default();
        osc.initialize(44100., MAX_BUFFER_SIZE, 1);
        let voice_mask = TMask::splat(true);

        let mut wt = Box::<BandLimitedWaveTables>::from(basic_shapes::WAVETABLES.as_slice());
        osc.replace_table(wt);

        let mut starting_phases = [0.0; MAX_UNISON];
        osc.replace_starting_phases(starting_phases);

        let mut notes = Simd::splat(0);
        let notes_stereo = split_stereo_mut(&mut notes);
        for (i, note) in notes_stereo.iter_mut().enumerate() {
            *note = u32x2::splat(9 + 12 * i as u32);
        }

        osc.reset(CLUSTER_IDX, voice_mask, &());
        osc.set_voice_notes(CLUSTER_IDX, voice_mask, Float::splat(1.0), notes);

        let mut intermediate_buffers = Box::new([new_vfloat_buffer::<Float>(MAX_BUFFER_SIZE)]);

        let buffers = BufferHandleLocal::toplevel(intermediate_buffers.as_mut())
            .with_indices(&[], &[Some(OutputBufferIndex::Local(0))])
            .with_buffer_pos(0, NonZeroUsize::new(MAX_BUFFER_SIZE).unwrap());

        osc.process(buffers, CLUSTER_IDX, &());

        let mut stdout = io::stdout().lock();

        writeln!(stdout, "[").unwrap();

        let (last, samples) = Cell::get_mut(intermediate_buffers[0].as_mut())
            .split_last_mut()
            .unwrap();

        for sample in samples.iter() {
            writeln!(stdout, "{sample:?},").unwrap();
        }

        writeln!(stdout, "{last:?}]").unwrap();
    }
}
