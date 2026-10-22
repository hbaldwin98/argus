//! What a screen reader is told of a graph: how big it is, then its first few links (GRAPH-4).

use std::collections::HashMap;

use argus_protocol::{GraphEdge, GraphNode};

/// How many links are named before "and more".
const NAMED: usize = 6;

/// `Graph of 4 nodes and 5 edges: A to B, B to C, and more`. A link to an id no node has is named
/// by the id.
pub fn describe(nodes: &[GraphNode], edges: &[GraphEdge]) -> String {
    let name: HashMap<&str, &str> = nodes.iter().map(|n| (n.id.as_str(), n.label.as_str())).collect();
    let said = |id: &str| name.get(id).map_or_else(|| id.to_owned(), |label| (*label).to_owned());
    let links = edges.iter().take(NAMED).map(|e| format!("{} to {}", said(&e.from), said(&e.to))).collect::<Vec<_>>().join(", ");
    let more = if edges.len() > NAMED { ", and more" } else { "" };
    let tail = if links.is_empty() { String::new() } else { format!(": {links}{more}") };
    format!("Graph of {} nodes and {} edges{tail}", nodes.len(), edges.len())
}

/// `Graph · 4 nodes`, which the figure opened larger is titled.
pub fn title(nodes: usize) -> String {
    format!("Graph \u{b7} {nodes} node{}", if nodes == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use argus_protocol::Tone;

    use super::*;

    fn node(id: &str, label: &str) -> GraphNode {
        GraphNode { id: id.into(), label: label.into(), tone: Tone::Neutral, opens: None }
    }

    fn edge(from: &str, to: &str) -> GraphEdge {
        GraphEdge { from: from.into(), to: to.into(), label: None }
    }

    #[test]
    fn the_size_comes_first_then_the_links_by_the_nodes_labels() {
        let said = describe(&[node("a", "Alpha"), node("b", "Beta")], &[edge("a", "b")]);
        assert_eq!(said, "Graph of 2 nodes and 1 edges: Alpha to Beta");
    }

    #[test]
    fn a_link_to_an_unknown_id_is_named_by_the_id() {
        assert_eq!(describe(&[node("a", "Alpha")], &[edge("a", "zed")]), "Graph of 1 nodes and 1 edges: Alpha to zed");
    }

    #[test]
    fn only_six_links_are_named_and_the_rest_are_and_more() {
        let edges: Vec<GraphEdge> = (0..7).map(|i| edge("a", &format!("n{i}"))).collect();
        let said = describe(&[node("a", "A")], &edges);
        assert!(said.ends_with("A to n5, and more"), "{said}");
        assert!(!said.contains("n6"));
        let six = describe(&[node("a", "A")], &edges[..6]);
        assert!(!six.contains("and more"));
    }

    #[test]
    fn no_edges_name_none() {
        assert_eq!(describe(&[node("a", "A")], &[]), "Graph of 1 nodes and 0 edges");
    }

    #[test]
    fn the_title_counts_the_nodes() {
        assert_eq!(title(1), "Graph \u{b7} 1 node");
        assert_eq!(title(3), "Graph \u{b7} 3 nodes");
    }
}
