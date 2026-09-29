//! Cursor's `agent` transcript: the JSONL its hooks name in
//! `transcript_path`, under `~/.cursor/projects/<slug>/agent-transcripts`.
//!
//! Each line is one message, a role and its content blocks, with no ids,
//! no times and no tool results: Cursor keeps what was asked and what was
//! done, not what came back. A tool call is therefore shown as done once
//! it is written. The person's text arrives wrapped in the tags Cursor
//! hands the model, and its reasoning as `[REDACTED]`, which says nothing.

use argus_protocol::transcript::{clip, MAX_TEXT_BYTES};
use argus_protocol::{Body, Entry, ToolState, Update};
use serde_json::Value;

use super::{input_text, summary};

const REDACTED: &str = "[REDACTED]";

pub(super) fn read(record: &Value, id: &str) -> Vec<Update> {
    let upsert = |part: usize, body: Body| {
        Update::Upsert(Entry {
            id: format!("{id}.{part}"),
            at: None,
            body,
        })
    };
    if record.get("type").and_then(Value::as_str) == Some("turn_ended") {
        return vec![upsert(0, Body::TurnEnd { millis: None })];
    }
    let Some(blocks) = record["message"]["content"].as_array() else {
        return Vec::new();
    };
    let user = record.get("role").and_then(Value::as_str) == Some("user");
    blocks
        .iter()
        .enumerate()
        .filter_map(|(part, block)| {
            let body = match block.get("type").and_then(Value::as_str)? {
                "text" => {
                    let raw = block.get("text").and_then(Value::as_str)?;
                    let text = if user { asked(raw) } else { said(raw) };
                    if text.is_empty() {
                        return None;
                    }
                    let text = clip(&text, MAX_TEXT_BYTES);
                    if user {
                        Body::Prompt { text }
                    } else {
                        Body::Reply { text }
                    }
                }
                "tool_use" => {
                    let input = block.get("input").unwrap_or(&Value::Null);
                    Body::ToolCall {
                        tool: block.get("name").and_then(Value::as_str).unwrap_or("tool").to_string(),
                        summary: summary(input),
                        input: input_text(input),
                        state: ToolState::Done,
                    }
                }
                _ => return None,
            };
            Some(upsert(part, body))
        })
        .collect()
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
