use std::path::{Path, PathBuf};

use argus_protocol::transcript::{Conversation, Entry, Transcript, MAX_ENTRIES, MAX_TEXT_BYTES};

use super::{conversation, read, Format, Reading, Source};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/harness/transcript/fixtures")
        .join(name)
}

fn read_fixture(format: Format, name: &str) -> Transcript {
    read(format, &fixture(name)).unwrap()
}

/// Each entry as the one line a preview would show of it.
fn described(transcript: &Transcript) -> Vec<String> {
    transcript
        .entries
        .iter()
        .map(|entry| match entry {
            Entry::User { text, .. } => format!("user: {text}"),
            Entry::Agent { text, .. } => format!("agent: {}", text.lines().next().unwrap_or("")),
            Entry::Tool { tool, summary, failed: false, .. } => format!("tool {tool}: {summary}"),
            Entry::Tool { tool, summary, failed: true, .. } => format!("tool {tool} failed: {summary}"),
        })
        .collect()
}

#[test]
fn claude_reads_as_the_conversation_a_person_had() {
    let transcript = read_fixture(Format::Claude, "claude.jsonl");
    assert_eq!(
        described(&transcript),
        [
            "user: /effort high",
            "user: Add a README section on the web client.",
            "tool Read: /repo/README.md",
            "tool Bash failed: Run the workspace tests",
            "user: also fix the test",
            "agent: The section is added.",
            "user: !git status",
        ]
    );
    let meta = &transcript.meta;
    assert_eq!(meta.title.as_deref(), Some("README web client section"));
    assert_eq!(meta.started.as_deref(), Some("2026-09-29T03:20:00.000Z"));
    assert_eq!(meta.ended.as_deref(), Some("2026-09-29T03:41:00.000Z"));
    assert_eq!(meta.turns, 4);
    assert_eq!(meta.cwd.as_deref(), Some("/repo"));
}

#[test]
fn claude_bookkeeping_sidechains_and_what_it_says_for_the_person_are_left_out() {
    let text = format!("{:?}", read_fixture(Format::Claude, "claude.jsonl").entries);
    for absent in ["subagent's own chatter", "Caveat", "continued from a previous", "Set effort", "interrupted"] {
        assert!(!text.contains(absent), "{absent} leaked into {text}");
    }
}

#[test]
fn codex_reads_its_own_items_and_leaves_the_raw_exchange_out() {
    let transcript = read_fixture(Format::Codex, "codex.jsonl");
    assert_eq!(
        described(&transcript),
        [
            "user: Add an argus server stop command",
            "agent: I'll trace the existing server subcommands.",
            "tool shell: cat .agents/skills/argus/SKILL.md",
            "tool shell failed: cargo test",
            "tool edit: main.rs",
            "agent: `argus server stop` is in.",
        ]
    );
    let meta = &transcript.meta;
    assert_eq!(meta.title.as_deref(), Some("Add an argus server stop command"));
    assert_eq!(meta.started.as_deref(), Some("2026-09-22T22:55:44.000Z"));
    assert_eq!(meta.ended.as_deref(), Some("2026-09-22T23:01:10.000Z"));
    assert_eq!(meta.turns, 1);
    assert_eq!(meta.cwd.as_deref(), Some("/repo"));
}

#[test]
fn cursor_reads_what_was_asked_and_done_without_its_wrappers() {
    let transcript = read_fixture(Format::Cursor, "cursor.jsonl");
    assert_eq!(
        described(&transcript),
        [
            "user: What would a sequence diagram feature take?",
            "agent: Exploring how Argus handles features.",
            "tool Grep: sequence|diagram",
            "tool Glob: **/*.rs",
            "agent: A diagram needs a store, a view and a hook command.",
        ]
    );
    // Cursor writes no times; the session list has Argus's own.
    assert_eq!(transcript.meta.started, None);
    assert_eq!(transcript.meta.turns, 1);
}

#[test]
fn pi_reads_messages_and_marks_a_call_its_result_says_failed() {
    let transcript = read_fixture(Format::Pi, "pi.jsonl");
    assert_eq!(
        described(&transcript),
        [
            "user: Add a bank-linking view",
            "agent: **Plan**",
            "tool read failed: AGENTS.md",
            "tool bash: rg modal-root",
            "agent: The view is added.",
        ]
    );
    let meta = &transcript.meta;
    assert_eq!(meta.title.as_deref(), Some("Bank linking"));
    assert_eq!(meta.started.as_deref(), Some("2026-09-23T13:55:31.775Z"));
    assert_eq!(meta.ended.as_deref(), Some("2026-09-23T13:57:00.000Z"));
    assert_eq!(meta.cwd.as_deref(), Some("/repo"));
}

#[test]
fn the_neutral_format_reads_entries_and_a_title_and_skips_a_kind_it_does_not_know() {
    let transcript = read_fixture(Format::Neutral, "neutral.jsonl");
    assert_eq!(
        described(&transcript),
        [
            "user: Tidy the release notes",
            "tool read: CHANGELOG.md",
            "tool shell failed: npm run lint",
            "agent: The notes are tidied.",
        ]
    );
    assert_eq!(transcript.meta.title.as_deref(), Some("Release notes"));
    assert_eq!(transcript.meta.started.as_deref(), Some("2026-10-14T09:00:00Z"));
    assert_eq!(transcript.meta.ended.as_deref(), Some("2026-10-14T09:02:00Z"));
}

#[test]
fn what_argus_hook_would_write_reads_back_as_the_same_entries() {
    let entries = vec![
        Entry::User { text: "hi".into(), at: Some("2026-10-14T09:00:00Z".into()) },
        Entry::Tool { tool: "bash".into(), summary: "ls".into(), at: None, failed: true },
        Entry::Agent { text: "done".into(), at: None },
    ];
    let mut reading = Reading::default();
    for entry in &entries {
        reading.line(Format::Neutral, &serde_json::to_string(entry).unwrap());
    }
    assert_eq!(reading.finish().entries, entries);
}

#[test]
fn a_line_that_is_not_json_or_not_known_reads_as_nothing() {
    for format in [Format::Claude, Format::Codex, Format::Cursor, Format::Pi, Format::Neutral] {
        let mut reading = Reading::default();
        reading.line(format, "not json");
        reading.line(format, r#"{"type":"future-record","x":1}"#);
        assert_eq!(reading.finish(), Transcript::default(), "{format:?}");
    }
}

#[test]
fn a_huge_reply_is_clipped_and_a_long_session_keeps_its_newest_entries() {
    let mut reading = Reading::default();
    reading.agent(&"x".repeat(MAX_TEXT_BYTES * 4), None);
    for n in 0..MAX_ENTRIES + 10 {
        reading.user(&format!("turn {n}"), None);
    }
    let transcript = reading.finish();
    assert_eq!(transcript.entries.len(), MAX_ENTRIES);
    assert_eq!(transcript.earlier, 11);
    assert_eq!(transcript.meta.turns as usize, MAX_ENTRIES + 10);
    assert_eq!(transcript.meta.title.as_deref(), Some("turn 0"));
    let Some(Entry::User { text, .. }) = transcript.entries.last() else { panic!("a turn") };
    assert_eq!(text, &format!("turn {}", MAX_ENTRIES + 9));

    let mut reading = Reading::default();
    reading.agent(&"x".repeat(MAX_TEXT_BYTES * 4), None);
    let Some(Entry::Agent { text, .. }) = reading.finish().entries.pop() else { panic!("a reply") };
    assert!(text.len() <= MAX_TEXT_BYTES);
}

#[test]
fn a_harness_that_keeps_no_transcript_says_so() {
    assert_eq!(conversation(None, "s1", Path::new("/c")), Conversation::NoTranscript);
}

#[test]
fn a_transcript_is_read_from_where_its_harness_keeps_it_or_said_to_be_missing() {
    let checkout = tempfile::tempdir().unwrap();
    let dir = checkout.path().join(".herdr").join("sessions");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(fixture("neutral.jsonl"), dir.join("s1.jsonl")).unwrap();
    let source = Source {
        format: Format::Neutral,
        paths: vec!["{checkout}/.herdr/*/{session_id}.jsonl".into()],
    };

    let Conversation::Read(transcript) = conversation(Some(&source), "s1", checkout.path()) else {
        panic!("read");
    };
    assert_eq!(transcript.meta.title.as_deref(), Some("Release notes"));
    assert_eq!(conversation(Some(&source), "s2", checkout.path()), Conversation::Missing);
}

#[test]
fn every_built_in_harness_that_keeps_a_transcript_names_its_format() {
    let formats: Vec<(String, Option<Format>)> = crate::harness::Harness::builtins()
        .into_iter()
        .map(|h| (h.name, h.transcript.map(|source| source.format)))
        .collect();
    for (name, expected) in [
        ("claude", Some(Format::Claude)),
        ("codex", Some(Format::Codex)),
        ("agent", Some(Format::Cursor)),
        ("pi", Some(Format::Pi)),
        ("opencode", None),
        ("agy", None),
        ("generic", None),
    ] {
        let found = formats.iter().find(|(n, _)| n == name).map(|(_, f)| *f);
        assert_eq!(found, Some(expected), "{name}");
    }
}
