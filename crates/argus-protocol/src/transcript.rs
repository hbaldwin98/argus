//! What a past session said, in one shape whichever harness wrote it: the
//! person's turns, the agent's replies and the tools it ran, with what can
//! be read of the session itself — its title, when it ran and how many
//! turns it had.
//!
//! The daemon reads a harness's own transcript file into this; it never
//! keeps one. It is a preview, read whole when asked for, never followed:
//! nothing here updates a copy a client holds.

use serde::{Deserialize, Serialize};

/// The most text one turn or reply keeps. A preview is read at a glance;
/// the whole of it is still in the harness's file.
pub const MAX_TEXT_BYTES: usize = 8 * 1024;

/// The longest a tool call's summary or a session's title gets.
pub const MAX_SUMMARY_CHARS: usize = 120;

/// The most entries a transcript keeps, the newest ones. A session that ran
/// for a day is thousands of tool calls, and how it ended is what a preview
/// is opened for.
pub const MAX_ENTRIES: usize = 500;

/// One thing said or done in a session.
///
/// Also a line of the neutral transcript format, which a harness Argus has
/// no reader for may write (DESIGN.md, "Transcripts"): each line is one of
/// these as JSON, `{"kind":"user","text":"…","at":"…"}`, or a
/// [`TITLE_KIND`] line naming the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    /// What the person asked. Each one is a turn.
    User {
        text: String,
        /// When the harness says it happened, as it wrote it (RFC 3339 for
        /// every harness that writes one). Carried rather than parsed: only
        /// a person reads it.
        #[serde(default)]
        at: Option<String>,
    },
    /// What the agent said, as markdown.
    Agent {
        text: String,
        #[serde(default)]
        at: Option<String>,
    },
    /// A tool the agent ran. `summary` is the one line a reader is shown —
    /// the command, the file, the pattern.
    Tool {
        tool: String,
        #[serde(default)]
        summary: String,
        #[serde(default)]
        at: Option<String>,
        /// Only when the harness recorded the call failing; a harness that
        /// keeps no results (Cursor) never says so.
        #[serde(default)]
        failed: bool,
    },
    /// A kind this build does not know, from a newer peer or a neutral
    /// line. Without it one new kind would fail the whole transcript; it
    /// is dropped by a reader and passed over by a client.
    #[serde(other)]
    Unknown,
}

/// The `kind` of a neutral transcript line that names the session rather
/// than adding to it: `{"kind":"title","text":"…"}`. The last one wins.
pub const TITLE_KIND: &str = "title";

/// What a transcript says about the session as a whole, where it is cheap
/// to read: everything here comes from the same pass that reads the
/// entries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    /// The title the harness gave the session, else the first thing the
    /// person asked, on one line.
    #[serde(default)]
    pub title: Option<String>,
    /// The first and last times the transcript records, as it wrote them.
    #[serde(default)]
    pub started: Option<String>,
    #[serde(default)]
    pub ended: Option<String>,
    /// How many times the person spoke, counting turns older than the
    /// entries kept.
    #[serde(default)]
    pub turns: u32,
    /// The directory the harness says the session ran in.
    #[serde(default)]
    pub cwd: Option<String>,
}

/// A session's transcript as read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transcript {
    #[serde(default)]
    pub meta: Meta,
    /// Oldest first, at most [`MAX_ENTRIES`] of them.
    #[serde(default)]
    pub entries: Vec<Entry>,
    /// How many older entries were left out to keep within
    /// [`MAX_ENTRIES`].
    #[serde(default)]
    pub earlier: u32,
}

/// What a session's preview can show of what was said in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Conversation {
    /// The harness keeps no transcript Argus can read. The session is still
    /// listed, from what Argus recorded of it.
    NoTranscript,
    /// The harness keeps one, and it is not where the harness says: moved,
    /// cleaned up, or never written.
    Missing,
    Read(Transcript),
}

/// `text`, cut to at most `max` bytes on a character boundary, with an
/// ellipsis saying so.
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
/// characters, for a tool call's summary or a title.
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
    fn a_neutral_line_is_an_entry_as_json() {
        // What a harness writes by hand: the optional fields left out.
        let read: Entry = serde_json::from_str(r#"{"kind":"tool","tool":"bash"}"#).unwrap();
        assert_eq!(
            read,
            Entry::Tool {
                tool: "bash".into(),
                summary: String::new(),
                at: None,
                failed: false,
            }
        );
        let user = Entry::User {
            text: "hi".into(),
            at: Some("2026-10-14T09:00:00Z".into()),
        };
        assert_eq!(
            serde_json::to_string(&user).unwrap(),
            r#"{"kind":"user","text":"hi","at":"2026-10-14T09:00:00Z"}"#
        );
    }

    #[test]
    fn a_conversation_survives_the_wire() {
        let conversation = Conversation::Read(Transcript {
            meta: Meta {
                title: Some("Add a stop command".into()),
                started: Some("2026-09-22T22:55:44.000Z".into()),
                ended: Some("2026-09-22T23:01:10.000Z".into()),
                turns: 1,
                cwd: Some("/repo".into()),
            },
            entries: vec![
                Entry::User { text: "Add a stop command".into(), at: None },
                Entry::Tool {
                    tool: "shell".into(),
                    summary: "cargo test".into(),
                    at: Some("2026-09-22T22:56:30.000Z".into()),
                    failed: true,
                },
                Entry::Agent { text: "Done.".into(), at: None },
            ],
            earlier: 3,
        });
        let bytes = rmp_serde::to_vec_named(&conversation).unwrap();
        let read: Conversation = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(read, conversation);

        let bytes = rmp_serde::to_vec_named(&Conversation::NoTranscript).unwrap();
        assert_eq!(rmp_serde::from_slice::<Conversation>(&bytes).unwrap(), Conversation::NoTranscript);
    }

    #[test]
    fn an_entry_of_a_kind_this_build_does_not_know_does_not_fail_its_transcript() {
        // A newer daemon's transcript, as an older client would receive it.
        #[derive(Serialize)]
        struct Memo {
            kind: &'static str,
            text: &'static str,
        }
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Newer {
            Known(Entry),
            New(Memo),
        }
        #[derive(Serialize)]
        struct NewerTranscript {
            entries: Vec<Newer>,
        }
        let newer = NewerTranscript {
            entries: vec![
                Newer::New(Memo { kind: "memo", text: "a kind added later" }),
                Newer::Known(Entry::Agent { text: "Done.".into(), at: None }),
            ],
        };
        let bytes = rmp_serde::to_vec_named(&newer).unwrap();
        let read: Transcript = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(read.entries, [Entry::Unknown, Entry::Agent { text: "Done.".into(), at: None }]);
    }
}
