//! Claude Code's transcript: the session JSONL under
//! `~/.claude/projects/<checkout>/<session>.jsonl`.
//!
//! Each record is one content block — a thinking block, a tool call, a text
//! reply — and a tool's result comes back as a user record naming the call.
//! Subagents write sidechain records, which their spawning call already
//! stands for. A message sent while the agent works arrives as a
//! `queued_command` attachment rather than a user record. Most other record
//! types are the CLI's own bookkeeping and say nothing a reader needs.

use serde_json::Value;

use super::{text, Reading};

pub(super) fn read(record: &Value, reading: &mut Reading) {
    if record.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return;
    }
    let at = text(record, "timestamp");
    reading.cwd(text(record, "cwd"));
    match text(record, "type") {
        Some("user") => user(record, at, reading),
        Some("assistant") => assistant(record, at, reading),
        Some("attachment") => attachment(record, at, reading),
        Some("summary") => reading.title(text(record, "summary").unwrap_or_default()),
        Some("custom-title") => reading.title(text(record, "customTitle").unwrap_or_default()),
        _ => reading.at(at),
    }
}

/// What the person typed, or a tool's result coming back: Claude files
/// both as the user's turn. `isMeta` marks what the CLI added on the
/// person's behalf — a skill's text, a caveat — which they never said.
fn user(record: &Value, at: Option<&str>, reading: &mut Reading) {
    let flag = |key: &str| record.get(key).and_then(Value::as_bool) == Some(true);
    if flag("isMeta") || flag("isCompactSummary") {
        return reading.at(at);
    }
    let content = &record["message"]["content"];
    if let Some(said) = content.as_str() {
        return typed(said, at, reading);
    }
    let Some(blocks) = content.as_array() else {
        return;
    };
    let mut said = Vec::new();
    for block in blocks {
        match text(block, "type") {
            Some("tool_result") => {
                if let Some(call) = text(block, "tool_use_id") {
                    let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
                    reading.finished(call, failed, at);
                }
            }
            Some("text") => said.extend(text(block, "text")),
            Some("image") => said.push("[image]"),
            _ => {}
        }
    }
    if !said.is_empty() {
        typed(&said.join("\n"), at, reading);
    }
}

/// A line the person's side of the conversation carries. Slash commands
/// and `!` shell commands arrive wrapped in the tags the CLI uses to tell
/// the model what happened, and are unwrapped back into what was typed.
/// Their output, the CLI's caveats and an interruption are nobody's turn.
fn typed(said: &str, at: Option<&str>, reading: &mut Reading) {
    let said = said.trim();
    if let Some(name) = tagged(said, "command-name") {
        let args = tagged(said, "command-args").unwrap_or_default();
        return reading.user(format!("{name} {args}").trim(), at);
    }
    if let Some(command) = tagged(said, "bash-input") {
        return reading.user(&format!("!{command}"), at);
    }
    let bookkeeping = [
        "<local-command-stdout>",
        "<local-command-stderr>",
        "<bash-stdout>",
        "<bash-stderr>",
        "<local-command-caveat>",
        "[Request interrupted",
    ];
    if bookkeeping.iter().any(|tag| said.starts_with(tag)) {
        return reading.at(at);
    }
    reading.user(said, at);
}

fn assistant(record: &Value, at: Option<&str>, reading: &mut Reading) {
    let Some(blocks) = record["message"]["content"].as_array() else {
        return reading.at(at);
    };
    for block in blocks {
        match text(block, "type") {
            Some("text") => reading.agent(text(block, "text").unwrap_or_default(), at),
            // Named by the harness's own id, because that is what its
            // result refers back to.
            Some("tool_use") => reading.tool(
                text(block, "id"),
                text(block, "name").unwrap_or("tool"),
                block.get("input").unwrap_or(&Value::Null),
                at,
                false,
            ),
            _ => reading.at(at),
        }
    }
}

/// A message the person sent while the agent was working, which the CLI
/// hands the model mid-turn rather than as a turn of its own. The other
/// attachments are context the CLI adds by itself.
fn attachment(record: &Value, at: Option<&str>, reading: &mut Reading) {
    let attachment = &record["attachment"];
    match (text(attachment, "type"), text(attachment, "prompt")) {
        (Some("queued_command"), Some(prompt)) => typed(prompt, at, reading),
        _ => reading.at(at),
    }
}

/// The text between `<tag>` and `</tag>`, trimmed, when `text` has both.
fn tagged(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = start + text[start..].find(&close)?;
    Some(text[start..end].trim().to_string())
}
