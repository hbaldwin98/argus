//! What a harness's own interface to its running session says, in Argus's
//! terms: the live channel, for a harness whose TUI is itself a client of
//! something Argus can also be a client of.
//!
//! Codex's TUI can run on its app-server (`codex --remote`), which serves a
//! thread to every client subscribed to it. Argus subscribes too and reads
//! the thread as it happens — each item as it starts and finishes, the
//! agent's reply token by token — and speaks back through the same channel:
//! a turn started or steered, an interrupt, an approval answered. Item ids
//! are the ones Codex writes to its rollout file, so what arrives live and
//! what is read from the file afterwards are the same entries.
//!
//! This module only translates. Running the server and the connection is
//! `state::live`'s.

use argus_protocol::transcript::{clip, one_line, MAX_TEXT_BYTES, MAX_THINKING_BYTES, MAX_TOOL_INPUT_BYTES, MAX_TOOL_OUTPUT_BYTES};
use argus_protocol::{Body, Choice, Entry, InboxItem, ToolState, Update};
use serde::Deserialize;
use serde_json::{json, Value};

/// Which live channel a harness has. Off unless an agent template asks for
/// it with `live = true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LiveChannel {
    CodexAppServer,
}

/// What the connection should do with one message from the app-server.
#[derive(Debug, Clone, PartialEq)]
pub enum Heard {
    /// Entries for the pane's conversation.
    Said(Vec<Update>),
    /// A turn began; steering and interrupting name it.
    TurnStarted(String, Vec<Update>),
    /// The turn ended.
    TurnEnded(Vec<Update>),
    /// The server asks a person to decide: the question to pose, and the
    /// request to answer once someone has.
    Asked { question: Update, request: Value, method: String },
    /// Someone answered a request, on whatever surface: the request id.
    Resolved(Value),
    Nothing,
}

/// Reads one message from the app-server.
pub fn hear(message: &Value) -> Heard {
    let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = message.get("params").unwrap_or(&Value::Null);
    // A request carries an id the answer must name; a notification none.
    if let Some(id) = message.get("id").filter(|_| !method.is_empty()) {
        return match question(method, id, params) {
            Some(question) => Heard::Asked {
                question,
                request: id.clone(),
                method: method.to_string(),
            },
            None => Heard::Nothing,
        };
    }
    match method {
        "item/started" | "item/completed" => Heard::Said(item(&params["item"])),
        "item/agentMessage/delta" => match (
            params.get("itemId").and_then(Value::as_str),
            params.get("delta").and_then(Value::as_str),
        ) {
            (Some(id), Some(delta)) => Heard::Said(vec![Update::AppendText {
                id: id.to_string(),
                delta: delta.to_string(),
            }]),
            _ => Heard::Nothing,
        },
        "turn/started" => match params["turn"].get("id").and_then(Value::as_str) {
            Some(turn) => Heard::TurnStarted(turn.to_string(), Vec::new()),
            None => Heard::Nothing,
        },
        "turn/completed" => {
            let turn = &params["turn"];
            let id = turn.get("id").and_then(Value::as_str).unwrap_or("turn");
            Heard::TurnEnded(vec![upsert(
                format!("turn:{id}"),
                Body::TurnEnd {
                    millis: turn.get("durationMs").and_then(Value::as_u64),
                },
            )])
        }
        "serverRequest/resolved" => match params.get("requestId") {
            Some(id) => Heard::Resolved(id.clone()),
            None => Heard::Nothing,
        },
        _ => Heard::Nothing,
    }
}

/// The question an approval request poses, named after the request so an
/// answer and a resolution find it.
fn question(method: &str, id: &Value, params: &Value) -> Option<Update> {
    let (prompt, choices): (String, &[(&str, &str)]) = match method {
        "item/commandExecution/requestApproval" => {
            let command = params
                .get("command")
                .map(|c| c.as_str().map_or_else(|| c.to_string(), str::to_string))
                .unwrap_or_default();
            let reason = params.get("reason").and_then(Value::as_str);
            let prompt = match reason {
                Some(reason) => format!("Run `{}`? {reason}", one_line(&command)),
                None => format!("Run `{}`?", one_line(&command)),
            };
            (prompt, &APPROVALS)
        }
        "item/fileChange/requestApproval" => {
            let reason = params.get("reason").and_then(Value::as_str).unwrap_or("Apply these file changes?");
            (reason.to_string(), &APPROVALS)
        }
        _ => return None,
    };
    let choices = choices
        .iter()
        .map(|(id, label)| Choice {
            id: id.to_string(),
            label: label.to_string(),
        })
        .collect();
    Some(upsert(
        question_id(id),
        Body::Question {
            prompt: clip(&prompt, MAX_TEXT_BYTES),
            choices,
            answered: None,
        },
    ))
}

/// What Codex offers for a command or a file change.
const APPROVALS: [(&str, &str); 4] = [
    ("accept", "Allow"),
    ("acceptForSession", "Allow for this session"),
    ("decline", "Decline"),
    ("cancel", "Decline and stop"),
];

/// The entry id a request's question goes by.
pub fn question_id(request: &Value) -> String {
    format!("approval:{}", request.as_str().map_or_else(|| request.to_string(), str::to_string))
}

/// A question marked with the answer someone gave it.
pub fn answered(question: &Update, choice: &str) -> Option<Update> {
    let Update::Upsert(entry) = question else { return None };
    let Body::Question { prompt, choices, .. } = &entry.body else { return None };
    Some(Update::Upsert(Entry {
        body: Body::Question {
            prompt: prompt.clone(),
            choices: choices.clone(),
            answered: Some(choice.to_string()),
        },
        ..entry.clone()
    }))
}

/// One of Codex's thread items, as entries. A command's output arrives
/// with its completion.
fn item(item: &Value) -> Vec<Update> {
    let Some(id) = item.get("id").and_then(Value::as_str).map(str::to_string) else {
        return Vec::new();
    };
    let text = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    match item.get("type").and_then(Value::as_str) {
        Some("userMessage") => {
            let said = content_text(item.get("content").unwrap_or(&Value::Null));
            if said.trim().is_empty() {
                return Vec::new();
            }
            vec![upsert(id, Body::Prompt { text: clip(said.trim(), MAX_TEXT_BYTES) })]
        }
        // Started empty and filled by deltas, then completed whole.
        Some("agentMessage") => vec![upsert(id, Body::Reply { text: clip(&text("text"), MAX_TEXT_BYTES) })],
        Some("reasoning") => {
            let summary = content_text(item.get("summary").unwrap_or(&Value::Null));
            if summary.trim().is_empty() {
                return Vec::new();
            }
            vec![upsert(id, Body::Thinking { text: clip(summary.trim(), MAX_THINKING_BYTES) })]
        }
        Some("commandExecution") => {
            let command = text("command");
            let status = text("status");
            let failed = item.get("exitCode").and_then(Value::as_i64).is_some_and(|code| code != 0)
                || status == "failed"
                || status == "declined";
            let state = match status.as_str() {
                "inProgress" => ToolState::Running,
                _ if failed => ToolState::Failed,
                _ => ToolState::Done,
            };
            let mut updates = vec![upsert(
                id.clone(),
                Body::ToolCall {
                    tool: "shell".to_string(),
                    summary: one_line(&command),
                    input: clip(&command, MAX_TOOL_INPUT_BYTES),
                    state,
                },
            )];
            let output = text("aggregatedOutput");
            if state != ToolState::Running && !output.trim().is_empty() {
                updates.push(upsert(
                    format!("{id}.result"),
                    Body::ToolResult {
                        call: id,
                        output: clip(&output, MAX_TOOL_OUTPUT_BYTES),
                        failed,
                    },
                ));
            }
            updates
        }
        Some("fileChange") => {
            let paths: Vec<String> = item
                .get("changes")
                .and_then(Value::as_array)
                .map(|changes| {
                    changes
                        .iter()
                        .filter_map(|c| c.get("path").and_then(Value::as_str))
                        .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
                        .collect()
                })
                .unwrap_or_default();
            let status = text("status");
            vec![upsert(
                id,
                Body::ToolCall {
                    tool: "edit".to_string(),
                    summary: one_line(&paths.join(", ")),
                    input: String::new(),
                    state: match status.as_str() {
                        "inProgress" => ToolState::Running,
                        "failed" | "declined" => ToolState::Failed,
                        _ => ToolState::Done,
                    },
                },
            )]
        }
        _ => Vec::new(),
    }
}

fn content_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.as_str().or_else(|| p.get("text").and_then(Value::as_str)))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn upsert(id: String, body: Body) -> Update {
    Update::Upsert(Entry { id, at: None, body })
}

/// The JSON-RPC request an inbox item becomes, given the thread and the
/// turn running on it, if any. `None` when there is nothing to say it to.
pub fn request(item: &InboxItem, thread: &str, turn: Option<&str>, id: u64) -> Option<Value> {
    let input = |text: &str| json!([{ "type": "text", "text": text }]);
    let (method, params) = match (item, turn) {
        (InboxItem::Message { text, steer: true }, Some(turn)) => (
            "turn/steer",
            json!({ "threadId": thread, "expectedTurnId": turn, "input": input(text) }),
        ),
        (InboxItem::Message { text, .. }, _) => (
            "turn/start",
            json!({ "threadId": thread, "input": input(text) }),
        ),
        (InboxItem::Interrupt, Some(turn)) => (
            "turn/interrupt",
            json!({ "threadId": thread, "turnId": turn }),
        ),
        _ => return None,
    };
    Some(json!({ "id": id, "method": method, "params": params }))
}

/// The response that answers an approval request with a person's choice.
pub fn answer(request: &Value, choice: &str) -> Value {
    json!({ "id": request, "result": { "decision": choice } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(heard: Heard) -> Vec<Update> {
        match heard {
            Heard::Said(updates) | Heard::TurnStarted(_, updates) | Heard::TurnEnded(updates) => updates,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_reply_starts_empty_grows_by_deltas_and_completes_whole() {
        let started = said(hear(&json!({
            "method": "item/started",
            "params": { "item": { "type": "agentMessage", "id": "msg_1", "text": "" } },
        })));
        assert!(matches!(&started[0], Update::Upsert(e) if e.id == "msg_1"));
        let delta = said(hear(&json!({
            "method": "item/agentMessage/delta",
            "params": { "itemId": "msg_1", "delta": "Build", "threadId": "t", "turnId": "u" },
        })));
        assert_eq!(delta, [Update::AppendText { id: "msg_1".into(), delta: "Build".into() }]);
        let done = said(hear(&json!({
            "method": "item/completed",
            "params": { "item": { "type": "agentMessage", "id": "msg_1", "text": "Build fixed." } },
        })));
        assert!(matches!(&done[0], Update::Upsert(Entry { body: Body::Reply { text }, .. }) if text == "Build fixed."));
    }

    #[test]
    fn a_command_is_running_then_done_with_its_output() {
        let running = said(hear(&json!({
            "method": "item/started",
            "params": { "item": { "type": "commandExecution", "id": "exec_1", "command": "cargo test", "status": "inProgress" } },
        })));
        assert!(matches!(&running[..], [Update::Upsert(Entry { body: Body::ToolCall { state: ToolState::Running, .. }, .. })]));
        let done = said(hear(&json!({
            "method": "item/completed",
            "params": { "item": { "type": "commandExecution", "id": "exec_1", "command": "cargo test",
              "status": "completed", "exitCode": 101, "aggregatedOutput": "1 failed" } },
        })));
        assert!(matches!(&done[0], Update::Upsert(Entry { body: Body::ToolCall { state: ToolState::Failed, .. }, .. })));
        assert!(matches!(&done[1], Update::Upsert(Entry { body: Body::ToolResult { failed: true, .. }, .. })));
    }

    #[test]
    fn an_approval_request_becomes_a_question_its_answer_goes_back_to() {
        let heard = hear(&json!({
            "id": 7,
            "method": "item/commandExecution/requestApproval",
            "params": { "command": "rm -rf target", "reason": "cleaning", "itemId": "i", "threadId": "t", "turnId": "u", "startedAtMs": 0 },
        }));
        let Heard::Asked { question, request, method } = heard else { panic!("{heard:?}") };
        assert_eq!(request, json!(7));
        assert_eq!(method, "item/commandExecution/requestApproval");
        let Update::Upsert(entry) = &question else { panic!() };
        assert_eq!(entry.id, "approval:7");
        let Body::Question { prompt, choices, answered: None } = &entry.body else { panic!() };
        assert_eq!(prompt, "Run `rm -rf target`? cleaning");
        assert_eq!(choices[0].id, "accept");
        assert_eq!(answer(&request, "decline"), json!({ "id": 7, "result": { "decision": "decline" } }));
        assert!(matches!(
            answered(&question, "decline"),
            Some(Update::Upsert(Entry { body: Body::Question { answered: Some(a), .. }, .. })) if a == "decline"
        ));
        assert_eq!(hear(&json!({ "method": "serverRequest/resolved", "params": { "requestId": 7, "threadId": "t" } })), Heard::Resolved(json!(7)));
    }

    #[test]
    fn a_message_starts_a_turn_steers_one_mid_turn_and_an_interrupt_needs_one() {
        let message = |steer| InboxItem::Message { text: "go".into(), steer };
        assert_eq!(request(&message(false), "t", None, 1).unwrap()["method"], "turn/start");
        assert_eq!(request(&message(true), "t", None, 1).unwrap()["method"], "turn/start");
        let steer = request(&message(true), "t", Some("u"), 2).unwrap();
        assert_eq!(steer["method"], "turn/steer");
        assert_eq!(steer["params"]["expectedTurnId"], "u");
        assert_eq!(steer["params"]["input"][0]["text"], "go");
        assert_eq!(request(&InboxItem::Interrupt, "t", Some("u"), 3).unwrap()["method"], "turn/interrupt");
        assert_eq!(request(&InboxItem::Interrupt, "t", None, 3), None);
    }

    #[test]
    fn a_turn_ending_is_marked_and_a_request_nobody_can_answer_here_is_ignored() {
        let ended = hear(&json!({ "method": "turn/completed", "params": { "threadId": "t", "turn": { "id": "u", "durationMs": 1200 } } }));
        assert!(matches!(ended, Heard::TurnEnded(ref u) if matches!(&u[0], Update::Upsert(Entry { body: Body::TurnEnd { millis: Some(1200) }, .. }))));
        assert_eq!(hear(&json!({ "id": 3, "method": "account/chatgptAuthTokens/refresh", "params": {} })), Heard::Nothing);
    }
}
