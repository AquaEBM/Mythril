use core::{
    iter, mem,
    ops::{Index, IndexMut},
};
use std::collections;

type HasherBuilder = fnv::FnvBuildHasher;

type HashMap<K, V> = collections::HashMap<K, V, HasherBuilder>;
type HashSet<T> = collections::HashSet<T, HasherBuilder>;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct PortIndex {
    pub port_index: usize,
    pub node_index: usize,
}

impl From<(usize, usize)> for PortIndex {
    fn from((node_index, port_index): (usize, usize)) -> Self {
        Self {
            port_index,
            node_index,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Port(HashMap<usize, HashSet<usize>>);

impl Port {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.values().all(|ports| ports.is_empty())
    }

    #[inline]
    pub fn iter_connections(&self) -> impl Iterator<Item = PortIndex> + '_ {
        self.0.iter().flat_map(|(&node_index, port_indices)| {
            port_indices.iter().map(move |&port_index| PortIndex {
                port_index,
                node_index,
            })
        })
    }

    #[inline]
    pub fn iter_connected_nodes(&self) -> impl Iterator<Item = &usize> {
        self.0.keys()
    }

    fn drain_connections(&mut self) -> impl Iterator<Item = PortIndex> + '_ {
        self.0.drain().flat_map(|(node_index, port_indices)| {
            port_indices.into_iter().map(move |port_index| PortIndex {
                node_index,
                port_index,
            })
        })
    }

    fn insert_port(
        &mut self,
        PortIndex {
            node_index,
            port_index,
        }: PortIndex,
    ) -> bool {
        let mut newly_inserted = true;

        self.0
            .entry(node_index)
            .and_modify(|indices| newly_inserted = indices.insert(port_index))
            .or_insert_with(|| HashSet::from_iter([port_index]));

        newly_inserted
    }

    #[inline]
    pub fn remove_port(
        &mut self,
        PortIndex {
            port_index,
            node_index,
        }: &PortIndex,
    ) -> bool {
        self.0
            .get_mut(node_index)
            .is_some_and(|ports| ports.remove(port_index))
    }

    #[inline]
    pub fn is_connected_to_node(&self, index: usize) -> bool {
        self.0.contains_key(&index)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Node {
    forward_ports: Vec<Port>,
    num_backward_ports: usize,
}

impl Node {
    fn with_reversed_io_layout(&self) -> Self {
        Self::new(self.num_backward_ports, self.forward_ports.len())
    }

    fn new(num_forward_ports: usize, num_backward_ports: usize) -> Self {
        Self {
            forward_ports: iter::repeat_with(Port::default)
                .take(num_forward_ports)
                .collect(),
            num_backward_ports,
        }
    }

    #[inline]
    pub fn get_forward_port(&self, index: usize) -> Option<&Port> {
        self.forward_ports.get(index)
    }

    #[inline]
    pub fn get_forward_port_mut(&mut self, index: usize) -> Option<&mut Port> {
        self.forward_ports.get_mut(index)
    }

    #[inline]
    pub fn iter_forward_ports(&self) -> impl Iterator<Item = &Port> {
        self.forward_ports.iter()
    }

    #[inline]
    pub fn num_forward_ports(&self) -> usize {
        self.forward_ports.len()
    }

    #[inline]
    pub fn num_backward_ports(&self) -> usize {
        self.num_backward_ports
    }

    #[inline]
    pub fn add_forward_port(&mut self) {
        self.forward_ports.push(Port::default());
    }

    #[inline]
    pub fn add_backward_port(&mut self) {
        self.num_backward_ports += 1;
    }
}

#[derive(Default, Debug)]
struct BufferAllocator {
    buffers: HashMap<PortIndex, usize>,
    ports: Vec<HashSet<PortIndex>>,
}

impl BufferAllocator {
    fn len(&self) -> usize {
        self.ports.len()
    }

    fn get_empty_set_index<T>(list: &mut Vec<HashSet<T>>) -> usize {
        list.iter()
            .enumerate()
            .find_map(|(i, port_idxs)| port_idxs.is_empty().then_some(i))
            .unwrap_or_else(|| {
                let tmp = list.len();
                list.push(HashSet::default());
                tmp
            })
    }

    fn get_free(&mut self) -> usize {
        Self::get_empty_set_index(&mut self.ports)
    }

    fn claim(&mut self, buffer_index: usize, ports: HashSet<PortIndex>) -> HashSet<PortIndex> {
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

                self.buffers.insert(*port, buffer_index);
                false
            })
            .collect()
    }

    fn remove_claim(&mut self, port: &PortIndex) -> usize {
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

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Task {
    Node {
        index: usize,
        inputs: Box<[usize]>,
        outputs: Box<[usize]>,
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
        index: usize,
        inputs: impl Into<Box<[usize]>>,
        outputs: impl Into<Box<[usize]>>,
    ) -> Self {
        Self::Node {
            index,
            inputs: inputs.into(),
            outputs: outputs.into(),
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
    process_order: Vec<usize>,
}

impl Scheduler {
    fn compile(self) -> (usize, Vec<Task>) {
        let mut allocator = BufferAllocator::default();
        let mut schedule = vec![];

        let Self {
            mut transposed,
            process_order,
        } = self;

        for node_index in process_order {
            let proc = transposed.get_node_mut(node_index).unwrap();

            let inputs = (0..proc.num_backward_ports)
                .map(|i| PortIndex {
                    node_index,
                    port_index: i,
                })
                .map(|port| allocator.remove_claim(&port))
                .collect();

            let outputs = proc
                .forward_ports
                .iter()
                .map(|port| {
                    if port.is_empty() {
                        usize::MAX
                    } else {
                        allocator.get_free()
                    }
                })
                .collect();

            schedule.push(Task::Node {
                index: node_index,
                inputs,
                outputs,
            });

            let Some(Task::Node { outputs, .. }) = schedule.last() else {
                unreachable!("huh??")
            };

            for (&buf_index, port) in outputs
                .clone()
                .iter()
                .zip(proc.forward_ports.iter_mut())
                .filter(|(&i, _)| i != usize::MAX)
            {
                for port in allocator.claim(buf_index, port.drain_connections().collect()) {
                    let other_buf_idx = allocator.remove_claim(&port);
                    let new_free_buf = allocator.get_free();
                    assert!(
                        allocator
                            .claim(new_free_buf, HashSet::from_iter([port]))
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
    nodes: Vec<Option<Node>>,
}

impl Index<usize> for AudioGraph {
    type Output = Node;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        self.get_node(index).unwrap()
    }
}

impl IndexMut<usize> for AudioGraph {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_node_mut(index).unwrap()
    }
}

impl Index<PortIndex> for AudioGraph {
    type Output = Port;
    #[inline]
    fn index(&self, index: PortIndex) -> &Self::Output {
        self.get_forward_port(index).unwrap()
    }
}

impl IndexMut<PortIndex> for AudioGraph {
    #[inline]
    fn index_mut(&mut self, index: PortIndex) -> &mut Self::Output {
        self.get_forward_port_mut(index).unwrap()
    }
}

impl AudioGraph {
    fn scheduler(&self, root_nodes: HashSet<usize>) -> Scheduler {
        let mut transposed = Self {
            nodes: self
                .nodes
                .iter()
                .map(|slot| slot.as_ref().map(Node::with_reversed_io_layout))
                .collect(),
        };

        let mut process_order = vec![];

        fn insert_opposite_ports(
            this: &mut AudioGraph,
            transposed: &AudioGraph,
            node_index: usize,
            order: &mut Vec<usize>,
        ) {
            if order.contains(&node_index) {
                return;
            }

            for (i, ports) in transposed
                .get_node(node_index)
                .unwrap()
                .iter_forward_ports()
                .enumerate()
            {
                let this_port = PortIndex {
                    node_index,
                    port_index: i,
                };

                for PortIndex {
                    port_index,
                    node_index,
                } in ports.iter_connections()
                {
                    let new = this.get_node_mut(node_index).unwrap().forward_ports[port_index]
                        .insert_port(this_port);
                    assert!(new, "INTERNAL ERRROR: port must be newly inserted");
                    insert_opposite_ports(this, transposed, node_index, order);
                }
            }

            order.push(node_index);
        }

        for node_idx in root_nodes {
            insert_opposite_ports(&mut transposed, self, node_idx, &mut process_order)
        }

        Scheduler {
            transposed,
            process_order,
        }
    }

    #[inline]
    #[must_use]
    pub fn try_insert_edge(
        &mut self,
        from: impl Into<PortIndex>,
        to: impl Into<PortIndex>,
    ) -> Result<bool, bool> {
        let from = from.into();
        let to = to.into();

        // If either of the ports don't exist, error out
        if self.get_forward_port(from).is_err()
            || self
                .get_node(to.node_index)
                .map_or(true, |node| to.port_index >= node.num_backward_ports())
        {
            return Err(false);
        }

        if self.is_connected(to.node_index, from.node_index) {
            return Err(true);
        }

        Ok(self[from].insert_port(to))
    }

    /// # Panics
    ///
    /// if no node exists at either `from` or `to`
    fn is_connected(&self, from: usize, to: usize) -> bool {
        if from == to {
            return true;
        }

        for port in self.get_node(from).unwrap().iter_forward_ports() {
            for &node in port.iter_connected_nodes() {
                if self.is_connected(node, to) {
                    return true;
                }
            }
        }

        false
    }

    #[inline]
    pub fn insert_node(&mut self, num_backward_ports: usize, num_forward_ports: usize) -> usize {
        let node = Some(Node::new(num_forward_ports, num_backward_ports));

        for (i, slot) in self.nodes.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = node;
                return i;
            }
        }

        let tmp = self.nodes.len();
        self.nodes.push(node);
        tmp
    }

    #[inline]
    pub fn get_node(&self, index: usize) -> Option<&Node> {
        self.nodes.get(index).and_then(Option::as_ref)
    }

    #[inline]
    pub fn get_node_mut(&mut self, index: usize) -> Option<&mut Node> {
        self.nodes.get_mut(index).and_then(Option::as_mut)
    }

    #[inline]
    pub fn get_forward_port(&self, index: impl Into<PortIndex>) -> Result<&Port, bool> {
        let PortIndex {
            node_index,
            port_index,
        } = index.into();

        self.get_node(node_index)
            .ok_or(false)?
            .get_forward_port(port_index)
            .ok_or(true)
    }

    #[inline]
    pub fn get_forward_port_mut(&mut self, index: impl Into<PortIndex>) -> Result<&mut Port, bool> {
        let PortIndex {
            port_index,
            node_index,
        } = index.into();

        self.get_node_mut(node_index)
            .ok_or(false)?
            .get_forward_port_mut(port_index)
            .ok_or(true)
    }

    /// # Panics
    ///
    /// If any of first `num_root_nodes` nodes of the graph must have more than 1 input,
    /// or if an internal error occured
    #[inline]
    pub fn compile(&self, root_nodes: HashSet<usize>) -> (usize, Vec<Task>) {
        self.scheduler(root_nodes).compile()
    }
}
