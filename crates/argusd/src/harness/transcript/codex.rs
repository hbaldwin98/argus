//! Codex's transcript: the rollout JSONL under
//! `~/.codex/sessions/<y>/<m>/<d>/rollout-<time>-<session>.jsonl`.
//!
//! Each finished item of a turn is written as an `item_completed` event in
//! Codex's own vocabulary — what the person said, what the agent said, a
//! command it ran, a file it changed — and that is what is read. The same
//! turn is also written as the raw model exchange (`response_item`), which
//! carries the instructions Codex injects and the script its tools run
//! inside, and is left out so nothing appears twice.

use serde_json::{json, Value};

use super::{text, Reading};

pub(super) fn read(record: &Value, reading: &mut Reading) {
    let at = text(record, "timestamp");
    let payload = &record["payload"];
    match (text(record, "type"), text(payload, "type")) {
        (Some("session_meta"), _) => {
            reading.cwd(text(payload, "cwd"));
            reading.at(at);
        }
        (Some("event_msg"), Some("item_completed")) => item(&payload["item"], at, reading),
        _ => reading.at(at),
    }
}

/// One finished item. Commands and file changes arrive already done, so a
/// call is written with whether it failed.
fn item(item: &Value, at: Option<&str>, reading: &mut Reading) {
    let failed = text(item, "status") == Some("failed");
    match text(item, "type") {
        Some("UserMessage") => reading.user(&blocks(&item["content"]), at),
        Some("AgentMessage") => reading.agent(&blocks(&item["content"]), at),
        Some("CommandExecution") => {
            let failed = failed || item.get("exit_code").and_then(Value::as_i64).is_some_and(|code| code != 0);
            // What Codex parsed the command as is what the person would
            // have typed; the argv is a shell wrapped round it.
            let shown = item
                .get("parsed_cmd")
                .and_then(|cmds| cmds.get(0))
                .and_then(|cmd| text(cmd, "cmd"))
                .map_or_else(|| command_line(item), str::to_string);
            reading.tool(None, "shell", &json!({ "command": shown }), at, failed);
        }
        Some("FileChange") => {
            let names: Vec<&str> = item
                .get("changes")
                .and_then(Value::as_object)
                .map(|changes| changes.keys().map(|path| path.rsplit(['/', '\\']).next().unwrap_or(path)).collect())
                .unwrap_or_default();
            reading.tool(None, "edit", &Value::String(names.join(", ")), at, failed);
        }
        // An item Argus has no reading for yet — a web search, an MCP call —
        // still shows that something was done.
        Some(other) if other.ends_with("Call") || other.ends_with("Search") => {
            let input = item.get("arguments").or_else(|| item.get("query")).unwrap_or(&Value::Null);
            reading.tool(None, other, input, at, failed);
        }
        _ => reading.at(at),
    }
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
            .filter_map(|b| text(b, "text"))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::String(text) => text.clone(),
        _ => String::new(),
    }
}
