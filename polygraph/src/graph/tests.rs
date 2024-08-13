use super::*;
use collections::HashSet;

// These tests aren't ideal, I have to print the compiled schedule and review it first,
// then insert it as the rhs of the assert directive if it's correct. This is inconvenient,
// since there are usually many correct schedules, and any update to the graph's traversal order
// will break these tests, in spite of, theoretically, still creating correct schedules.

#[test]
#[should_panic]
fn insert_basic_cycle() {
    let mut graph = AudioGraph::default();

    let node1 = graph.insert_node(1, 1);
    let node2 = graph.insert_node(1, 1);

    assert!(graph.try_insert_edge((node1, 0), (node2, 0)).unwrap());
    graph.try_insert_edge((node2, 0), (node1, 0)).unwrap();
}

#[test]
#[should_panic]
fn insert_complex_cycle() {
    let mut graph = AudioGraph::default();

    let node1 = graph.insert_node(0, 1);
    let node2 = graph.insert_node(1, 1);
    let node3 = graph.insert_node(1, 1);
    let node4 = graph.insert_node(1, 0);

    assert!(graph.try_insert_edge((node1, 0), (node2, 0)).unwrap());
    assert!(graph.try_insert_edge((node2, 0), (node3, 0)).unwrap());
    assert!(graph.try_insert_edge((node3, 0), (node4, 0)).unwrap());
    graph.try_insert_edge((node4, 0), (node1, 0)).unwrap();
}

#[test]
fn insert_redundant_edge() {
    let mut graph = AudioGraph::default();

    let node1 = graph.insert_node(0, 1);
    let node2 = graph.insert_node(1, 0);

    assert!(graph.try_insert_edge((node1, 0), (node2, 0)).unwrap());
    assert!(graph.try_insert_edge((node1, 0), (node2, 0)).unwrap() == false);
}

#[test]
fn test_basic() {
    let mut graph = AudioGraph::default();

    let master = graph.insert_node(0, 1);
    let node = graph.insert_node(1, 0);

    assert!(graph.try_insert_edge((master, 0), (node, 0)).unwrap());

    let (num_buffers, schedule) = graph.compile(HashSet::from_iter([master]));

    assert_eq!(
        schedule,
        &[Task::node(node, [], [0]), Task::node(master, [0], []),]
    );

    assert_eq!(num_buffers, 1);
}

#[test]
fn test_chain() {
    let mut graph = AudioGraph::default();

    let master = graph.insert_node(0, 1);
    let node1 = graph.insert_node(1, 0);
    let node2 = graph.insert_node(1, 1);
    let node3 = graph.insert_node(1, 1);

    assert!(graph.try_insert_edge((master, 0), (node3, 0)).unwrap());
    assert!(graph.try_insert_edge((node3, 0), (node2, 0)).unwrap());
    assert!(graph.try_insert_edge((node2, 0), (node1, 0)).unwrap());

    let (num_buffers, schedule) = graph.compile(HashSet::from_iter([master]));

    assert_eq!(
        schedule,
        &[
            Task::node(node1, [], [0]),
            Task::node(node2, [0], [0]),
            Task::node(node3, [0], [0]),
            Task::node(master, [0], []),
        ]
    );

    assert_eq!(num_buffers, 1);
}

#[test]
fn test_mutiple_outputs() {
    let mut graph = AudioGraph::default();

    let o1 = graph.insert_node(0, 1);
    let o2 = graph.insert_node(0, 1);
    let o3 = graph.insert_node(0, 1);
    let o4 = graph.insert_node(0, 1);

    let n1 = graph.insert_node(1, 0);
    let n2 = graph.insert_node(1, 0);
    let n3 = graph.insert_node(1, 0);
    let n4 = graph.insert_node(1, 0);

    assert!(graph.try_insert_edge((o1, 0), (n1, 0)).unwrap());
    assert!(graph.try_insert_edge((o2, 0), (n2, 0)).unwrap());
    assert!(graph.try_insert_edge((o3, 0), (n3, 0)).unwrap());
    assert!(graph.try_insert_edge((o4, 0), (n4, 0)).unwrap());

    let (num_buffers, schedule) = graph.compile(HashSet::from_iter([o1, o2, o3, o4]));

    assert_eq!(
        schedule,
        [
            Task::node(n2, [], [0]),
            Task::node(o2, [0], []),
            Task::node(n1, [], [0]),
            Task::node(o1, [0], []),
            Task::node(n4, [], [0]),
            Task::node(o4, [0], []),
            Task::node(n3, [], [0]),
            Task::node(o3, [0], []),
        ]
    );

    assert_eq!(num_buffers, 1);
}

#[test]
fn test_adder() {
    let mut graph = AudioGraph::default();

    let master = graph.insert_node(0, 1);

    let left = graph.insert_node(1, 0);
    let right = graph.insert_node(1, 0);

    assert!(graph.try_insert_edge((master, 0), (left, 0)).unwrap());
    assert!(graph.try_insert_edge((master, 0), (right, 0)).unwrap());

    let (num_buffers, schedule) = graph.compile(HashSet::from_iter([master]));

    // println!("{schedule:#?}");

    assert_eq!(
        schedule,
        [
            Task::node(left, [], [0]),
            Task::node(right, [], [1]),
            Task::sum(1, 0, 0),
            Task::node(master, [0], []),
        ]
    );

    assert_eq!(num_buffers, 2);
}

#[test]
fn test_multiple_adders() {
    let mut graph = AudioGraph::default();

    let master = graph.insert_node(0, 1);
    let a = graph.insert_node(1, 0);
    let b = graph.insert_node(1, 0);
    let c = graph.insert_node(1, 0);

    assert!(graph.try_insert_edge((master, 0), (a, 0)).unwrap());
    assert!(graph.try_insert_edge((master, 0), (b, 0)).unwrap());
    assert!(graph.try_insert_edge((master, 0), (c, 0)).unwrap());

    let (num_buffers, schedule) = graph.compile(HashSet::from_iter([master]));

    // println!("{schedule:#?}");

    assert_eq!(
        schedule,
        [
            Task::node(a, [], [0]),
            Task::node(c, [], [1]),
            Task::sum(1, 0, 0),
            Task::node(b, [], [1]),
            Task::sum(1, 0, 0),
            Task::node(master, [0], []),
        ]
    );

    assert_eq!(num_buffers, 2);
}

#[test]
fn test_w_graph() {
    let mut graph = AudioGraph::default();

    let master1 = graph.insert_node(0, 1);
    let master2 = graph.insert_node(0, 1);
    let master3 = graph.insert_node(0, 1);

    let n1 = graph.insert_node(1, 0);
    let n2 = graph.insert_node(1, 0);

    assert!(graph.try_insert_edge((master1, 0), (n1, 0)).unwrap());
    assert!(graph.try_insert_edge((master2, 0), (n1, 0)).unwrap());
    assert!(graph.try_insert_edge((master2, 0), (n2, 0)).unwrap());
    assert!(graph.try_insert_edge((master3, 0), (n2, 0)).unwrap());

    let (num_buffers, schedule) = graph.compile(HashSet::from_iter([master2, master1, master3]));

    // println!("{schedule:#?}");

    assert_eq!(
        schedule,
        [
            Task::node(n2, [], [0]),
            Task::node(n1, [], [1]),
            Task::sum(1, 0, 2,),
            Task::node(master2, [2], []),
            Task::node(master1, [1], []),
            Task::node(master3, [0], []),
        ],
    );

    assert_eq!(num_buffers, 3);
}

#[test]
fn mutiple_input_ports() {
    let mut graph = AudioGraph::default();

    let master = graph.insert_node(0, 1);
    let node = graph.insert_node(1, 4);
    let generator = graph.insert_node(1, 0);

    assert!(graph.try_insert_edge((node, 0), (generator, 0)).unwrap());
    assert!(graph.try_insert_edge((node, 1), (generator, 0)).unwrap());
    assert!(graph.try_insert_edge((node, 2), (generator, 0)).unwrap());
    assert!(graph.try_insert_edge((node, 3), (generator, 0)).unwrap());
    assert!(graph.try_insert_edge((master, 0), (node, 0)).unwrap());

    let (num_buffers, schedule) = graph.compile(HashSet::from_iter([master]));

    assert_eq!(
        schedule,
        [
            Task::node(generator, [], [0]),
            Task::node(node, [0, 0, 0, 0], [0]),
            Task::node(master, [0], []),
        ]
    );

    assert_eq!(num_buffers, 1);
}
