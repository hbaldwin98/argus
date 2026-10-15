//! Cursor's `agent` transcript: the JSONL under
//! `~/.cursor/projects/<checkout>/agent-transcripts`.
//!
//! Each line is one message, a role and its content blocks, with no ids,
//! no times and no tool results: Cursor keeps what was asked and what was
//! done, not what came back, so no call is ever known to have failed. The
//! person's text arrives wrapped in the tags Cursor hands the model, and
//! its reasoning as `[REDACTED]`, which says nothing.

use serde_json::Value;

use super::{text, Reading};

const REDACTED: &str = "[REDACTED]";

pub(super) fn read(record: &Value, reading: &mut Reading) {
    let Some(blocks) = record["message"]["content"].as_array() else {
        return;
    };
    let user = text(record, "role") == Some("user");
    for block in blocks {
        match text(block, "type") {
            Some("text") => {
                let raw = text(block, "text").unwrap_or_default();
                if user {
                    reading.user(&asked(raw), None);
                } else {
                    reading.agent(&said(raw), None);
                }
            }
            Some("tool_use") => reading.tool(
                None,
                text(block, "name").unwrap_or("tool"),
                block.get("input").unwrap_or(&Value::Null),
                None,
                false,
            ),
            _ => {}
        }
    }
}

/// What the person asked: the `<user_query>` Cursor wraps it in, without
/// the timestamp it adds beside it.
fn asked(raw: &str) -> String {
    let query = raw
        .split_once("<user_query>")
        .and_then(|(_, rest)| rest.split_once("</user_query>"))
        .map_or(raw, |(query, _)| query);
    query.trim().to_string()
}

/// What the agent said, without the placeholders its hidden reasoning
/// leaves.
fn said(raw: &str) -> String {
    raw.split("\n\n")
        .filter(|paragraph| paragraph.trim() != REDACTED)
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string()
}
