use super::{*, wavetable::{BandLimitedWaveTables, LenderReciever}};
use crate::NUM_VECTORS;
use std::sync::Arc;
use arrayvec::ArrayVec;
use nih_plug::prelude::Param;
use params::WTOscParams;
use smoothing::*;

mod cluster;
mod oscillator;
pub mod voice;
mod util;

use cluster::WTOscVoiceCluster;

#[derive(Default)]
pub struct WTOscParamValues {
    detune: LinearSmoother,
    transpose: LinearSmoother,
    frame: UInt,
    random: LinearSmoother,
    level: LinearSmoother,
    stereo: LinearSmoother,
    pan: LinearSmoother,
    num_unison_voices: UInt,
    sr: f32,
}

impl WTOscParamValues {

    fn get<P: Param>(param: &P) -> P::Plain {
        param.unmodulated_plain_value()
    }

    fn get_splat<P: Param>(p: &P) -> Simd<P::Plain, MAX_VECTOR_WIDTH>
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
    table: LenderReciever<BandLimitedWaveTables>,
    clusters: ArrayVec<WTOscVoiceCluster, NUM_VECTORS>,
}

impl Default for WTOsc {
    fn default() -> Self {
        let params = Default::default();

        Self::new(params)
    }
}

impl WTOsc {

    pub fn new(params: Arc<WTOscParams>) -> Self {

        let table = {
            let mut lock = params.wavetable.lock().expect("Issue unlocking the lock");
            lock.create_new_reciever()
        };

        Self {
            params,
            table,
            clusters: Default::default(),
            param_values: Default::default(),
        }
    }

    pub fn remove_voice(&mut self, cluster_idx: usize, voice_idx: usize) {

        self.clusters.get_mut(cluster_idx).and_then(|cluster| cluster.remove_voice(voice_idx));
    }

    pub fn push_cluster(&mut self) {

        (!self.clusters.is_full()).then(|| {

            let cluster = WTOscVoiceCluster::from_param_values(
                &self.param_values,
                self.clusters.len()
            );
            self.clusters.push(cluster);
        });
    }

    pub fn push_voice(&mut self, note: u8, cluster_idx: usize) {
        self.clusters.get_mut(cluster_idx).and_then(
            |cluster| cluster.push_voice(&self.param_values, cluster_idx, note)
        );
    }

    pub fn update_smoothers(&mut self, num_samples: usize) {

        let param_values = &self.param_values;

        self.clusters
            .iter_mut()
            .enumerate()
            .for_each(|(i, cluster)| cluster.update_smoothers(param_values, i, num_samples))
    }

    pub fn reset(&mut self) {
        self.clusters.iter_mut().for_each(WTOscVoiceCluster::reset);
    }

    pub fn initialize(&mut self, sr: f32) {
        self.param_values.sr = sr;
        self.param_values.update_values(&self.params);
        self.params.load_wavetable();
    }

    pub fn update_param_smoothers(&mut self, num_samples: usize) {
        self.param_values.update_smoothers(&self.params, num_samples);
        self.table.update_item();
    }

    pub fn process_all(&mut self) -> f32x2 {

        let table = unsafe { self.table.current() };
        
        sum_to_stereo_sample(
            self.clusters
                .iter_mut()
                .map(|block| block.process(table))
                .sum()
        )
    }

    pub fn params(&self) -> Arc<WTOscParams> {
        self.params.clone()
    }
}