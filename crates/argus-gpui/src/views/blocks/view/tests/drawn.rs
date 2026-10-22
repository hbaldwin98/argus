//! The blocks that are drawn: a chart, a spark, a sequence diagram and a graph. What a reader is told
//! of each, what hovering reads, and what a press asks Rust for (CHART-n, DIAGRAM-n, GRAPH-n).

use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, FlowLine, PageDiagram, PageParticipant};
use gpui_kit::TestAppContext;
use serde_json::{json, Value};

use super::*;

fn agent() -> ViewCtx {
    ViewCtx { can_press: true, ..ViewCtx::agent(7, "v") }
}

fn bars() -> Value {
    json!([{"kind": "chart", "shape": "bar", "unit": "tokens", "x": ["start", "plan"], "series": [
        {"name": "injected", "values": [10600, 9000]}, {"name": "brief", "values": [3100, null], "tone": "good"}]}])
}

// CHART-8, CHART-6
#[gpui_kit::test]
fn a_chart_says_what_it_shows_and_each_bar_reads_its_value(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, bars());
    assert_eq!(label(cx, &rig, "7/v.0-chart").as_deref(), Some("bar chart of injected, brief over 2 points, from 3.1K to 10.6K tokens"));
    assert_eq!(label(cx, &rig, "7/v.0-chart-s0-bar0").as_deref(), Some("start: 10,600 tokens"));
    assert_eq!(label(cx, &rig, "7/v.0-chart-s1-bar0").as_deref(), Some("start: 3,100 tokens"));
    assert!(!exists(cx, &rig, "7/v.0-chart-s1-bar1"), "a gap draws no bar");
}

// CHART-7
#[gpui_kit::test]
fn a_legend_is_drawn_only_when_there_is_more_than_one_series(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, bars());
    assert!(exists(cx, &rig, "7/v.0-chart-legend"));
    let one = open(cx, 700.0, json!([{"kind": "chart", "shape": "line", "x": ["a", "b"], "series": [{"name": "only", "values": [1, 2]}]}]));
    assert!(!exists(cx, &one, "7/v.0-chart-legend"));
}

// CHART-5, CHART-6
#[gpui_kit::test]
fn a_line_chart_draws_a_dot_for_each_value_present_and_none_for_a_gap(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, json!([{"kind": "chart", "shape": "line", "x": ["a", "b", "c"], "series": [{"name": "s", "values": [1, null, 3]}]}]));
    assert_eq!(label(cx, &rig, "7/v.0-chart-s0-dot0").as_deref(), Some("a: 1"));
    // Dots are numbered among those drawn, so the second is the third value.
    assert_eq!(label(cx, &rig, "7/v.0-chart-s0-dot1").as_deref(), Some("c: 3"));
    assert!(!exists(cx, &rig, "7/v.0-chart-s0-dot2"), "the gap has no dot");
}

#[gpui_kit::test]
fn a_chart_is_no_wider_than_720_and_keeps_its_proportions(cx: &mut TestAppContext) {
    let rig = open(cx, 1100.0, bars());
    let area = bounds(cx, &rig, "7/v.0-chart-s0-bar0").unwrap();
    let whole = bounds(cx, &rig, "7/v.0-chart").unwrap();
    assert!(f32::from(whole.size.width) >= 1000.0, "the box is as wide as its panel");
    // The first bar's band is a sixth of a 720-wide plot, not of the panel.
    let first = f32::from(area.origin.x - whole.origin.x);
    assert!(first < 130.0, "bars sit inside the chart's 720, got {first}");
}

// CHART-1
#[gpui_kit::test]
fn a_spark_chart_and_a_stats_trend_each_say_where_the_trend_went(cx: &mut TestAppContext) {
    let rig = open(
        cx,
        700.0,
        json!([
            {"kind": "chart", "shape": "spark", "x": [], "series": [{"name": "t", "values": [2, 9, null, 4]}]},
            {"kind": "stats", "stats": [{"label": "Brief", "value": "148", "trend": [1, 2, 3]}, {"label": "Tests", "value": "5"}]}
        ]),
    );
    assert_eq!(label(cx, &rig, "7/v.0-chart-spark").as_deref(), Some("trend over 3 points, from 2 to 9"));
    assert_eq!(label(cx, &rig, "7/v.1-stat0-spark").as_deref(), Some("trend over 3 points, from 1 to 3"));
    assert!(!exists(cx, &rig, "7/v.1-stat1-spark"), "a stat with no trend has no spark");
}

pub(super) fn flow() -> PageDiagram {
    let who = |id: &str| PageParticipant { id: id.into(), label: id.into(), actor: false };
    PageDiagram {
        source: "sequenceDiagram\na->>b: hi".into(),
        participants: vec![who("a"), who("b")],
        rows: vec![
            FlowLine::Step { number: 1, from: "a".into(), to: Some("b".into()), reply: false, text: "hi".into() },
            FlowLine::Step { number: 2, from: "b".into(), to: Some("a".into()), reply: true, text: "hello".into() },
        ],
    }
}

pub(super) fn with_diagrams(diagrams: Vec<PageDiagram>) -> ViewCtx {
    ViewCtx { diagrams: Rc::new(diagrams), ..agent() }
}

// DIAGRAM-5, DIAGRAM-2
#[gpui_kit::test]
fn a_diagram_the_reading_holds_is_drawn_and_names_its_steps_and_lifelines(cx: &mut TestAppContext) {
    let rig = open_as(cx, 700.0, json!([{"kind": "diagram", "source": "sequenceDiagram\na->>b: hi"}]), with_diagrams(vec![flow()]));
    assert_eq!(label(cx, &rig, "7/v.0-diagram-scroll").as_deref(), Some("Sequence diagram: 2 steps between a, b"));
}

// DIAGRAM-1
#[gpui_kit::test]
fn a_source_rust_could_not_read_is_shown_as_code(cx: &mut TestAppContext) {
    let rig = open_as(cx, 700.0, json!([{"kind": "diagram", "source": "graph TD; A-->B"}]), with_diagrams(vec![flow()]));
    assert!(exists(cx, &rig, "7/v.0-code"), "it is still what the agent meant, and readable");
    assert!(!exists(cx, &rig, "7/v.0-diagram-scroll"));
}

#[gpui_kit::test]
fn a_diagram_with_no_rows_says_nothing_to_draw_and_keeps_its_source(cx: &mut TestAppContext) {
    let empty = PageDiagram { rows: vec![], ..flow() };
    let rig = open_as(cx, 700.0, json!([{"kind": "diagram", "source": "sequenceDiagram\na->>b: hi"}]), with_diagrams(vec![empty]));
    assert!(exists(cx, &rig, "7/v.0-diagram-empty"));
    assert!(exists(cx, &rig, "7/v.0-diagram-source"));
}

#[gpui_kit::test]
fn the_source_folds_open_and_stays_as_the_person_left_it(cx: &mut TestAppContext) {
    let block = json!([{"kind": "diagram", "source": "sequenceDiagram\na->>b: hi"}]);
    let rig = open_as(cx, 700.0, block.clone(), with_diagrams(vec![flow()]));
    assert!(!exists(cx, &rig, "7/v.0-diagram-source-text"));
    click(cx, &rig, "7/v.0-diagram-source");
    assert!(exists(cx, &rig, "7/v.0-diagram-source-text"));
    show(cx, &rig, block, with_diagrams(vec![flow()]));
    assert!(exists(cx, &rig, "7/v.0-diagram-source-text"), "a republish must not close it under the reader");
}

pub(super) fn graph() -> Value {
    json!([{"kind": "graph", "nodes": [
        {"id": "a", "label": "start", "tone": "active", "opens": "release"},
        {"id": "b", "label": "a label that is clipped because it is very long indeed"},
        {"id": "c", "label": "done", "tone": "good"}],
        "edges": [{"from": "a", "to": "b", "label": "go"}, {"from": "b", "to": "c"}, {"from": "c", "to": "c"}, {"from": "c", "to": "nobody"}]}])
}

// GRAPH-4
#[gpui_kit::test]
fn a_graph_says_how_big_it_is_and_what_it_links_and_each_node_says_its_label(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, graph());
    assert_eq!(
        label(cx, &rig, "7/v.0-graph").as_deref(),
        Some("Graph of 3 nodes and 4 edges: start to a label that is clipped because it is very long indeed, a label that is clipped because it is very long indeed to done, done to done, done to nobody")
    );
    assert_eq!(label(cx, &rig, "7/v.0-graph-node1").as_deref(), Some("a label that is clipped because it is very long indeed"), "the clipped box still says the whole text");
}

// GRAPH-2
#[gpui_kit::test]
fn a_node_that_opens_a_view_is_pressed_where_it_is_and_listed_under_the_graph(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, graph());
    click(cx, &rig, "7/v.0-graph-node1");
    assert!(calls(&rig).is_empty(), "a node that opens nothing is not a link");
    // It is part of the drawing, so a press on it opens the drawing larger instead (FIGURE-1).
    cx.update_window(rig.handle, |_, window, cx| {
        assert!(crate::kit::overlay::host::Overlays::get(cx).close(crate::kit::overlay::kind::Overlay::Figure, window, cx));
    })
    .unwrap();
    click(cx, &rig, "7/v.0-graph-node0");
    assert_eq!(calls(&rig), [said(Call::OpenSavedView { id: "release".into(), from: None })]);
    assert!(exists(cx, &rig, "7/v.0-graph-link0-opens"), "the keyboard's way in");
    assert!(!exists(cx, &rig, "7/v.0-graph-link1-opens"));
}

// GRAPH-1
#[gpui_kit::test]
fn a_graph_with_no_nodes_says_nothing_to_draw(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, json!([{"kind": "graph", "nodes": [], "edges": []}]));
    assert!(exists(cx, &rig, "7/v.0-graph-frame"));
    assert!(!exists(cx, &rig, "7/v.0-graph"));
}

#[gpui_kit::test]
fn a_graph_is_scaled_down_to_a_narrow_panel_and_never_up_past_its_laid_out_size(cx: &mut TestAppContext) {
    let rig = open(cx, 1000.0, graph());
    let wide = bounds(cx, &rig, "7/v.0-graph-node0").unwrap().size.width;
    set_width(cx, &rig, 220.0);
    let narrow = bounds(cx, &rig, "7/v.0-graph-node0").unwrap().size.width;
    assert!(narrow < wide, "{narrow:?} against {wide:?}");
    set_width(cx, &rig, 2000.0);
    let wider = bounds(cx, &rig, "7/v.0-graph-node0").unwrap().size.width;
    assert_eq!(wider, wide, "more room does not make it bigger");
}
