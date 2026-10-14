//! Reading a session's transcript, in whichever format its harness keeps
//! it, into the protocol's harness-neutral [`Transcript`].
//!
//! Every harness that keeps a transcript writes JSON lines, one record per
//! line, appended as the conversation goes. A format's reader takes one
//! record at a time into a [`Reading`] and knows nothing of files; finding
//! the file is `locate`'s question. Read only, and only when asked: nothing
//! here follows a session as it runs.

use std::io::{BufRead, BufReader};
use std::path::Path;

use argus_protocol::transcript::{
    clip, one_line, Conversation, Entry, Transcript, MAX_ENTRIES, MAX_TEXT_BYTES,
};
use serde_json::Value;

mod claude;
mod codex;
mod cursor;
mod locate;
mod neutral;
mod pi;

/// Which harness's format a transcript is in, and so how to read it. A
/// `[[harness]]` block names one, since a block cannot bring a reader of
/// its own; `neutral` is the format a harness Argus knows nothing of can
/// write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Claude,
    Codex,
    Cursor,
    Pi,
    Neutral,
}

impl Format {
    /// The name a config uses for it. Unknown names are the caller's to
    /// report, so a config naming a format this build lacks still loads.
    pub fn named(name: &str) -> Option<Format> {
        Some(match name.to_ascii_lowercase().as_str() {
            "claude" => Format::Claude,
            "codex" => Format::Codex,
            "cursor" => Format::Cursor,
            "pi" => Format::Pi,
            "neutral" => Format::Neutral,
            _ => return None,
        })
    }

    /// Where the harness that writes this format keeps a session's
    /// transcript, as `locate` templates. Neutral has no home of its own:
    /// whoever writes it says where.
    pub fn default_paths(self) -> Vec<String> {
        let paths: &[&str] = match self {
            // Under a directory named for the checkout's path; a session
            // id is a UUID, so any project directory will do.
            Format::Claude => &["~/.claude/projects/*/{session_id}.jsonl"],
            Format::Codex => &["~/.codex/sessions/*/*/*/rollout-*-{session_id}.jsonl"],
            // Older builds write the file straight into the directory,
            // newer ones into a directory of its own.
            Format::Cursor => &[
                "~/.cursor/projects/*/agent-transcripts/{session_id}.jsonl",
                "~/.cursor/projects/*/agent-transcripts/{session_id}/{session_id}.jsonl",
            ],
            Format::Pi => &["~/.pi/agent/sessions/*/*_{session_id}.jsonl"],
            Format::Neutral => &[],
        };
        paths.iter().map(|p| p.to_string()).collect()
    }

    fn read_record(self, record: &Value, reading: &mut Reading) {
        match self {
            Format::Claude => claude::read(record, reading),
            Format::Codex => codex::read(record, reading),
            Format::Cursor => cursor::read(record, reading),
            Format::Pi => pi::read(record, reading),
            Format::Neutral => neutral::read(record, reading),
        }
    }
}

/// Where a harness keeps its transcripts and what they are written in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub format: Format,
    /// `locate` templates, tried in order.
    pub paths: Vec<String>,
}

impl Source {
    /// The format, kept where its harness keeps it.
    pub fn of(format: Format) -> Source {
        Source { format, paths: format.default_paths() }
    }
}

/// What a session's preview shows of its conversation: `source` is its
/// harness's, `None` for one that keeps no transcript Argus can read.
pub fn conversation(source: Option<&Source>, session_id: &str, checkout: &Path) -> Conversation {
    let Some(source) = source else {
        return Conversation::NoTranscript;
    };
    match locate::find(&source.paths, session_id, checkout) {
        Some(path) => read(source.format, &path).map_or(Conversation::Missing, Conversation::Read),
        None => Conversation::Missing,
    }
}

/// The whole of one transcript file. A line that is not JSON, or a record
/// the format does not know, is passed over: a harness adds record types
/// faster than Argus learns them, and one it does not know is not a reason
/// to stop reading the ones it does.
pub fn read(format: Format, path: &Path) -> std::io::Result<Transcript> {
    let mut file = BufReader::new(std::fs::File::open(path)?);
    let mut reading = Reading::default();
    let mut line = Vec::new();
    loop {
        line.clear();
        if file.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        reading.line(format, &String::from_utf8_lossy(&line));
    }
    Ok(reading.finish())
}

#[derive(Debug, Default)]
pub struct Reading {
    transcript: Transcript,
    /// Tool calls by the harness's own id, which a later result names.
    calls: std::collections::HashMap<String, usize>,
}

impl Reading {
    fn line(&mut self, format: Format, line: &str) {
        if let Ok(record) = serde_json::from_str::<Value>(line.trim()) {
            format.read_record(&record, self);
        }
    }

    /// A time a record was written. Records are appended, so the first is
    /// when the session started and the last when it was last heard from.
    fn at(&mut self, at: Option<&str>) {
        let Some(at) = at.filter(|at| !at.is_empty()) else {
            return;
        };
        let meta = &mut self.transcript.meta;
        if meta.started.is_none() {
            meta.started = Some(at.to_string());
        }
        meta.ended = Some(at.to_string());
    }

    fn user(&mut self, text: &str, at: Option<&str>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.at(at);
        let meta = &mut self.transcript.meta;
        meta.turns += 1;
        // A slash or shell command says how the session was set up, not
        // what it was for.
        if meta.title.is_none() && !text.starts_with(['/', '!']) {
            meta.title = Some(one_line(text));
        }
        self.transcript.entries.push(Entry::User {
            text: clip(text, MAX_TEXT_BYTES),
            at: at.map(str::to_string),
        });
    }

    fn agent(&mut self, text: &str, at: Option<&str>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.at(at);
        self.transcript.entries.push(Entry::Agent {
            text: clip(text, MAX_TEXT_BYTES),
            at: at.map(str::to_string),
        });
    }

    /// A tool call; `id` is the harness's own, for one whose result comes
    /// in a record of its own.
    fn tool(&mut self, id: Option<&str>, tool: &str, input: &Value, at: Option<&str>, failed: bool) {
        self.at(at);
        if let Some(id) = id {
            self.calls.insert(id.to_string(), self.transcript.entries.len());
        }
        self.transcript.entries.push(Entry::Tool {
            tool: one_line(tool),
            summary: summary(input),
            at: at.map(str::to_string),
            failed,
        });
    }

    /// A call's failure, which some harnesses record only later, in a
    /// result keyed by their own id for the call.
    fn finished(&mut self, id: &str, failed: bool, at: Option<&str>) {
        self.at(at);
        let Some(&i) = self.calls.get(id) else {
            return;
        };
        if let Some(Entry::Tool { failed: was, .. }) = self.transcript.entries.get_mut(i) {
            *was = failed;
        }
    }

    fn title(&mut self, title: &str) {
        let title = one_line(title);
        if !title.is_empty() {
            self.transcript.meta.title = Some(title);
        }
    }

    fn cwd(&mut self, cwd: Option<&str>) {
        if let Some(cwd) = cwd.filter(|cwd| !cwd.is_empty()) {
            self.transcript.meta.cwd.get_or_insert_with(|| cwd.to_string());
        }
    }

    fn finish(mut self) -> Transcript {
        let entries = &mut self.transcript.entries;
        if entries.len() > MAX_ENTRIES {
            let earlier = entries.len() - MAX_ENTRIES;
            entries.drain(..earlier);
            self.transcript.earlier = u32::try_from(earlier).unwrap_or(u32::MAX);
        }
        self.transcript
    }
}

/// A tool call's one-line summary, from whichever of the inputs harnesses
/// commonly give a tool says most about what it is doing. A description
/// the agent wrote for a person comes first, then what it ran or looked
/// for, then the thing acted on — a search names its pattern before the
/// directory it searched.
fn summary(input: &Value) -> String {
    const KEYS: [&str; 13] = [
        "description",
        "command",
        "cmd",
        "pattern",
        "glob_pattern",
        "query",
        "url",
        "file_path",
        "filePath",
        "path",
        "notebook_path",
        "skill",
        "prompt",
    ];
    KEYS.iter()
        .find_map(|key| input.get(key).and_then(Value::as_str))
        .or_else(|| input.as_str())
        .map(one_line)
        .unwrap_or_default()
}

/// The text of a content value that may be a plain string or a list of
/// blocks, as Anthropic-shaped messages carry both. Images are named rather
/// than dropped, so a turn that was only a screenshot does not read as
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

fn text<'a>(record: &'a Value, key: &str) -> Option<&'a str> {
    record.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests;
