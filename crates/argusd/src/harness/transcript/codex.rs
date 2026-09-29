//! Codex's transcript: the rollout JSONL its hooks name in
//! `transcript_path`.
//!
//! Each finished item of a turn is written as an `item_completed` event in
//! Codex's own vocabulary — what the person said, what the agent said, a
//! command it ran with its output, a file it changed — and that is what is
//! read. The same turn is also written as the raw model exchange
//! (`response_item`), which carries the instructions Codex injects and the
//! script its tools run inside, and is left out so nothing appears twice.

use argus_protocol::transcript::{clip, one_line, MAX_TEXT_BYTES, MAX_THINKING_BYTES, MAX_TOOL_INPUT_BYTES, MAX_TOOL_OUTPUT_BYTES};
use argus_protocol::{Body, Entry, ToolState, Update};
use serde_json::Value;

use super::{input_text, summary};

pub(super) fn read(record: &Value, id: &str) -> Vec<Update> {
    if record.get("type").and_then(Value::as_str) != Some("event_msg") {
        return Vec::new();
    }
    let at = record.get("timestamp").and_then(Value::as_str).map(str::to_string);
    let payload = &record["payload"];
    let upsert = |id: String, body: Body| {
        Update::Upsert(Entry {
            id,
            at: at.clone(),
            body,
        })
    };
    match payload.get("type").and_then(Value::as_str) {
        Some("item_completed") => item(&payload["item"], id, upsert),
        Some("task_complete") => vec![upsert(
            format!("{id}.0"),
            Body::TurnEnd {
                millis: payload.get("duration_ms").and_then(Value::as_u64),
            },
        )],
        Some("turn_aborted") => vec![upsert(
            format!("{id}.0"),
            Body::Notice {
                text: "Turn interrupted".to_string(),
            },
        )],
        _ => Vec::new(),
    }
}

/// One finished item. Commands and file changes arrive already done, so a
/// call and its result are written together.
fn item(item: &Value, line: &str, upsert: impl Fn(String, Body) -> Update) -> Vec<Update> {
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .map_or_else(|| format!("{line}.0"), str::to_string);
    let text = |key: &str| blocks(item.get(key).unwrap_or(&Value::Null));
    match item.get("type").and_then(Value::as_str) {
        Some("UserMessage") => {
            let said = text("content");
            if said.trim().is_empty() {
                return Vec::new();
            }
            vec![upsert(id, Body::Prompt { text: clip(said.trim(), MAX_TEXT_BYTES) })]
        }
        Some("AgentMessage") => {
            let said = text("content");
            if said.trim().is_empty() {
                return Vec::new();
            }
            vec![upsert(id, Body::Reply { text: clip(said.trim(), MAX_TEXT_BYTES) })]
        }
        Some("Reasoning") => {
            let thought = strings(item.get("summary_text").unwrap_or(&Value::Null));
            if thought.trim().is_empty() {
                return Vec::new();
            }
            vec![upsert(id, Body::Thinking { text: clip(thought.trim(), MAX_THINKING_BYTES) })]
        }
        Some("CommandExecution") => {
            let command = command_line(item);
            let failed = item.get("exit_code").and_then(Value::as_i64).is_some_and(|code| code != 0)
                || item.get("status").and_then(Value::as_str) == Some("failed");
            let shown = item
                .get("parsed_cmd")
                .and_then(|cmds| cmds.get(0))
                .and_then(|cmd| cmd.get("cmd"))
                .and_then(Value::as_str)
                .map_or_else(|| one_line(&command), one_line);
            let output = item
                .get("aggregated_output")
                .and_then(Value::as_str)
                .unwrap_or_default();
            finished(&id, "shell", shown, clip(&command, MAX_TOOL_INPUT_BYTES), output, failed, upsert)
        }
        Some("FileChange") => {
            let changes = item.get("changes").and_then(Value::as_object);
            let paths: Vec<&str> = changes.map_or_else(Vec::new, |c| c.keys().map(String::as_str).collect());
            let diff: String = changes
                .map(|c| {
                    c.iter()
                        .map(|(path, change)| {
                            let body = change.get("unified_diff").and_then(Value::as_str).unwrap_or_default();
                            format!("{path}\n{body}")
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            let failed = item.get("status").and_then(Value::as_str) == Some("failed");
            let names: Vec<&str> = paths.iter().map(|p| p.rsplit('/').next().unwrap_or(p)).collect();
            let call = Body::ToolCall {
                tool: "edit".to_string(),
                summary: one_line(&names.join(", ")),
                input: clip(&diff, MAX_TOOL_INPUT_BYTES),
                state: if failed { ToolState::Failed } else { ToolState::Done },
            };
            vec![upsert(id, call)]
        }
        // An item Argus has no reading for yet — a web search, an MCP call —
        // still shows that something was done.
        Some(other) if other.ends_with("Call") || other.ends_with("Search") => {
            let input = item.get("arguments").or_else(|| item.get("query")).unwrap_or(&Value::Null);
            let call = Body::ToolCall {
                tool: other.to_string(),
                summary: summary(input),
                input: input_text(input),
                state: ToolState::Done,
            };
            vec![upsert(id, call)]
        }
        _ => Vec::new(),
    }
}

/// A call that is over, and what it printed.
fn finished(
    id: &str,
    tool: &str,
    summary: String,
    input: String,
    output: &str,
    failed: bool,
    upsert: impl Fn(String, Body) -> Update,
) -> Vec<Update> {
    let state = if failed { ToolState::Failed } else { ToolState::Done };
    let mut updates = vec![upsert(
        id.to_string(),
        Body::ToolCall {
            tool: tool.to_string(),
            summary,
            input,
            state,
        },
    )];
    if !output.trim().is_empty() {
        updates.push(upsert(
            format!("{id}.result"),
            Body::ToolResult {
                call: id.to_string(),
                output: clip(output, MAX_TOOL_OUTPUT_BYTES),
                failed,
            },
        ));
    }
    updates
}

/// The command as the person would have typed it: the script a shell was
/// handed, rather than the shell itself.
fn command_line(item: &Value) -> String {
    match item.get("command") {
        Some(Value::Array(parts)) => {
            let parts: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
            match parts.as_slice() {
                [_, flag, script] if flag.starts_with('-') && flag.contains('c') => script.to_string(),
                _ => parts.join(" "),
            }
        }
        Some(Value::String(command)) => command.clone(),
        _ => String::new(),
    }
}

/// The text of Codex's content blocks, `Text` and `text` alike.
fn blocks(content: &Value) -> String {
    match content {
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::String(text) => text.clone(),
        _ => String::new(),
    }
}

fn strings(value: &Value) -> String {
    match value {
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.as_str().or_else(|| p.get("text").and_then(Value::as_str)))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::String(text) => text.clone(),
        _ => String::new(),
    }
}
