//! `argus-hook reconcile`: reading the reconcile the person asked for,
//! proposing its lines, and reporting what was carried out.
//!
//! The proposal is JSON on stdin, the agent's to write and the daemon's to
//! check, so this wraps it for the `reconcile` endpoint and relays the
//! daemon's refusal, which names the line at fault. The read is laid out
//! for the agent: what was asked, each line by the number a report names,
//! and the exact words of the lines the person ticked.

use std::io::{Read, Write};

use argus_protocol::board::reconcile::Stage;
use argus_protocol::{Failed, Feature, ProposalWrite, ReconcileChange};

use crate::*;

pub(crate) fn reconcile(rest: &[&str]) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", reconcile_message(rest, &env_url(), &env_token(), std::io::stdin()));
}

/// `reconcile`, `reconcile propose --stdin`, `reconcile report [--failed
/// <line> <error>]...`.
fn reconcile_message(rest: &[&str], base_url: &str, token: &str, mut stdin: impl Read) -> String {
    let change = match rest {
        [] => ReconcileChange::Read,
        ["propose", "--stdin"] => {
            let mut body = String::new();
            if let Err(e) = stdin.read_to_string(&mut body) {
                return format!("could not propose: {e}");
            }
            let proposal: ProposalWrite = match serde_json::from_str(&body) {
                Ok(proposal) => proposal,
                Err(e) => return format!("could not propose: not a proposal: {e}"),
            };
            ReconcileChange::Propose { proposal }
        }
        ["propose", ..] => return "could not propose: propose wants --stdin with the proposal's JSON".into(),
        ["report", flags @ ..] => match failures(flags) {
            Ok(failed) => ReconcileChange::Report { failed },
            Err(message) => return format!("could not report: {message}"),
        },
        _ => return "reconcile: use `reconcile`, `reconcile propose --stdin` or `reconcile report`".into(),
    };
    match call::<hook::call::Reconcile>(base_url, token, &change) {
        Ok(feature) => format_reconcile(&feature, &change),
        Err(Refusal::Unreadable) => "done".into(),
        Err(refusal) => refusal.said("could not reach the reconcile"),
    }
}

fn failures(flags: &[&str]) -> Result<Vec<Failed>, String> {
    let mut failed = Vec::new();
    let mut rest = flags;
    while let [flag, tail @ ..] = rest {
        if *flag != "--failed" {
            return Err(format!("`{flag}` is not --failed"));
        }
        let [line, tail @ ..] = tail else {
            return Err("--failed wants the line's number, then what the tracker said".into());
        };
        let line = line.parse().map_err(|_| format!("`{line}` is not a line's number"))?;
        let end = tail.iter().position(|arg| *arg == "--failed").unwrap_or(tail.len());
        failed.push(Failed { line, error: tail[..end].join(" ") });
        rest = &tail[end..];
    }
    Ok(failed)
}

fn format_reconcile(feature: &Feature, change: &ReconcileChange) -> String {
    let mut out = Vec::new();
    let said = match change {
        ReconcileChange::Read => None,
        ReconcileChange::Propose { .. } => Some("Proposed. Every line waits unticked; wait for the person."),
        ReconcileChange::Report { .. } => Some("Reported."),
    };
    out.extend(said.map(str::to_string));
    out.push(format!("#{} {} — reconcile with its tickets", feature.number, feature.title));
    for link in &feature.links {
        let role = if link.role.is_empty() { String::new() } else { format!(" ({})", link.role) };
        out.push(format!("  {} {}{role}  {}", link.tracker, link.key, link.url));
    }
    let Some(reconcile) = &feature.reconcile else {
        out.push("No reconcile is under way.".into());
        if let Some(last) = &feature.reconciled {
            out.push(format!("Last carried out by {} at {}.", last.by, last.at));
        }
        return out.join("\n");
    };
    out.push(format!("The person asked: {}", reconcile.asked));
    match reconcile.stage() {
        Stage::Asked => out.push(format!(
            "{} was asked and has not proposed. Read the map, the feature and the tickets, write nothing, then \
             `argus-hook reconcile propose --stdin`.",
            reconcile.by
        )),
        Stage::Proposed => {
            out.push(format!("Proposed by {}; nothing is ticked yet. Carry out nothing until told.", reconcile.by));
            for (i, line) in reconcile.lines.iter().enumerate() {
                let failed = line.failed.as_ref().map(|e| format!("  [failed: {e}]")).unwrap_or_default();
                out.push(format!("  {i}. {}  {} {} — {}{failed}", line.heading(), line.key, line.does, line.why));
            }
        }
        Stage::Carrying => {
            out.push("Carry out exactly these, in these words, and nothing else; then `argus-hook reconcile report`, \
                      with `--failed <line> \"<what the tracker said>\"` for each that failed:"
                .into());
            for ticked in &reconcile.carrying {
                let Some(line) = reconcile.lines.get(ticked.line) else {
                    continue;
                };
                out.push(format!("  {}. {}  {} {}", ticked.line, line.heading(), line.key, line.does));
                if !ticked.text.is_empty() {
                    out.push("     text, word for word:".into());
                    out.extend(ticked.text.lines().map(|l| format!("       {l}")));
                }
                if !line.call.is_empty() {
                    out.push("     the call:".into());
                    out.extend(line.call.lines().map(|l| format!("       {l}")));
                }
            }
        }
    }
    if !reconcile.noticed.is_empty() {
        out.push("Noticed, not proposed (never act on these):".into());
        out.extend(reconcile.noticed.iter().map(|n| format!("  {}  {}", n.key, n.text)));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::{PaneId, ProposedLine, Reconcile, Ticked};
    use std::io::Cursor;

    const BASE: &str = "http://127.0.0.1:1/pane/1";

    fn carrying() -> Feature {
        let line = ProposedLine {
            to: "Jira".into(),
            key: "ARG-142".into(),
            does: "comment with decision D2".into(),
            call: "Jira add comment\n  issue ARG-142".into(),
            text: "the agent's words".into(),
            ..ProposedLine::default()
        };
        Feature {
            number: 7,
            title: "Trim context injection".into(),
            reconcile: Some(Reconcile {
                asked: "put D2 on ARG-142".into(),
                pane: PaneId(32),
                by: "pane #32".into(),
                at: 0,
                proposed: true,
                lines: vec![line.clone(), line],
                noticed: Vec::new(),
                carrying: vec![Ticked { line: 1, text: "the person's words\nsecond line".into() }],
            }),
            ..Feature::default()
        }
    }

    #[test]
    fn the_ticked_lines_read_in_the_persons_words_and_nothing_else() {
        let read = format_reconcile(&carrying(), &ReconcileChange::Read);
        assert!(read.contains("1. Out to Jira  ARG-142 comment with decision D2"), "{read}");
        assert!(read.contains("       the person's words\n       second line"), "{read}");
        assert!(!read.contains("the agent's words"), "the proposed text gave way to the ticked: {read}");
        assert!(!read.contains("  0. "), "an unticked line is not offered: {read}");
    }

    #[test]
    fn a_report_names_each_failed_line_and_what_the_tracker_said() {
        let failed = failures(&["--failed", "1", "403", "from", "Jira", "--failed", "3", "gone"]).unwrap();
        assert_eq!(failed, [Failed { line: 1, error: "403 from Jira".into() }, Failed { line: 3, error: "gone".into() }]);
        assert!(failures(&["--failed", "one", "x"]).is_err());
        assert!(failures(&["--done"]).is_err());
    }

    #[test]
    fn a_malformed_command_is_refused_before_the_daemon_is_asked() {
        for (args, stdin) in [(&["propose"][..], "{}"), (&["propose", "--stdin"], "not json"), (&["wipe"], "")] {
            let said = reconcile_message(args, BASE, "t", Cursor::new(stdin));
            assert!(!said.contains("unavailable"), "{args:?}: {said}");
        }
    }
}
