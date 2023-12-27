use super::{*, wavetable::BandLimitedWaveTables};
use crate::NUM_VECTORS;
use core::{array, cell::Cell};
use std::{sync::Arc, path::Path};
use nih_plug::prelude::Param;
use params::WTOscParams;

mod cluster;
mod oscillator;
pub mod voice;

use cluster::WTOscVoiceCluster;

#[derive(Default)]
pub struct WTOscParamValues {
    detune: LinearSmoother,
    transpose: LinearSmoother,
    frame: UInt,
    random: LinearSmoother,
    starting_phases: CenterDetuned<Float>,
    level: LinearSmoother,
    stereo: LinearSmoother,
    pan: LinearSmoother,
    num_unison_voices: UInt,
    sr: f32,
}

impl WTOscParamValues {

    fn update_starting_phases(&mut self) {
        self.starting_phases.all_mut().for_each(|phase| {
            let random = Simd::<f32, STEREO_VOICES_PER_VECTOR>::from_array(
                array::from_fn(|_i| rand::random())
            );

            const DOUBLE_SIZE: [usize ; FLOATS_PER_VECTOR] = {
                let mut array = [0 ; FLOATS_PER_VECTOR];
                let mut i = 0;
                while i < STEREO_VOICES_PER_VECTOR {
                    array[2 * i] = i;
                    array[2 * i + 1] = i;
                    i += 1;
                }

                array
            };
            
            *phase = simd_swizzle!(random, DOUBLE_SIZE);
        });
    }

    fn get<P: Param>(param: &P) -> P::Plain {
        param.unmodulated_plain_value()
    }

    fn get_splat<P: Param>(p: &P) -> Simd<P::Plain, FLOATS_PER_VECTOR>
    where
        P::Plain: SimdElement
    {
        Simd::splat(Self::get(p))
    }

    pub fn update_smoothers(&mut self, p: &WTOscParams, num_samples: usize) {

        self.detune.set_target(Simd::splat(Self::get(&p.detune) * Self::get(&p.detune_range)), num_samples);
        self.transpose.set_target(Self::get_splat(&p.transpose), num_samples);
        self.frame = Simd::splat(Self::get(&p.frame) as u32);
        self.random.set_target(Self::get_splat(&p.random), num_samples);
        self.level.set_target(Self::get_splat(&p.level), num_samples);
        self.stereo.set_target(Self::get_splat(&p.stereo_unison), num_samples);
        self.pan.set_target(Self::get_splat(&p.pan), num_samples);
        self.num_unison_voices = Simd::splat(Self::get(&p.num_unison_voices) as u32);
    }

    pub fn update_values(&mut self, p: &WTOscParams) {
        self.detune.set_instantly(Simd::splat(Self::get(&p.detune) * Self::get(&p.detune_range)));
        self.transpose.set_instantly(Self::get_splat(&p.transpose));
        self.frame = Simd::splat(Self::get(&p.frame) as u32);
        self.random.set_instantly(Self::get_splat(&p.random));
        self.level.set_instantly(Self::get_splat(&p.level));
        self.stereo.set_instantly(Self::get_splat(&p.stereo_unison));
        self.pan.set_instantly(Self::get_splat(&p.pan));
        self.num_unison_voices = Simd::splat(Self::get(&p.num_unison_voices) as u32);
    }

    pub fn advance_n(&mut self, n: u32) {
        self.detune.tick_n(n);
        self.transpose.tick_n(n);
        self.random.tick_n(n);
        self.level.tick_n(n);
        self.stereo.tick_n(n);
        self.pan.tick_n(n);
    }
}

pub struct WTOsc {

    params: Arc<WTOscParams>,
    pub param_values: WTOscParamValues,
    table_reciever: LenderReciever<BandLimitedWaveTables>,
    table: Arc<BandLimitedWaveTables>,
    clusters: [WTOscVoiceCluster ; NUM_VECTORS],
}

impl WTOsc {

    pub fn new(params: Arc<WTOscParams>, table_reciever: LenderReciever<BandLimitedWaveTables>) -> Self {

        let mut output = Self {
            params,
            table_reciever,
            table: BandLimitedWaveTables::with_frame_count(0),
            clusters: Default::default(),
            param_values: Default::default(),
        };

        output.param_values.update_starting_phases();

        output
    }

    pub fn activate_voice(
        &mut self,
        cluster_idx: usize,
        voice_idx: usize,
        note: u8,
    ) -> Option<bool> {
        self.clusters.get_mut(cluster_idx).map(
            |cluster| cluster.activate_voice(&self.param_values, cluster_idx, voice_idx, note)
        )
    }

    pub fn deactivate_voice(&mut self, cluster_idx: usize, voice_idx: usize) -> Option<bool> {

        self.clusters
            .get_mut(cluster_idx)
            .map(|cluster| cluster.deactivate_voice(voice_idx))
    }

    pub fn activate_cluster(&mut self, index: usize) -> bool {

        if let Some(cluster) = self.clusters.get_mut(index) {
            cluster.activate(
                &self.param_values,
                index
            );
            return true;
        }

        false
    }

    pub fn deactivate_cluster(&mut self, index: usize) -> bool {

        if let Some(cluster) = self.clusters.get_mut(index) {
            cluster.deactivate();
            return true;
        }
        false
    }

    pub fn update_smoothers(&mut self, num_samples: usize) {

        let param_values = &mut self.param_values;

        param_values.advance_n(num_samples as u32);

        self.clusters
            .iter_mut()
            .enumerate()
            .for_each(|(i, cluster)| cluster.set_params_smoothed(param_values, i, num_samples))
    }

    pub fn reset(&mut self) {
        self.clusters.iter_mut().for_each(WTOscVoiceCluster::reset);
    }

    pub fn load_wavetable_non_realtime(&mut self, path: impl AsRef<Path>) {
        self.table = BandLimitedWaveTables::from_file(path);
    }

    pub fn load_default_wavetable(&mut self) {
        self.load_wavetable_non_realtime(
            concat!(
                include_str!("../../wavetable_folder_path.txt"),
                "/Basic Shapes.WAV"
            )
        );
    }

    pub fn initialize(&mut self, sr: f32) {
        self.param_values.sr = sr;
        self.param_values.update_values(&self.params);
        self.load_default_wavetable();
    }

    pub fn update_param_smoothers(&mut self, num_samples: usize) {
        self.param_values.update_smoothers(&self.params, num_samples);
        if let Some(table) = self.table_reciever.recv_latest() {
            self.table = table;
        }
    }

    pub fn process_buffer(
        &mut self,
        mut active_cluster_idxs: impl Iterator<Item = usize>,
        buffer: &[Cell<Float>]
    ) -> bool {
        let table = self.table.as_ref();

        if let Some(i) = active_cluster_idxs.next() {

            let cluster = unsafe { self.clusters.get_unchecked_mut(i) };
            
            for sample in buffer {
                sample.set(cluster.process(table));    
            }
        } else {
            return false;
        }

        for i in active_cluster_idxs {
            let cluster = unsafe { self.clusters.get_unchecked_mut(i) };
            
            for sample in buffer {
                sample.set(sample.get() + cluster.process(table));    
            }
        }

        true
    }

    #[inline]
    pub fn process_all(&mut self) -> f32x2 {

        let table = self.table.as_ref();

        sum_to_stereo_sample(
            self.clusters
                .iter_mut()
                .map(|cluster| cluster.process(table))
                .sum()
        )
    }

    pub fn params(&self) -> Arc<WTOscParams> {
        self.params.clone()
    }
}