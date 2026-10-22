use argus_protocol::{Chip, Column, Tone};

use super::*;
use crate::layout::docks::Panel;

fn chip(label: &str) -> Chip {
    Chip { label: label.into(), tone: Tone::Neutral }
}

fn text(text: &str) -> Value {
    Value::Text { text: text.into() }
}

fn row(status: Option<&str>, cells: Vec<Value>) -> ExtTableRow {
    ExtTableRow { key: "k".into(), cells, status: status.map(chip) }
}

fn table(columns: &[&str], rows: Vec<ExtTableRow>) -> ExtTable {
    ExtTable {
        panel: Panel::extension("docker", "containers"),
        extension: "docker".into(),
        view: "containers".into(),
        title: "Containers".into(),
        page: false,
        columns: columns.iter().map(|title| Column { title: (*title).into(), numeric: false, optional: false }).collect(),
        rows,
        actions: Vec::new(),
        selected: None,
        default: None,
        console: None,
    }
}

#[test]
fn brackets_and_case_do_not_hide_a_cell_that_repeats_the_status() {
    assert!(echoes(&row(Some("exited (137)"), vec![]), Some(&text("exited 137"))));
    assert!(echoes(&row(Some("Running"), vec![]), Some(&Value::Chip { chip: chip(" running ") })));
}

#[test]
fn a_cell_that_says_more_than_the_status_is_not_an_echo() {
    assert!(!echoes(&row(Some("exited"), vec![]), Some(&text("exited 137"))));
    assert!(!echoes(&row(None, vec![]), Some(&text("exited"))));
    assert!(!echoes(&row(Some("exited"), vec![]), None));
    assert!(!echoes(&row(Some("exited"), vec![]), Some(&Value::Number { number: 1.0, unit: None })));
}

#[test]
fn a_column_every_cell_of_which_echoes_the_status_is_dropped_and_the_status_leads() {
    let rows = vec![
        row(Some("running"), vec![text("api"), text("Running")]),
        row(Some("exited (137)"), vec![text("minio"), text("exited 137")]),
    ];
    let shaped = shape(&table(&["Name", "State"], rows));
    assert!(shaped.status);
    let titles: Vec<&str> = shaped.columns.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Status", "Name"]);
    assert_eq!(shaped.rows[1], vec![Value::Chip { chip: chip("exited (137)") }, text("minio")]);
}

#[test]
fn a_column_with_one_cell_that_differs_is_kept_and_the_echoes_in_it_are_blanked() {
    let rows = vec![
        row(Some("running"), vec![text("api"), text("running")]),
        row(Some("exited"), vec![text("minio"), text("exited 137")]),
    ];
    let shaped = shape(&table(&["Name", "State"], rows));
    assert_eq!(shaped.columns.len(), 3);
    assert_eq!(shaped.rows[0][2], Value::Empty, "the repeat says nothing new");
    assert_eq!(shaped.rows[1][2], text("exited 137"), "the difference is the news");
}

#[test]
fn a_row_without_a_status_leads_with_a_blank_while_another_has_one() {
    let rows = vec![row(Some("running"), vec![text("a")]), row(None, vec![text("b")])];
    let shaped = shape(&table(&["Name"], rows));
    assert_eq!(shaped.rows[1][0], Value::Empty);
}

#[test]
fn a_table_with_no_status_anywhere_is_its_own_columns_and_cells() {
    let rows = vec![row(None, vec![text("a"), text("b")])];
    let shaped = shape(&table(&["Name", "Image"], rows));
    assert!(!shaped.status);
    assert_eq!(shaped.columns.len(), 2);
    assert_eq!(shaped.rows, vec![vec![text("a"), text("b")]]);
}

#[test]
fn a_table_with_no_rows_keeps_every_column() {
    let shaped = shape(&table(&["Name", "Image"], Vec::new()));
    assert_eq!(shaped.columns.len(), 2);
    assert!(shaped.rows.is_empty());
}
