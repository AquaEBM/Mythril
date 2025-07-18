use super::*;
use core::{fmt, marker, mem, ops, ptr, slice};

pub struct BufferList<T> {
    ptr: NonNull<T>,
    samples_per_buf: usize,
    num_bufs: usize,
    marker: marker::PhantomData<T>,
}

impl<T: fmt::Debug> fmt::Debug for BufferList<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries((0..self.num_bufs).map(|i| unsafe { self.get_buf_unchecked(i) }))
            .finish()
    }
}

impl<T> Default for BufferList<T> {
    fn default() -> Self {
        Self::empty()
    }
}

unsafe impl<T: Send> Send for BufferList<T> {}
unsafe impl<T: Sync> Sync for BufferList<T> {}

impl<T> BufferList<mem::MaybeUninit<T>> {
    pub const unsafe fn assume_init(self) -> BufferList<T> {
        let out = BufferList {
            ptr: self.ptr.cast(),
            marker: marker::PhantomData,
            samples_per_buf: self.samples_per_buf,
            num_bufs: self.num_bufs,
        };

        mem::forget(self);

        out
    }
}

impl<T> BufferList<T> {
    pub const fn num_samples(&self) -> usize {
        unsafe { self.samples_per_buf.unchecked_mul(self.num_bufs) }
    }

    pub const fn empty() -> Self {
        Self {
            ptr: NonNull::dangling(),
            samples_per_buf: 0,
            num_bufs: 0,
            marker: marker::PhantomData,
        }
    }

    pub fn new(samples_per_buf: usize, num_bufs: usize) -> Self
    where
        T: Default,
    {
        let boxed = iter::repeat_with(T::default)
            .take(num_bufs.checked_mul(samples_per_buf).unwrap())
            .collect();

        Self {
            samples_per_buf,
            num_bufs,
            ptr: Box::into_non_null(boxed).as_non_null_ptr(),
            marker: marker::PhantomData,
        }
    }

    pub fn new_uninit(samples_per_buf: usize, num_bufs: usize) -> BufferList<mem::MaybeUninit<T>> {
        let uninit = Box::new_uninit_slice(num_bufs.checked_mul(samples_per_buf).unwrap());

        BufferList {
            samples_per_buf,
            num_bufs,
            ptr: Box::into_non_null(uninit).as_non_null_ptr(),
            marker: marker::PhantomData,
        }
    }

    pub fn new_zeroed(samples_per_buf: usize, num_bufs: usize) -> BufferList<mem::MaybeUninit<T>> {
        let uninit = Box::new_zeroed_slice(num_bufs.checked_mul(samples_per_buf).unwrap());

        BufferList {
            samples_per_buf,
            num_bufs,
            ptr: Box::into_non_null(uninit).as_non_null_ptr(),
            marker: marker::PhantomData,
        }
    }

    pub const unsafe fn get_buf_ptr_unchecked(&self, index: usize) -> NonNull<[T]> {
        NonNull::slice_from_raw_parts(
            unsafe { self.ptr.add(self.samples_per_buf.unchecked_mul(index)) },
            self.samples_per_buf,
        )
    }

    pub const fn get_buf_ptr(&self, index: usize) -> Option<NonNull<[T]>> {
        if index < self.num_bufs {
            Some(unsafe { self.get_buf_ptr_unchecked(index) })
        } else {
            None
        }
    }

    pub const unsafe fn get_buf_unchecked(&self, index: usize) -> &[T] {
        unsafe { self.get_buf_ptr_unchecked(index).as_ref() }
    }

    pub const unsafe fn get_buf_mut_unchecked(&mut self, index: usize) -> &mut [T] {
        unsafe { self.get_buf_ptr_unchecked(index).as_mut() }
    }

    pub const fn get_buf(&self, index: usize) -> Option<&[T]> {
        match self.get_buf_ptr(index) {
            Some(ptr) => Some(unsafe { ptr.as_ref() }),
            _ => None,
        }
    }

    pub const fn get_buf_mut(&mut self, index: usize) -> Option<&mut [T]> {
        match self.get_buf_ptr(index) {
            Some(mut ptr) => Some(unsafe { ptr.as_mut() }),
            _ => None,
        }
    }

    pub const unsafe fn get_disjoint_unchecked_mut<const N: usize>(
        &mut self,
        indices: [usize; N],
    ) -> [&mut [T]; N] {
        const fn empty_slice<'a, T>() -> &'a mut [T] {
            &mut []
        }

        let mut outs = [const { empty_slice() }; N];

        let mut i = 0;
        while i < N {
            outs[i] = unsafe { self.get_buf_ptr_unchecked(indices[i]).as_mut() };
            i += 1;
        }

        outs
    }

    pub const fn get_disjoint_mut<const N: usize>(
        &mut self,
        indices: [usize; N],
    ) -> Result<[&mut [T]; N], slice::GetDisjointMutError> {
        const fn check_disjointness<const N: usize>(
            indices: &[usize; N],
            len: usize,
        ) -> Result<(), slice::GetDisjointMutError> {
            let mut i = 0;

            // no for loops in const hahahaha
            while i < N {
                let idx = indices[i];
                if idx >= len {
                    return Err(slice::GetDisjointMutError::IndexOutOfBounds);
                }

                let mut j = 0;
                while j < i {
                    let other_idx = indices[j];
                    if idx == other_idx {
                        return Err(slice::GetDisjointMutError::OverlappingIndices);
                    }

                    j += 1;
                }

                i += 1;
            }

            Ok(())
        }

        match check_disjointness(&indices, self.num_bufs) {
            Ok(()) => Ok(unsafe { self.get_disjoint_unchecked_mut(indices) }),
            Err(b) => Err(b),
        }
    }
}

impl<T> ops::Index<usize> for BufferList<T> {
    type Output = [T];
    fn index(&self, index: usize) -> &Self::Output {
        self.get_buf(index).unwrap()
    }
}

impl<T> ops::IndexMut<usize> for BufferList<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_buf_mut(index).unwrap()
    }
}

impl<T> Drop for BufferList<T> {
    fn drop(&mut self) {
        let num_samples = self.num_samples();
        let slice = NonNull::slice_from_raw_parts(self.ptr, num_samples);
        drop(unsafe { Box::from_non_null(slice) })
    }
}

#[inline]
pub fn delay_slice<T>(buf: &mut [T], delay_buf: &mut [T]) {
    let delay_len = delay_buf.len();

    if delay_len == 0 {
        return;
    }

    let mut chunks = buf.chunks_exact_mut(delay_len);

    let delay_buf_ptr = delay_buf.as_mut_ptr();

    for samples in &mut chunks {
        unsafe {
            ptr::swap_nonoverlapping(delay_buf_ptr, samples.as_mut_ptr(), delay_len);
        }
    }

    let rem = chunks.into_remainder();
    let rem_len = rem.len();

    unsafe {
        ptr::swap_nonoverlapping(delay_buf_ptr, rem.as_mut_ptr(), rem_len);
    }

    delay_buf.rotate_left(rem_len);
}

#[cfg(test)]
mod tests {

    use core::array;

    use super::*;

    #[test]
    pub fn it_works() {
        const DELAY_LEN: usize = 6;
        const NUM_DELAYS: usize = 4;
        const NUM_SAMPLES: usize = 12;

        let mut a = BufferList::new(DELAY_LEN, NUM_DELAYS);

        assert!(a.get_buf(NUM_DELAYS.saturating_add(3)).is_none());
        assert!(a.get_buf(NUM_DELAYS).is_none());
        assert_eq!(
            a.get_disjoint_mut([NUM_DELAYS.saturating_sub(2); 2])
                .unwrap_err(),
            slice::GetDisjointMutError::OverlappingIndices
        );
        assert_eq!(
            a.get_disjoint_mut([NUM_DELAYS.saturating_sub(2), NUM_DELAYS.saturating_add(1)])
                .unwrap_err(),
            slice::GetDisjointMutError::IndexOutOfBounds,
        );

        let mut samples1: [_; NUM_SAMPLES] = array::from_fn(|i| (i + 1) as f32);
        let mut samples2 = samples1.map(|i| i + NUM_SAMPLES as f32);

        println!("buffers_before: {a:#?}");
        println!("samples1_before: {samples1:#?}");
        println!("samples2_before: {samples2:#?}");

        let samples1_expected = array::from_fn(|i| {
            i.checked_sub(DELAY_LEN)
                .map(|delayed| samples1[delayed])
                .unwrap_or_default()
        });

        let samples2_expected = array::from_fn(|i| {
            i.checked_sub(DELAY_LEN)
                .map(|delayed| samples2[delayed])
                .unwrap_or_default()
        });

        let [buf1, buf2] = a.get_disjoint_mut([2, 3]).unwrap();

        delay_slice(&mut samples1, buf1);
        delay_slice(&mut samples2, buf2);

        println!("buffers_after: {a:#?}");
        println!("samples1_after: {samples1:#?}");
        println!("samples2_after: {samples2:#?}");

        assert_eq!(samples1, samples1_expected);
        assert_eq!(samples2, samples2_expected);
    }
}
