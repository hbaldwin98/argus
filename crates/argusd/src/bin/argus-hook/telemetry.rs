//! Telemetry: what model an agent runs, how full its context is, what it
//! has spent, and which tool it is in. Harness-neutral on the wire; this
//! module is the adapter for harnesses that report through hook commands.
//!
//! A hook event says what it can. Claude Code, Codex and Cursor name the
//! tool (`tool_name`) and the event (`hook_event_name`); Codex and Cursor
//! name the model; Claude Code and Codex point at a transcript whose tail
//! holds the latest usage. opencode and pi report from their plugins, and
//! anything else can run `argus-hook telemetry` itself.

use std::io::{Seek, SeekFrom};

use argus_protocol::AgentTelemetry;
use serde_json::Value;

use super::*;

/// How much of a transcript's end is read. The latest usage record is near
/// the end, and a long conversation's file is many megabytes.
const TRANSCRIPT_TAIL: u64 = 512 * 1024;

/// `argus-hook telemetry --model M --context N --window N --input N
/// --output N --cost USD --tool NAME | --tool-done`
pub(super) fn telemetry(rest: &[&str]) {
    let report = parse_flags(rest);
    if !report.is_empty() {
        post_telemetry(
            &endpoint_url(&env_url(), Endpoint::Telemetry),
            &env_token(),
            &report,
            None,
        );
    }
}

pub(super) fn parse_flags(rest: &[&str]) -> AgentTelemetry {
    let mut report = AgentTelemetry::default();
    let mut args = rest.iter();
    while let Some(flag) = args.next() {
        let number =
            |value: Option<&&str>| value.and_then(|v| v.replace(['_', ','], "").parse().ok());
        match *flag {
            "--model" => report.model = args.next().map(|v| v.to_string()),
            "--context" => report.context_tokens = number(args.next()),
            "--window" => report.context_window = number(args.next()),
            "--input" => report.input_tokens = number(args.next()),
            "--output" => report.output_tokens = number(args.next()),
            "--cost" => {
                report.cost_usd = args
                    .next()
                    .and_then(|v| v.trim_start_matches('$').parse().ok())
            }
            "--tool" => report.tool = args.next().map(|v| v.to_string()),
            "--tool-done" => report.tool = Some(String::new()),
            _ => {}
        }
    }
    report
}

pub(super) fn post_telemetry(
    url: &str,
    token: &str,
    report: &AgentTelemetry,
    session: Option<&str>,
) {
    if let (Some(base), Ok(body)) = (pane_base(url), serde_json::to_string(report)) {
        let _ = post_as(
            &endpoint_url(&base, Endpoint::Telemetry),
            token,
            &body,
            session,
        );
    }
}

/// Everything a hook event's payload, and the transcript it names, say.
pub(super) fn from_hook(raw: &str) -> AgentTelemetry {
    let Some(v) = json_value(raw) else {
        return AgentTelemetry::default();
    };
    let mut report = v
        .get("transcript_path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(|path| from_transcript(std::path::Path::new(path)))
        .unwrap_or_default();

    if let Some(model) = model_name(v.get("model")) {
        report.model = Some(model);
    }

    let event = v
        .get("hook_event_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let tool = v
        .get("tool_name")
        .and_then(Value::as_str)
        .or_else(|| v.pointer("/toolCall/name").and_then(Value::as_str))
        .map(str::to_string)
        .or_else(|| event.contains("shell").then(|| "shell".to_string()));
    if event.starts_with("post")
        || event.starts_with("after")
        || matches!(
            event.as_str(),
            "stop" | "userpromptsubmit" | "beforesubmitprompt" | "sessionend"
        )
    {
        report.tool = Some(String::new());
    } else if event.starts_with("pre") || event.starts_with("before") {
        report.tool = tool;
    }
    report
}

/// A model named as a string, or as an object carrying an id or name.
fn model_name(value: Option<&Value>) -> Option<String> {
    let value = value?;
    let name = value.as_str().or_else(|| {
        ["id", "display_name", "name"]
            .iter()
            .find_map(|key| value.get(key).and_then(Value::as_str))
    })?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

pub(super) fn from_transcript(path: &std::path::Path) -> AgentTelemetry {
    let Ok(mut file) = std::fs::File::open(path) else {
        return AgentTelemetry::default();
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(TRANSCRIPT_TAIL);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return AgentTelemetry::default();
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return AgentTelemetry::default();
    }
    let text = String::from_utf8_lossy(&buf);
    // A tail read from the middle of a line starts with half of one.
    let lines = text.lines().skip(usize::from(start > 0));
    transcript_lines(lines)
}

/// Reads Claude Code's and Codex's JSONL transcripts. Later records win, so
/// the result is the conversation as it stands now.
pub(super) fn transcript_lines<'a>(lines: impl Iterator<Item = &'a str>) -> AgentTelemetry {
    let mut report = AgentTelemetry::default();
    for line in lines {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match v.get("type").and_then(Value::as_str) {
            // Claude Code: every assistant message carries its model and
            // the usage of the request that produced it.
            Some("assistant") => {
                let Some(message) = v.get("message") else {
                    continue;
                };
                if let Some(model) = model_name(message.get("model")).filter(|m| m != "<synthetic>")
                {
                    report.model = Some(model);
                }
                if let Some(usage) = message.get("usage") {
                    let field = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
                    let context = field("input_tokens")
                        + field("cache_creation_input_tokens")
                        + field("cache_read_input_tokens")
                        + field("output_tokens");
                    if context > 0 {
                        report.context_tokens = Some(context);
                    }
                }
            }
            // Codex: the model is set per turn, and usage arrives as
            // token_count events that also carry the context window.
            Some("turn_context") => {
                if let Some(model) = model_name(v.pointer("/payload/model")) {
                    report.model = Some(model);
                }
            }
            Some("event_msg")
                if v.pointer("/payload/type").and_then(Value::as_str) == Some("token_count") =>
            {
                let Some(info) = v.pointer("/payload/info").filter(|i| !i.is_null()) else {
                    continue;
                };
                if let Some(total) = info
                    .pointer("/last_token_usage/total_tokens")
                    .and_then(Value::as_u64)
                {
                    report.context_tokens = Some(total);
                }
                if let Some(input) = info
                    .pointer("/total_token_usage/input_tokens")
                    .and_then(Value::as_u64)
                {
                    report.input_tokens = Some(input);
                }
                if let Some(output) = info
                    .pointer("/total_token_usage/output_tokens")
                    .and_then(Value::as_u64)
                {
                    report.output_tokens = Some(output);
                }
                if let Some(window) = info.get("model_context_window").and_then(Value::as_u64) {
                    report.context_window = Some(window);
                }
            }
            _ => {}
        }
    }
    report
}
