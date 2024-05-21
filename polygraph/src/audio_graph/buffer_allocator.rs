use super::io::Ports;

use super::*;

#[derive(Debug, Clone, Default)]
pub(super) struct BufferAllocator {
    claims: HashMap<Port, BufferIndex>,
    ports: HashMap<OutputBufferIndex, Ports>,
    free_buffers: HashSet<OutputBufferIndex>,
    num_intermediate_buffers: usize,
}

impl BufferAllocator {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn num_intermediate_buffers(&self) -> usize {
        self.num_intermediate_buffers
    }

    pub(super) fn free_buffer(&mut self, port: &Port) -> Option<BufferIndex> {
        let buf_index = self.claims.remove(port);

        if let Some(buf) = buf_index {
            self.try_remove_reservation(buf, port);
        }

        buf_index
    }

    fn remove_reservation(&mut self, buf: OutputBufferIndex, port: &Port) {
        let ports = self.ports.get_mut(&buf).unwrap();
        assert!(ports.remove_port(port));
        if ports.is_empty() {
            self.ports.remove(&buf);
            assert!(self.free_buffers.insert(buf));
        }
    }

    fn try_remove_reservation(&mut self, buf: BufferIndex, port: &Port) {
        if let BufferIndex::Output(output_buf) = buf {
            self.remove_reservation(output_buf, port)
        }
    }

    pub(super) fn reserve_free_buffer(&mut self, ports: Ports) -> Option<OutputBufferIndex> {
        if !ports.is_empty() {
            let buf = if let Some(buf) = self.free_buffers.iter().next().copied() {
                self.free_buffers.remove(&buf);
                buf
            } else {
                let new_buf_index = OutputBufferIndex::Local(self.num_intermediate_buffers);
                self.num_intermediate_buffers += 1;

                new_buf_index
            };

            assert!(self.ports.insert(buf, ports).is_none());

            return Some(buf);
        }
        None
    }

    pub(super) fn insert_claim(
        &mut self,
        buffer: BufferIndex,
        port: Port,
    ) -> Option<(BufferIndex, OutputBufferIndex)> {
        if let Some(buf) = self.free_buffer(&port) {
            self.try_remove_reservation(buffer, &port);

            let mut port_singleton = Ports::default();
            port_singleton.insert_port(port);

            let new_buf = self.reserve_free_buffer(port_singleton).unwrap();

            self.claims.insert(port, BufferIndex::Output(new_buf));

            Some((buf, new_buf))
        } else {
            self.claims.insert(port, buffer);
            None
        }
    }
}
