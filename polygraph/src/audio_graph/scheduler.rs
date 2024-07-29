use super::*;

use buffer_allocator::BufferAllocator;

#[derive(Debug, Clone)]
pub(super) struct Scheduler {
    outputs: AudioGraphIO,
    process_schedule: Vec<NodeIndex>,
}

impl Scheduler {
    pub(super) fn for_graph(graph: &AudioGraphIO) -> Self {
        let mut visited = HashSet::default();

        let mut outputs = graph.with_opposite_config();

        let mut process_schedule = vec![];
        outputs.insert_opposite_ports(
            graph,
            NodeIndex::Global,
            &mut visited,
            &mut process_schedule,
        );

        Self {
            process_schedule,
            outputs,
        }
    }

    pub(super) fn schedule(
        &self,
        node_index: NodeIndex,
        final_schedule: &mut Vec<ProcessTask>,
        buffer_allocator: &mut BufferAllocator,
    ) {
        let output_buffers = if let NodeIndex::Processor(index) = node_index {
            let inputs = self
                .outputs
                .opposite_port_indices(node_index)
                .map(|port| buffer_allocator.free_buffer(&port))
                .collect();

            let outputs: Box<[_]> = self.outputs[node_index]
                .ports()
                .iter()
                .map(|ports| buffer_allocator.reserve_free_buffer(ports.clone()))
                .collect();

            let output_buffers: Box<[_]> = outputs
                .iter()
                .map(|&maybe_buf| maybe_buf.map(BufferIndex::Output))
                .collect();

            final_schedule.push(ProcessTask::Process {
                proc_index: index,
                inputs,
                outputs,
            });

            output_buffers
        } else {
            self.outputs[node_index]
                .ports()
                .iter()
                .enumerate()
                .map(|(i, ports)| (!ports.is_empty()).then_some(BufferIndex::SuperInput(i)))
                .collect()
        };

        output_buffers
            .iter()
            .zip(self.outputs[node_index].ports().iter())
            .filter_map(|(buf_idx, ports)| buf_idx.map(|idx| (idx, ports)))
            .for_each(|(buffer, ports)| {
                ports
                    .iter_ports()
                    .filter_map(|port| buffer_allocator.insert_claim(buffer, port))
                    .for_each(|(prev_claim, new_output)| {
                        final_schedule.push(ProcessTask::Sum {
                            left: buffer,
                            right: prev_claim,
                            output: new_output,
                        })
                    })
            });
    }

    pub(super) fn compile(&self) -> (Vec<ProcessTask>, usize) {
        let mut final_schedule = vec![];
        let mut buf_allocator = BufferAllocator::new();

        for &node in &self.process_schedule {
            self.schedule(node, &mut final_schedule, &mut buf_allocator);
        }

        let mut buffer_replacements = HashMap::default();
        let mut buffer_copies = HashMap::default();

        for port in self.outputs.opposite_port_indices(NodeIndex::Global) {
            let this_port_idx = port.index;

            if let Some(buf) = buf_allocator.free_buffer(&port) {
                match buf {
                    BufferIndex::SuperInput(_i) => {
                        buffer_copies
                            .entry(buf)
                            .or_insert_with(HashSet::default)
                            .insert(this_port_idx);
                    }

                    BufferIndex::Output(OutBufIndex::Local(i)) => {
                        if let Some(&index) = buffer_replacements.get(&i) {
                            buffer_copies
                                .entry(BufferIndex::Output(OutBufIndex::Super(index)))
                                .or_insert_with(HashSet::default)
                                .insert(this_port_idx);
                        } else {
                            buffer_replacements.insert(i, this_port_idx);
                        }
                    }

                    _ => unreachable!(),
                }
            }
        }

        final_schedule
            .iter_mut()
            .for_each(|task| task.replace_and_shift_output_buffers(&buffer_replacements));

        final_schedule.extend(buffer_copies.iter().map(|(&input, outputs)| {
            ProcessTask::CopyToMasterOutput {
                input,
                outputs: outputs.iter().copied().collect(),
            }
        }));

        (
            final_schedule,
            buf_allocator.num_intermediate_buffers() - buffer_replacements.len(),
        )
    }
}
