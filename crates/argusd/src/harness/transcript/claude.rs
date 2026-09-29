//! Claude Code's transcript: the session JSONL its hooks name in
//! `transcript_path`.
//!
//! Each record is one content block — a thinking block, a tool call, a text
//! reply — written when the block is finished, so a reply arrives whole
//! rather than token by token. Subagents write sidechain records, which
//! their spawning call's result already sums up. A message sent while the
//! agent works arrives as a `queued_command` attachment rather than a user
//! record. Most other record types are
//! the CLI's own bookkeeping (modes, titles, file snapshots, the remote
//! bridge) and say nothing a person reading the conversation needs.

use argus_protocol::transcript::{clip, MAX_TEXT_BYTES, MAX_THINKING_BYTES, MAX_TOOL_OUTPUT_BYTES};
use argus_protocol::{Body, Entry, ToolState, Update};
use serde_json::Value;

use super::{blocks_text, input_text, summary};

pub(super) fn read(record: &Value, id: &str) -> Vec<Update> {
    if record.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return Vec::new();
    }
    let at = record
        .get("timestamp")
        .and_then(Value::as_str)
        .map(str::to_string);
    let line = Line { id, at };
    match record.get("type").and_then(Value::as_str) {
        Some("user") => line.user(record),
        Some("assistant") => line.assistant(record),
        Some("system") => line.system(record).into_iter().collect(),
        Some("attachment") => line.attachment(record).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// The record being read: what its entries are named after and stamped
/// with.
struct Line<'a> {
    id: &'a str,
    at: Option<String>,
}

impl Line<'_> {
    fn entry(&self, part: usize, body: Body) -> Update {
        self.entry_with_id(format!("{}.{part}", self.id), body)
    }

    fn entry_with_id(&self, id: String, body: Body) -> Update {
        Update::Upsert(Entry {
            id,
            at: self.at.clone(),
            body,
        })
    }

    /// What the person typed, or a tool's result coming back: Claude files
    /// both as the user's turn. `isMeta` marks what the CLI added on the
    /// person's behalf — a skill's text, a caveat — which they never said.
    fn user(&self, record: &Value) -> Vec<Update> {
        let flag = |key: &str| record.get(key).and_then(Value::as_bool) == Some(true);
        if flag("isMeta") || flag("isCompactSummary") {
            return Vec::new();
        }
        let content = &record["message"]["content"];
        if let Some(text) = content.as_str() {
            return self.typed(0, text).into_iter().collect();
        }
        let Some(blocks) = content.as_array() else {
            return Vec::new();
        };

        let mut updates = Vec::new();
        let mut said = Vec::new();
        for (part, block) in blocks.iter().enumerate() {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_result") => updates.extend(self.tool_result(part, block)),
                Some("text") => said.extend(block.get("text").and_then(Value::as_str)),
                Some("image") => said.push("[image]"),
                _ => {}
            }
        }
        if !said.is_empty() {
            updates.extend(self.typed(blocks.len(), &said.join("\n")));
        }
        updates
    }

    /// A line the person's side of the conversation carries. Slash commands
    /// and `!` shell commands arrive wrapped in the tags the CLI uses to
    /// tell the model what happened, and are unwrapped back into what was
    /// typed; their output is a notice rather than something anyone said.
    fn typed(&self, part: usize, text: &str) -> Option<Update> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        if let Some(name) = tagged(text, "command-name") {
            let args = tagged(text, "command-args").unwrap_or_default();
            let command = format!("{name} {args}").trim().to_string();
            return Some(self.entry(part, Body::Prompt { text: command }));
        }
        if let Some(command) = tagged(text, "bash-input") {
            return Some(self.entry(part, Body::Prompt { text: format!("!{command}") }));
        }
        for tag in ["local-command-stdout", "local-command-stderr", "bash-stdout", "bash-stderr"] {
            if let Some(output) = tagged(text, tag) {
                return (!output.is_empty()).then(|| {
                    self.entry(part, Body::Notice { text: clip(&output, MAX_TEXT_BYTES) })
                });
            }
        }
        if text.starts_with("<local-command-caveat>") {
            return None;
        }
        if text.starts_with("[Request interrupted") {
            return Some(self.entry(part, Body::Notice { text: text.to_string() }));
        }
        Some(self.entry(part, Body::Prompt { text: clip(text, MAX_TEXT_BYTES) }))
    }

    /// A tool's result, filed under the call it answers, and that call
    /// marked finished.
    fn tool_result(&self, part: usize, block: &Value) -> Vec<Update> {
        let Some(call) = block.get("tool_use_id").and_then(Value::as_str) else {
            return Vec::new();
        };
        let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
        let output = blocks_text(block.get("content").unwrap_or(&Value::Null));
        let state = if failed { ToolState::Failed } else { ToolState::Done };
        vec![
            Update::ToolState {
                id: call.to_string(),
                state,
            },
            self.entry(
                part,
                Body::ToolResult {
                    call: call.to_string(),
                    output: clip(&output, MAX_TOOL_OUTPUT_BYTES),
                    failed,
                },
            ),
        ]
    }

    fn assistant(&self, record: &Value) -> Vec<Update> {
        let Some(blocks) = record["message"]["content"].as_array() else {
            return Vec::new();
        };
        blocks
            .iter()
            .enumerate()
            .filter_map(|(part, block)| self.assistant_block(part, block))
            .collect()
    }

    fn assistant_block(&self, part: usize, block: &Value) -> Option<Update> {
        let text_of = |key: &str| {
            block
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
        };
        match block.get("type").and_then(Value::as_str)? {
            "text" => {
                let text = clip(text_of("text")?, MAX_TEXT_BYTES);
                Some(self.entry(part, Body::Reply { text }))
            }
            "thinking" => {
                let text = clip(text_of("thinking")?, MAX_THINKING_BYTES);
                Some(self.entry(part, Body::Thinking { text }))
            }
            // Named by the harness's own id, because that is what its
            // result refers back to.
            "tool_use" => {
                let id = block.get("id").and_then(Value::as_str)?.to_string();
                let input = block.get("input").unwrap_or(&Value::Null);
                let body = Body::ToolCall {
                    tool: text_of("name").unwrap_or("tool").to_string(),
                    summary: summary(input),
                    input: input_text(input),
                    state: ToolState::Running,
                };
                Some(self.entry_with_id(id, body))
            }
            _ => None,
        }
    }

    /// A message the person sent while the agent was working, which the
    /// CLI hands the model mid-turn rather than as a turn of its own. The
    /// other attachments are context the CLI adds by itself.
    fn attachment(&self, record: &Value) -> Option<Update> {
        let attachment = record.get("attachment")?;
        if attachment.get("type").and_then(Value::as_str) != Some("queued_command") {
            return None;
        }
        let text = attachment.get("prompt").and_then(Value::as_str)?;
        self.typed(0, text)
    }

    /// The few system records that mark something a reader should see: a
    /// turn ending, the context being compacted, the API failing, a slash
    /// command run, and the recap the CLI writes for someone coming back.
    fn system(&self, record: &Value) -> Option<Update> {
        let content = record.get("content").and_then(Value::as_str);
        let body = match record.get("subtype").and_then(Value::as_str)? {
            "local_command" => return self.typed(0, content?),
            "away_summary" => Body::Notice {
                text: clip(content?, MAX_TEXT_BYTES),
            },
            "turn_duration" => Body::TurnEnd {
                millis: record.get("durationMs").and_then(Value::as_u64),
            },
            "compact_boundary" => Body::Divider {
                text: "Context compacted".to_string(),
            },
            "api_error" => Body::Notice {
                text: content.unwrap_or("The API returned an error").to_string(),
            },
            _ => return None,
        };
        Some(self.entry(0, body))
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
