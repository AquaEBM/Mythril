use super::*;

pub struct Lender<T: ?Sized> {
    ring_buffers: Vec<rtrb::Producer<Arc<T>>>,
    lent: Vec<Arc<T>>,
}

impl<T: ?Sized> Default for Lender<T> {
    fn default() -> Self {
        Self {
            ring_buffers: Vec::new(),
            lent: Vec::new(),
        }
    }
}

impl<T: ?Sized> Lender<T> {
    pub fn lend(&mut self, item: Arc<T>) {
        for producer in self.ring_buffers.iter_mut() {
            producer.push(item.clone()).unwrap();
        }

        self.lent.push(item);
    }

    pub fn cleanup(&mut self) {
        self.lent.retain(|item| Arc::strong_count(item) != 1);
        self.ring_buffers
            .retain(|producer| !producer.is_abandoned());
    }

    pub fn create_lendee(&mut self) -> Lendee<T> {
        let (producer, reciever) = rtrb::RingBuffer::new(256);
        self.ring_buffers.push(producer);

        Lendee {
            ring_buffer: reciever,
        }
    }
}

pub struct Lendee<T: ?Sized> {
    ring_buffer: rtrb::Consumer<Arc<T>>,
}

impl<T: ?Sized> Lendee<T> {
    pub fn recv_next(&mut self) -> Option<Arc<T>> {
        self.ring_buffer.pop().ok()
    }

    pub fn recv_latest(&mut self) -> Option<Arc<T>> {
        iter::from_fn(|| self.recv_next()).last()
    }
}