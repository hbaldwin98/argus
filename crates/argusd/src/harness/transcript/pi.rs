//! pi's transcript: the session JSONL under
//! `~/.pi/agent/sessions/<checkout>/<time>_<session>.jsonl`.
//!
//! Each conversation message is a `message` record holding the provider's
//! message: the person's, the assistant's text and tool calls, and each
//! tool's result as a message of its own that names the call. The session
//! header says where it ran, and a `session_info` record the name the
//! person gave it; pi's other records — model and thinking-level changes —
//! are settings, not conversation.

use serde_json::Value;

use super::{blocks_text, text, Reading};

pub(super) fn read(record: &Value, reading: &mut Reading) {
    let at = text(record, "timestamp");
    match text(record, "type") {
        Some("message") => {}
        Some("session") => {
            reading.cwd(text(record, "cwd"));
            reading.session_id(text(record, "id"));
            return reading.at(at);
        }
        Some("session_info") => {
            reading.title(text(record, "name").unwrap_or_default());
            return reading.at(at);
        }
        _ => return reading.at(at),
    }
    let message = &record["message"];
    let content = message.get("content").unwrap_or(&Value::Null);
    match text(message, "role") {
        Some("user") => reading.user(&blocks_text(content), at),
        Some("assistant") => {
            for block in content.as_array().into_iter().flatten() {
                match text(block, "type") {
                    Some("text") => reading.agent(text(block, "text").unwrap_or_default(), at),
                    Some("toolCall") => reading.tool(
                        text(block, "id"),
                        text(block, "name").unwrap_or("tool"),
                        block.get("arguments").unwrap_or(&Value::Null),
                        at,
                        false,
                    ),
                    _ => reading.at(at),
                }
            }
        }
        Some("toolResult") => {
            let failed = message.get("isError").and_then(Value::as_bool) == Some(true);
            match text(message, "toolCallId") {
                Some(call) => reading.finished(call, failed, at),
                None => reading.at(at),
            }
        }
        _ => reading.at(at),
    }
}
