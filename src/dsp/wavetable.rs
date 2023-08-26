use hound::{SampleFormat, WavReader};
use realfft::{RealFftPlanner, num_complex::Complex32};
use rtrb::{Producer, Consumer, RingBuffer};
use std::{sync::Arc, path::Path, ops::{Deref, DerefMut, Index}};

use super::*;

#[repr(transparent)]
pub struct BandLimitedWaveTables {
    data: [f32 ; Self::TOTAL_LEN]
}

impl Default for BandLimitedWaveTables {
    fn default() -> Self {
        Self { data: [0. ; Self::TOTAL_LEN] }
    }
}

impl Deref for BandLimitedWaveTables {
    type Target = [f32];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for BandLimitedWaveTables {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

const ONE: UInt = const_splat(1);

impl BandLimitedWaveTables {

    /// How many octaves of frequency content our wavetables have, this
    /// is also the base 2 logarithm of the number of samples in each frame
    const NUM_OCTAVES: usize = 11;
    const V_NUM_OCTAVES: UInt = const_splat(Self::NUM_OCTAVES as u32);
    /// fractional part bits
    const FRACT_BITS: UInt = const_splat(u32::BITS - Self::NUM_OCTAVES as u32);
    /// the number of frames of our wavetables
    pub const NUM_FRAMES: usize = 256;
    /// total number of samples in the entire wavetable
    const TOTAL_LEN: usize = (1 << Self::NUM_OCTAVES) * (Self::NUM_OCTAVES as usize + 1) * Self::NUM_FRAMES;

    const PHASE_MASK: UInt = const_splat((1 << Self::NUM_OCTAVES as u32) - 1);

    const NUM_MIPMAPS: UInt = const_splat(Self::NUM_OCTAVES as u32 + 1);

    pub fn resample_select(&self, phase_delta: UInt, frame: UInt, phase: UInt, mask: TMask) -> Float {

        let octaves = map(phase_delta, u32::leading_zeros).simd_min(Self::V_NUM_OCTAVES);

        let fract = fxp_to_flp(phase << Self::V_NUM_OCTAVES);

        let table_start = octaves + frame * Self::NUM_MIPMAPS << Self::V_NUM_OCTAVES;
        
        let phase_a = phase >> Self::FRACT_BITS;
        let phase_b = phase_a + ONE & Self::PHASE_MASK;

        let (a, b) = unsafe { (
            gather_select_unchecked(self, table_start + phase_a, mask, ZERO_F),
            gather_select_unchecked(self, table_start + phase_b, mask, ZERO_F)
        ) };

        lerp(a, b, fract)
    }

    pub fn resample(&self, phase_delta: UInt, frame: UInt, phase: UInt) -> Float {

        let octaves = map(phase_delta, u32::leading_zeros).simd_min(Self::V_NUM_OCTAVES);

        let fract = fxp_to_flp(phase << Self::V_NUM_OCTAVES);

        let table_start = octaves + frame * Self::NUM_MIPMAPS << Self::V_NUM_OCTAVES;

        let phase_a = phase >> Self::FRACT_BITS;
        let phase_b = phase_a + ONE & Self::PHASE_MASK;

        let (a, b) = unsafe { (
            gather(self, table_start + phase_a),
            gather(self, table_start + phase_b)
        ) };

        lerp(a, b, fract)
    }

    pub fn from_file(path: impl AsRef<Path>) -> Arc<Self> {

        let reader = WavReader::open(path).unwrap();

        assert!(reader.len() == (Self::NUM_FRAMES << Self::NUM_OCTAVES) as u32);
        assert!(reader.spec().sample_format == SampleFormat::Float);

        // required in order to avoid a stack overflow in debug builds
        // SAFETY: zero (0.0) is a valid f32 value
        let mut table = unsafe { Arc::<Self>::new_zeroed().assume_init() };

        let table_mut = Arc::get_mut(&mut table).unwrap();

        table_mut
            .chunks_exact_mut(1 << Self::NUM_OCTAVES)
            .skip(Self::NUM_OCTAVES)
            .step_by(Self::NUM_OCTAVES + 1)
            .flatten()
            .zip(reader.into_samples().map(Result::unwrap))
            .for_each(|(table_sample, file_sample)| *table_sample = file_sample);

        table_mut.create_mipmaps();

        table
    }

    pub fn create_mipmaps(&mut self) {

        let mut fft = RealFftPlanner::<f32>::new();

        let table_size: usize = 1 << Self::NUM_OCTAVES;

        let r2c = fft.plan_fft_forward(table_size);

        let mut spectrum = r2c.make_output_vec();
        let mut mipmap_scratch = spectrum.clone();
        let mut spectrum_scratch = spectrum.clone();
        let mut wave_scratch = r2c.make_input_vec();
        
        let c2r = fft.plan_fft_inverse(table_size);

        for table in self.chunks_exact_mut(table_size * (Self::NUM_OCTAVES + 1)) {

            let (mipmaps, full_table) = table.split_at_mut(table_size * Self::NUM_OCTAVES);

            wave_scratch.copy_from_slice(full_table);

            r2c.process_with_scratch(&mut wave_scratch, &mut spectrum, &mut spectrum_scratch).unwrap();

            let mut partials = 1 << (Self::NUM_OCTAVES - 1);
            
            for mipmap in mipmaps.chunks_exact_mut(table_size).rev() {
                let pass_band = &spectrum[..partials / 2 + 1];

                let (pb, sb) = spectrum_scratch.split_at_mut(partials / 2 + 1);

                sb.fill(Complex32::new(0., 0.));
                pb.copy_from_slice(pass_band);

                c2r.process_with_scratch(&mut spectrum_scratch, mipmap, &mut mipmap_scratch).unwrap();

                mipmap.iter_mut().for_each(|sample| *sample /= table_size as f32);

                partials /= 2;
            }
        }
    }
}

impl Index<usize> for BandLimitedWaveTables {
    type Output = [f32];

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index < Self::NUM_FRAMES);
        let full_table_len = (Self::NUM_OCTAVES + 1) << Self::NUM_OCTAVES;
        let full_table_index_offset = Self::NUM_OCTAVES << Self::NUM_OCTAVES;
        let index = full_table_len * index + full_table_index_offset;
        &self.data[index..index + (1 << Self::NUM_OCTAVES)]
    }
}

pub struct SharedLender<T: ?Sized> {
    ring_buffers: Vec<Producer<Arc<T>>>,
    drop_queue: Vec<Arc<T>>,
}

impl<T: ?Sized> Default for SharedLender<T> {
    fn default() -> Self {
        Self {
            ring_buffers: Default::default(),
            drop_queue: Default::default() 
        }
    }
}

impl<T: ?Sized> SharedLender<T> {

    pub fn add(&mut self, item: Arc<T>) {

        self.ring_buffers
            .iter_mut()
            .for_each( |producer| {
                let _ = producer.push(item.clone());
            });

        self.drop_queue.push(item);
    }

    pub fn update_drop_queue(&mut self) {
        self.drop_queue.retain(|item| Arc::strong_count(item) != 1);
        self.ring_buffers.retain(|producer| !producer.is_abandoned());
    }

    pub fn current(&self) -> Option<&T> {
        self.drop_queue.last().map(Deref::deref)
    }

    pub fn create_new_reciever(&mut self) -> LenderReciever<T> {

        let value = self.drop_queue.first().map(Arc::clone);

        let (producer, reciever) = RingBuffer::new(128);
        self.ring_buffers.push(producer);

        LenderReciever {
            current: value,
            ring_buffer: reciever
        }
    }
}

pub struct LenderReciever<T: ?Sized> {
    current: Option<Arc<T>>,
    ring_buffer: Consumer<Arc<T>>,
}

impl<T: ?Sized> LenderReciever<T> {

    pub fn update_item(&mut self) {
        while let Ok(item) = self.ring_buffer.pop() {
            debug_assert!(Arc::strong_count(&item) > 1);
            self.current = Some(item);
        }
    }

    pub unsafe fn current(&self) -> &T {
        self.current.as_deref().unwrap_unchecked()
    }
}