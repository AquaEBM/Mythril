use super::*;
use core::{convert::identity as id, ops::Not};

// These tests aren't ideal, I have to print the compiled schedule and review it first,
// then insert it as the rhs of the afinal ssert directive if it's correct. This is inconvenient,
// since there are usually many correct schedules, and any update to the graph's traversal order
// will break these tests, in spite of, theoretically, still creating correct schedules.

#[test]
fn insert_basic_cycle() {
    let mut graph = AudioGraph::default();

    let node1 = graph.insert_node_id(0, [0], [0]);
    let node2 = graph.insert_node_id(0, [0], [0]);

    assert!(graph.try_insert_edge((node1, 0), (node2, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((node2, 0), (node1, 0)).is_err_and(id));
}

#[test]
fn insert_complex_cycle() {
    let mut graph = AudioGraph::default();

    let a = graph.insert_node_id(0, [], [0]);
    let b = graph.insert_node_id(0, [0], [0]);
    let c = graph.insert_node_id(0, [0], [0]);
    let d = graph.insert_node_id(0, [0], []);

    assert!(graph.try_insert_edge((a, 0), (b, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((b, 0), (c, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((c, 0), (d, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((d, 0), (a, 0)).is_err_and(Not::not));
}

#[test]
fn insert_redundant_edge() {
    let mut graph = AudioGraph::default();

    let a = graph.insert_node_id(0, [], [0]);
    let b = graph.insert_node_id(0, [0], []);

    assert!(graph.try_insert_edge((a, 0), (b, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((a, 0), (b, 0)).is_ok_and(Not::not));
}

#[test]
fn test_basic() {
    let mut graph = AudioGraph::default();

    let master = graph.insert_node_id(0, [], [0]);
    let node = graph.insert_node_id(0, [0], []);

    assert!(graph.try_insert_edge((master, 0), (node, 0)).is_ok_and(id));

    let (num_buffers, schedule) = graph.compile([master]);

    assert_eq!(
        schedule,
        &[
            Task::node(node, [], [(0, 0)]),
            Task::node(master, [(0, 0)], []),
        ]
    );

    assert_eq!(num_buffers, 1);
}

#[test]
fn test_chain() {
    let mut graph = AudioGraph::default();

    let master = graph.insert_node_id(0, [], [0]);
    let node1 = graph.insert_node_id(0, [0], []);
    let node2 = graph.insert_node_id(0, [0], [0]);
    let node3 = graph.insert_node_id(0, [0], [0]);

    assert!(graph.try_insert_edge((master, 0), (node3, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((node3, 0), (node2, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((node2, 0), (node1, 0)).is_ok_and(id));

    let (num_buffers, schedule) = graph.compile([master]);

    assert_eq!(
        schedule,
        &[
            Task::node(node1, [], [(0, 0)]),
            Task::node(node2, [(0, 0)], [(0, 0)]),
            Task::node(node3, [(0, 0)], [(0, 0)]),
            Task::node(master, [(0, 0)], []),
        ]
    );

    assert_eq!(num_buffers, 1);
}

#[test]
fn test_mutiple_outputs() {
    let mut graph = AudioGraph::default();

    let o1 = graph.insert_node_id(0, [], [0]);
    let o2 = graph.insert_node_id(0, [], [0]);
    let o3 = graph.insert_node_id(0, [], [0]);
    let o4 = graph.insert_node_id(0, [], [0]);

    let n1 = graph.insert_node_id(0, [0], []);
    let n2 = graph.insert_node_id(0, [0], []);
    let n3 = graph.insert_node_id(0, [0], []);
    let n4 = graph.insert_node_id(0, [0], []);

    assert!(graph.try_insert_edge((o1, 0), (n1, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((o2, 0), (n2, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((o3, 0), (n3, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((o4, 0), (n4, 0)).is_ok_and(id));

    let (num_buffers, schedule) = graph.compile([o1, o2, o3, o4]);

    assert_eq!(
        schedule,
        [
            Task::node(n2, [], [(0, 0)]),
            Task::node(o2, [(0, 0)], []),
            Task::node(n1, [], [(0, 0)]),
            Task::node(o1, [(0, 0)], []),
            Task::node(n4, [], [(0, 0)]),
            Task::node(o4, [(0, 0)], []),
            Task::node(n3, [], [(0, 0)]),
            Task::node(o3, [(0, 0)], []),
        ]
    );

    assert_eq!(num_buffers, 1);
}

#[test]
fn test_adder() {
    let mut graph = AudioGraph::default();

    let master = graph.insert_node_id(0, [], [0]);

    let left = graph.insert_node_id(0, [0], []);
    let right = graph.insert_node_id(0, [0], []);

    assert!(graph.try_insert_edge((master, 0), (left, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((master, 0), (right, 0)).is_ok_and(id));

    let (num_buffers, schedule) = graph.compile([master]);

    // println!("{schedule:#?}");

    assert_eq!(
        schedule,
        [
            Task::node(left, [], [(0, 0)]),
            Task::node(right, [], [(0, 1)]),
            Task::sum(1, 0, 0),
            Task::node(master, [(0, 0)], []),
        ]
    );

    assert_eq!(num_buffers, 2);
}

#[test]
fn test_multiple_adders() {
    let mut graph = AudioGraph::default();

    let master = graph.insert_node_id(0, [], [0]);
    let a = graph.insert_node_id(0, [0], []);
    let b = graph.insert_node_id(0, [0], []);
    let c = graph.insert_node_id(0, [0], []);

    assert!(graph.try_insert_edge((master, 0), (a, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((master, 0), (b, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((master, 0), (c, 0)).is_ok_and(id));

    let (num_buffers, schedule) = graph.compile([master]);

    // println!("{schedule:#?}");

    assert_eq!(
        schedule,
        [
            Task::node(a, [], [(0, 0)]),
            Task::node(c, [], [(0, 1)]),
            Task::sum(1, 0, 0),
            Task::node(b, [], [(0, 1)]),
            Task::sum(1, 0, 0),
            Task::node(master, [(0, 0)], []),
        ]
    );

    assert_eq!(num_buffers, 2);
}

#[test]
fn test_m_graph() {
    let mut graph = AudioGraph::default();

    let master1 = graph.insert_node_id(0, [], [0]);
    let master2 = graph.insert_node_id(0, [], [0]);
    let master3 = graph.insert_node_id(0, [], [0]);

    let n1 = graph.insert_node_id(0, [0], []);
    let n2 = graph.insert_node_id(0, [0], []);

    assert!(graph.try_insert_edge((master1, 0), (n1, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((master2, 0), (n1, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((master2, 0), (n2, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((master3, 0), (n2, 0)).is_ok_and(id));

    let (num_buffers, schedule) = graph.compile([master2, master1, master3]);

    // println!("{schedule:#?}");

    assert_eq!(
        schedule,
        [
            Task::node(n2, [], [(0, 0)]),
            Task::node(n1, [], [(0, 1)]),
            Task::sum(1, 0, 2),
            Task::node(master2, [(0, 2)], []),
            Task::node(master1, [(0, 1)], []),
            Task::node(master3, [(0, 0)], []),
        ],
    );

    assert_eq!(num_buffers, 3);
}

#[test]
fn mutiple_input_ports() {
    let mut graph = AudioGraph::default();

    let master = graph.insert_node_id(0, [], [0]);
    let node = graph.insert_node_id(0, [0], [0, 1, 2, 3]);
    let g = graph.insert_node_id(0, [0], []);

    assert!(graph.try_insert_edge((node, 0), (g, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((node, 1), (g, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((node, 2), (g, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((node, 3), (g, 0)).is_ok_and(id));
    assert!(graph.try_insert_edge((master, 0), (node, 0)).is_ok_and(id));

    let (num_buffers, schedule) = graph.compile([master]);

    // println!("{schedule:#?}");

    assert_eq!(
        schedule,
        [
            Task::node(g, [], [(0, 0)]),
            Task::node(node, [(0, 0), (1, 0), (2, 0), (3, 0)], [(0, 0)]),
            Task::node(master, [(0, 0)], []),
        ]
    );

    assert_eq!(num_buffers, 1);
}
