use super::*;
use util::semitones_to_ratio;

#[derive(Default, Clone, Copy)]
pub struct Oscillator {
    /// phase delta before unison detuning, transposition
    pub base_phase_delta: Float,
    phase_delta: LogSmoother,
    phase: UInt,
    old_frame: UInt,
    new_frame: UInt,
}

impl Oscillator {

    pub fn advance_phase(&mut self) -> UInt {

        let phase_delta_fixed_point = flp_to_fxp(self.phase_delta.get_current());

        self.phase += phase_delta_fixed_point;

        phase_delta_fixed_point
    }

    pub fn randomize_phase(&mut self, randomisation: Float) {

        let mut phase = Simd::splat(0.);

        as_mut_stereo_sample_array(&mut phase)
            .iter_mut()
            .for_each(|sample| *sample = Simd::splat(rand::random()));

        self.phase = flp_to_fxp(phase * randomisation);
    }

    pub fn update_phase_delta_smoother(&mut self) {
        self.phase_delta.tick()
    }

    pub fn reset_phase(&mut self) {
        self.phase = Simd::splat(0);
    }

    pub fn set_detune_semitones_smoothed(&mut self, semitones: Float, num_samples: usize) {
        let detune_ratio = semitones_to_ratio(semitones);
        self.phase_delta.set_target(self.base_phase_delta * detune_ratio, num_samples);
    }

    pub fn set_detune_semitones(&mut self, semitones: Float) {
        self.phase_delta.set_instantly(self.base_phase_delta * semitones_to_ratio(semitones));
    }

    pub fn set_frame_for_smoothing(&mut self, frame: UInt) {
        self.old_frame = self.new_frame;
        self.new_frame = frame;
    }

    pub fn set_frame(&mut self, frame: UInt) {
        self.old_frame = frame;
        self.new_frame = frame;
    }

    pub fn advance_and_resample_select(&mut self, table: &BandLimitedWaveTables, mask: TMask) -> Float {
        self.update_phase_delta_smoother();
        let phase_delta = self.advance_phase();
        table.resample_select(phase_delta, self.new_frame, self.phase, mask)
    }

    pub fn advance_and_resample(&mut self, table: &BandLimitedWaveTables) -> Float {
        self.update_phase_delta_smoother();
        let phase_delta = self.advance_phase();
        table.resample(phase_delta, self.new_frame, self.phase)
    }
}