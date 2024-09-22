use core::{borrow::Borrow, hash::Hash, mem, ops::Index};
use fnv::{FnvHashMap, FnvHashSet};
use std::collections::hash_map::Entry;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub struct Port<NodeID, PortID>(FnvHashMap<NodeID, FnvHashSet<PortID>>);

impl<NodeID, PortID> Default for Port<NodeID, PortID> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<NodeID: Hash + Eq, PortID: Hash + Eq> PartialEq for Port<NodeID, PortID> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<NodeID: Hash + Eq, PortID: Hash + Eq> Eq for Port<NodeID, PortID> {}

impl<NodeID, PortID> Port<NodeID, PortID> {
    #[inline]
    pub fn connections(&self) -> &FnvHashMap<NodeID, FnvHashSet<PortID>> {
        &self.0
    }
}

impl<NodeID: Hash + Eq, PortID: Hash + Eq> Port<NodeID, PortID> {
    #[inline]
    fn insert_port(&mut self, (node_index, port_index): (NodeID, PortID)) -> bool {
        match self.0.entry(node_index) {
            Entry::Occupied(e) => e.into_mut().insert(port_index),
            Entry::Vacant(e) => {
                e.insert(FnvHashSet::from_iter([port_index]));
                true
            }
        }
    }

    #[inline]
    pub fn remove_port(&mut self, (node_index, port_index): (&NodeID, &PortID)) -> bool {
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

    #[inline]
    pub fn is_connected_to_node<Q: Hash + Eq + ?Sized>(&self, id: &Q) -> bool
    where
        NodeID: Borrow<Q>,
    {
        self.connections()
            .get(id)
            .is_some_and(|ports| !ports.is_empty())
    }
}

#[derive(Clone, Debug)]
pub struct Node<NodeID, PortID> {
    pub latency: u64,
    backward_port_ids: FnvHashSet<PortID>,
    forward_ports: FnvHashMap<PortID, Port<NodeID, PortID>>,
}

impl<NodeID, PortID> Default for Node<NodeID, PortID> {
    fn default() -> Self {
        Self {
            latency: Default::default(),
            backward_port_ids: Default::default(),
            forward_ports: Default::default(),
        }
    }
}

impl<NodeID, PortID: Hash + Eq> Node<NodeID, PortID> {
    fn new(
        latency: u64,
        backward_port_ids: impl IntoIterator<Item = PortID>,
        forward_port_ids: impl IntoIterator<Item = PortID>,
    ) -> Self {
        Self {
            latency,
            backward_port_ids: backward_port_ids.into_iter().collect(),
            forward_ports: forward_port_ids
                .into_iter()
                .map(|id| (id, Port::default()))
                .collect(),
        }
    }

    fn with_reversed_io_layout(&self) -> Self
    where
        NodeID: Clone,
        PortID: Clone,
    {
        Self::new(
            self.latency,
            self.forward_ports.keys().cloned(),
            self.backward_port_ids.iter().cloned(),
        )
    }

    #[inline]
    pub fn get_forward_port_mut<Q: Hash + Eq + ?Sized>(
        &mut self,
        id: &Q,
    ) -> Option<&mut Port<NodeID, PortID>>
    where
        PortID: Borrow<Q>,
    {
        self.forward_ports.get_mut(id)
    }

    #[inline]
    pub fn add_forward_port(&mut self, id: PortID) -> Result<&mut Port<NodeID, PortID>, PortID> {
        match self.forward_ports.entry(id) {
            Entry::Occupied(e) => Ok(e.into_mut()),
            Entry::Vacant(e) => Err(e.into_key()),
        }
    }

    #[inline]
    pub fn add_backward_port(&mut self, id: PortID) -> Result<(), PortID> {
        if let Some(_) = self.backward_port_ids.get(&id) {
            return Err(id);
        }

        self.backward_port_ids.insert(id);
        Ok(())
    }
}

impl<NodeID, PortID> Node<NodeID, PortID> {
    #[inline]
    pub fn forward_ports(&self) -> &FnvHashMap<PortID, Port<NodeID, PortID>> {
        &self.forward_ports
    }

    #[inline]
    pub fn backward_port_ids(&self) -> &FnvHashSet<PortID> {
        &self.backward_port_ids
    }
}

#[derive(Debug)]
struct BufferAllocator<NodeID, PortID> {
    buffers: FnvHashMap<(NodeID, PortID), usize>,
    ports: Vec<FnvHashSet<(NodeID, PortID)>>,
}

impl<NodeID, PortID> Default for BufferAllocator<NodeID, PortID> {
    fn default() -> Self {
        Self {
            buffers: Default::default(),
            ports: Default::default(),
        }
    }
}

impl<NodeID, PortID> BufferAllocator<NodeID, PortID> {
    fn len(&self) -> usize {
        self.ports.len()
    }

    fn get_empty_set_index<T>(list: &mut Vec<FnvHashSet<T>>) -> usize {
        list.iter()
            .enumerate()
            .find_map(|(i, port_idxs)| port_idxs.is_empty().then_some(i))
            .unwrap_or_else(|| {
                let tmp = list.len();
                list.push(FnvHashSet::default());
                tmp
            })
    }

    fn get_free(&mut self) -> usize {
        Self::get_empty_set_index(&mut self.ports)
    }
}

impl<NodeID: Hash + Eq, PortID: Hash + Eq> BufferAllocator<NodeID, PortID> {
    fn claim(
        &mut self,
        buffer_index: usize,
        ports: FnvHashSet<(NodeID, PortID)>,
    ) -> FnvHashSet<(NodeID, PortID)>
    where
        NodeID: Clone,
        PortID: Clone,
    {
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

    fn remove_claim(&mut self, port: &(NodeID, PortID)) -> usize {
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

#[derive(Clone, Debug)]
pub enum Task<NodeID, PortID> {
    Node {
        id: NodeID,
        inputs: FnvHashMap<PortID, usize>,
        outputs: FnvHashMap<PortID, usize>,
    },
    Sum {
        left: usize,
        right: usize,
        output: usize,
    },
}

impl<NodeID: PartialEq, PortID: Hash + Eq> PartialEq for Task<NodeID, PortID> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Node {
                    id: l_id,
                    inputs: l_inputs,
                    outputs: l_outputs,
                },
                Self::Node {
                    id: r_id,
                    inputs: r_inputs,
                    outputs: r_outputs,
                },
            ) => l_id == r_id && l_inputs == r_inputs && l_outputs == r_outputs,
            (
                Self::Sum {
                    left: l_left,
                    right: l_right,
                    output: l_output,
                },
                Self::Sum {
                    left: r_left,
                    right: r_right,
                    output: r_output,
                },
            ) => l_left == r_left && l_right == r_right && l_output == r_output,
            _ => false,
        }
    }
}

impl<NodeID: Eq, PortID: Hash + Eq> Eq for Task<NodeID, PortID> {}

impl<NodeID, PortID: Hash + Eq> Task<NodeID, PortID> {
    #[inline]
    pub fn node(
        index: NodeID,
        inputs: impl IntoIterator<Item = (PortID, usize)>,
        outputs: impl IntoIterator<Item = (PortID, usize)>,
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
struct Scheduler<NodeID, PortID> {
    transposed: AudioGraph<NodeID, PortID>,
    process_order: Vec<NodeID>,
}

impl<NodeID: Hash + Eq + Clone, PortID: Hash + Eq + Clone> Scheduler<NodeID, PortID> {
    fn compile(self) -> (usize, Vec<Task<NodeID, PortID>>) {
        let mut allocator = BufferAllocator::<NodeID, PortID>::default();
        let mut schedule = vec![];

        let Self {
            mut transposed,
            process_order,
        } = self;

        for id in process_order {
            let proc = transposed.get_node_mut(&id).unwrap();

            let inputs = proc
                .backward_port_ids()
                .iter()
                .map(|port_id| {
                    (
                        port_id.clone(),
                        allocator.remove_claim(&(id.clone(), port_id.clone())),
                    )
                })
                .collect();

            let outputs = proc
                .forward_ports
                .iter()
                .map(|(id, port)| {
                    (
                        id.clone(),
                        if port.connections().is_empty() {
                            usize::MAX
                        } else {
                            allocator.get_free()
                        },
                    )
                })
                .collect();

            schedule.push(Task::Node {
                id,
                inputs,
                outputs,
            });

            let Some(Task::Node { outputs, .. }) = schedule.last() else {
                unreachable!("huh??")
            };

            for (buf_index, port) in outputs
                .clone()
                .into_values()
                .zip(proc.forward_ports.values_mut())
                .filter(|(i, _)| i != &usize::MAX)
            {
                for port_idx in allocator.claim(
                    buf_index,
                    port.connections()
                        .iter()
                        .flat_map(|(node, ports)| {
                            ports.iter().map(move |p| (node.clone(), p.clone()))
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

#[derive(Clone, Debug)]
pub struct AudioGraph<NodeID, PortID> {
    nodes: FnvHashMap<NodeID, Node<NodeID, PortID>>,
}

impl<NodeID, PortID> Default for AudioGraph<NodeID, PortID> {
    #[inline]
    fn default() -> Self {
        Self {
            nodes: Default::default(),
        }
    }
}

impl<NodeID: Borrow<Q> + Eq + Hash, Q: ?Sized + Eq + Hash, PortID> Index<&Q>
    for AudioGraph<NodeID, PortID>
{
    type Output = Node<NodeID, PortID>;
    #[inline]
    fn index(&self, key: &Q) -> &Self::Output {
        self.get_node(key).expect("no node found for this id")
    }
}

impl<NodeID: Hash + Eq + Clone, PortID: Hash + Eq + Clone> AudioGraph<NodeID, PortID> {
    #[inline]
    fn insert_opposite_ports(
        &mut self,
        transposed: &Self,
        node_index: &NodeID,
        processed: &mut Vec<NodeID>,
    ) {
        if processed.contains(node_index) {
            return;
        }

        let node = transposed.get_node(node_index).unwrap();

        for (id, port) in node.forward_ports().iter() {
            let connections = port.connections();

            for (node_idx, port_indices) in connections.iter() {
                self.insert_opposite_ports(transposed, node_idx, processed);

                for port_idx in port_indices {
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
                            unreachable!(
                                "Hash and Eq implementations for NodeID might be inconsistent"
                            );
                        };

                        node
                    };

                    let new = node
                        .get_forward_port_mut(port_idx)
                        .unwrap()
                        .insert_port((node_index.clone(), id.clone()));

                    assert!(new, "INTERNAL ERRROR: port must be newly inserted");
                }
            }
        }

        processed.push(node_index.clone());
    }

    #[inline]
    fn scheduler(&self, root_nodes: FnvHashSet<NodeID>) -> Scheduler<NodeID, PortID> {
        let mut transposed = Self::default();

        let mut process_order = vec![];

        for node_idx in root_nodes {
            assert!(
                transposed
                    .try_insert_node(
                        node_idx.clone(),
                        self.get_node(&node_idx).unwrap().with_reversed_io_layout()
                    )
                    .is_ok(),
                "Hash and Eq implementations for NodeID might be inconsistent"
            );
            transposed.insert_opposite_ports(self, &node_idx, &mut process_order);
        }

        Scheduler {
            transposed,
            process_order,
        }
    }

    /// # Panics
    ///
    /// If any of first `num_root_nodes` nodes of the graph must have more than 1 input,
    /// or if an internal error occured
    #[inline]
    pub fn compile(
        &self,
        root_nodes: impl IntoIterator<Item = NodeID>,
    ) -> (usize, Vec<Task<NodeID, PortID>>) {
        self.scheduler(FnvHashSet::from_iter(root_nodes)).compile()
    }
}

impl<NodeID: Hash + Eq, PortID> AudioGraph<NodeID, PortID> {
    #[inline]
    #[must_use]
    pub fn try_insert_edge(
        &mut self,
        from: (NodeID, PortID),
        to: (NodeID, PortID),
    ) -> Result<bool, bool>
    where
        PortID: Hash + Eq,
    {
        // If either of the ports don't exist, error out
        if self
            .get_node(&from.0)
            .and_then(|node| node.forward_ports().get(&from.1))
            .is_none()
            || self
                .get_node(&to.0)
                .map_or(true, |node| !node.backward_port_ids().contains(&to.1))
        {
            return Err(false);
        }

        if self.is_connected(&to.0, &from.0) {
            return Err(true);
        }

        Ok(self
            .get_node_mut(&from.0)
            .unwrap()
            .get_forward_port_mut(&from.1)
            .unwrap()
            .insert_port(to))
    }

    /// # Panics
    ///
    /// if no node exists at either `from` or `to`
    fn is_connected(&self, from: &NodeID, to: &NodeID) -> bool {
        if from == to {
            return true;
        }

        for port in self.get_node(from).unwrap().forward_ports().values() {
            for node in port.connections().keys() {
                if self.is_connected(node, to) {
                    return true;
                }
            }
        }

        false
    }

    #[inline]
    pub fn get_node<Q: Hash + Eq + ?Sized>(&self, index: &Q) -> Option<&Node<NodeID, PortID>>
    where
        NodeID: Borrow<Q>,
    {
        self.nodes.get(index)
    }

    #[inline]
    pub fn get_node_mut<Q: Hash + Eq + ?Sized>(
        &mut self,
        index: &Q,
    ) -> Option<&mut Node<NodeID, PortID>>
    where
        NodeID: Borrow<Q>,
    {
        self.nodes.get_mut(index)
    }

    #[inline]
    pub fn try_insert_node(
        &mut self,
        id: NodeID,
        node: Node<NodeID, PortID>,
    ) -> Result<&mut Node<NodeID, PortID>, Node<NodeID, PortID>> {
        match self.nodes.entry(id) {
            Entry::Occupied(_) => Err(node),
            Entry::Vacant(e) => Ok(e.insert(node)),
        }
    }
}

impl<PortID: Hash + Eq> AudioGraph<u64, PortID> {
    #[inline]
    pub fn insert_node_id(
        &mut self,
        latency: u64,
        backward_port_ids: impl IntoIterator<Item = PortID>,
        forward_port_ids: impl IntoIterator<Item = PortID>,
    ) -> u64 {
        for i in 0.. {
            if !self.nodes.contains_key(&i) {
                let _ = self
                    .try_insert_node(i, Node::new(latency, backward_port_ids, forward_port_ids));
                return i;
            }
        }

        panic!("Index overflow")
    }
}
