use core::convert::identity;

use super::*;

#[test]
#[should_panic]
fn insert_basic_cycle() {
    let mut graph = AudioGraph::default();

    let node1 = graph.insert_node(1, 1);
    let node2 = graph.insert_node(1, 1);

    assert!(graph
        .try_insert_edge((node1, 0), (node2, 0))
        .is_ok_and(identity));
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

    assert!(graph
        .try_insert_edge((node1, 0), (node2, 0))
        .is_ok_and(identity));
    assert!(graph
        .try_insert_edge((node2, 0), (node3, 0))
        .is_ok_and(identity));
    assert!(graph
        .try_insert_edge((node3, 0), (node4, 0))
        .is_ok_and(identity));
    graph.try_insert_edge((node4, 0), (node1, 0)).unwrap();
}

#[test]
fn insert_redundant_edge() {
    let mut graph = AudioGraph::default();

    let node1 = graph.insert_node(0, 1);
    let node2 = graph.insert_node(1, 0);

    assert!(graph.try_insert_edge((node1, 0), (node2, 0)).unwrap() == true);
    assert!(graph.try_insert_edge((node1, 0), (node2, 0)).unwrap() == false);
}