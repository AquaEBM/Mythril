use alloc::sync::Arc;

pub struct SharedLender<T: ?Sized> {
    ring_buffers: Vec<rtrb::Producer<Arc<T>>>,
    drop_queue: Vec<Arc<T>>,
}

impl<T: ?Sized> Default for SharedLender<T> {
    fn default() -> Self {
        Self {
            ring_buffers: Vec::new(),
            drop_queue: Vec::new(),
        }
    }
}

impl<T: ?Sized> SharedLender<T> {
    pub fn send(&mut self, item: Arc<T>) {
        for producer in &mut self.ring_buffers {
            producer.push(item.clone()).unwrap();
        }

        self.drop_queue.push(item);
    }

    pub fn update_drop_queue(&mut self) {
        self.drop_queue.retain(|item| Arc::strong_count(item) != 1);
        self.ring_buffers
            .retain(|producer| !producer.is_abandoned());
    }

    pub fn create_new_reciever(&mut self) -> LenderReciever<T> {
        let (producer, reciever) = rtrb::RingBuffer::new(256);
        self.ring_buffers.push(producer);

        LenderReciever {
            ring_buffer: reciever,
        }
    }
}

pub struct LenderReciever<T: ?Sized> {
    ring_buffer: rtrb::Consumer<Arc<T>>,
}

impl<T: ?Sized> LenderReciever<T> {
    pub fn recv_next(&mut self) -> Option<Arc<T>> {
        self.ring_buffer.pop().ok()
    }

    pub fn recv_latest(&mut self) -> Option<Arc<T>> {
        let mut output = None;
        while let Some(item) = self.recv_next() {
            output = Some(item);
        }

        output
    }
}
