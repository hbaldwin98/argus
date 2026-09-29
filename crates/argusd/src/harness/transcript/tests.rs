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

