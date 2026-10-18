//! How a message is written in a transcript: its kind, then its fields as
//! JSON with runs of equal items folded and highlight spans on one line,
//! and a message of a kind already written as only what changed since.
//!
//! The last is what keeps a deliberate change small: a field added to the
//! tree is a diff where a transcript first shows a tree, not at every tree
//! after it.

use std::collections::HashMap;
use std::fmt::Write as _;

use serde_json::Value;

/// The fewest a run of equal items must number to be written once.
const RUN: usize = 3;

/// The last message of each kind written, as it was written.
#[derive(Default)]
pub struct Render {
    last: HashMap<String, Value>,
}

impl Render {
    /// A request, always written whole: it is what the step is about.
    pub fn sent(&mut self, kind: &str, body: Value) -> String {
        entry("->", kind, &fold(body))
    }

    pub fn heard(&mut self, kind: &str, body: Value) -> String {
        let body = fold(body);
        let text = match self.last.get(kind) {
            Some(last) if *last == body => format!("<- {kind} (the same as the last)\n"),
            Some(last) => {
                let mut text = format!("<- {kind} (the last, changed)\n");
                changes(last, &body, &mut String::new(), &mut text);
                text
            }
            None => entry("<-", kind, &body),
        };
        self.last.insert(kind.to_owned(), body);
        text
    }
}

/// A message's variant and its fields, as serde's external tagging has
/// them: a unit variant is a bare string, any other a one-key object.
pub fn kind_and_body(msg: &impl serde::Serialize) -> (String, Value) {
    match serde_json::to_value(msg).expect("a wire message serializes") {
        Value::String(kind) => (kind, Value::Null),
        Value::Object(fields) if fields.len() == 1 => fields.into_iter().next().unwrap(),
        other => panic!("not an externally tagged message: {other}"),
    }
}

fn entry(arrow: &str, kind: &str, body: &Value) -> String {
    if body.is_null() {
        format!("{arrow} {kind}\n")
    } else {
        format!("{arrow} {kind} {}\n", serde_json::to_string_pretty(body).unwrap())
    }
}

/// Each path at which `now` differs from `was`, with what it holds now.
/// Objects are compared key by key and arrays index by index, so an item
/// added to a list is written alone; anything else that differs is
/// written whole at its path.
fn changes(was: &Value, now: &Value, path: &mut String, out: &mut String) {
    match (was, now) {
        (Value::Object(a), Value::Object(b)) => {
            let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            for key in keys {
                let len = path.len();
                let _ = write!(path, ".{key}");
                match (a.get(key), b.get(key)) {
                    (Some(x), Some(y)) => changes(x, y, path, out),
                    (_, Some(y)) => line(path, y, out),
                    (_, None) => { let _ = writeln!(out, "   {path} (gone)"); }
                }
                path.truncate(len);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            for i in 0..a.len().max(b.len()) {
                let len = path.len();
                let _ = write!(path, "[{i}]");
                match (a.get(i), b.get(i)) {
                    (Some(x), Some(y)) => changes(x, y, path, out),
                    (_, Some(y)) => line(path, y, out),
                    (_, None) => { let _ = writeln!(out, "   {path} (gone)"); }
                }
                path.truncate(len);
            }
        }
        _ if was == now => {}
        _ => line(path, now, out),
    }
}

fn line(path: &str, value: &Value, out: &mut String) {
    let path = if path.is_empty() { "(all)" } else { path };
    let _ = writeln!(out, "   {path} = {}", serde_json::to_string_pretty(value).unwrap());
}

/// Folds runs of equal items, so a pane's blank grid is a line rather than
/// two thousand, and writes each list of highlight spans as one line: a
/// highlighted file is otherwise eight lines of JSON per token.
fn fold(value: Value) -> Value {
    match value {
        Value::Array(items) => {
            let mut out = Vec::new();
            let mut items = items.into_iter().map(fold).peekable();
            while let Some(item) = items.next() {
                let mut count = 1;
                while items.peek() == Some(&item) {
                    items.next();
                    count += 1;
                }
                if count >= RUN {
                    out.push(serde_json::json!({ format!("{count} times"): item }));
                } else {
                    out.extend(std::iter::repeat_n(item, count));
                }
            }
            Value::Array(out)
        }
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(k, v)| match (k.as_str(), spans(&v)) {
                    ("spans", Some(line)) => (k, Value::String(line)),
                    _ => (k, fold(v)),
                })
                .collect(),
        ),
        other => other,
    }
}

/// `Keyword 0..3, Function 7..13`, for a list every item of which is a
/// span: a `kind`, a `start` and an `end` and nothing else.
fn spans(value: &Value) -> Option<String> {
    let items = value.as_array()?;
    let words: Option<Vec<String>> = items
        .iter()
        .map(|span| {
            let span = span.as_object().filter(|s| s.len() == 3)?;
            Some(format!("{} {}..{}", span.get("kind")?.as_str()?, span.get("start")?, span.get("end")?))
        })
        .collect();
    words.map(|words| words.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_kind_seen_before_is_written_as_what_changed() {
        let mut render = Render::default();
        render.heard("Tree", json!({ "panes": [], "name": "a" }));
        let again = render.heard("Tree", json!({ "panes": [1], "name": "a", "new": true }));
        assert_eq!(again, "<- Tree (the last, changed)\n   .new = true\n   .panes[0] = 1\n");
        assert_eq!(render.heard("Tree", json!({ "panes": [1], "name": "a", "new": true })), "<- Tree (the same as the last)\n");
    }

    #[test]
    fn spans_are_written_on_one_line_and_runs_are_folded() {
        let folded = fold(json!({
            "spans": [{ "kind": "Keyword", "start": 0, "end": 3 }],
            "cells": [0, 0, 0, 1],
        }));
        assert_eq!(folded, json!({ "spans": "Keyword 0..3", "cells": [{ "3 times": 0 }, 1] }));
    }
}
