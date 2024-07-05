use core::iter;

pub mod io;
use io::{AudioGraphIO, NodeIO};

mod buffer_allocator;

pub mod errors;
use errors::{EdgeInsertError, EdgeNotFound};

use super::buffer::{BufferIndex, OutputBufferIndex};

mod scheduler;
use scheduler::Scheduler;

#[cfg(test)]
mod tests;

use std::collections;

use fnv::FnvBuildHasher;

type HashMap<K, V> = collections::HashMap<K, V, FnvBuildHasher>;
type HashSet<V> = collections::HashSet<V, FnvBuildHasher>;

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub enum NodeIndex {
    Global,
    Processor(usize),
}

impl NodeIndex {
    pub fn is_global(&self) -> bool {
        *self == NodeIndex::Global
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub struct Port {
    pub index: usize,
    pub node_index: NodeIndex,
}

impl Port {
    pub fn new(index: usize, node_index: NodeIndex) -> Self {
        Self { index, node_index }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum ProcessTask {
    Sum {
        left_input: BufferIndex,
        right_input: BufferIndex,
        output: OutputBufferIndex,
    },
    CopyToMasterOutput {
        input: BufferIndex,
        outputs: Box<[usize]>,
    },
    Process {
        index: usize,
        inputs: Box<[Option<BufferIndex>]>,
        outputs: Box<[Option<OutputBufferIndex>]>,
    },
}

impl ProcessTask {
    fn filter_outputs_bufs<'a>(
        inputs: impl Iterator<Item = &'a mut BufferIndex>,
        outputs: impl Iterator<Item = &'a mut OutputBufferIndex>,
    ) -> impl Iterator<Item = &'a mut OutputBufferIndex> {
        inputs
            .filter_map(|buf| {
                if let BufferIndex::Output(buffer) = buf {
                    Some(buffer)
                } else {
                    None
                }
            })
            .chain(outputs)
    }

    fn replace_and_shift_output_buffers(&mut self, buffer_replacements: &HashMap<usize, usize>) {
        fn replace_with_master<'a>(
            buffers: impl Iterator<Item = &'a mut OutputBufferIndex>,
            buffer_replacements: &HashMap<usize, usize>,
        ) {
            buffers.for_each(|buf| {
                if let OutputBufferIndex::Local(idx) = buf {
                    if let Some(&i) = buffer_replacements.get(idx) {
                        *buf = OutputBufferIndex::Master(i);
                    } else {
                        *idx -= buffer_replacements
                            .keys()
                            .copied()
                            .map(|i| (*idx > i) as usize)
                            .sum::<usize>();
                    }
                }
            });
        }

        match self {
            ProcessTask::Sum {
                left_input,
                right_input,
                output,
            } => replace_with_master(
                Self::filter_outputs_bufs(
                    [left_input, right_input].into_iter(),
                    iter::once(output),
                ),
                buffer_replacements,
            ),

            ProcessTask::Process {
                inputs, outputs, ..
            } => replace_with_master(
                Self::filter_outputs_bufs(
                    inputs.iter_mut().filter_map(Option::as_mut),
                    outputs.iter_mut().filter_map(Option::as_mut),
                ),
                buffer_replacements,
            ),
            _ => (),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AudioGraph {
    transposed: AudioGraphIO,
}

impl AudioGraph {
    pub fn with_global_io_config(num_inputs: usize, num_outputs: usize) -> Self {
        Self {
            transposed: AudioGraphIO::with_global_io_config(num_inputs, num_outputs),
        }
    }

    pub fn insert_processor(&mut self, num_inputs: usize, num_outputs: usize) -> usize {
        self.transposed.insert_processor(num_inputs, num_outputs)
    }

    pub fn insert_edge(&mut self, from: Port, to: Port) -> Result<bool, EdgeInsertError> {
        self.transposed.insert_edge(to, from)
    }

    pub fn remove_processor(&mut self, index: usize) -> bool {
        self.transposed.remove_processor(index)
    }

    pub fn remove_edge(&mut self, from: Port, to: Port) -> Result<bool, EdgeNotFound> {
        self.transposed.remove_edge(to, from)
    }

    fn get_scheduler(&self) -> Scheduler {
        Scheduler::for_graph(&self.transposed)
    }

    pub(crate) fn compile(&self) -> (Vec<ProcessTask>, usize) {
        self.get_scheduler().compile()
    }

    pub fn get_io(&self, index: NodeIndex) -> Option<&NodeIO> {
        self.transposed.get_node(index)
    }

    pub fn iter_processor_io(&self) -> impl Iterator<Item = (usize, &NodeIO)> {
        self.transposed.iter_processor_io()
    }
}
