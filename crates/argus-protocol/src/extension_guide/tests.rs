//! Holding the guide to the types: every JSON example and every literal the
//! walkthrough's Python sends is read through them, and every endpoint,
//! value kind, dialog, answer and event is shown at least once.

use std::collections::BTreeSet;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value as Json;

use super::*;
use crate::agent_view::{value_ok, Value};
use crate::extension_keys::{action_key_refusal, command_key_refusal};
use crate::extensions::{
    CommandScope, ExtAbout, ExtAnswer, ExtCommand, ExtDialog, ExtDialogKind, ExtDialogRaised, ExtEvent, ExtPane,
    ExtPaneOpened, ExtPlace, ExtView, Refusals,
};
use crate::hook::ExtEndpoint;

/// Each fenced block: its info string and its text.
fn fenced(text: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut open: Option<(String, String)> = None;
    for line in text.lines() {
        match (&mut open, line.strip_prefix("```")) {
            (None, Some(info)) => open = Some((info.trim().to_string(), String::new())),
            (Some(_), Some(_)) => blocks.push(open.take().unwrap()),
            (Some((_, body)), None) => {
                body.push_str(line);
                body.push('\n');
            }
            (None, None) => {}
        }
    }
    assert!(open.is_none(), "a block is never closed");
    blocks
}

/// Whether every field the example writes is one the type reads back the
/// same: serde ignores a field it does not know, so a renamed one would
/// otherwise read as left out.
fn within(example: &Json, read: &Json) -> bool {
    match (example, read) {
        (Json::Object(example), Json::Object(read)) => {
            example.iter().all(|(key, value)| read.get(key).is_some_and(|read| within(value, read)))
        }
        (Json::Array(example), Json::Array(read)) => {
            example.len() == read.len() && example.iter().zip(read).all(|(e, r)| within(e, r))
        }
        (Json::Number(example), Json::Number(read)) => example.as_f64() == read.as_f64(),
        _ => example == read,
    }
}

/// `json` read as `T`, every field of it accounted for.
fn read<T: DeserializeOwned + Serialize>(json: &Json, what: &str) -> T {
    let value: T = serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{what}: {e}\n{json}"));
    let again = serde_json::to_value(&value).unwrap();
    assert!(within(json, &again), "{what}: a field the type does not read\n{json}\nread back as {again}");
    value
}

fn value_kind(value: &Value) -> usize {
    match value {
        Value::Text { .. } => 0,
        Value::Number { .. } => 1,
        Value::Bool { .. } => 2,
        Value::Date { .. } => 3,
        Value::DateTime { .. } => 4,
        Value::Person { .. } => 5,
        Value::People { .. } => 6,
        Value::Chip { .. } => 7,
        Value::Chips { .. } => 8,
        Value::Link { .. } => 9,
        Value::Ticket { .. } => 10,
        Value::Empty => 11,
    }
}

fn dialog_kind(kind: &ExtDialogKind) -> usize {
    match kind {
        ExtDialogKind::Confirm { .. } => 0,
        ExtDialogKind::Prompt { .. } => 1,
        ExtDialogKind::Pick { .. } => 2,
        ExtDialogKind::Form { .. } => 3,
    }
}

fn event_kind(event: &ExtEvent) -> usize {
    match event {
        ExtEvent::Action { .. } => 0,
        ExtEvent::Command { .. } => 1,
        ExtEvent::Answer { answer: ExtAnswer::Yes, .. } => 2,
        ExtEvent::Answer { answer: ExtAnswer::Text(_), .. } => 3,
        ExtEvent::Answer { answer: ExtAnswer::Picked(_), .. } => 4,
        ExtEvent::Answer { answer: ExtAnswer::Form(_), .. } => 5,
        ExtEvent::Cancel { .. } => 6,
    }
}

fn checked_view(view: ExtView) {
    let view = view.checked().unwrap_or_else(|e| panic!("a view in the guide is refused: {e}"));
    for action in &view.actions {
        if let Some(key) = &action.key {
            assert_eq!(action_key_refusal(key), None, "the guide's action key {key} is refused");
        }
    }
}

fn checked_commands(commands: Vec<ExtCommand>) {
    for command in commands {
        let command = command.checked().unwrap_or_else(|e| panic!("a command in the guide is refused: {e}"));
        if let Some(key) = &command.key {
            assert_eq!(command_key_refusal(key), None, "the guide's key for {} is refused", command.name);
        }
    }
}

#[test]
fn every_placeholder_is_filled_from_its_constant() {
    let guide = guide();
    assert!(!guide.contains("{{"), "a placeholder no constant fills: {}", &guide[guide.find("{{").unwrap()..][..40]);
    assert!(guide.contains(&format!("| Rows in a view | {MAX_TABLE_ROWS} |")));
    assert!(guide.contains("`box`, `server`"));
    assert!(guide.contains("Ctrl Space"), "the leader is spelled, not left blank");
    for (name, _) in fills() {
        assert!(SOURCE.contains(&format!("{{{{{name}}}}}")), "{name} is filled but never used");
    }
}

#[test]
fn the_limits_table_writes_no_number_of_its_own() {
    let after = SOURCE.lines().skip_while(|line| *line != "## Limits").skip(1);
    let table: Vec<&str> = after.skip_while(|line| !line.starts_with('|')).take_while(|line| line.starts_with('|')).collect();
    assert!(table.len() > 10, "the limits table is where the test looks");
    for row in &table[2..] {
        assert!(row.contains("{{"), "a limit written as a number drifts from its constant: {row}");
    }
}

#[test]
fn every_json_example_reads_as_the_type_it_shows() {
    let guide = guide();
    let (mut values, mut dialogs, mut events) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let mut read_blocks = 0;
    for (info, body) in fenced(&guide) {
        let mut words = info.split_whitespace();
        if words.next() != Some("json") {
            continue;
        }
        let tag = words.next().unwrap_or_else(|| panic!("a json block names no type:\n{body}"));
        let json: Json = serde_json::from_str(&body).unwrap_or_else(|e| panic!("{tag}: not JSON: {e}\n{body}"));
        match tag {
            "view" => checked_view(read(&json, tag)),
            "values" => {
                for value in read::<Vec<Value>>(&json, tag) {
                    value_ok(&value, "value").unwrap();
                    values.insert(value_kind(&value));
                }
            }
            "commands" => checked_commands(read(&json, tag)),
            "refusals" => {
                read::<Refusals>(&json, tag);
            }
            "about" => {
                read::<ExtAbout>(&json, tag).checked().unwrap();
            }
            "dialog" => {
                dialogs.insert(dialog_kind(&read::<ExtDialog>(&json, tag).kind));
            }
            "raised" => {
                read::<ExtDialogRaised>(&json, tag);
            }
            "pane" => {
                read::<ExtPane>(&json, tag);
            }
            "opened" => {
                read::<ExtPaneOpened>(&json, tag);
            }
            "event" => {
                let event: ExtEvent = read(&json, tag);
                // What the daemon writes, so shown whole: no field left out.
                assert!(within(&serde_json::to_value(&event).unwrap(), &json), "an event shown short: {json}");
                events.insert(event_kind(&event));
            }
            other => panic!("a json block of a type the test does not know: {other}"),
        }
        read_blocks += 1;
    }
    assert!(read_blocks > 20, "only {read_blocks} blocks read");
    assert_eq!(values.len(), 12, "every value kind is shown");
    assert_eq!(dialogs.len(), 4, "every dialog kind is shown");
    assert_eq!(events.len(), 7, "every event, and every kind of answer, is shown");
}

/// A Python literal of the walkthrough's as JSON: its booleans and `None`
/// spelled as JSON's, and its trailing commas dropped.
fn python_to_json(literal: &str) -> String {
    let chars: Vec<char> = literal.chars().collect();
    let mut out = String::new();
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' {
                out.push(chars[i + 1]);
                i += 1;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c.is_ascii_alphabetic() {
            let word: String = chars[i..].iter().take_while(|c| c.is_ascii_alphanumeric()).collect();
            out.push_str(match word.as_str() {
                "True" => "true",
                "False" => "false",
                "None" => "null",
                other => panic!("a Python literal holds a name, {other}: {literal}"),
            });
            i += word.len();
            continue;
        } else {
            let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            let trailing = c == ',' && next.is_some_and(|c| *c == ']' || *c == '}');
            if !trailing {
                out.push(c);
            }
        }
        i += 1;
    }
    out
}

/// The text of a literal starting at `rest`, through its closing bracket,
/// read on across `lines` when it spans them.
fn literal<'a>(rest: &str, lines: &mut impl Iterator<Item = &'a str>) -> String {
    let mut text = rest.to_string();
    loop {
        let depth = python_to_json_depth(&text);
        if depth == 0 {
            return text;
        }
        text.push('\n');
        text.push_str(lines.next().expect("a literal that never closes"));
    }
}

fn python_to_json_depth(text: &str) -> i32 {
    let (mut depth, mut in_string, mut escaped) = (0, false, false);
    for c in text.chars() {
        match (in_string, c) {
            (true, _) if escaped => escaped = false,
            (true, '\\') => escaped = true,
            (true, '"') => in_string = false,
            (false, '"') => in_string = true,
            (false, '{' | '[') => depth += 1,
            (false, '}' | ']') => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// Every top-level `NAME = literal` and `NAME["field"] = literal` in the
/// walkthrough's Python, in order, as `NAME` or `NAME.field` and its JSON.
fn python_literals(guide: &str) -> Vec<(String, Json)> {
    let mut found = Vec::new();
    for (info, body) in fenced(guide) {
        if info != "python" {
            continue;
        }
        let mut lines = body.lines();
        while let Some(line) = lines.next() {
            let Some((target, rest)) = line.split_once(" = ") else { continue };
            if !rest.starts_with(['{', '[', '"']) || line.starts_with(char::is_whitespace) {
                continue;
            }
            let (name, field) = match target.split_once("[\"") {
                Some((name, field)) => (name, Some(field.trim_end_matches("\"]"))),
                None => (target, None),
            };
            if !name.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
                continue;
            }
            let text = literal(rest, &mut lines);
            let json = serde_json::from_str(&python_to_json(&text)).unwrap_or_else(|e| panic!("{target}: {e}\n{text}"));
            found.push((field.map_or(name.to_string(), |field| format!("{name}.{field}")), json));
        }
    }
    found
}

#[test]
fn what_the_walkthrough_sends_reads_as_the_types_and_keeps_its_keys() {
    let mut view: Option<Json> = None;
    let mut seen = BTreeSet::new();
    for (target, json) in python_literals(&guide()) {
        match target.as_str() {
            "VIEW" => view = Some(json),
            field if field.starts_with("VIEW.") => {
                let view = view.as_mut().expect("a field set on a view not yet written");
                view[&field["VIEW.".len()..]] = json;
            }
            "COMMANDS" => checked_commands(read(&json, "COMMANDS")),
            "ABOUT" => {
                read::<ExtAbout>(&json, "ABOUT").checked().unwrap();
            }
            "DIALOG" => {
                read::<ExtDialog>(&json, "DIALOG");
            }
            "PANE" => {
                read::<ExtPane>(&json, "PANE");
            }
            other => panic!("the walkthrough sends {other}, which the test does not read"),
        }
        if let Some(view) = &view {
            checked_view(read(view, &target));
        }
        seen.insert(target);
    }
    for expected in ["VIEW", "VIEW.actions", "VIEW.place", "VIEW.icon", "COMMANDS", "ABOUT", "DIALOG", "PANE"] {
        assert!(seen.contains(expected), "the walkthrough no longer sends {expected}");
    }
    let icon: String = serde_json::from_value(view.unwrap()["icon"].clone()).unwrap();
    assert!(EXT_ICONS.contains(&icon.as_str()), "the walkthrough's icon is one the window draws");
}

#[test]
fn every_endpoint_and_scope_is_in_the_guide() {
    let guide = guide();
    for endpoint in ExtEndpoint::ALL {
        let request = format!("`{} {}`", endpoint.method(), endpoint.suffix());
        assert!(guide.contains(&request), "{request} is not in the guide");
    }
    for scope in [CommandScope::Anywhere, CommandScope::Checkout, CommandScope::Pane] {
        let spelled = serde_json::to_string(&scope).unwrap();
        assert!(guide.contains(&spelled), "{spelled} is not in the guide");
    }
    assert!(guide.contains(&serde_json::to_string(&CommandScope::View("containers".into())).unwrap().replace(':', ": ")));
    for place in [ExtPlace::Panel, ExtPlace::Stage] {
        assert!(guide.contains(&serde_json::to_string(&place).unwrap()));
    }
}

#[test]
fn a_python_literal_reads_as_json() {
    let text = "{\"a\": True, \"b\": [None, \"x, ]\"],\n}";
    assert_eq!(python_to_json(text), "{\"a\": true, \"b\": [null, \"x, ]\"]\n}");
}
