//! Which agent inside a harness session an installed hook's event speaks
//! for, and what the session leaves running when its own turn ends.
//!
//! Claude Code runs subagents, and the agents of a dynamic workflow, inside
//! the session and fires the session's own hooks for them, tagged with the
//! agent's `agent_id`. Reported under the session alone, a subagent's tool
//! call reads as the pane's own turn and its numbers as the pane's
//! telemetry. Reported under a reporter of its own, the daemon lists it as
//! a child of the pane, which is what it is.

use serde_json::Value;

/// What Claude Code calls the agents a dynamic workflow runs. A workflow
/// can run dozens of them, so they share one row rather than taking eight.
const WORKFLOW_AGENT: &str = "workflow-subagent";

/// The child row an event's agent reports on.
#[derive(Debug, PartialEq)]
pub(super) struct Child {
    /// The reporter the daemon files the row under: the session with the
    /// agent appended, so it can never be the pane's own.
    pub reporter: String,
    /// What the row is called, when this event can say.
    pub label: Option<String>,
    /// Whether the row is still at work after an event that reports an
    /// end: one workflow agent finishing is not its workflow finishing.
    pub still_working: bool,
}

/// The child an event speaks for, or `None` for the session's own loop.
pub(super) fn child(event: &Value, session: &str) -> Option<Child> {
    let agent = event.get("agent_id")?.as_str().filter(|id| !id.is_empty())?;
    let kind = event.get("agent_type").and_then(Value::as_str).unwrap_or("");
    let starting = event.get("hook_event_name").and_then(Value::as_str) == Some("SubagentStart");
    if kind == WORKFLOW_AGENT {
        let running = running_workflows(event);
        return Some(Child {
            reporter: workflow_reporter(session),
            label: workflow_label(&running).or_else(|| starting.then(|| "workflow".to_string())),
            still_working: !running.is_empty(),
        });
    }
    Some(Child {
        reporter: format!("{session}/{agent}"),
        label: (starting && !kind.is_empty()).then(|| kind.to_string()),
        still_working: false,
    })
}

/// The workflows still running in the background when this event fired.
///
/// The session's turn ends while a workflow it started keeps going, and
/// says so: `Stop` lists the background work still in flight.
pub(super) fn running_workflows(event: &Value) -> Vec<String> {
    let Some(tasks) = event.get("background_tasks").and_then(Value::as_array) else {
        return Vec::new();
    };
    tasks
        .iter()
        .filter(|task| task.get("type").and_then(Value::as_str) == Some("workflow"))
        .filter(|task| {
            matches!(
                task.get("status").and_then(Value::as_str),
                Some("running" | "pending")
            )
        })
        .map(|task| {
            ["name", "description", "id"]
                .iter()
                .find_map(|key| task.get(*key).and_then(Value::as_str).filter(|s| !s.is_empty()))
                .unwrap_or("workflow")
                .to_string()
        })
        .collect()
}

/// The one reporter every workflow agent in a session shares.
pub(super) fn workflow_reporter(session: &str) -> String {
    format!("{session}/workflows")
}

pub(super) fn workflow_label(running: &[String]) -> Option<String> {
    (!running.is_empty()).then(|| format!("workflow {}", running.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(raw: &str) -> Value {
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn the_sessions_own_loop_is_no_child() {
        let own = event(r#"{"session_id":"s","hook_event_name":"PreToolUse","tool_name":"Bash"}"#);
        assert_eq!(child(&own, "s"), None);
    }

    #[test]
    fn a_subagent_is_a_child_named_after_its_type_when_it_starts() {
        let start = event(
            r#"{"session_id":"s","agent_id":"a1","agent_type":"Explore","hook_event_name":"SubagentStart"}"#,
        );
        assert_eq!(
            child(&start, "s"),
            Some(Child { reporter: "s/a1".into(), label: Some("Explore".into()), still_working: false })
        );
        // Its tool calls land on the same row without renaming it.
        let tool = event(
            r#"{"session_id":"s","agent_id":"a1","agent_type":"Explore","hook_event_name":"PreToolUse"}"#,
        );
        assert_eq!(child(&tool, "s").unwrap().label, None);
        assert_eq!(child(&tool, "s").unwrap().reporter, "s/a1");
    }

    #[test]
    fn every_workflow_agent_shares_one_row() {
        let one = event(r#"{"agent_id":"a1","agent_type":"workflow-subagent","hook_event_name":"SubagentStart"}"#);
        let two = event(r#"{"agent_id":"a2","agent_type":"workflow-subagent","hook_event_name":"PreToolUse"}"#);
        assert_eq!(child(&one, "s").unwrap().reporter, "s/workflows");
        assert_eq!(child(&two, "s").unwrap().reporter, "s/workflows");
        assert_eq!(child(&one, "s").unwrap().label.as_deref(), Some("workflow"));
    }

    #[test]
    fn a_workflow_agent_finishing_leaves_its_running_workflow_at_work() {
        // Recorded from Claude Code 2.1.287: each agent's SubagentStop still
        // lists the workflow as running, and only the wake-up turn's Stop
        // lists nothing.
        let stop = event(
            r#"{"agent_id":"a1","agent_type":"workflow-subagent","hook_event_name":"SubagentStop",
                "background_tasks":[{"id":"wk1","type":"workflow","status":"running","description":"probe hooks","name":"probe"}]}"#,
        );
        assert_eq!(
            child(&stop, "s"),
            Some(Child {
                reporter: "s/workflows".into(),
                label: Some("workflow probe".into()),
                still_working: true,
            })
        );
        let last = event(
            r#"{"agent_id":"a1","agent_type":"workflow-subagent","hook_event_name":"SubagentStop","background_tasks":[]}"#,
        );
        assert!(!child(&last, "s").unwrap().still_working);
    }

    #[test]
    fn only_running_workflows_are_background_work_worth_a_row() {
        // A background shell can run for as long as the session does, so
        // listing it would keep a row working forever.
        let stop = event(
            r#"{"hook_event_name":"Stop","background_tasks":[
                {"id":"b1","type":"shell","status":"running","description":"npm run dev"},
                {"id":"w1","type":"workflow","status":"completed","name":"old"},
                {"id":"w2","type":"workflow","status":"pending","description":"audit"},
                {"id":"w3","type":"workflow","status":"running","name":"probe"}]}"#,
        );
        assert_eq!(running_workflows(&stop), vec!["audit".to_string(), "probe".to_string()]);
        assert!(running_workflows(&event(r#"{"hook_event_name":"Stop"}"#)).is_empty());
    }
}
