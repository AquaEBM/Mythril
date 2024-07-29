use super::*;

#[test]
#[should_panic]
fn insert_basic_cycle() {
    let mut graph = AudioGraph::with_global_io_config(0, 0);

    let node1 = graph.insert_processor(1, 1);
    let node2 = graph.insert_processor(1, 1);

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Processor(node2)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node2)),
            Port::new(0, NodeIndex::Processor(node1)),
        )
        .unwrap();
}

#[test]
#[should_panic]
fn insert_cycle_in_complex_graph() {
    let mut graph = AudioGraph::with_global_io_config(1, 1);

    let node1 = graph.insert_processor(1, 1);
    let node2 = graph.insert_processor(1, 1);

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Global),
            Port::new(0, NodeIndex::Processor(node1)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Global),
            Port::new(0, NodeIndex::Processor(node2)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node2)),
            Port::new(0, NodeIndex::Processor(node1)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node2)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Processor(node2)),
        )
        .unwrap();
}

#[test]
fn insert_redundant_edge() {
    let mut graph = AudioGraph::with_global_io_config(0, 0);

    let node1 = graph.insert_processor(0, 1);
    let node2 = graph.insert_processor(1, 0);

    let newly_inserted1 = graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Processor(node2)),
        )
        .unwrap();

    let newly_inserted2 = graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Processor(node2)),
        )
        .unwrap();

    assert!(newly_inserted1 && !newly_inserted2)
}

#[test]
fn test_basic() {
    let mut graph = AudioGraph::with_global_io_config(1, 1);
    let node_index = graph.insert_processor(1, 1);

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node_index)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Global),
            Port::new(0, NodeIndex::Processor(node_index)),
        )
        .unwrap();

    let (schedule, num_buffers) = graph.compile();

    assert!(
        schedule
            == &[ProcessTask::Process {
                proc_index: 0,
                inputs: Box::from([Some(BufferIndex::SuperInput(0))]),
                outputs: Box::new([Some(OutBufIndex::Super(0))])
            }]
            && num_buffers == 0
    );
}

#[test]
fn basic_chain() {
    let mut graph = AudioGraph::with_global_io_config(1, 1);
    let first_node_index = graph.insert_processor(1, 1);
    let second_node_index = graph.insert_processor(1, 1);

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(second_node_index)),
            Port::new(0, NodeIndex::Processor(first_node_index)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Global),
            Port::new(0, NodeIndex::Processor(second_node_index)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(first_node_index)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    let (schedule, num_buffers) = graph.compile();

    assert!(
        schedule
            == &[
                ProcessTask::Process {
                    proc_index: second_node_index,
                    inputs: Box::new([Some(BufferIndex::SuperInput(0))]),
                    outputs: Box::new([Some(OutBufIndex::Super(0))])
                },
                ProcessTask::Process {
                    proc_index: first_node_index,
                    inputs: Box::new([Some(BufferIndex::Output(OutBufIndex::Super(0)))]),
                    outputs: Box::new([Some(OutBufIndex::Super(0))])
                },
            ]
            && num_buffers == 0
    )
}

#[test]
fn basic_adder() {
    let mut graph = AudioGraph::with_global_io_config(0, 1);
    let node1 = graph.insert_processor(0, 1);
    let node2 = graph.insert_processor(0, 1);

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node2)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    let (schedule, num_buffers) = graph.compile();

    assert!(
        schedule
            == &[
                ProcessTask::Process {
                    proc_index: node1,
                    inputs: Box::new([]),
                    outputs: Box::new([Some(OutBufIndex::Super(0))]),
                },
                ProcessTask::Process {
                    proc_index: node2,
                    inputs: Box::new([]),
                    outputs: Box::new([Some(OutBufIndex::Local(0))]),
                },
                ProcessTask::Sum {
                    left: BufferIndex::Output(OutBufIndex::Local(0)),
                    right: BufferIndex::Output(OutBufIndex::Super(0)),
                    output: OutBufIndex::Super(0),
                }
            ]
            && num_buffers == 1
    )
}

#[test]
fn multiple_adds() {
    let mut graph = AudioGraph::with_global_io_config(0, 1);
    let node1 = graph.insert_processor(0, 1);
    let node2 = graph.insert_processor(0, 1);
    let node3 = graph.insert_processor(0, 1);

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node2)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node3)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    let (schedule, num_buffers) = graph.compile();

    assert!(
        schedule
            == &[
                ProcessTask::Process {
                    proc_index: node1,
                    inputs: Box::new([]),
                    outputs: Box::new([Some(OutBufIndex::Super(0))]),
                },
                ProcessTask::Process {
                    proc_index: node2,
                    inputs: Box::new([]),
                    outputs: Box::new([Some(OutBufIndex::Local(0))]),
                },
                ProcessTask::Sum {
                    left: BufferIndex::Output(OutBufIndex::Local(0)),
                    right: BufferIndex::Output(OutBufIndex::Super(0)),
                    output: OutBufIndex::Super(0),
                },
                ProcessTask::Process {
                    proc_index: node3,
                    inputs: Box::new([]),
                    outputs: Box::new([Some(OutBufIndex::Local(0))]),
                },
                ProcessTask::Sum {
                    left: BufferIndex::Output(OutBufIndex::Local(0)),
                    right: BufferIndex::Output(OutBufIndex::Super(0)),
                    output: OutBufIndex::Super(0),
                }
            ]
            && num_buffers == 1
    )
}

/// This test should be checked manually for correctness
#[test]
fn diamond() {
    let mut graph = AudioGraph::with_global_io_config(0, 1);
    let node1 = graph.insert_processor(0, 1);
    let node2 = graph.insert_processor(1, 1);
    let node3 = graph.insert_processor(1, 1);

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Processor(node2)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Processor(node3)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node2)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node3)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    let (schedule, num_buffers) = graph.compile();

    println!("schedule: {schedule:#?}");
    println!("num_buffers: {num_buffers}");
}

/// This test should be checked manually for correctness
#[test]
fn multi_parrallel() {
    let mut graph = AudioGraph::with_global_io_config(0, 1);
    let node1 = graph.insert_processor(0, 1);
    let node2 = graph.insert_processor(1, 1);
    let node3 = graph.insert_processor(1, 1);
    let node4 = graph.insert_processor(1, 1);
    let node5 = graph.insert_processor(1, 1);

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Processor(node2)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Processor(node3)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Processor(node4)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Processor(node5)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node2)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node3)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node4)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node5)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    let (schedule, num_buffers) = graph.compile();

    println!("{schedule:#?}");
    println!("num_buffers: {num_buffers}");
}

#[test]
fn m_structure() {
    let mut graph = AudioGraph::with_global_io_config(0, 3);
    let node1 = graph.insert_processor(0, 1);
    let node2 = graph.insert_processor(0, 1);

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(1, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node2)),
            Port::new(1, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node2)),
            Port::new(2, NodeIndex::Global),
        )
        .unwrap();

    let (schedule, num_buffers) = graph.compile();

    assert!(
        schedule
            == &[
                ProcessTask::Process {
                    proc_index: node1,
                    inputs: Box::new([]),
                    outputs: Box::new([Some(OutBufIndex::Super(0))]),
                },
                ProcessTask::Process {
                    proc_index: node2,
                    inputs: Box::new([]),
                    outputs: Box::new([Some(OutBufIndex::Super(2))]),
                },
                ProcessTask::Sum {
                    left: BufferIndex::Output(OutBufIndex::Super(2)),
                    right: BufferIndex::Output(OutBufIndex::Super(0)),
                    output: OutBufIndex::Super(1),
                }
            ]
            && num_buffers == 0
    )
}

/// This test should be checked manually for correctness
#[test]
fn multiple_global_outputs() {
    let mut graph = AudioGraph::with_global_io_config(0, 3);
    let node = graph.insert_processor(0, 1);

    for i in 0..3 {
        graph
            .insert_edge(
                Port::new(0, NodeIndex::Processor(node)),
                Port::new(i, NodeIndex::Global),
            )
            .unwrap();
    }

    let (schedule, num_buffers) = graph.compile();

    println!("{schedule:#?}");
    println!("num_buffers: {num_buffers}");
}

/// This test should be checked manually for correctness
#[test]
fn copy_global_input_to_global_outputs() {
    let mut graph = AudioGraph::with_global_io_config(1, 3);

    for i in 0..3 {
        graph
            .insert_edge(
                Port::new(0, NodeIndex::Global),
                Port::new(i, NodeIndex::Global),
            )
            .unwrap();
    }

    let (schedule, num_buffers) = graph.compile();

    println!("{schedule:#?}");
    println!("num_buffers: {num_buffers}");
}

/// This test should be checked manually for correctness
#[test]
fn complex() {
    let mut graph = AudioGraph::with_global_io_config(1, 3);

    let node1 = graph.insert_processor(0, 1);

    let node2 = graph.insert_processor(1, 1);

    let node3 = graph.insert_processor(1, 1);

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Global),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Processor(node2)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node1)),
            Port::new(0, NodeIndex::Processor(node3)),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node2)),
            Port::new(0, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node2)),
            Port::new(1, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node3)),
            Port::new(1, NodeIndex::Global),
        )
        .unwrap();

    graph
        .insert_edge(
            Port::new(0, NodeIndex::Processor(node3)),
            Port::new(2, NodeIndex::Global),
        )
        .unwrap();

    let (schedule, num_buffers) = graph.compile();

    println!("{schedule:#?}");
    println!("num_buffers: {num_buffers}");
}
