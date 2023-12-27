use hound::{SampleFormat, WavReader};
use realfft::{RealFftPlanner, num_complex::Complex32};
use std::{path::Path, mem};

use super::*;

#[repr(transparent)]
pub struct BandLimitedWaveTables {
    data: [[[f32 ; Self::TABLE_SIZE] ; Self::NUM_MIPMAPS]]
}

impl BandLimitedWaveTables {

    #[inline]
    pub fn as_slice(&self) -> &[[[f32 ; Self::TABLE_SIZE] ; Self::NUM_MIPMAPS]] {
        &self.data
    }

    pub fn with_frame_count(num_frames: usize) -> Arc<Self> {
        // SAFETY: zero (0.0) is a valid float value
        unsafe {
            mem::transmute::<Arc<[[[f32 ; Self::TABLE_SIZE] ; Self::NUM_MIPMAPS]]>, Arc<Self>>(
                Arc::new_zeroed_slice(num_frames).assume_init()
            )
        }
    }

    #[inline]
    fn as_mut_slice(&mut self) -> &mut [[[f32 ; Self::TABLE_SIZE] ; Self::NUM_MIPMAPS]] {
        &mut self.data
    }

    #[inline]
    fn as_ptr(&self) -> *const f32 {
        self.as_slice().as_ptr().cast()
    }

    /// How many octaves of frequency content our wavetables have, this
    /// is also the base 2 logarithm of the number of samples in each frame
    pub const NUM_OCTAVES: usize = 11;
    const V_NUM_OCTAVES: UInt = const_splat(Self::NUM_OCTAVES as u32);
    /// number of elements in each mipmap
    pub const TABLE_SIZE: usize = 1 << Self::NUM_OCTAVES;
    /// fractional part bits
    const FRACT_BITS: UInt = const_splat(u32::BITS - Self::NUM_OCTAVES as u32);
    const PHASE_MASK: UInt = const_splat(Self::TABLE_SIZE as u32 - 1);
    pub const NUM_MIPMAPS: usize = Self::NUM_OCTAVES + 1;
    const V_NUM_MIPMAPS: UInt = const_splat(Self::NUM_OCTAVES as u32 + 1);

    #[inline]
    fn get_resample_data(phase: UInt, frame: UInt, phase_delta: UInt) -> (Float, UInt, UInt) {
        let octaves = map(phase_delta, u32::leading_zeros).simd_min(Self::V_NUM_OCTAVES);

        let fract = fxp_to_flp(phase << Self::V_NUM_OCTAVES);

        let table_start = octaves + frame * Self::V_NUM_MIPMAPS << Self::V_NUM_OCTAVES;

        const ONE: UInt = const_splat(1);

        let phase_a = phase >> Self::FRACT_BITS;
        let phase_b = phase_a + ONE & Self::PHASE_MASK;

        (fract, table_start + phase_a, table_start + phase_b)
    }

    #[inline]
    pub fn resample_select(&self, phase_delta: UInt, frame: UInt, phase: UInt, mask: Mask) -> Float {

        let (fract, start_idx, end_idx) = Self::get_resample_data(phase, frame, phase_delta);

        let this = self.as_ptr();

        const ZERO_F: Float = const_splat(0.);

        let (a, b) = unsafe { (
            gather_select_unchecked(this, start_idx, mask, ZERO_F),
            gather_select_unchecked(this, end_idx, mask, ZERO_F)
        ) };

        lerp(a, b, fract)
    }

    #[inline]
    pub fn resample(&self, phase_delta: UInt, frame: UInt, phase: UInt) -> Float {

        let (fract, start_idx, end_idx) = Self::get_resample_data(phase, frame, phase_delta);

        let this = self.as_ptr();

        let (a, b) = unsafe { (
            gather_unchecked(this, start_idx),
            gather_unchecked(this, end_idx)
        ) };

        lerp(a, b, fract)
    }

    pub fn from_file(path: impl AsRef<Path>) -> Arc<Self> {

        let reader = WavReader::open(path).unwrap();
        let num_samples = reader.len() as usize;

        assert!(num_samples % Self::TABLE_SIZE == 0);
        assert!(reader.spec().sample_format == SampleFormat::Float);

        let num_frames = num_samples / Self::TABLE_SIZE;

        let mut table = Self::with_frame_count(num_frames);

        let table_mut = Arc::get_mut(&mut table).unwrap();

        for (output, input) in table_mut
            .as_mut_slice()
            .iter_mut()
            .map(|mipmaps| mipmaps.last_mut().unwrap())
            .flatten()
            .zip(reader.into_samples().map(Result::unwrap))
        {
            *output = input;
        }

        table_mut.create_mipmaps();

        table
    }

    pub fn create_mipmaps(&mut self) {

        let mut fft = RealFftPlanner::<f32>::new();

        let table_size: usize = 1 << Self::NUM_OCTAVES;
        let normalisation_factor = 1. / table_size as f32;

        let r2c = fft.plan_fft_forward(table_size);

        let mut spectrum = r2c.make_output_vec();
        let mut mipmap_scratch = spectrum.clone();
        let mut spectrum_scratch = spectrum.clone();
        let mut wave_scratch = r2c.make_input_vec();

        let c2r = fft.plan_fft_inverse(table_size);

        for table in self.as_mut_slice() {

            let (full_table, mipmaps) = table.split_last_mut().unwrap();

            wave_scratch.copy_from_slice(full_table);

            r2c.process_with_scratch(&mut wave_scratch, &mut spectrum, &mut spectrum_scratch).unwrap();

            let mut partials = 1 << (Self::NUM_OCTAVES - 1);

            for mipmap in mipmaps.iter_mut().rev() {
                let pass_band = &spectrum[..partials / 2 + 1];

                let (pb, sb) = spectrum_scratch.split_at_mut(partials / 2 + 1);

                sb.fill(Complex32::new(0., 0.));
                pb.copy_from_slice(pass_band);

                c2r.process_with_scratch(&mut spectrum_scratch, mipmap, &mut mipmap_scratch).unwrap();

                mipmap.iter_mut().for_each(|sample| *sample *= normalisation_factor);

                partials /= 2;
            }
        }
    }
}