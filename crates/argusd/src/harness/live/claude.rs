//! Claude Code's API stream, as the tee reads it: the server-sent events of
//! a streaming Messages response, read into the draft a phone shows while
//! the reply is written.
//!
//! Only text and thinking are drafted. A tool call is written to the
//! transcript the moment its block ends, which is soon enough, and its
//! input streams as fragments of JSON nobody would read. The finished text
//! arrives through the transcript file as usual; a draft only fills the
//! wait for it.

use std::collections::HashMap;

use argus_protocol::Draft;
use serde_json::Value;

/// Whether a request's reply is worth drafting: it streams, and it is an
/// agent's turn — one that offers tools — rather than the small calls
/// Claude Code makes for itself, such as naming a conversation.
pub fn worth_drafting(request: &[u8]) -> bool {
    let Ok(body) = serde_json::from_slice::<Value>(request) else {
        return false;
    };
    body.get("stream").and_then(Value::as_bool) == Some(true)
        && body
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| !tools.is_empty())
}

/// Server-sent events taken off a byte stream as each completes.
#[derive(Default)]
pub struct Events {
    buffer: Vec<u8>,
}

impl Events {
    /// The events `bytes` completes, as their `data` parsed. An event whose
    /// data is not JSON is skipped: the stream is forwarded untouched
    /// regardless, and a draft missing a piece is only a draft.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Value> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some((end, gap)) = event_end(&self.buffer) {
            let event: Vec<u8> = self.buffer.drain(..end + gap).take(end).collect();
            let text = String::from_utf8_lossy(&event);
            let data: String = text
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(|data| data.strip_prefix(' ').unwrap_or(data))
                .collect::<Vec<_>>()
                .join("\n");
            if let Ok(value) = serde_json::from_str(&data) {
                events.push(value);
            }
        }
        events
    }
}

/// Where the first complete event in `buffer` ends, and how long the blank
/// line after it is.
fn event_end(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = buffer.windows(2).position(|w| w == b"\n\n").map(|i| (i, 2));
    let crlf = buffer.windows(4).position(|w| w == b"\r\n\r\n").map(|i| (i, 4));
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (one, other) => one.or(other),
    }
}

/// One reply's reading: which of its blocks are being drafted, since a
/// delta names its block only by index.
#[derive(Default)]
pub struct Reading {
    drafting: Option<u64>,
    kinds: HashMap<u64, bool>,
}

impl Reading {
    /// The draft changes one event makes.
    pub fn event(&mut self, event: &Value) -> Vec<Draft> {
        let index = event.get("index").and_then(Value::as_u64);
        match event.get("type").and_then(Value::as_str) {
            Some("content_block_start") => {
                let thinking = match event["content_block"].get("type").and_then(Value::as_str) {
                    Some("text") => false,
                    Some("thinking") => true,
                    _ => return Vec::new(),
                };
                let Some(index) = index else { return Vec::new() };
                self.kinds.insert(index, thinking);
                self.drafting = Some(index);
                vec![Draft::Start { thinking }]
            }
            Some("content_block_delta") if index.is_some() && index == self.drafting => {
                let delta = &event["delta"];
                let text = match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => delta.get("text"),
                    Some("thinking_delta") => delta.get("thinking"),
                    _ => None,
                };
                text.and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                    .map(|text| vec![Draft::More { text: text.to_string() }])
                    .unwrap_or_default()
            }
            Some("content_block_stop") if index.is_some() && index == self.drafting => {
                self.drafting = None;
                vec![Draft::Done]
            }
            // A reply cut off mid-block still ends its draft.
            Some("message_stop" | "error") => self.finish(),
            _ => Vec::new(),
        }
    }

    /// Ends whatever is being drafted, as the stream ending does.
    pub fn finish(&mut self) -> Vec<Draft> {
        match self.drafting.take() {
            Some(_) => vec![Draft::Done],
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reply as Anthropic's API streams it, split at awkward places.
    const STREAM: &str = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"Reading the README\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: ping\ndata: {\"type\":\"ping\"}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\", world\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"Bash\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"command\\\"\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );

    #[test]
    fn a_streamed_reply_drafts_its_thinking_and_text_and_not_its_tool_call() {
        let mut events = Events::default();
        let mut reading = Reading::default();
        let mut drafts = Vec::new();
        // Seven bytes at a time: events arrive split wherever TCP likes.
        for chunk in STREAM.as_bytes().chunks(7) {
            for event in events.feed(chunk) {
                drafts.extend(reading.event(&event));
            }
        }
        assert_eq!(
            drafts,
            [
                Draft::Start { thinking: true },
                Draft::More { text: "Reading the README".into() },
                Draft::Done,
                Draft::Start { thinking: false },
                Draft::More { text: "Hello".into() },
                Draft::More { text: ", world".into() },
                Draft::Done,
            ]
        );
    }

    #[test]
    fn a_reply_cut_off_mid_block_still_ends_its_draft() {
        let mut reading = Reading::default();
        reading.event(&serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text"}}));
        assert_eq!(reading.finish(), [Draft::Done]);
        assert!(reading.finish().is_empty());
    }

    #[test]
    fn crlf_separated_events_are_read_too() {
        let mut events = Events::default();
        let got = events.feed(b"data: {\"type\":\"ping\"}\r\n\r\ndata: {\"type\":\"message_stop\"}\r\n\r\n");
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn only_a_streaming_turn_with_tools_is_drafted() {
        assert!(worth_drafting(br#"{"stream":true,"tools":[{"name":"Bash"}],"messages":[]}"#));
        assert!(!worth_drafting(br#"{"stream":true,"tools":[],"messages":[]}"#), "naming a conversation");
        assert!(!worth_drafting(br#"{"stream":false,"tools":[{"name":"Bash"}]}"#));
        assert!(!worth_drafting(b"not json"));
    }
}
