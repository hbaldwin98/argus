//! What a harness's own transcript says, read into Argus's entries.
//!
//! Every harness that keeps a transcript writes it as JSON lines, one record
//! per line, appended as the conversation goes. A dialect reads one line at
//! a time and knows nothing of files: which file, from where and how often
//! is `state::transcripts`' question. A line is named by where it sits in
//! its file, so the entries it yields keep their ids however often it is
//! read again.

use argus_protocol::transcript::{clip, one_line, MAX_TOOL_INPUT_BYTES};
use argus_protocol::Update;
use serde::Deserialize;
use serde_json::Value;

mod claude;

/// Which harness wrote a transcript, and so how to read it. A `[[harness]]`
/// block in the config names one with `transcript = "claude"`, since a
/// block cannot bring a parser of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    Claude,
}

impl Dialect {
    /// The updates one line of a transcript makes. `id` names the line; an
    /// entry the line yields is `id` and its place in the line, unless the
    /// harness gave it an id of its own (a tool call's, which the result
    /// refers back to). A line this cannot read yields nothing: a harness
    /// adds record types faster than Argus learns them, and one it does not
    /// know is not a reason to stop reading the ones it does.
    pub fn read(self, line: &str, id: &str) -> Vec<Update> {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match self {
            Dialect::Claude => claude::read(&record, id),
        }
    }
}

/// A tool call's one-line summary, from whichever of the inputs harnesses
/// commonly give a tool says most about what it is doing. A description
/// the agent wrote for a person comes first, then the thing acted on.
fn summary(input: &Value) -> String {
    const KEYS: [&str; 12] = [
        "description",
        "command",
        "cmd",
        "file_path",
        "filePath",
        "path",
        "notebook_path",
        "pattern",
        "url",
        "query",
        "skill",
        "prompt",
    ];
    KEYS.iter()
        .find_map(|key| input.get(key).and_then(Value::as_str))
        .or_else(|| input.as_str())
        .map(one_line)
        .unwrap_or_default()
}

/// A tool call's input as the detail behind its summary.
fn input_text(input: &Value) -> String {
    let text = match input {
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    clip(&text, MAX_TOOL_INPUT_BYTES)
}

/// The text of a content value that may be a plain string or a list of
/// blocks, as Anthropic-shaped messages carry both. Images are named rather
/// than dropped, so a result that was only a screenshot does not read as
/// empty.
fn blocks_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| match block.get("type").and_then(Value::as_str) {
                Some("text") => block.get("text").and_then(Value::as_str).map(str::to_string),
                Some("image") => Some("[image]".to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests;
