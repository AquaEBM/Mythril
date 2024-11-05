use super::*;
use core::{array, convert::identity as id, iter::zip, ops::Not};

// These tests aren't ideal, I have to print the compiled schedule and review it first,
// then insert it as the rhs of the final assert directive if it's correct. This is inconvenient,
// since there are usually many correct schedules, and any update to the graph's traversal order
// will break these tests, in spite of, theoretically, still creating correct schedules.

#[test]
fn basic_cycle() {
    let mut graph = AudioGraph::default();

    let mut node1 = Node::default();
    let node1_input_id = node1.add_input();
    let node1_output_id = node1.add_output();
    let node1_id = graph.insert_node(node1);

    let mut node2 = Node::default();
    let node2_input_id = node2.add_input();
    let node2_output_id = node2.add_output();
    let node2_id = graph.insert_node(node2);

    assert!(graph
        .try_insert_edge(
            (node2_id.clone(), node2_output_id),
            (node1_id.clone(), node1_input_id),
        )
        .is_ok_and(id));
    assert!(graph
        .try_insert_edge((node1_id, node1_output_id), (node2_id, node2_input_id))
        .is_err_and(id));
}

#[test]
fn insert_redundant_edge() {
    let mut graph = AudioGraph::default();

    let mut node1 = Node::default();
    let node1_output = node1.add_output();
    let node1_id = graph.insert_node(node1);

    let mut node2 = Node::default();
    let node2_input = node2.add_input();
    let node2_id = graph.insert_node(node2);

    assert!(graph
        .try_insert_edge(
            (node1_id.clone(), node1_output.clone()),
            (node2_id.clone(), node2_input.clone()),
        )
        .is_ok_and(id));
    assert!(graph
        .try_insert_edge((node1_id, node1_output), (node2_id, node2_input))
        .is_ok_and(Not::not));
}

#[test]
fn test_basic() {
    let mut graph = AudioGraph::default();

    let mut master = Node::default();
    let master_input_id = master.add_input();
    let master_id = graph.insert_node(master);

    let mut node = Node::default();
    let node_output_id = node.add_output();
    let node_id = graph.insert_node(node);

    assert!(graph
        .try_insert_edge(
            (node_id.clone(), node_output_id.clone()),
            (master_id.clone(), master_input_id.clone()),
        )
        .is_ok_and(id));

    let (num_buffers, schedule) = graph.compile([master_id.clone()]);

    assert_eq!(
        schedule,
        &[
            Task::node(node_id, [], [(node_output_id, 0)]),
            Task::node(master_id, [(master_input_id, 0)], []),
        ]
    );

    assert_eq!(num_buffers, 1);
}

#[test]
fn test_chain() {
    let mut graph = AudioGraph::default();

    let mut master = Node::default();
    let master_input_id = master.add_input();
    let master_id = graph.insert_node(master);

    let mut node1 = Node::default();
    let node1_output_id = node1.add_output();
    let node1_id = graph.insert_node(node1);

    let mut node2 = Node::default();
    let node2_output_id = node2.add_output();
    let node2_input_id = node2.add_input();
    let node2_id = graph.insert_node(node2);

    let mut node3 = Node::default();
    let node3_output_id = node3.add_output();
    let node3_input_id = node3.add_input();
    let node3_id = graph.insert_node(node3);

    assert!(graph
        .try_insert_edge(
            (node1_id.clone(), node1_output_id.clone()),
            (node2_id.clone(), node2_input_id.clone())
        )
        .is_ok_and(id));
    assert!(graph
        .try_insert_edge(
            (node2_id.clone(), node2_output_id.clone()),
            (node3_id.clone(), node3_input_id.clone())
        )
        .is_ok_and(id));
    assert!(graph
        .try_insert_edge(
            (node3_id.clone(), node3_output_id.clone()),
            (master_id.clone(), master_input_id.clone())
        )
        .is_ok_and(id));

    let (num_buffers, schedule) = graph.compile([master_id.clone()]);

    assert_eq!(
        schedule,
        &[
            Task::node(node1_id, [], [(node1_output_id, 0)]),
            Task::node(node2_id, [(node2_input_id, 0)], [(node2_output_id, 0)]),
            Task::node(node3_id, [(node3_input_id, 0)], [(node3_output_id, 0)]),
            Task::node(master_id, [(master_input_id, 0)], []),
        ]
    );

    assert_eq!(num_buffers, 1);
}

#[test]
fn test_mutiple_outputs() {
    let mut graph = AudioGraph::default();

    let mut master: [_; 4] = array::from_fn(|_| Node::default());
    let mut node = master.clone();

    let master_input_id = master.each_mut().map(Node::add_input);
    let node_output_id = node.each_mut().map(Node::add_output);

    let mut insert_node = |node| graph.insert_node(node);

    let master_id = master.map(&mut insert_node);
    let node_id = node.map(insert_node);

    assert!(zip(
        zip(&node_id, &node_output_id),
        zip(&master_id, &master_input_id),
    )
    .all(|((node, output), (master, input))| graph
        .try_insert_edge(
            (node.clone(), output.clone()),
            (master.clone(), input.clone()),
        )
        .is_ok_and(id)));

    let (num_buffers, schedule) = graph.compile(master_id.clone());

    assert!(zip(
        zip(node_id, node_output_id),
        zip(master_id, master_input_id),
    )
    .all(|((node, output), (master, input))| {
        let process_task = Task::node(node, [], [(output, 0)]);
        let proc_task_pos = schedule
            .iter()
            .position(|task| task == &process_task)
            .unwrap();

        let master_task = Task::node(master, [(input, 0)], []);
        let master_task_pos = schedule
            .iter()
            .position(|task| task == &master_task)
            .unwrap();

        proc_task_pos < master_task_pos
    }));

    assert_eq!(num_buffers, 1);
}

#[test]
fn test_adder() {
    let mut graph = AudioGraph::default();

    let mut master = Node::default();
    let master_input_id = master.add_input();
    let master_id = graph.insert_node(master);

    let [(left_output_id, left_id), (right_output_id, right_id)] = array::from_fn(|_| {
        let mut node = Node::default();
        (node.add_output(), graph.insert_node(node))
    });

    assert!(graph
        .try_insert_edge(
            (left_id.clone(), left_output_id.clone()),
            (master_id.clone(), master_input_id.clone()),
        )
        .is_ok_and(id));
    assert!(graph
        .try_insert_edge(
            (right_id.clone(), right_output_id.clone()),
            (master_id.clone(), master_input_id.clone()),
        )
        .is_ok_and(id));

    let (num_buffers, schedule) = graph.compile([master_id.clone()]);

    // println!("{schedule:#?}");

    assert_eq!(
        schedule,
        [
            Task::node(left_id, [], [(left_output_id, 0)]),
            Task::node(right_id, [], [(right_output_id, 1)]),
            Task::sum(1, 0, 0),
            Task::node(master_id, [(master_input_id, 0)], []),
        ]
    );

    assert_eq!(num_buffers, 2);
}

#[test]
fn test_multiple_adders() {
    let mut graph = AudioGraph::default();

    let mut master = Node::default();
    let master_input = master.add_input();
    let master_id = graph.insert_node(master);

    let nodes: [_; 3] = array::from_fn(|_i| {
        let mut node = Node::default();
        (node.add_output(), graph.insert_node(node))
    });

    for (node_output, node_id) in &nodes {
        assert!(graph
            .try_insert_edge(
                (node_id.clone(), node_output.clone()),
                (master_id.clone(), master_input.clone())
            )
            .is_ok_and(id));
    }

    let (num_buffers, schedule) = graph.compile([master_id.clone()]);

    println!("{schedule:#?}");

    let [(node_a_output_id, node_a_id), (node_b_output_id, node_b_id), (node_c_output_id, node_c_id)] =
        nodes;

    assert_eq!(
        schedule,
        [
            Task::node(node_a_id, [], [(node_a_output_id, 0)]),
            Task::node(node_c_id, [], [(node_c_output_id, 1)]),
            Task::sum(1, 0, 0),
            Task::node(node_b_id, [], [(node_b_output_id, 1)]),
            Task::sum(1, 0, 0),
            Task::node(master_id, [(master_input, 0)], []),
        ]
    );

    assert_eq!(num_buffers, 2);
}

#[test]
fn test_m_graph() {
    let mut graph = AudioGraph::default();

    let mut master_nodes: [_; 3] = array::from_fn(|_i| Node::default());

    let master_input_ids = master_nodes.each_mut().map(|node| node.add_input());
    let master_ids = master_nodes.map(|node| graph.insert_node(node));

    let [(n1_output_id, n1_id), (n2_output_id, n2_id)] = array::from_fn(|_i| {
        let mut n1 = Node::default();
        (n1.add_output(), graph.insert_node(n1))
    });

    // As an example of the above comment, it is possible to schedule this graph in a way that requires
    // 3 buffers, because the traversal order when computing said schedule depends on the hash function.

    // bad insertion order

    // for (master_port, node_port) in [
    //     (
    //         (master_ids[0].clone(), master_input_ids[0].clone()),
    //         (n1_id.clone(), n1_output_id.clone()),
    //     ),
    //     (
    //         (master_ids[1].clone(), master_input_ids[1].clone()),
    //         (n1_id.clone(), n1_output_id.clone()),
    //     ),
    //     (
    //         (master_ids[1].clone(), master_input_ids[1].clone()),
    //         (n2_id.clone(), n2_output_id.clone()),
    //     ),
    //     (
    //         (master_ids[2].clone(), master_input_ids[2].clone()),
    //         (n2_id.clone(), n2_output_id.clone()),
    //     ),
    // ] {
    //     assert!(graph.try_insert_edge(node_port, master_port).is_ok_and(id));
    // }

    // good insertion order

    for (master_port, node_port) in [
        (
            (master_ids[1].clone(), master_input_ids[1].clone()),
            (n1_id.clone(), n1_output_id.clone()),
        ),
        (
            (master_ids[0].clone(), master_input_ids[0].clone()),
            (n1_id.clone(), n1_output_id.clone()),
        ),
        (
            (master_ids[0].clone(), master_input_ids[0].clone()),
            (n2_id.clone(), n2_output_id.clone()),
        ),
        (
            (master_ids[2].clone(), master_input_ids[2].clone()),
            (n2_id.clone(), n2_output_id.clone()),
        ),
    ] {
        assert!(graph.try_insert_edge(node_port, master_port).is_ok_and(id));
    }

    let (num_buffers, schedule) = graph.compile(master_ids.clone());

    // println!("{schedule:#?}");

    let [master1, master2, master3] = master_ids;
    let [master1_input, master2_input, master3_input] = master_input_ids;

    // assert_eq!(
    //     schedule,
    //     [
    //         Task::node(n2_id, [], [(n2_output_id, 0)]),
    //         Task::node(n1_id, [], [(n1_output_id, 1)]),
    //         Task::sum(1, 0, 2),
    //         Task::node(master2, [(master2_input, 2)], []),
    //         Task::node(master1, [(master1_input, 1)], []),
    //         Task::node(master3, [(master3_input, 0)], []),
    //     ],
    // );

    // assert_eq!(num_buffers, 3);

    assert_eq!(
        schedule,
        [
            Task::node(n1_id, [], [(n1_output_id, 0)]),
            Task::node(master2, [(master2_input, 0)], []),
            Task::node(n2_id, [], [(n2_output_id, 1)]),
            Task::sum(1, 0, 0),
            Task::node(master1, [(master1_input, 0)], []),
            Task::node(master3, [(master3_input, 1)], []),
        ],
    );

    assert_eq!(num_buffers, 2);
}

#[test]
fn mutiple_input_ports() {
    let mut graph = AudioGraph::default();

    let mut master = Node::default();
    let master_input_id = master.add_input();
    let master_id = graph.insert_node(master);

    let mut source_node = Node::default();
    let source_node_output_id = source_node.add_output();
    let source_node_id = graph.insert_node(source_node);

    let mut sink_node = Node::default();
    let sink_node_input_ids: [_; 4] = array::from_fn(|_i| sink_node.add_input());
    let sink_node_output_id = sink_node.add_output();
    let sink_node_id = graph.insert_node(sink_node);

    for sink_node_input_id in &sink_node_input_ids {
        assert!(graph
            .try_insert_edge(
                (source_node_id.clone(), source_node_output_id.clone()),
                (sink_node_id.clone(), sink_node_input_id.clone())
            )
            .is_ok_and(id));
    }

    assert!(graph.try_insert_edge(
        (sink_node_id.clone(), sink_node_output_id.clone()),
        (master_id.clone(), master_input_id.clone())
    ).is_ok_and(id));

    let (num_buffers, schedule) = graph.compile([master_id.clone()]);

    // println!("{schedule:#?}");

    assert_eq!(
        schedule,
        [
            Task::node(source_node_id, [], [(source_node_output_id, 0)]),
            Task::node(sink_node_id, sink_node_input_ids.map(|id| (id, 0)), [(sink_node_output_id, 0)]),
            Task::node(master_id, [(master_input_id, 0)], []),
        ]
    );

    assert_eq!(num_buffers, 1);
}
