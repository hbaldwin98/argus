//! pi's transcript: the session JSONL its extension names through
//! `getSessionFile()`.
//!
//! Each conversation message is a `message` record holding the provider's
//! message: the person's, the assistant's thinking, text and tool calls,
//! and each tool's result as a message of its own that names the call.
//! pi's other records — the session header, model and thinking-level
//! changes — are settings, not conversation.

use argus_protocol::transcript::{clip, MAX_TEXT_BYTES, MAX_THINKING_BYTES, MAX_TOOL_OUTPUT_BYTES};
use argus_protocol::{Body, Entry, ToolState, Update};
use serde_json::Value;

use super::{blocks_text, input_text, summary};

pub(super) fn read(record: &Value, id: &str) -> Vec<Update> {
    let at = record.get("timestamp").and_then(Value::as_str).map(str::to_string);
    let upsert = |id: String, body: Body| {
        Update::Upsert(Entry {
            id,
            at: at.clone(),
            body,
        })
    };
    match record.get("type").and_then(Value::as_str) {
        Some("message") => {}
        Some("compaction") => {
            return vec![upsert(
                format!("{id}.0"),
                Body::Divider {
                    text: "Context compacted".to_string(),
                },
            )]
        }
        _ => return Vec::new(),
    }
    let message = &record["message"];
    let content = message.get("content").unwrap_or(&Value::Null);
    match message.get("role").and_then(Value::as_str) {
        Some("user") => {
            let text = blocks_text(content);
            if text.trim().is_empty() {
                return Vec::new();
            }
            vec![upsert(format!("{id}.0"), Body::Prompt { text: clip(text.trim(), MAX_TEXT_BYTES) })]
        }
        Some("assistant") => {
            let mut updates: Vec<Update> = content
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .filter_map(|(part, block)| assistant_block(block, &format!("{id}.{part}")).map(|(id, body)| upsert(id, body)))
                .collect();
            if let Some(error) = message.get("errorMessage").and_then(Value::as_str) {
                updates.push(upsert(format!("{id}.error"), Body::Notice { text: error.to_string() }));
            }
            updates
        }
        Some("toolResult") => {
            let Some(call) = message.get("toolCallId").and_then(Value::as_str) else {
                return Vec::new();
            };
            let failed = message.get("isError").and_then(Value::as_bool) == Some(true);
            vec![
                Update::ToolState {
                    id: call.to_string(),
                    state: if failed { ToolState::Failed } else { ToolState::Done },
                },
                upsert(
                    format!("{id}.0"),
                    Body::ToolResult {
                        call: call.to_string(),
                        output: clip(&blocks_text(content), MAX_TOOL_OUTPUT_BYTES),
                        failed,
                    },
                ),
            ]
        }
        _ => Vec::new(),
    }
}

/// One block of an assistant message, and the id it is named by: its own
/// for a tool call, which the result names, else its place.
fn assistant_block(block: &Value, place: &str) -> Option<(String, Body)> {
    let text_of = |key: &str| {
        block
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
    };
    match block.get("type").and_then(Value::as_str)? {
        "text" => Some((place.to_string(), Body::Reply { text: clip(text_of("text")?, MAX_TEXT_BYTES) })),
        "thinking" => Some((
            place.to_string(),
            Body::Thinking {
                text: clip(text_of("thinking")?, MAX_THINKING_BYTES),
            },
        )),
        "toolCall" => {
            let input = block.get("arguments").unwrap_or(&Value::Null);
            Some((
                text_of("id")?.to_string(),
                Body::ToolCall {
                    tool: text_of("name").unwrap_or("tool").to_string(),
                    summary: summary(input),
                    input: input_text(input),
                    state: ToolState::Running,
                },
            ))
        }
        _ => None,
    }
}
