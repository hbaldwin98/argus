//! Structured events from an agent turn, harness-neutral on the wire.
//!
//! Adapters map harness-specific lifecycle and transcript signals into this
//! vocabulary so the daemon and client can show one timeline per pane.

use serde::{Deserialize, Serialize};

/// How many events a pane keeps. Older ones drop off the front so a long
/// session cannot grow the tree without bound.
pub const MAX_TRANSCRIPT_EVENTS: usize = 200;

/// One step in what the agent did, in the order it happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTranscriptEvent {
    pub kind: TranscriptKind,
    /// Human-readable payload when the harness has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// For [`TranscriptKind::Tool`], which tool started or finished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

/// The phases a harness can report. Names match what operators expect from
/// a coding agent, not any one vendor's event strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptKind {
    Prompt,
    Thinking,
    Tool,
    Output,
    Result,
}

impl TranscriptKind {
    pub const ALL: [TranscriptKind; 5] = [
        TranscriptKind::Prompt,
        TranscriptKind::Thinking,
        TranscriptKind::Tool,
        TranscriptKind::Output,
        TranscriptKind::Result,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TranscriptKind::Prompt => "prompt",
            TranscriptKind::Thinking => "thinking",
            TranscriptKind::Tool => "tool",
            TranscriptKind::Output => "output",
            TranscriptKind::Result => "result",
        }
    }

    pub fn parse(s: &str) -> Option<TranscriptKind> {
        TranscriptKind::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
    }
}
