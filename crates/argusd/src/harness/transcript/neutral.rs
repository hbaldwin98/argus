//! The neutral transcript format: what a harness Argus has no reader for
//! writes so its sessions can be previewed.
//!
//! Each line is an [`Entry`] as JSON — `{"kind":"user","text":"…"}`,
//! `{"kind":"agent",…}`, `{"kind":"tool","tool":"…","summary":"…"}`, each
//! with an optional RFC 3339 `at` — or a `{"kind":"title","text":"…"}` line
//! naming the session. The shape is the protocol's, so what is read here
//! is what a client is sent.

use argus_protocol::transcript::{Entry, TITLE_KIND};
use serde_json::Value;

use super::{text, Reading};

pub(super) fn read(record: &Value, reading: &mut Reading) {
    if text(record, "kind") == Some(TITLE_KIND) {
        return reading.title(text(record, "text").unwrap_or_default());
    }
    let Ok(entry) = serde_json::from_value::<Entry>(record.clone()) else {
        return;
    };
    match entry {
        Entry::User { text, at } => reading.user(&text, at.as_deref()),
        Entry::Agent { text, at } => reading.agent(&text, at.as_deref()),
        Entry::Tool { tool, summary, at, failed } => {
            reading.tool(None, &tool, &Value::String(summary), at.as_deref(), failed)
        }
        Entry::Unknown => {}
    }
}
