use core::{hash::Hash, mem, ops::Index};
use fnv::{FnvHashMap, FnvHashSet};
use std::collections::hash_map::Entry;

#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub struct InputID(u32);
#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub struct NodeID(u32);
#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub struct OutputID(u32);

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Input(FnvHashMap<NodeID, FnvHashSet<OutputID>>);

impl Input {
    #[inline]
    pub fn connections(&self) -> &FnvHashMap<NodeID, FnvHashSet<OutputID>> {
        &self.0
    }

    #[inline]
    fn insert_output(&mut self, (node_index, port_index): (NodeID, OutputID)) -> bool {
        match self.0.entry(node_index) {
            Entry::Occupied(e) => e.into_mut().insert(port_index),
            Entry::Vacant(e) => {
                e.insert(FnvHashSet::from_iter([port_index]));
                true
            }
        }
    }

    #[inline]
    pub fn remove_port(&mut self, (node_index, port_index): (&NodeID, &OutputID)) -> bool {
        let mut empty = false;

        let tmp = self.0.get_mut(node_index).is_some_and(|ports| {
            let tmp = ports.remove(port_index);
            empty = ports.is_empty();
            tmp
        });

        if empty {
            self.0.remove(node_index);
        }

        tmp
    }
}

#[derive(Clone, Debug, Default)]
pub struct Node {
    pub latency: u64,
    output_ids: FnvHashSet<OutputID>,
    inputs: FnvHashMap<InputID, Input>,
}

impl Node {
    fn with_reversed_io_layout(&self) -> Self {
        Self {
            latency: self.latency,
            output_ids: self
                .inputs
                .keys()
                .cloned()
                .map(|InputID(id)| OutputID(id))
                .collect(),
            inputs: self
                .output_ids
                .iter()
                .map(|id| (InputID(id.clone().0), Input::default()))
                .collect(),
        }
    }

    #[inline]
    pub fn get_input_mut(&mut self, id: &InputID) -> Option<&mut Input> {
        self.inputs.get_mut(id)
    }

    #[inline]
    pub fn add_input(&mut self) -> InputID {
        for id in (0..).into_iter().map(InputID) {
            if !self.inputs.contains_key(&id) {
                self.inputs.insert(id.clone(), Input::default());
                return id;
            }
        }

        panic!("Index overflow")
    }

    #[inline]
    pub fn remove_input(&mut self, id: &InputID) -> Option<Input> {
        self.inputs.remove(id)
    }

    #[inline]
    pub fn add_output(&mut self) -> OutputID {
        for id in (0..).into_iter().map(OutputID) {
            if !self.output_ids.contains(&id) {
                self.output_ids.insert(id.clone());
                return id;
            }
        }

        panic!("Index overflow")
    }
}

impl Node {
    #[inline]
    pub fn inputs(&self) -> &FnvHashMap<InputID, Input> {
        &self.inputs
    }

    #[inline]
    pub fn output_ids(&self) -> &FnvHashSet<OutputID> {
        &self.output_ids
    }
}

#[derive(Debug, Default)]
struct BufferAllocator {
    buffers: FnvHashMap<(NodeID, InputID), usize>,
    ports: Vec<FnvHashSet<(NodeID, InputID)>>,
}

impl BufferAllocator {
    fn len(&self) -> usize {
        self.ports.len()
    }

    fn get_free(&mut self) -> usize {
        fn get_or_insert_empty_set_index<T>(list: &mut Vec<FnvHashSet<T>>) -> usize {
            list.iter()
                .enumerate()
                .find_map(|(i, port_idxs)| port_idxs.is_empty().then_some(i))
                .unwrap_or_else(|| {
                    let tmp = list.len();
                    list.push(FnvHashSet::default());
                    tmp
                })
        }

        get_or_insert_empty_set_index(&mut self.ports)
    }
}

impl BufferAllocator {
    fn claim(
        &mut self,
        buffer_index: usize,
        ports: FnvHashSet<(NodeID, InputID)>,
    ) -> FnvHashSet<(NodeID, InputID)> {
        let port_idxs = &mut self.ports[buffer_index];

        assert!(
            mem::replace(port_idxs, ports).is_empty(),
            "INTERNAL ERROR: cannot claim currently claimed buffer"
        );

        port_idxs
            .extract_if(|port| {
                if self.buffers.contains_key(port) {
                    return true;
                }

                self.buffers.insert(port.clone(), buffer_index);
                false
            })
            .collect()
    }

    fn remove_claim(&mut self, port: &(NodeID, InputID)) -> usize {
        let i = self.buffers.remove(port).unwrap();

        assert!(
            self.ports
                .get_mut(i)
                .expect("INTERNAL ERROR: expected reserved buffer to have a port list entry")
                .remove(port),
            "INTERNAL ERROR: port reserves a buffer but is not in it's port list entry"
        );

        i
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Task {
    Node {
        id: NodeID,
        inputs: FnvHashMap<InputID, usize>,
        outputs: FnvHashMap<OutputID, usize>,
    },
    Sum {
        left: usize,
        right: usize,
        output: usize,
    },
}

impl Task {
    #[inline]
    pub fn node(
        index: NodeID,
        inputs: impl IntoIterator<Item = (InputID, usize)>,
        outputs: impl IntoIterator<Item = (OutputID, usize)>,
    ) -> Self {
        Self::Node {
            id: index,
            inputs: inputs.into_iter().collect(),
            outputs: outputs.into_iter().collect(),
        }
    }

    #[inline]
    pub fn sum(left: usize, right: usize, output: usize) -> Self {
        Self::Sum {
            left,
            right,
            output,
        }
    }
}

#[derive(Debug)]
struct Scheduler {
    transposed: AudioGraph,
    process_order: Vec<NodeID>,
}

impl Scheduler {
    fn compile(self) -> (usize, Vec<Task>) {
        let mut allocator = BufferAllocator::default();
        let mut schedule = vec![];

        let Self {
            mut transposed,
            process_order,
        } = self;

        for node_id in process_order {
            let node = transposed.get_node_mut(&node_id).unwrap();

            let inputs = node
                .output_ids()
                .iter()
                .map(|OutputID(id)| {
                    let id = InputID(id.clone());
                    (id.clone(), allocator.remove_claim(&(node_id.clone(), id)))
                })
                .collect();

            let outputs = node
                .inputs()
                .iter()
                .map(|(InputID(id), port)| {
                    (
                        OutputID(id.clone()),
                        if port.connections().is_empty() {
                            usize::MAX
                        } else {
                            allocator.get_free()
                        },
                    )
                })
                .collect();

            schedule.push(Task::Node {
                id: node_id,
                inputs,
                outputs,
            });

            let Some(Task::Node { outputs, .. }) = schedule.last() else {
                panic!()
            };

            for (buf_index, port) in outputs
                .clone()
                .into_values()
                .zip(node.inputs.values_mut())
                .filter(|(i, _)| i != &usize::MAX)
            {
                for port_idx in allocator.claim(
                    buf_index,
                    port.connections()
                        .iter()
                        .flat_map(|(node, ports)| {
                            ports
                                .iter()
                                .map(move |p| (node.clone(), InputID(p.clone().0)))
                        })
                        .collect(),
                ) {
                    let other_buf_idx = allocator.remove_claim(&port_idx);
                    let new_free_buf = allocator.get_free();
                    assert!(
                        allocator
                            .claim(new_free_buf, FnvHashSet::from_iter([port_idx]))
                            .is_empty(),
                        "INTERNAL ERROR: redundant claims cleared yet still found"
                    );

                    schedule.push(Task::Sum {
                        left: buf_index,
                        right: other_buf_idx,
                        output: new_free_buf,
                    });
                }
            }
        }

        (allocator.len(), schedule)
    }
}

#[derive(Clone, Debug, Default)]
pub struct AudioGraph {
    nodes: FnvHashMap<NodeID, Node>,
}

impl Index<&NodeID> for AudioGraph {
    type Output = Node;
    #[inline]
    fn index(&self, key: &NodeID) -> &Self::Output {
        self.get_node(key).expect("no node found for this id")
    }
}

impl AudioGraph {
    #[inline]
    fn fill_inputs(&mut self, transposed: &Self, node_index: &NodeID, processed: &mut Vec<NodeID>) {
        if processed.contains(node_index) {
            return;
        }

        let node = transposed.get_node(node_index).unwrap();

        for (output_id, input) in node.inputs().iter() {
            let output_id = OutputID(output_id.clone().0);

            for (node_idx, port_indices) in input.connections().iter() {
                self.fill_inputs(transposed, node_idx, processed);

                for input_id in port_indices.iter().cloned().map(|OutputID(id)| InputID(id)) {
                    let node = if let Some(node) = self.get_node_mut(node_idx) {
                        node
                    } else {
                        let Ok(node) = self.try_insert_node(
                            node_idx.clone(),
                            transposed
                                .get_node(node_idx)
                                .unwrap()
                                .with_reversed_io_layout(),
                        ) else {
                            unreachable!("inconsistent Hash and Eq implementations for NodeID?");
                        };

                        node
                    };

                    let new = node
                        .get_input_mut(&input_id)
                        .unwrap()
                        .insert_output((node_index.clone(), output_id.clone()));

                    assert!(new, "INTERNAL ERRROR: port must be newly inserted");
                }
            }
        }

        processed.push(node_index.clone());
    }

    #[inline]
    fn scheduler(&self, root_nodes: FnvHashSet<NodeID>) -> Scheduler {
        let mut transposed = Self::default();

        let mut process_order = vec![];

        for node_idx in root_nodes {
            assert!(transposed
                .try_insert_node(
                    node_idx.clone(),
                    self.get_node(&node_idx).unwrap().with_reversed_io_layout()
                )
                .is_ok(),);
            transposed.fill_inputs(self, &node_idx, &mut process_order);
        }

        Scheduler {
            transposed,
            process_order,
        }
    }

    #[inline]
    pub fn compile(&self, root_nodes: impl IntoIterator<Item = NodeID>) -> (usize, Vec<Task>) {
        self.scheduler(FnvHashSet::from_iter(root_nodes)).compile()
    }
}

impl AudioGraph {
    #[inline]
    #[must_use]
    pub fn try_insert_edge(
        &mut self,
        from: (NodeID, OutputID),
        to: (NodeID, InputID),
    ) -> Result<bool, bool> {
        // If either of the ports don't exist, error out
        if self
            .get_node(&to.0)
            .and_then(|node| node.inputs().get(&to.1))
            .is_none()
            || self
                .get_node(&from.0)
                .map_or(true, |node| !node.output_ids().contains(&from.1))
        {
            return Err(false);
        }

        if self.is_connected(&from.0, &to.0) {
            return Err(true);
        }

        Ok(self
            .get_node_mut(&to.0)
            .unwrap()
            .get_input_mut(&to.1)
            .unwrap()
            .insert_output(from))
    }

    /// # Panics
    ///
    /// if no node exists at either `from` or `to`
    fn is_connected(&self, from: &NodeID, to: &NodeID) -> bool {
        if from == to {
            return true;
        }

        for port in self.get_node(from).unwrap().inputs().values() {
            for node in port.connections().keys() {
                if self.is_connected(node, to) {
                    return true;
                }
            }
        }

        false
    }

    #[inline]
    pub fn get_node(&self, index: &NodeID) -> Option<&Node> {
        self.nodes.get(index)
    }

    #[inline]
    pub fn get_node_mut(&mut self, index: &NodeID) -> Option<&mut Node> {
        self.nodes.get_mut(index)
    }

    #[inline]
    fn try_insert_node(&mut self, id: NodeID, node: Node) -> Result<&mut Node, (&mut Node, Node)> {
        match self.nodes.entry(id) {
            Entry::Occupied(e) => Err((e.into_mut(), node)),
            Entry::Vacant(e) => Ok(e.insert(node)),
        }
    }

    #[inline]
    pub fn insert_node(&mut self, node: Node) -> NodeID {
        for i in (0..).into_iter().map(NodeID) {
            if !self.nodes.contains_key(&i) {
                self.nodes.insert(i.clone(), node);
                return i;
            }
        }

        panic!("Index overflow")
    }
}
