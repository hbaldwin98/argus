//! A pane's conversation as a client reads it: entries taken from what the
//! harness itself records, and the updates that keep a copy of them current.
//!
//! The daemon never stores a transcript. It reads the harness's own file, or
//! takes what a harness pushes, and turns either into these; a client holds
//! the copy. Updates are shaped so the same entry can arrive twice — from a
//! fresh tail and from the stream that continues it — without doubling.

use serde::{Deserialize, Serialize};

/// The most text one prompt, reply or notice keeps. A reply longer than this
/// is a document someone should open on a desk, not scroll through on a
/// phone, and the whole of it is still in the harness's file.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;

/// The most a thinking block keeps. It is collapsed by default, so what it
/// holds is only ever a glance.
pub const MAX_THINKING_BYTES: usize = 8 * 1024;

/// The most a tool call's input keeps. The summary line is what is read;
/// this is the detail behind it.
pub const MAX_TOOL_INPUT_BYTES: usize = 2 * 1024;

/// The most a tool's output keeps. Enough to see what a command said, not a
/// copy of every file an agent read.
pub const MAX_TOOL_OUTPUT_BYTES: usize = 4 * 1024;

/// The longest a tool call's summary line gets.
pub const MAX_SUMMARY_CHARS: usize = 120;

/// One thing said or done in a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Stable for as long as the source is: reading the same record again
    /// gives the same id, which is what makes an upsert of it idempotent.
    pub id: String,
    /// When the harness says it happened, as it wrote it (RFC 3339 for
    /// every harness that writes one). Carried rather than parsed: only a
    /// person reads it.
    #[serde(default)]
    pub at: Option<String>,
    pub body: Body,
}

/// What an entry is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Body {
    /// What the person asked. Each one starts a turn, so the prompt is the
    /// turn's boundary as well.
    Prompt { text: String },
    /// What the agent said, as markdown.
    Reply { text: String },
    /// The agent's reasoning, where the harness keeps it readable.
    Thinking { text: String },
    /// A tool the agent ran. `summary` is the one line a phone shows —
    /// the command, the file, the pattern — and `input` the detail behind
    /// it.
    ToolCall {
        tool: String,
        summary: String,
        input: String,
        state: ToolState,
    },
    /// What a tool call returned, filed under the call by its id. Its own
    /// entry because it arrives in its own record, often much later.
    ToolResult {
        call: String,
        output: String,
        failed: bool,
    },
    /// Something the harness said that is neither side of the
    /// conversation: a slash command's output, an interruption.
    Notice { text: String },
    /// The agent finished a turn.
    TurnEnd {
        #[serde(default)]
        millis: Option<u64>,
    },
    /// Everything above belongs to an earlier conversation: the pane's
    /// agent started over, resumed another session, or compacted its
    /// context.
    Divider { text: String },
    /// Something the agent is waiting on a person to choose — a permission
    /// it asked for — posed by a harness that can take the answer through
    /// its inbox. `answered` is the choice made, from any surface; an
    /// answered question is only a record.
    Question {
        prompt: String,
        choices: Vec<Choice>,
        #[serde(default)]
        answered: Option<String>,
    },
}

/// One answer a question offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    /// What the harness is told.
    pub id: String,
    /// What a person reads.
    pub label: String,
}

/// Where a tool call has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolState {
    Running,
    Done,
    Failed,
}

/// One change to a client's copy of a transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Update {
    /// Add the entry at the end, or replace the one with its id where it
    /// stands.
    Upsert(Entry),
    /// More text for an entry that is still being written. Only a live
    /// source sends these; a file is only ever read in whole records.
    AppendText { id: String, delta: String },
    /// A tool call has finished. Sent apart from the result so a call whose
    /// output is not worth showing still stops spinning.
    ToolState { id: String, state: ToolState },
    /// What the agent is writing right now, before it is an entry: seen as
    /// it streams, and never stored. The finished text arrives as an entry
    /// of its own, which the draft gives way to.
    Draft(Draft),
}

/// A change to the one draft a conversation has at a time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Draft {
    /// The agent began a reply, or its reasoning; anything drafted before
    /// is over.
    Start { thinking: bool },
    /// More of it.
    More { text: String },
    /// The agent finished it; its entry follows.
    Done,
}

/// The most one push of entries may hold on the wire. What each entry
/// keeps is clipped on arrival besides.
pub const MAX_PUSH_BYTES: usize = 256 * 1024;

/// Entries a harness pushes to the pane API (`POST /pane/<id>/transcript`,
/// or `argus-hook transcript`) when its transcript is not a file Argus can
/// read. `fresh` replaces everything pushed before: what a plugin sends
/// when it replays a whole session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Push {
    #[serde(default)]
    pub fresh: bool,
    pub updates: Vec<Update>,
}

/// An update with every text in it held to its budget. Everything a
/// harness pushes goes through this, since the daemon cannot know how big
/// a plugin thought an entry should be.
pub fn clipped(update: Update) -> Update {
    match update {
        Update::Upsert(mut entry) => {
            entry.body = match entry.body {
                Body::Prompt { text } => Body::Prompt { text: clip(&text, MAX_TEXT_BYTES) },
                Body::Reply { text } => Body::Reply { text: clip(&text, MAX_TEXT_BYTES) },
                Body::Thinking { text } => Body::Thinking { text: clip(&text, MAX_THINKING_BYTES) },
                Body::Notice { text } => Body::Notice { text: clip(&text, MAX_TEXT_BYTES) },
                Body::Divider { text } => Body::Divider { text: clip(&text, MAX_TEXT_BYTES) },
                Body::ToolCall { tool, summary, input, state } => Body::ToolCall {
                    tool: clip(&tool, 64),
                    summary: one_line(&summary),
                    input: clip(&input, MAX_TOOL_INPUT_BYTES),
                    state,
                },
                Body::ToolResult { call, output, failed } => Body::ToolResult {
                    call,
                    output: clip(&output, MAX_TOOL_OUTPUT_BYTES),
                    failed,
                },
                turn @ Body::TurnEnd { .. } => turn,
                Body::Question { prompt, choices, answered } => Body::Question {
                    prompt: clip(&prompt, MAX_TEXT_BYTES),
                    choices: choices
                        .into_iter()
                        .take(8)
                        .map(|c| Choice {
                            id: clip(&c.id, 64),
                            label: one_line(&c.label),
                        })
                        .collect(),
                    answered: answered.map(|a| clip(&a, 64)),
                },
            };
            entry.id = clip(&entry.id, 256);
            Update::Upsert(entry)
        }
        Update::AppendText { id, delta } => Update::AppendText {
            id,
            delta: clip(&delta, MAX_TEXT_BYTES),
        },
        state @ Update::ToolState { .. } => state,
        Update::Draft(Draft::More { text }) => Update::Draft(Draft::More {
            text: clip(&text, MAX_TEXT_BYTES),
        }),
        draft @ Update::Draft(_) => draft,
    }
}

/// Where the part of a transcript a client holds begins, for asking for
/// what came before it. Opaque to a client: it only ever hands one back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Earlier {
    /// Which of the files the pane's conversation has lived in, oldest
    /// first. A conversation that started over moves to a new file.
    pub file: u32,
    /// The byte in that file where the held part begins.
    pub offset: u64,
}

/// `text`, cut to at most `max` bytes on a character boundary, with an
/// ellipsis saying so. Every entry's text goes through this before it leaves
/// the daemon, because a transcript is as big as whatever an agent read.
pub fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max.saturating_sub('…'.len_utf8());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// `text` flattened to one line of at most [`MAX_SUMMARY_CHARS`]
/// characters, for a tool call's summary.
pub fn one_line(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(MAX_SUMMARY_CHARS) {
        Some((i, _)) => format!("{}…", flat[..i].trim_end()),
        None => flat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipped_text_stays_within_its_budget_on_a_character_boundary() {
        let text = "é".repeat(100);
        let clipped = clip(&text, 11);
        assert!(clipped.len() <= 11, "{} bytes", clipped.len());
        assert!(clipped.ends_with('…'));
        assert_eq!(clip("short", 11), "short");
    }

    #[test]
    fn a_summary_is_one_line_and_bounded() {
        assert_eq!(one_line("cargo  test\n  --workspace"), "cargo test --workspace");
        let long = "x ".repeat(200);
        assert!(one_line(&long).chars().count() <= MAX_SUMMARY_CHARS + 1);
    }

    #[test]
    fn a_pushed_entry_is_held_to_its_budget() {
        let huge = "x".repeat(MAX_TEXT_BYTES * 2);
        let Update::Upsert(entry) = clipped(Update::Upsert(Entry {
            id: "1".into(),
            at: None,
            body: Body::Reply { text: huge },
        })) else {
            panic!("still an upsert");
        };
        let Body::Reply { text } = entry.body else { panic!("still a reply") };
        assert!(text.len() <= MAX_TEXT_BYTES);
    }

    #[test]
    fn a_push_reads_from_the_json_a_plugin_writes() {
        let push: Push = serde_json::from_str(
            r#"{"fresh":true,"updates":[{"Upsert":{"id":"p1","body":{"Prompt":{"text":"hi"}}}},{"AppendText":{"id":"p2","delta":"more"}}]}"#,
        )
        .unwrap();
        assert!(push.fresh);
        assert_eq!(push.updates.len(), 2);
    }

    #[test]
    fn an_update_survives_the_wire() {
        let update = Update::Upsert(Entry {
            id: "0:10.1".into(),
            at: Some("2026-09-29T03:23:28.518Z".into()),
            body: Body::ToolCall {
                tool: "Bash".into(),
                summary: "cargo test".into(),
                input: "{\"command\":\"cargo test\"}".into(),
                state: ToolState::Running,
            },
        });
        let bytes = rmp_serde::to_vec_named(&update).unwrap();
        let read: Update = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(read, update);
    }
}
