use super::*;
use crate::agent_view::{
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
    let section = |blocks| Block::Section { heading: "s".into(), blocks };
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
        stats: vec![Stat { label: "l".into(), value: "v".into(), delta: None, tone: Tone::Neutral, trend }],
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
        stats: vec![Stat { label: "l".into(), value: "v".into(), delta: None, tone: Tone::Neutral, trend: vec![1.0; n] }],
    };
    assert!(view(vec![trend(MAX_TREND_POINTS)]).checked().is_ok());
    assert!(refused(vec![trend(MAX_TREND_POINTS + 1)]).starts_with("blocks[0].stats[0].trend:"));

    let graph = |edges: usize| Block::Graph {
        nodes: vec![GraphNode { id: "a".into(), label: "a".into(), tone: Tone::Neutral }],
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
