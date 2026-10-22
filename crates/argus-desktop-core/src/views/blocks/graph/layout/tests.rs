use argus_protocol::Tone;

use super::*;

/// A stand-in's centre sits half its size in from where it starts.
const SLACK: f32 = 4.0;

fn graph(names: &[&str], links: &[(&str, &str, Option<&str>)]) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let nodes = names.iter().map(|id| GraphNode { id: (*id).into(), label: format!("{id} a fairly long label here"), tone: Tone::Neutral, opens: None }).collect();
    let edges = links.iter().map(|(from, to, label)| GraphEdge { from: (*from).into(), to: (*to).into(), label: label.map(str::to_owned) }).collect();
    (nodes, edges)
}

/// The left and top of everything drawn: boxes and every point of every route.
fn margins(l: &Layout) -> (f32, f32) {
    let xs = l.nodes.iter().flat_map(|p| [p.x, p.x + p.w]).chain(l.routes.iter().flatten().flat_map(|r| r.points.iter().map(|p| p.0)));
    let ys = l.nodes.iter().flat_map(|p| [p.y, p.y + p.h]).chain(l.routes.iter().flatten().flat_map(|r| r.points.iter().map(|p| p.1)));
    (xs.fold(f32::INFINITY, f32::min), ys.fold(f32::INFINITY, f32::min))
}

// The loop a saved view drew a third of its width wide (reliab's change flow): back edges and long
// skips drifted every layer right. The page's own test of it, GRAPH-3.
#[test]
fn a_graph_with_long_skips_and_back_edges_starts_at_its_padding() {
    let (nodes, edges) = graph(
        &["edit", "approve", "inner", "route", "prop", "repro", "stop", "summary", "fix", "ack", "reviewer", "verdict", "push", "ci"],
        &[
            ("edit", "inner", None),
            ("approve", "push", None),
            ("inner", "route", Some("findings")),
            ("route", "prop", None),
            ("route", "repro", Some("agent kinds")),
            ("route", "stop", Some("human kinds")),
            ("stop", "ack", None),
            ("prop", "summary", Some("clean")),
            ("repro", "fix", None),
            ("fix", "inner", Some("re-run")),
            ("summary", "reviewer", None),
            ("reviewer", "verdict", None),
            ("verdict", "push", Some("pass")),
            ("push", "ci", None),
            ("ack", "inner", Some("acked")),
            ("edit", "reviewer", Some("protected change")),
        ],
    );
    for right in [false, true] {
        let (left, top) = margins(&layout(&nodes, &edges, right));
        assert!(left <= PAD + SLACK, "left margin {left} (right: {right})");
        assert!(top <= PAD + SLACK, "top margin {top} (right: {right})");
    }
}

/// The page's generator, so the same 300 graphs are tried.
fn random(seed: u64) -> impl FnMut() -> f64 {
    let mut s = seed;
    move || {
        s = (s * 1_103_515_245 + 12345) % 2_147_483_648;
        s as f64 / 2_147_483_648.0
    }
}

fn random_graph(seed: u64) -> (Vec<GraphNode>, Vec<GraphEdge>, bool) {
    let mut r = random(seed);
    let n = 1 + (r() * 12.0) as usize;
    let names: Vec<String> = (0..n).map(|i| format!("n{i}")).collect();
    let count = (r() * n as f64 * 2.0) as usize;
    let links: Vec<(String, String)> = (0..count).map(|_| (names[(r() * n as f64) as usize].clone(), names[(r() * n as f64) as usize].clone())).collect();
    let right = r() < 0.5;
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let edges: Vec<(&str, &str, Option<&str>)> = links.iter().map(|(a, b)| (a.as_str(), b.as_str(), None)).collect();
    let (nodes, edges) = graph(&refs, &edges);
    (nodes, edges, right)
}

#[test]
fn any_graph_starts_at_its_padding_along_both_axes() {
    for seed in 1..=300 {
        let (nodes, edges, right) = random_graph(seed);
        let (left, top) = margins(&layout(&nodes, &edges, right));
        assert!(left <= PAD + SLACK, "seed {seed}: left margin {left}");
        assert!(top <= PAD + SLACK, "seed {seed}: top margin {top}");
    }
}

#[test]
fn the_same_graph_is_laid_out_the_same_way_every_time() {
    for seed in 1..=60 {
        let (nodes, edges, right) = random_graph(seed);
        assert_eq!(layout(&nodes, &edges, right), layout(&nodes, &edges, right), "seed {seed}");
    }
}

#[test]
fn no_two_boxes_overlap_and_every_box_is_inside_the_drawing() {
    for seed in 1..=300 {
        let (nodes, edges, right) = random_graph(seed);
        let l = layout(&nodes, &edges, right);
        for (i, a) in l.nodes.iter().enumerate() {
            assert!(a.x >= 0.0 && a.y >= 0.0 && a.x + a.w <= l.width + 1e-3 && a.y + a.h <= l.height + 1e-3, "seed {seed}: box {i} outside");
            for b in &l.nodes[i + 1..] {
                let apart = a.x + a.w <= b.x + 1e-3 || b.x + b.w <= a.x + 1e-3 || a.y + a.h <= b.y + 1e-3 || b.y + b.h <= a.y + 1e-3;
                assert!(apart, "seed {seed}: boxes overlap");
            }
        }
    }
}

#[test]
fn every_edge_between_two_known_distinct_nodes_has_a_route_of_right_angles() {
    for seed in 1..=200 {
        let (nodes, edges, right) = random_graph(seed);
        let l = layout(&nodes, &edges, right);
        for (e, route) in edges.iter().zip(&l.routes) {
            let route = route.as_ref().unwrap_or_else(|| panic!("seed {seed}: no route for {}->{}", e.from, e.to));
            if route.is_loop {
                continue;
            }
            for pair in route.points.windows(2) {
                assert!((pair[0].0 - pair[1].0).abs() < 1e-3 || (pair[0].1 - pair[1].1).abs() < 1e-3, "seed {seed}: a slanted hop");
            }
        }
    }
}

#[test]
fn an_edge_to_an_unknown_id_has_no_route_and_does_not_disturb_the_rest() {
    let (nodes, edges) = graph(&["a", "b"], &[("a", "b", None), ("a", "ghost", None), ("ghost", "b", None)]);
    let l = layout(&nodes, &edges, false);
    assert!(l.routes[0].is_some() && l.routes[1].is_none() && l.routes[2].is_none());
    let (_, plain) = graph(&["a", "b"], &[("a", "b", None)]);
    assert_eq!(l.nodes, layout(&nodes, &plain, false).nodes);
}

#[test]
fn a_route_runs_from_the_senders_far_edge_to_the_receivers_near_one() {
    let (nodes, edges) = graph(&["a", "b"], &[("a", "b", None)]);
    let l = layout(&nodes, &edges, false);
    let route = l.routes[0].as_ref().unwrap();
    let (a, b) = (&l.nodes[0], &l.nodes[1]);
    assert!(l.nodes[1].y > l.nodes[0].y, "b is a layer under a");
    assert_eq!(route.points.first().unwrap().1, a.y + a.h);
    assert_eq!(route.points.last().unwrap().1, b.y);
}

#[test]
fn a_back_edge_is_drawn_from_its_own_from_to_its_own_to_though_the_layers_run_the_other_way() {
    let (nodes, edges) = graph(&["a", "b"], &[("a", "b", None), ("b", "a", None)]);
    let l = layout(&nodes, &edges, false);
    let back = l.routes[1].as_ref().unwrap();
    let (a, b) = (&l.nodes[0], &l.nodes[1]);
    assert_eq!(back.points.first().unwrap().1, b.y, "it leaves b at the edge nearest a");
    assert_eq!(back.points.last().unwrap().1, a.y + a.h);
}

#[test]
fn a_long_edge_bends_through_the_layers_it_skips() {
    let (nodes, edges) = graph(&["a", "b", "c"], &[("a", "b", None), ("b", "c", None), ("a", "c", None)]);
    let l = layout(&nodes, &edges, false);
    assert!(l.routes[2].as_ref().unwrap().points.len() > l.routes[0].as_ref().unwrap().points.len());
}

#[test]
fn right_runs_the_layers_across_and_down_runs_them_down() {
    let (nodes, edges) = graph(&["a", "b"], &[("a", "b", None)]);
    let down = layout(&nodes, &edges, false);
    let right = layout(&nodes, &edges, true);
    assert!(down.nodes[1].y > down.nodes[0].y && (down.nodes[1].x - down.nodes[0].x).abs() < 1e-3);
    assert!(right.nodes[1].x > right.nodes[0].x && (right.nodes[1].y - right.nodes[0].y).abs() < 1e-3);
}

#[test]
fn a_node_is_as_wide_as_its_label_clipped_and_thirty_four_high() {
    let (nodes, edges) = graph(&["a"], &[]);
    let l = layout(&nodes, &edges, false);
    // "a a fairly long label here" is 26 characters: 26 * 7 + 28.
    assert_eq!((l.nodes[0].w, l.nodes[0].h), (26.0 * 7.0 + 28.0, 34.0));
    let mut long = nodes;
    long[0].label = "x".repeat(100);
    assert_eq!(layout(&long, &[], false).nodes[0].w, 28.0 * 7.0 + 28.0);
    long[0].label = "x".into();
    assert_eq!(layout(&long, &[], false).nodes[0].w, 72.0, "never narrower than 72");
}

#[test]
fn layers_are_spaced_further_for_a_label_and_most_for_the_widest_when_running_right() {
    let (nodes, plain) = graph(&["a", "b"], &[("a", "b", None)]);
    let (_, down_label) = graph(&["a", "b"], &[("a", "b", Some("hi"))]);
    let (_, right_long) = graph(&["a", "b"], &[("a", "b", Some("a very much longer label"))]);
    let base = layout(&nodes, &plain, false);
    let labelled = layout(&nodes, &down_label, false);
    assert!((labelled.height - base.height - 14.0).abs() < 1e-3);
    let right_base = layout(&nodes, &plain, true);
    let right_wide = layout(&nodes, &right_long, true);
    assert!((right_wide.width - right_base.width - 24.0 * 7.0).abs() < 1e-3);
}

#[test]
fn a_node_joined_to_itself_loops_out_of_its_far_side_and_the_drawing_makes_room() {
    let (nodes, edges) = graph(&["a"], &[("a", "a", None)]);
    let l = layout(&nodes, &edges, false);
    let route = l.routes[0].as_ref().unwrap();
    assert!(route.is_loop);
    let p = &l.nodes[0];
    assert_eq!(route.points[0], (p.x + p.w, p.y + p.h / 2.0 - 6.0));
    assert_eq!(route.mid, (p.x + p.w + 18.0, p.y + p.h / 2.0));
    let (_, plain) = graph(&["a"], &[]);
    assert_eq!(l.width, layout(&nodes, &plain, false).width + 24.0);
}

#[test]
fn a_graph_of_no_nodes_is_just_its_padding() {
    let l = layout(&[], &[], false);
    assert_eq!((l.width, l.height), (32.0, 32.0));
    assert!(l.nodes.is_empty());
}

#[test]
fn ties_keep_the_order_the_agent_wrote() {
    let (nodes, edges) = graph(&["root", "x", "y", "z"], &[("root", "x", None), ("root", "y", None), ("root", "z", None)]);
    let l = layout(&nodes, &edges, false);
    assert!(l.nodes[1].x < l.nodes[2].x && l.nodes[2].x < l.nodes[3].x);
}
