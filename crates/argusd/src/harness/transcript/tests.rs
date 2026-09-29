use argus_protocol::{Body, Entry, ToolState, Update};

use super::Dialect;

const CLAUDE: &str = include_str!("fixtures/claude.jsonl");

/// Every update a fixture makes, each line named by its number the way the
/// daemon names one by its offset.
fn read_all(dialect: Dialect, fixture: &str) -> Vec<Update> {
    fixture
        .lines()
        .enumerate()
        .flat_map(|(n, line)| dialect.read(line, &format!("0:{n}")))
        .collect()
}

fn entries(updates: &[Update]) -> Vec<&Entry> {
    updates
        .iter()
        .filter_map(|update| match update {
            Update::Upsert(entry) => Some(entry),
            _ => None,
        })
        .collect()
}

#[test]
fn claude_reads_as_the_conversation_a_person_had() {
    let updates = read_all(Dialect::Claude, CLAUDE);
    let bodies: Vec<&Body> = entries(&updates).into_iter().map(|e| &e.body).collect();

    let expected = [
        Body::Prompt { text: "/effort high".into() },
        Body::Notice { text: "Set effort level to high".into() },
        Body::Prompt { text: "Add a README section on the web client.".into() },
        Body::Thinking {
            text: "The README has a usage section; the web client belongs after it.".into(),
        },
        Body::ToolCall {
            tool: "Read".into(),
            summary: "/repo/README.md".into(),
            input: "{\n  \"file_path\": \"/repo/README.md\"\n}".into(),
            state: ToolState::Running,
        },
        Body::ToolResult {
            call: "toolu_read".into(),
            output: "# Argus\n\nA terminal workspace.".into(),
            failed: false,
        },
    ];
    assert_eq!(&bodies[..expected.len()], expected.iter().collect::<Vec<_>>());

    let rest: Vec<String> = bodies[expected.len()..]
        .iter()
        .map(|body| match body {
            Body::ToolCall { tool, summary, .. } => format!("call {tool}: {summary}"),
            Body::ToolResult { failed, output, .. } => format!("result failed={failed}: {output}"),
            Body::Reply { text } => format!("reply: {}", text.lines().next().unwrap_or("")),
            Body::TurnEnd { millis } => format!("turn end {millis:?}"),
            Body::Notice { text } => format!("notice: {text}"),
            Body::Divider { text } => format!("divider: {text}"),
            Body::Prompt { text } => format!("prompt: {text}"),
            Body::Thinking { .. } => "thinking".into(),
        })
        .collect();
    assert_eq!(
        rest,
        [
            "call Bash: Run the workspace tests",
            "result failed=true: error: 1 test failed",
            "prompt: also fix the test",
            "reply: The section is added.",
            "turn end Some(41000)",
            "notice: [Request interrupted by user]",
            "divider: Context compacted",
            "notice: The README section is in; one unrelated test still fails.",
            "prompt: !git status",
        ]
    );
}

#[test]
fn a_claude_tool_call_is_named_by_its_own_id_and_its_result_finishes_it() {
    let updates = read_all(Dialect::Claude, CLAUDE);
    let call = entries(&updates)
        .into_iter()
        .find(|e| matches!(&e.body, Body::ToolCall { tool, .. } if tool == "Bash"))
        .unwrap();
    assert_eq!(call.id, "toolu_bash");
    assert_eq!(call.at.as_deref(), Some("2026-09-29T03:21:05.000Z"));
    assert!(updates.contains(&Update::ToolState {
        id: "toolu_bash".into(),
        state: ToolState::Failed,
    }));
    assert!(updates.contains(&Update::ToolState {
        id: "toolu_read".into(),
        state: ToolState::Done,
    }));
}

#[test]
fn reading_a_claude_line_again_names_its_entries_the_same() {
    let line = CLAUDE.lines().nth(4).unwrap();
    assert_eq!(
        Dialect::Claude.read(line, "0:4"),
        Dialect::Claude.read(line, "0:4")
    );
    let updates = Dialect::Claude.read(line, "0:4");
    let [Update::Upsert(entry)] = updates.as_slice() else {
        panic!("one prompt");
    };
    assert_eq!(entry.id, "0:4.0");
}

#[test]
fn claude_bookkeeping_sidechains_and_what_it_says_for_the_person_are_left_out() {
    let updates = read_all(Dialect::Claude, CLAUDE);
    let text: Vec<String> = entries(&updates)
        .iter()
        .map(|e| format!("{:?}", e.body))
        .collect();
    for absent in ["subagent's own chatter", "Caveat", "continued from a previous"] {
        assert!(
            text.iter().all(|t| !t.contains(absent)),
            "{absent} leaked into {text:#?}"
        );
    }
}

#[test]
fn a_line_that_is_not_json_or_not_known_yields_nothing() {
    assert!(Dialect::Claude.read("not json", "0:0").is_empty());
    assert!(Dialect::Claude
        .read(r#"{"type":"future-record","x":1}"#, "0:0")
        .is_empty());
}

#[test]
fn a_huge_tool_output_is_clipped_before_it_leaves_the_daemon() {
    let output = "x".repeat(100_000);
    let line = serde_json::json!({
        "type": "user",
        "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "content": output}]},
    })
    .to_string();
    let updates = Dialect::Claude.read(&line, "0:0");
    let Some(Update::Upsert(Entry { body: Body::ToolResult { output, .. }, .. })) = updates.last()
    else {
        panic!("a result");
    };
    assert!(output.len() <= argus_protocol::transcript::MAX_TOOL_OUTPUT_BYTES);
}


/// Each entry as one line of text: its kind and what a phone would show.
fn described(updates: &[Update]) -> Vec<String> {
    entries(updates)
        .iter()
        .map(|entry| match &entry.body {
            Body::Prompt { text } => format!("prompt: {text}"),
            Body::Reply { text } => format!("reply: {}", text.lines().next().unwrap_or("")),
            Body::Thinking { text } => format!("thinking: {text}"),
            Body::ToolCall { tool, summary, state, .. } => format!("call {tool} {state:?}: {summary}"),
            Body::ToolResult { output, failed, .. } => format!("result failed={failed}: {output}"),
            Body::Notice { text } => format!("notice: {text}"),
            Body::TurnEnd { millis } => format!("turn end {millis:?}"),
            Body::Divider { text } => format!("divider: {text}"),
        })
        .collect()
}

#[test]
fn codex_reads_its_own_items_and_leaves_the_raw_exchange_out() {
    let updates = read_all(Dialect::Codex, include_str!("fixtures/codex.jsonl"));
    assert_eq!(
        described(&updates),
        [
            "prompt: Add an argus server stop command",
            "thinking: Finding the restart path first",
            "reply: I'll trace the existing server subcommands.",
            "call shell Done: cat .agents/skills/argus/SKILL.md",
            "result failed=false: # Argus\n\nArgus shows this conversation as a pane.",
            "call shell Failed: cargo test",
            "result failed=true: error: 1 test failed",
            "call edit Done: main.rs",
            "reply: `argus server stop` is in.",
            "turn end Some(287000)",
            "notice: Turn interrupted",
        ]
    );
    let prompt = entries(&updates)[0];
    assert_eq!(prompt.id, "u-1", "named by Codex's own item id");
    assert_eq!(prompt.at.as_deref(), Some("2026-09-22T22:56:03.680Z"));
}

#[test]
fn cursor_reads_what_was_asked_and_done_without_its_wrappers() {
    let updates = read_all(Dialect::Cursor, include_str!("fixtures/cursor.jsonl"));
    assert_eq!(
        described(&updates),
        [
            "prompt: What would a sequence diagram feature take?",
            "reply: Exploring how Argus handles features.",
            "call Grep Done: sequence|diagram",
            "call Glob Done: **/*.rs",
            "reply: A diagram needs a store, a view and a hook command.",
            "turn end None",
        ]
    );
}

#[test]
fn pi_reads_messages_and_files_each_result_under_its_call() {
    let updates = read_all(Dialect::Pi, include_str!("fixtures/pi.jsonl"));
    assert_eq!(
        described(&updates),
        [
            "prompt: Add a bank-linking view",
            "thinking: **Reading project guidance**",
            "reply: **Plan**",
            "call read Running: AGENTS.md",
            "call bash Running: rg modal-root",
            "result failed=true: ENOENT: no such file or directory",
            "result failed=false: 263:.modal-root:empty{display:none}",
            "reply: The view is added.",
            "notice: rate limited",
        ]
    );
    assert!(updates.contains(&Update::ToolState {
        id: "call_1".into(),
        state: ToolState::Failed,
    }));
    assert!(updates.contains(&Update::ToolState {
        id: "call_2".into(),
        state: ToolState::Done,
    }));
}

#[test]
fn every_built_in_harness_that_keeps_a_transcript_names_its_dialect() {
    let dialects: Vec<(String, Option<Dialect>)> = crate::harness::Harness::builtins()
        .into_iter()
        .map(|h| (h.name, h.transcript))
        .collect();
    for (name, expected) in [
        ("claude", Some(Dialect::Claude)),
        ("codex", Some(Dialect::Codex)),
        ("agent", Some(Dialect::Cursor)),
        ("pi", Some(Dialect::Pi)),
        ("opencode", None),
        ("agy", None),
        ("generic", None),
    ] {
        let found = dialects.iter().find(|(n, _)| n == name).map(|(_, d)| *d);
        assert_eq!(found, Some(expected), "{name}");
    }
}
