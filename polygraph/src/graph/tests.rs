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

    #[rustfmt::skip]
    assert!(zip(
        zip(&node_id, &node_output_id),
        zip(&master_id, &master_input_id),
    ).all(|((node, output), (master, input))| graph.try_insert_edge(
        (node.clone(), output.clone()),
        (master.clone(), input.clone()),
    ).is_ok_and(id)));

    let (num_buffers, schedule) = graph.compile(master_id.clone());

    #[rustfmt::skip]
    assert!(zip(
        zip(node_id, node_output_id),
        zip(master_id, master_input_id),
    ).all(|((node, output), (master, input))| {

        let process_task = Task::node(node, [], [(output, 0)]);
        let master_task = Task::node(master, [(input, 0)], []);
        
        schedule.iter().position(|task| task == &process_task).unwrap() <
        schedule.iter().position(|task| task == &master_task).unwrap()
    }));

    assert_eq!(num_buffers, 1);
}

// #[test]
// fn test_adder() {
//     let mut graph = AudioGraph::default();

//     let master = graph.insert_node_id(0, [], [0]);

//     let left = graph.insert_node_id(0, [0], []);
//     let right = graph.insert_node_id(0, [0], []);

//     assert!(graph.try_insert_edge((master, 0), (left, 0)).is_ok_and(id));
//     assert!(graph.try_insert_edge((master, 0), (right, 0)).is_ok_and(id));

//     let (num_buffers, schedule) = graph.compile([master]);

//     // println!("{schedule:#?}");

//     assert_eq!(
//         schedule,
//         [
//             Task::node(left, [], [(0, 0)]),
//             Task::node(right, [], [(0, 1)]),
//             Task::sum(1, 0, 0),
//             Task::node(master, [(0, 0)], []),
//         ]
//     );

//     assert_eq!(num_buffers, 2);
// }

// #[test]
// fn test_multiple_adders() {
//     let mut graph = AudioGraph::default();

//     let master = graph.insert_node_id(0, [], [0]);
//     let a = graph.insert_node_id(0, [0], []);
//     let b = graph.insert_node_id(0, [0], []);
//     let c = graph.insert_node_id(0, [0], []);

//     assert!(graph.try_insert_edge((master, 0), (a, 0)).is_ok_and(id));
//     assert!(graph.try_insert_edge((master, 0), (b, 0)).is_ok_and(id));
//     assert!(graph.try_insert_edge((master, 0), (c, 0)).is_ok_and(id));

//     let (num_buffers, schedule) = graph.compile([master]);

//     // println!("{schedule:#?}");

//     assert_eq!(
//         schedule,
//         [
//             Task::node(a, [], [(0, 0)]),
//             Task::node(c, [], [(0, 1)]),
//             Task::sum(1, 0, 0),
//             Task::node(b, [], [(0, 1)]),
//             Task::sum(1, 0, 0),
//             Task::node(master, [(0, 0)], []),
//         ]
//     );

//     assert_eq!(num_buffers, 2);
// }

// #[test]
// fn test_m_graph() {
//     let mut graph = AudioGraph::default();

//     let master1 = graph.insert_node_id(0, [], [0]);
//     let master2 = graph.insert_node_id(0, [], [0]);
//     let master3 = graph.insert_node_id(0, [], [0]);

//     let n1 = graph.insert_node_id(0, [0], []);
//     let n2 = graph.insert_node_id(0, [0], []);

//     assert!(graph.try_insert_edge((master1, 0), (n1, 0)).is_ok_and(id));
//     assert!(graph.try_insert_edge((master2, 0), (n1, 0)).is_ok_and(id));
//     assert!(graph.try_insert_edge((master2, 0), (n2, 0)).is_ok_and(id));
//     assert!(graph.try_insert_edge((master3, 0), (n2, 0)).is_ok_and(id));

//     let (num_buffers, schedule) = graph.compile([master2, master1, master3]);

//     // println!("{schedule:#?}");

//     assert_eq!(
//         schedule,
//         [
//             Task::node(n2, [], [(0, 0)]),
//             Task::node(n1, [], [(0, 1)]),
//             Task::sum(1, 0, 2),
//             Task::node(master2, [(0, 2)], []),
//             Task::node(master1, [(0, 1)], []),
//             Task::node(master3, [(0, 0)], []),
//         ],
//     );

//     assert_eq!(num_buffers, 3);
// }

// #[test]
// fn mutiple_input_ports() {
//     let mut graph = AudioGraph::default();

//     let master = graph.insert_node_id(0, [], [0]);
//     let node = graph.insert_node_id(0, [0], [0, 1, 2, 3]);
//     let g = graph.insert_node_id(0, [0], []);

//     assert!(graph.try_insert_edge((node, 0), (g, 0)).is_ok_and(id));
//     assert!(graph.try_insert_edge((node, 1), (g, 0)).is_ok_and(id));
//     assert!(graph.try_insert_edge((node, 2), (g, 0)).is_ok_and(id));
//     assert!(graph.try_insert_edge((node, 3), (g, 0)).is_ok_and(id));
//     assert!(graph.try_insert_edge((master, 0), (node, 0)).is_ok_and(id));

//     let (num_buffers, schedule) = graph.compile([master]);

//     // println!("{schedule:#?}");

//     assert_eq!(
//         schedule,
//         [
//             Task::node(g, [], [(0, 0)]),
//             Task::node(node, [(0, 0), (1, 0), (2, 0), (3, 0)], [(0, 0)]),
//             Task::node(master, [(0, 0)], []),
//         ]
//     );

//     assert_eq!(num_buffers, 1);
// }
