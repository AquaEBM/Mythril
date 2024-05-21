use core::{array, iter, mem};

use simd_util::{
    simd::{num::SimdFloat, LaneCount, SupportedLaneCount},
    Float, TMask, UInt,
};

#[derive(Clone, Copy)]
pub enum VoiceEvent<S: SimdFloat> {
    Activate {
        note: S::Bits,
        velocity: S,
        cluster_idx: usize,
        mask: S::Mask,
    },

    Deactivate {
        velocity: S,
        cluster_idx: usize,
        mask: S::Mask,
    },

    Move {
        from: (usize, usize),
        to: (usize, usize),
    },
}

pub trait VoiceManager<S: SimdFloat> {
    fn note_on(&mut self, note: u8, vel: f32);
    fn note_off(&mut self, note: u8, vel: f32);
    fn note_free(&mut self, note: u8);
    fn flush_events(&mut self, events: &mut Vec<VoiceEvent<S>>);
    fn set_max_polyphony(&mut self, max_num_clusters: usize);
    fn get_voice_mask(&self, cluster_idx: usize) -> S::Mask;
}

#[derive(Default)]
struct VoiceEventCache<const N: usize>
where
    LaneCount<N>: SupportedLaneCount,
{
    mask_cache: Box<[TMask<N>]>,
    vel_cache: Box<[Float<N>]>,
    note_cache: Box<[UInt<N>]>,
}

impl<const N: usize> VoiceEventCache<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    pub fn clear_and_set_capacity(&mut self, num_clusters: usize) {
        self.mask_cache = iter::repeat(TMask::splat(false))
            .take(num_clusters)
            .collect();

        self.vel_cache = iter::repeat(Float::splat(0.0)).take(num_clusters).collect();

        self.note_cache = iter::repeat(UInt::splat(0)).take(num_clusters).collect();
    }

    pub fn activate_index(&mut self, index: usize, vel: f32, note: Option<u8>) {
        let v = N / 2;
        let (i, j) = (index / v, index % v);
        let j1 = 2 * j;
        let j2 = j1 + 1;

        let mask = &mut self.mask_cache[i];
        mask.set(j1, true);
        mask.set(j2, true);

        let vels = &mut self.vel_cache[i];
        vels[j1] = vel;
        vels[j2] = vel;

        if let Some(note) = note {
            let notes = &mut self.note_cache[i];
            notes[j1] = note.into();
            notes[j2] = note.into();
        }
    }

    pub fn take_data(&mut self) -> impl Iterator<Item = (usize, TMask<N>, Float<N>, UInt<N>)> + '_ {
        self.mask_cache
            .iter_mut()
            .zip(self.vel_cache.iter_mut())
            .zip(self.note_cache.iter_mut())
            .enumerate()
            .filter_map(|(i, ((mask, vel), note))| {
                mask.any()
                    .then(|| (i, mem::take(mask), mem::take(vel), mem::take(note)))
            })
    }
}

#[derive(Default)]
pub struct StackVoiceManager<const N: usize>
where
    LaneCount<N>: SupportedLaneCount,
{
    voices: Vec<u8>,
    event_cache: VoiceEventCache<N>,
    add_pending: Vec<(u8, f32)>,
    free_pending: Vec<u8>,
    deactivate_pending: Vec<(u8, f32)>,
}

fn push_within_capacity_stable<T>(vec: &mut Vec<T>, val: T) -> bool {
    let can_push = vec.len() < vec.capacity();
    if can_push {
        vec.push(val)
    }
    can_push
}

impl<const N: usize> VoiceManager<Float<N>> for StackVoiceManager<N>
where
    LaneCount<N>: SupportedLaneCount,
{
    fn note_on(&mut self, note: u8, vel: f32) {
        push_within_capacity_stable(&mut self.add_pending, (note, vel));
    }

    fn note_off(&mut self, note: u8, vel: f32) {
        push_within_capacity_stable(&mut self.deactivate_pending, (note, vel));
    }

    fn note_free(&mut self, note: u8) {
        push_within_capacity_stable(&mut self.free_pending, note);
    }

    fn flush_events(&mut self, events: &mut Vec<VoiceEvent<Float<N>>>) {
        // handle voices scheduled to be deactivated first
        for (note, vel) in self.deactivate_pending.drain(..) {
            if let Some(i) = self.voices.iter().position(|&note_id| note_id == note) {
                self.event_cache.activate_index(i, vel, None);
            }
        }

        events.extend(
            self.event_cache
                .take_data()
                .map(|(cluster_idx, mask, velocity, _)| VoiceEvent::Deactivate {
                    velocity,
                    cluster_idx,
                    mask,
                }),
        );

        // then those scheduled to be freed
        for freed_note in self.free_pending.drain(..) {
            if let Some(i) = self
                .voices
                .iter()
                .position(|&note_id| note_id == freed_note)
            {
                // fill the gap with a voice scheduled to be activated
                if let Some((added_note, vel)) = self.add_pending.pop() {
                    self.voices[i] = added_note;

                    self.event_cache.activate_index(i, vel, Some(added_note));

                // if there are no voices scheduled to be activated
                // move a voice from the top of the stack to the empty gap
                } else if let Some(replacement_note) = self.voices.pop() {
                    if let Some(note) = self.voices.get_mut(i) {
                        *note = replacement_note;
                        let from = self.voices.len();

                        let v = N / 2;

                        events.push(VoiceEvent::Move {
                            from: (from / v, from % v),
                            to: (i / v, i % v),
                        });
                    }
                }
            }
        }

        for (added_note, vel) in self.add_pending.drain(..) {
            let i = self.voices.len();
            if push_within_capacity_stable(&mut self.voices, added_note) {
                self.event_cache.activate_index(i, vel, Some(added_note));
            }
        }

        events.extend(
            self.event_cache
                .take_data()
                .map(|(cluster_idx, mask, velocity, note)| VoiceEvent::Activate {
                    note,
                    velocity,
                    cluster_idx,
                    mask,
                }),
        );
    }

    fn set_max_polyphony(&mut self, max_num_clusters: usize) {
        let stereo_voices_per_vector = N / 2;
        let total_num_voices = max_num_clusters * stereo_voices_per_vector;

        let cache_cap = total_num_voices * 4;

        self.voices = Vec::with_capacity(cache_cap);
        self.free_pending = Vec::with_capacity(cache_cap);
        self.deactivate_pending = Vec::with_capacity(cache_cap);
        self.add_pending = Vec::with_capacity(cache_cap);

        self.event_cache.clear_and_set_capacity(max_num_clusters);
    }

    fn get_voice_mask(&self, cluster_idx: usize) -> TMask<N> {
        TMask::from_array(array::from_fn(|i| cluster_idx + i / 2 < self.voices.len()))
    }
}
