use super::*;
use crate::views::agent_view::{
    Column, Field, FormField, GraphEdge, GraphNode, MatrixCell, Series, Stat, Tab, Tone,
};

fn view(blocks: Vec<Block>) -> AgentView {
    AgentView { id: "ci".into(), title: "CI on main".into(), blocks, ..Default::default() }
}

fn text(t: &str) -> Value {
    Value::Text { text: t.into() }
}

fn md() -> Block {
    Block::Markdown { text: "x".into() }
}

fn act(id: &str) -> ViewAction {
    ViewAction { id: id.into(), label: id.into(), tone: Tone::Neutral, confirm: None }
}

fn table(rows: Vec<Vec<Value>>) -> Block {
    Block::Table {
        columns: vec![
            Column { title: "Run".into(), numeric: false, optional: false },
            Column { title: "Took".into(), numeric: true, optional: false },
        ],
        rows,
        sorted_by: None,
        total: None,
        row_keys: Vec::new(),
        select: RowSelect::None,
        actions: Vec::new(),
        details: Vec::new(),
    }
}

fn fields(value: Value) -> Block {
    Block::Fields { fields: vec![Field { label: "f".into(), value }] }
}

fn refused(blocks: Vec<Block>) -> String {
    view(blocks).checked().unwrap_err()
}

fn form(fields: Vec<FormField>, submit: ViewAction) -> Block {
    Block::Form { id: "f".into(), fields, submit }
}

fn number_field(name: &str, input: FormInput) -> FormField {
    FormField { name: name.into(), label: name.into(), input }
}

#[test]
fn a_row_with_the_wrong_number_of_cells_is_named_by_its_path() {
    let bad = view(vec![md(), table(vec![vec![text("4812")]])]);
    assert_eq!(bad.checked().unwrap_err(), "blocks[1].rows[0] has 1 cells, the table has 2 columns");
}

#[test]
fn a_table_past_its_rows_is_refused() {
    let row = || vec![text("a"), text("b")];
    assert!(view(vec![table((0..MAX_TABLE_ROWS).map(|_| row()).collect())]).checked().is_ok());
    let err = refused(vec![table((0..=MAX_TABLE_ROWS).map(|_| row()).collect())]);
    assert!(err.starts_with("blocks[0]: a table has at most 200 rows"), "{err}");
}

#[test]
fn blocks_nest_only_so_deep() {
    let section = |blocks| Block::Section { heading: "s".into(), blocks, opens: None };
    let deep = |n: usize| (0..n).fold(vec![md()], |inner, _| vec![section(inner)]);
    assert!(view(deep(MAX_BLOCK_DEPTH - 1)).checked().is_ok());
    let err = refused(deep(MAX_BLOCK_DEPTH));
    assert!(err.contains("nest at most 4 deep"), "{err}");
}

#[test]
fn a_refusal_inside_a_container_is_named_in_the_grammar_a_patch_reads() {
    let bad = Block::Progress { label: "p".into(), value: 1.0, max: 0.0, tone: Tone::Neutral, note: None };
    let grid = Block::Grid { min: Default::default(), items: vec![md(), bad.clone()] };
    assert!(refused(vec![grid]).starts_with("blocks[0].items[1]"));
    let columns = Block::Columns { columns: vec![vec![md()], vec![md(), bad.clone()]] };
    assert!(refused(vec![columns]).starts_with("blocks[0].columns[1][1]"));
    let tabs = Block::Tabs { tabs: vec![Tab { title: "t".into(), blocks: vec![bad.clone()] }] };
    assert!(refused(vec![tabs]).starts_with("blocks[0].tabs[0].blocks[0]"));
    let mut rows = table(vec![vec![text("a"), text("b")]]);
    if let Block::Table { details, row_keys, .. } = &mut rows {
        *details = vec![Some(vec![md(), bad])];
        *row_keys = vec!["k".into()];
    }
    assert!(refused(vec![rows]).starts_with("blocks[0].details[0][1]"));
}

#[test]
fn only_a_web_link_is_a_link() {
    let link = |url: &str| view(vec![fields(Value::Link { label: "d".into(), url: url.into() })]);
    assert!(link("https://example.com/doc").checked().is_ok());
    let err = link("file:///etc/passwd").checked().unwrap_err();
    assert_eq!(err, "blocks[0].fields[0].value: a link is an http or https URL");
    let mut subject = view(vec![]);
    subject.subject = Some("javascript:alert(1)".into());
    assert!(subject.checked().unwrap_err().starts_with("subject:"));
}

#[test]
fn a_view_needs_an_id_and_a_title() {
    let mut v = view(vec![]);
    v.id = "  ".into();
    assert_eq!(v.checked().unwrap_err(), "a view needs an id");
    let mut v = view(vec![]);
    v.title = String::new();
    assert_eq!(v.checked().unwrap_err(), "a view needs a title");
}

#[test]
fn a_number_near_the_top_of_f64_is_refused_wherever_it_is_drawn() {
    let huge = 1.7e308;
    let chart = Block::Chart {
        shape: Default::default(),
        x: vec!["a".into(), "b".into()],
        series: vec![Series { name: "s".into(), values: vec![Some(1.0), Some(huge)], tone: Tone::Neutral }],
        unit: None,
    };
    assert!(refused(vec![chart]).starts_with("blocks[0].series[0].values[1]:"));

    let stat = |trend| Block::Stats {
        stats: vec![Stat { label: "l".into(), value: "v".into(), delta: None, tone: Tone::Neutral, trend, opens: None }],
    };
    assert!(refused(vec![stat(vec![0.0, huge])]).starts_with("blocks[0].stats[0].trend[1]:"));

    let progress = |value, max| Block::Progress { label: "p".into(), value, max, tone: Tone::Neutral, note: None };
    assert!(refused(vec![progress(huge, 10.0)]).starts_with("blocks[0].value:"));
    assert!(refused(vec![progress(1.0, huge)]).starts_with("blocks[0].max:"));
    assert!(view(vec![progress(MAX_VIEW_NUMBER, MAX_VIEW_NUMBER)]).checked().is_ok());

    let cell = Value::Progress { value: huge, max: 1.0, tone: Tone::Neutral };
    assert!(refused(vec![fields(cell)]).starts_with("blocks[0].fields[0].value.value:"));

    let number = Value::Number { number: -huge, unit: None };
    assert!(refused(vec![fields(number)]).starts_with("blocks[0].fields[0].value:"));

    let matrix = Block::Matrix {
        rows: vec!["r".into()],
        columns: vec!["c".into()],
        cells: vec![vec![MatrixCell { tone: Tone::Neutral, weight: huge, label: None }]],
    };
    assert!(refused(vec![matrix]).starts_with("blocks[0].cells[0]:"));

    for (what, input) in [
        ("default", FormInput::Number { default: Some(huge), min: None, max: None }),
        ("min", FormInput::Number { default: None, min: Some(-huge), max: None }),
        ("max", FormInput::Number { default: None, min: None, max: Some(huge) }),
    ] {
        let err = refused(vec![form(vec![number_field("n", input)], act("go"))]);
        assert!(err.starts_with(&format!("blocks[0].fields[0].{what}:")), "{err}");
    }
}

#[test]
fn a_chart_point_that_is_not_finite_is_refused() {
    let chart = |v| Block::Chart {
        shape: Default::default(),
        x: vec!["a".into()],
        series: vec![Series { name: "s".into(), values: vec![Some(v)], tone: Tone::Neutral }],
        unit: None,
    };
    assert!(refused(vec![chart(f64::NAN)]).contains("values[0]"));
    assert!(refused(vec![chart(f64::INFINITY)]).contains("values[0]"));
}

#[test]
fn a_trend_and_a_graph_keep_their_counts() {
    let trend = |n| Block::Stats {
        stats: vec![Stat { label: "l".into(), value: "v".into(), delta: None, tone: Tone::Neutral, trend: vec![1.0; n], opens: None }],
    };
    assert!(view(vec![trend(MAX_TREND_POINTS)]).checked().is_ok());
    assert!(refused(vec![trend(MAX_TREND_POINTS + 1)]).starts_with("blocks[0].stats[0].trend:"));

    let graph = |edges: usize| Block::Graph {
        nodes: vec![GraphNode { id: "a".into(), label: "a".into(), tone: Tone::Neutral, opens: None }],
        edges: (0..edges).map(|_| GraphEdge { from: "a".into(), to: "a".into(), label: None }).collect(),
        direction: Default::default(),
    };
    assert!(view(vec![graph(MAX_GRAPH_EDGES)]).checked().is_ok());
    assert!(refused(vec![graph(MAX_GRAPH_EDGES + 1)]).starts_with("blocks[0].edges:"));
}

#[test]
fn an_id_or_label_that_reaches_a_terminal_has_no_control_characters() {
    let mut v = view(vec![]);
    v.id = "a\u{1b}[2J".into();
    assert_eq!(v.checked().unwrap_err(), "id: no control characters");

    let mut v = view(vec![]);
    v.actions = vec![act("go\n")];
    assert_eq!(v.checked().unwrap_err(), "actions[0].id: no control characters");

    let mut v = view(vec![]);
    v.actions = vec![ViewAction { label: "Go\r\nnow".into(), ..act("go") }];
    assert_eq!(v.checked().unwrap_err(), "actions[0].label: no control characters");

    let named = form(vec![number_field("na\tme", FormInput::Toggle { default: false })], act("go"));
    assert_eq!(refused(vec![named]), "blocks[0].fields[0].name: no control characters");

    let mut id = form(vec![number_field("n", FormInput::Toggle { default: false })], act("go"));
    if let Block::Form { id: form_id, .. } = &mut id {
        *form_id = "f\u{7}".into();
    }
    assert_eq!(refused(vec![id]), "blocks[0].id: no control characters");

    let mut keyed = table(vec![vec![text("a"), text("b")]]);
    if let Block::Table { row_keys, .. } = &mut keyed {
        *row_keys = vec!["k\nrm -rf".into()];
    }
    assert_eq!(refused(vec![keyed]), "blocks[0].row_keys[0]: no control characters");
}

#[test]
fn two_actions_with_one_id_are_refused_wherever_they_are() {
    let mut v = view(vec![Block::Card {
        title: None,
        chip: None,
        tone: Tone::Neutral,
        blocks: vec![],
        actions: vec![act("retry")],
        opens: None,
    }]);
    v.actions = vec![act("retry")];
    let err = v.checked().unwrap_err();
    assert!(err.starts_with("blocks[0].actions[0]:") && err.contains("\"retry\" is used twice"), "{err}");

    let row = Block::Row {
        align: Default::default(),
        items: vec![RowItem::Action { action: act("a") }, RowItem::Action { action: act("a") }],
    };
    assert!(refused(vec![row]).starts_with("blocks[0].items[1].action:"));

    let submit = form(vec![number_field("n", FormInput::Toggle { default: false })], act("a"));
    let mut tab = table(vec![vec![text("a"), text("b")]]);
    if let Block::Table { actions, row_keys, .. } = &mut tab {
        *actions = vec![act("a")];
        *row_keys = vec!["k".into()];
    }
    assert!(refused(vec![tab, submit]).starts_with("blocks[1].submit:"));
}

#[test]
fn a_kind_this_argus_does_not_know_is_refused_in_a_view_an_agent_writes() {
    let json = r#"{"id": "v", "title": "t", "blocks": [
        {"kind": "markdown", "text": "x"},
        {"kind": "fields", "fields": [{"label": "x", "value": {"kind": "sparkle"}}]},
        {"kind": "sparkle"}
    ]}"#;
    let view: AgentView = serde_json::from_str(json).unwrap();
    assert_eq!(view.clone().checked().unwrap_err(), "blocks[1].fields[0].value: no value kind by that name");
    let mut view = view;
    view.blocks.remove(1);
    assert_eq!(view.checked().unwrap_err(), "blocks[1]: no block kind by that name");
}

#[test]
fn a_path_that_leaves_the_checkout_is_refused() {
    let path = |p: &str| view(vec![fields(Value::Path { path: p.into(), line: Some(3) })]);
    assert!(path("src/lib.rs").checked().is_ok());
    for bad in ["../x", "/etc/passwd", "C:\\x", "a/.. /b", ""] {
        let err = path(bad).checked().unwrap_err();
        assert!(err.starts_with("blocks[0].fields[0].value: a path is inside the checkout"), "{bad}: {err}");
    }
}

#[test]
fn a_link_names_a_view_by_an_id_that_reads_back() {
    let link = |id: &str| view(vec![fields(Value::View { id: id.into(), label: None })]);
    assert!(link("deps").checked().is_ok(), "the view need not be saved yet");
    for bad in ["", " padded ", "a\u{1b}b", &"x".repeat(MAX_VIEW_ID_BYTES + 1)] {
        let err = link(bad).checked().unwrap_err();
        assert!(err.starts_with("blocks[0].fields[0].value.id"), "{bad:?}: {err}");
    }
}

#[test]
fn every_element_that_opens_a_view_is_held_to_the_same_rule() {
    let bad = Some(" x".to_string());
    let section = Block::Section { heading: "s".into(), blocks: vec![], opens: bad.clone() };
    assert!(refused(vec![section]).starts_with("blocks[0].opens"));
    let item = |opens| Block::List {
        items: vec![crate::Item { who: None, at: None, title: None, body: None, chip: None, opens }],
    };
    assert!(view(vec![item(Some("deps".into()))]).checked().is_ok());
    assert_eq!(refused(vec![item(bad)]), "blocks[0].items[0].opens: a saved view's id is not empty and has no space around it");
}

/// Each place a block holds a link, a value or an action of its own, with a
/// fault there and the refusal that names it. The paths are what an agent
/// is told to fix, so they hold byte for byte however the walk is written.
#[test]
fn every_leaf_is_refused_by_the_path_that_names_it() {
    const LINK: &str = "a saved view's id is not empty and has no space around it";
    const VALUE: &str = "a link is an http or https URL";
    const ACTION: &str = "an action needs a label";
    let link = serde_json::json!(" x");
    let value = serde_json::json!({ "kind": "link", "label": "l", "url": "file:///x" });
    let view_value = serde_json::json!({ "kind": "view", "id": " x" });
    let action = serde_json::json!({ "id": "a", "label": "" });
    let good = serde_json::json!({ "id": "ok", "label": "Ok" });
    let toggle = serde_json::json!([{ "name": "n", "label": "n", "input": { "kind": "toggle" } }]);
    let table = |cell: &serde_json::Value, actions: serde_json::Value, details: serde_json::Value| {
        serde_json::json!({ "kind": "table",
            "columns": [{ "title": "a" }, { "title": "b" }],
            "rows": [[{ "kind": "empty" }, cell]], "row_keys": ["k"],
            "actions": actions, "details": details })
    };
    let empty = serde_json::json!({ "kind": "empty" });
    let cases = [
        (serde_json::json!({ "kind": "embed", "view": "" }), format!("blocks[0].view: {LINK}")),
        (serde_json::json!({ "kind": "section", "heading": "s", "blocks": [], "opens": link }), format!("blocks[0].opens: {LINK}")),
        (serde_json::json!({ "kind": "details", "summary": "s", "blocks": [], "opens": link }), format!("blocks[0].opens: {LINK}")),
        (serde_json::json!({ "kind": "card", "blocks": [], "opens": link }), format!("blocks[0].opens: {LINK}")),
        (serde_json::json!({ "kind": "card", "blocks": [], "actions": [action] }), format!("blocks[0].actions[0]: {ACTION}")),
        (table(&empty, serde_json::json!([action]), serde_json::json!([])), format!("blocks[0].actions[0]: {ACTION}")),
        (serde_json::json!({ "kind": "fields", "fields": [{ "label": "f", "value": value }] }), format!("blocks[0].fields[0].value: {VALUE}")),
        (table(&value, serde_json::json!([]), serde_json::json!([])), format!("blocks[0].rows[0][1]: {VALUE}")),
        (serde_json::json!({ "kind": "list", "items": [{ "opens": link }] }), format!("blocks[0].items[0].opens: {LINK}")),
        (serde_json::json!({ "kind": "checklist", "items": [{ "label": "c", "done": false, "opens": link }] }), format!("blocks[0].items[0].opens: {LINK}")),
        (serde_json::json!({ "kind": "timeline", "events": [{ "title": "e", "opens": link }] }), format!("blocks[0].events[0].opens: {LINK}")),
        (serde_json::json!({ "kind": "stats", "stats": [{ "label": "s", "value": "1", "opens": link }] }), format!("blocks[0].stats[0].opens: {LINK}")),
        (serde_json::json!({ "kind": "graph", "nodes": [{ "id": "n", "label": "n", "opens": link }], "edges": [] }), format!("blocks[0].nodes[0].opens: {LINK}")),
        (
            serde_json::json!({ "kind": "tree", "nodes": [{ "label": "a", "children": [{ "label": "b" }, { "label": "c", "opens": link }] }] }),
            format!("blocks[0].nodes[0].children[1].opens: {LINK}"),
        ),
        (
            serde_json::json!({ "kind": "tree", "nodes": [{ "label": "a", "children": [{ "label": "b" }, { "label": "c", "value": value }] }] }),
            format!("blocks[0].nodes[0].children[1].value: {VALUE}"),
        ),
        (
            serde_json::json!({ "kind": "board", "columns": [{ "title": "c", "cards": [{ "title": "a" }, { "title": "b", "opens": link }] }] }),
            format!("blocks[0].columns[0].cards[1].opens: {LINK}"),
        ),
        (
            serde_json::json!({ "kind": "board", "columns": [{ "title": "c", "cards": [{ "title": "a" }, { "title": "b", "value": value }] }] }),
            format!("blocks[0].columns[0].cards[1].value: {VALUE}"),
        ),
        (
            serde_json::json!({ "kind": "row", "items": [{ "kind": "action", "action": good }, { "kind": "value", "value": value }] }),
            format!("blocks[0].items[1].value: {VALUE}"),
        ),
        (serde_json::json!({ "kind": "row", "items": [{ "kind": "action", "action": action }] }), format!("blocks[0].items[0].action: {ACTION}")),
        (serde_json::json!({ "kind": "form", "id": "f", "fields": toggle, "submit": action }), format!("blocks[0].submit: {ACTION}")),
        (
            table(&empty, serde_json::json!([]), serde_json::json!([[{ "kind": "section", "heading": "s", "blocks": [], "opens": link }]])),
            format!("blocks[0].details[0][0].opens: {LINK}"),
        ),
        (serde_json::json!({ "kind": "fields", "fields": [{ "label": "f", "value": view_value }] }), format!("blocks[0].fields[0].value.id: {LINK}")),
        (table(&view_value, serde_json::json!([]), serde_json::json!([])), format!("blocks[0].rows[0][1].id: {LINK}")),
    ];
    for (block, refusal) in cases {
        let block: Block = serde_json::from_value(block).unwrap();
        assert_eq!(refused(vec![block.clone()]), refusal, "{block:?}");
    }

    let mut header = view(vec![]);
    header.actions = vec![act("ok"), ViewAction { label: String::new(), ..act("a") }];
    assert_eq!(header.checked().unwrap_err(), format!("actions[1]: {ACTION}"));
}

#[test]
fn a_view_that_serialises_past_its_bound_is_refused() {
    assert!(view(vec![Block::Divider; 10]).checked().is_ok());
    let err = refused(vec![Block::Divider; 15_000]);
    assert_eq!(err, format!("a view is at most {MAX_VIEW_BYTES} bytes"));
}

#[test]
fn a_view_cannot_embed_itself_even_deep_inside() {
    let embed = |id: &str| Block::Embed { view: id.into() };
    assert!(view(vec![embed("other")]).checked().is_ok());
    let nested = Block::Section { heading: "s".into(), blocks: vec![embed("ci")], opens: None };
    assert_eq!(refused(vec![nested]), "blocks[0].blocks[0]: a view cannot embed itself");
    assert_eq!(refused(vec![embed("")]), "blocks[0].view: a saved view's id is not empty and has no space around it");
}
