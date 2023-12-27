use core::{num::NonZeroU8, iter};

pub struct VoiceManager<const VOICES_PER_VECTOR: usize, const NUM_VECTORS: usize> {
    notes: [[Option<NonZeroU8> ; VOICES_PER_VECTOR] ; NUM_VECTORS],
    num_active_voices: [usize ; NUM_VECTORS],
    enabled_vectors_bitmask: u128,
}

impl<const V: usize, const N: usize> Default for VoiceManager<V, N> {
    fn default() -> Self {
        Self {
            num_active_voices: [0 ; N],
            notes: [[None ; V] ; N],
            enabled_vectors_bitmask: 0,
        }
    }
}

impl<const V: usize, const N: usize> VoiceManager<V, N> {

    pub fn add_voice(&mut self, n: u8) -> Option<(usize, usize)> {

        for (i, notes) in self.notes.iter_mut().enumerate() {
            for (j, note) in notes.iter_mut().enumerate() {
    
                if note.is_none() {
                    *note = NonZeroU8::new(n + 1);
                    self.num_active_voices[i] += 1;
                    self.enabled_vectors_bitmask |= 1 << i;
                    return Some((i, j));
                }
            }
        }
        None
    }

    pub fn remove_voice(&mut self, n: u8) -> Option<(usize, usize)> {
   
        let v = n + 1;
        for (i, vectors) in self.notes.iter_mut().enumerate() {
            for (j, note) in vectors.iter_mut().enumerate() {
   
                if let Some(k) = note.as_mut() {
                    if k.get() == v {
                        *note = None;
                        let active = &mut self.num_active_voices[i];
                        *active -= 1;
                        if *active == 0 {
                            self.enabled_vectors_bitmask &= !(1 << i);
                        }
                        return Some((i, j));
                    }
                }
            }
        }
        None
    }

    pub fn num_voices_in_cluster(&self, index: usize) -> usize {
        self.num_active_voices[index]
    }

    pub fn active_clusters(&self) -> impl Iterator<Item = usize> {
        let mut enabled = self.enabled_vectors_bitmask;
        let mut accumulator = 0;
        iter::from_fn(move || (enabled != 0).then(|| {
            let n = enabled.trailing_zeros() as usize + 1;
            accumulator += n;
            enabled >>= n;
            accumulator - 1
        }))
    }
}