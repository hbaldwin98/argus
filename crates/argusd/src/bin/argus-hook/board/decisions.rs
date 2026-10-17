//! `argus-hook decisions` and `decide`: the current feature's decision board
//! drawn as the tree it is, and the writes that add, answer, withdraw or
//! rewrite a decision.

use std::io::Read;

use crate::*;

/// This feature's decision board, drawn as the tree it is. Read rather than
/// written, and on stdout for the same reason `context` is: it is what an
/// agent picking up a feature is meant to consult before adding to it.
pub(crate) fn decisions(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", decisions_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

pub(crate) fn decisions_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let args = rest.get(1..).unwrap_or_default();
    match rest.first().copied() {
        Some(verb @ ("withdraw" | "restore")) => {
            return withdraw_or_restore(verb, args, base_url, token)
        }
        Some("resolve") => return resolve_decision(args, base_url, token),
        Some("show") => return show_decision(args, base_url, token),
        Some("rewrite") => return rewrite_decision(args, base_url, token),
        _ => {}
    }
    let board: DecisionBoard = match read::<hook::call::Decisions>("decisions", rest, base_url, token) {
        Ok(board) => board,
        Err(message) => return message,
    };
    if board.decisions.is_empty() {
        return "nothing decided under this feature yet".to_string();
    }
    format_decision_board(&board)
}

fn decision_id(raw: Option<&&str>) -> Option<i64> {
    raw?.trim_start_matches('#').parse::<i64>().ok()
}

/// `decisions withdraw <id>` and `decisions restore <id>`: takes a decision
/// recorded in error back, keeping it in the history, and undoes that.
fn withdraw_or_restore(verb: &str, args: &[&str], base_url: &str, token: &str) -> String {
    let id = match args {
        [id] => decision_id(Some(id)),
        _ => None,
    };
    let Some(id) = id else {
        return format!("could not change decision: {verb} wants the number `decisions` prints");
    };
    let (change, done) = if verb == "withdraw" {
        (DecisionChange::Withdraw { id }, "withdrawn")
    } else {
        (DecisionChange::Restore { id }, "restored")
    };
    match post_decision_change(&change, base_url, token) {
        Ok(()) => format!("decision {id} {done}"),
        Err(message) => message,
    }
}

/// `decisions resolve <id> <answer> [--because <why>]`: answers an open
/// decision with what the person chose.
fn resolve_decision(args: &[&str], base_url: &str, token: &str) -> String {
    let change = match parse_resolve_args(args) {
        Ok(change) => change,
        Err(message) => return format!("could not change decision: {message}"),
    };
    match post_decision_change(&change, base_url, token) {
        Ok(()) => match &change {
            DecisionChange::Resolve { id, answer, .. } => format!("decision {id} resolved: {answer}"),
            _ => "the decision changed".to_string(),
        },
        Err(message) => message,
    }
}

fn parse_resolve_args(args: &[&str]) -> Result<DecisionChange, String> {
    let Some(id) = decision_id(args.first()) else {
        return Err("resolve wants the number `decisions` prints, then the answer".to_string());
    };
    let mut answer: Vec<&str> = Vec::new();
    let mut because = None;
    let mut rest = args[1..].iter().copied();
    while let Some(arg) = rest.next() {
        match arg {
            "--because" => {
                let why = rest
                    .next()
                    .ok_or_else(|| "--because needs something after it".to_string())?;
                because = Some(why.to_string());
            }
            other if other.starts_with("--") => {
                return Err(format!("{other} is not one of --because"))
            }
            word => answer.push(word),
        }
    }
    let answer = answer.join(" ");
    if answer.trim().is_empty() {
        return Err("resolve wants the answer after the number".to_string());
    }
    Ok(DecisionChange::Resolve {
        id,
        answer,
        because,
    })
}

/// `decisions show <id>`: one decision as the document `rewrite` reads
/// back, so correcting one is print, edit, send.
fn show_decision(args: &[&str], base_url: &str, token: &str) -> String {
    let [id] = args else {
        return "could not read decisions: show wants the number `decisions` prints".to_string();
    };
    let Some(id) = decision_id(Some(id)) else {
        return "could not read decisions: show wants the number `decisions` prints".to_string();
    };
    let board: DecisionBoard = match read::<hook::call::Decisions>("decisions", &[], base_url, token) {
        Ok(board) => board,
        Err(message) => return message,
    };
    match board.decisions.iter().find(|d| d.id == id) {
        Some(decision) => DecisionText::from(decision)
            .markdown(decision.state == DecisionState::Open, false)
            .trim_end()
            .to_string(),
        None => format!("could not read decisions: decision {id} is not under this feature"),
    }
}

/// `decisions rewrite <id> --stdin`: replaces what a decision says with the
/// document on standard input, in the shape `show` prints.
fn rewrite_decision(args: &[&str], base_url: &str, token: &str) -> String {
    let id = match args {
        [id, "--stdin"] => decision_id(Some(id)),
        _ => None,
    };
    let Some(id) = id else {
        return "could not change decision: rewrite wants the number `decisions` prints, then \
                --stdin with the document `decisions show` prints"
            .to_string();
    };
    let mut document = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut document) {
        return format!("could not change decision: could not read the document: {e}");
    }
    let text = match DecisionText::from_markdown(&document).checked() {
        Ok(text) => text,
        Err(message) => return format!("could not change decision: {message}"),
    };
    match post_decision_change(&DecisionChange::Rewrite { id, text }, base_url, token) {
        Ok(()) => format!("decision {id} rewritten"),
        Err(message) => message,
    }
}

/// The board the daemon answers with is not read: the change is what was
/// asked, and it landed.
fn post_decision_change(change: &DecisionChange, base_url: &str, token: &str) -> Result<(), String> {
    match call::<hook::call::ChangeDecision>(base_url, token, change) {
        Ok(_) | Err(Refusal::Unreadable) => Ok(()),
        Err(refusal) => Err(refusal.said("could not change decision")),
    }
}

/// Said above a board that holds questions, because a tree read quickly
/// reads as a list of things decided.
pub(super) fn open_question_lead(decisions: &[Decision]) -> Option<String> {
    let open = decisions.iter().filter(|d| d.open()).count();
    (open > 0).then(|| {
        format!(
            "{open} below marked (open) are questions, not decisions: the person's to answer, \
             not yours."
        )
    })
}

fn format_decision_board(board: &DecisionBoard) -> String {
    // Named for the feature rather than the project: the board an agent
    // reads is one feature's, and a heading that said otherwise would
    // invite exactly the project-wide pile this scoping ended.
    let scope = board
        .decisions
        .first()
        .and_then(|d| d.feature.as_deref())
        .and_then(|slug| board.features.iter().find(|f| f.slug == slug))
        .map(|f| f.title.clone())
        .unwrap_or_else(|| board.name.clone());
    let mut lines = vec![format!("Decisions on {scope}, newest last:")];
    lines.extend(open_question_lead(&board.decisions));
    for row in board.tree_rows() {
        push_decision_lines(&mut lines, &row);
    }
    lines.join("\n")
}

pub(super) fn push_decision_lines(
    lines: &mut Vec<String>,
    row: &argus_protocol::DecisionTreeRow<'_>,
) {
    let decision = row.decision;
    // The id leads because it is what the next decision is hung off,
    // and the branch is what says which one that would be under.
    let mut marks = Vec::new();
    // Accepted is what a reader assumes, so only the others are said.
    if decision.state != DecisionState::Accepted {
        marks.push(decision.state.to_string());
    }
    if let Some(by) = decision.superseded_by {
        marks.push(format!("superseded by #{by}"));
    }
    if let Some(task) = decision.task {
        marks.push(format!("under task #{task}"));
    }
    let mark = match marks.is_empty() {
        true => String::new(),
        false => format!("  ({})", marks.join(", ")),
    };
    lines.push(format!(
        "{}#{} {}{mark}",
        decision_branch(row),
        decision.id,
        decision.chose
    ));
    let continuation = decision_continuation(row);
    let mut field = |label: &str, text: &str| {
        // A field written as a paragraph keeps its lines under its label.
        let mut text = text.lines();
        if let Some(first) = text.next() {
            lines.push(format!("{continuation}   {label}{first}"));
        }
        lines.extend(text.map(|line| format!("{continuation}     {line}")));
    };
    if let Some(context) = &decision.context {
        field("context: ", context);
    }
    if let Some(detail) = &decision.detail {
        field("decision: ", detail);
    }
    for option in &decision.options {
        field("option: ", option);
    }
    if let Some(over) = &decision.over {
        field("over: ", over);
    }
    if let Some(because) = &decision.because {
        field("because: ", because);
    }
    for consequence in &decision.consequences {
        field(&format!("{} ", consequence.sign()), &consequence.text);
    }
}

fn decision_branch(row: &argus_protocol::DecisionTreeRow<'_>) -> String {
    if row.depth == 0 {
        return String::new();
    }
    let mut branch = decision_ancestor_guides(row);
    branch.push_str(if row.has_next_sibling {
        "├─ "
    } else {
        "└─ "
    });
    branch
}

fn decision_continuation(row: &argus_protocol::DecisionTreeRow<'_>) -> String {
    let mut continuation = decision_ancestor_guides(row);
    if row.depth > 0 {
        continuation.push_str(if row.has_next_sibling { "│  " } else { "   " });
    }
    continuation.push_str(if row.has_children { "│  " } else { "   " });
    continuation
}

fn decision_ancestor_guides(row: &argus_protocol::DecisionTreeRow<'_>) -> String {
    row.ancestor_continuations
        .iter()
        .map(|continues| if *continues { "│  " } else { "   " })
        .collect()
}

/// Appends one decision. Reports the id it was given, because that id is
/// the only part of the answer the agent has to keep.
pub(crate) fn decide(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", decide_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

pub(crate) fn decide_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let write = match parse_decide_args(rest) {
        Ok(write) => write,
        Err(message) => return format!("could not record the decision: {message}"),
    };
    match call::<hook::call::Decide>(base_url, token, &write) {
        Ok(decision) => {
            let place = match (decision.parent, write.supersedes) {
                (_, Some(old)) => format!(" replacing #{old}"),
                (Some(parent), None) => format!(" under #{parent}"),
                (None, None) => String::new(),
            };
            let id = decision.id;
            match decision.state {
                DecisionState::Accepted => {
                    format!("recorded decision #{id}{place}: {}", decision.chose)
                }
                // Said as a question, with how it ends: an agent that reads
                // "recorded" has been told the matter is closed.
                DecisionState::Open => format!(
                    "asked open decision #{id}{place}: {}\nIt is the person's to answer. When \
                     they have, `argus-hook decisions resolve {id} \"<their answer>\"`.",
                    decision.chose
                ),
                DecisionState::Rejected => {
                    format!("recorded rejected decision #{id}{place}: {}", decision.chose)
                }
            }
        }
        // The write landed; only the account of it did not.
        Err(Refusal::Unreadable) => "recorded the decision".to_string(),
        Err(refusal) => refusal.said("could not record the decision"),
    }
}

/// `decide <what was chosen> [--over <what against>] [--because <why>]
/// [--under <id>] [--supersedes <id>] [--task <id>] [--context <text>]
/// [--detail <text>] [--consequence <text>]... [--rejected]`, or `decide
/// <the question> --open [--option <text>]...` for one not made yet.
///
/// The chosen thing is positional because it is the one field a decision
/// cannot be recorded without, and an agent writing the common case should
/// not have to name it.
pub(crate) fn parse_decide_args(rest: &[&str]) -> Result<DecisionWrite, String> {
    let mut write = DecisionWrite::default();
    let mut chose: Vec<&str> = Vec::new();
    let mut states: Vec<DecisionState> = Vec::new();
    let mut args = rest.iter().copied();
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| format!("{arg} needs something after it"))
        };
        match arg {
            "--over" => write.over = Some(value()?.to_string()),
            "--because" => write.because = Some(value()?.to_string()),
            "--context" => write.context = Some(value()?.to_string()),
            "--detail" => write.detail = Some(value()?.to_string()),
            "--under" => {
                write.under = Some(id_arg(arg, "decision", value()?)?);
            }
            "--supersedes" => {
                write.supersedes = Some(id_arg(arg, "decision", value()?)?);
            }
            "--task" => write.task = Some(id_arg(arg, "task", value()?)?),
            "--option" => write.options.push(value()?.to_string()),
            "--consequence" => {
                let raw = value()?;
                let consequence = Consequence::parse(raw)
                    .ok_or_else(|| format!("--consequence {raw:?} says nothing"))?;
                write.consequences.push(consequence);
            }
            "--open" => states.push(DecisionState::Open),
            "--rejected" => states.push(DecisionState::Rejected),
            other if other.starts_with("--") => {
                return Err(format!(
                    "{other} is not one of --over, --because, --under, --supersedes, --task, \
                     --context, --detail, --consequence, --open, --option, --rejected"
                ))
            }
            word => chose.push(word),
        }
    }
    write.state = match states.as_slice() {
        [] => DecisionState::Accepted,
        [state] => *state,
        _ => return Err("a decision is open or rejected, not both".to_string()),
    };
    write.chose = chose.join(" ");
    write.checked().map_err(str::to_string)
}

fn id_arg(flag: &str, what: &str, raw: &str) -> Result<i64, String> {
    raw.trim_start_matches('#')
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| format!("{flag} wants a {what} number, not {raw:?}"))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    #[test]
    fn withdrawing_a_decision_wants_its_number() {
        let base = "http://127.0.0.1:1/pane/1";
        for args in [&["withdraw"][..], &["withdraw", "first"], &["restore", "1", "2"]] {
            let message = decisions_message(args, base, "t");
            assert!(message.contains("wants the number"), "{args:?}: {message}");
        }
    }

    #[test]
    fn a_question_is_asked_with_its_options_under_the_task_that_raised_it() {
        let write = parse_decide_args(&[
            "which", "store?", "--open", "--option", "sqlite", "--option", "a file per feature",
            "--task", "#12", "--context", "two processes write",
        ])
        .unwrap();
        assert_eq!(write.chose, "which store?");
        assert_eq!(write.state, DecisionState::Open);
        assert_eq!(write.options, ["sqlite", "a file per feature"]);
        assert_eq!(write.task, Some(12));
        assert_eq!(write.context.as_deref(), Some("two processes write"));
    }

    #[test]
    fn a_made_decision_says_what_it_costs_and_buys() {
        let write = parse_decide_args(&[
            "sqlite", "--detail", "one file in WAL mode", "--consequence", "+one transaction",
            "--consequence", "- a migration ladder", "--consequence", "no sign is a gain",
        ])
        .unwrap();
        assert_eq!(write.state, DecisionState::Accepted);
        assert_eq!(write.detail.as_deref(), Some("one file in WAL mode"));
        let read: Vec<_> = write.consequences.iter().map(|c| (c.gain, c.text.as_str())).collect();
        assert_eq!(
            read,
            [(true, "one transaction"), (false, "a migration ladder"), (true, "no sign is a gain")]
        );
        let turned_down = parse_decide_args(&["a file per feature", "--rejected"]).unwrap();
        assert_eq!(turned_down.state, DecisionState::Rejected);
    }

    #[test]
    fn a_decision_flag_that_cannot_mean_anything_is_refused_before_it_travels() {
        for (bad, expected) in [
            (&["x", "--open", "--rejected"][..], "not both"),
            (&["x", "--option", "y"], "only an open decision has options"),
            (&["x", "--open", "--supersedes", "3"], "a question cannot replace"),
            (&["x", "--task", "twelve"], "wants a task number"),
            (&["x", "--consequence", " + "], "says nothing"),
            (&["x", "--option"], "needs something after it"),
        ] {
            let error = parse_decide_args(bad).unwrap_err();
            assert!(error.contains(expected), "{bad:?} gave {error:?}");
        }
    }

    #[test]
    fn resolving_a_decision_wants_its_number_and_then_the_answer() {
        assert_eq!(
            parse_resolve_args(&["#9", "sqlite,", "in", "WAL", "--because", "both need migrations"]),
            Ok(DecisionChange::Resolve {
                id: 9,
                answer: "sqlite, in WAL".into(),
                because: Some("both need migrations".into()),
            })
        );
        for bad in [&[][..], &["sqlite"], &["9"], &["9", "--because", "why"], &["9", "x", "--why", "y"]] {
            assert!(parse_resolve_args(bad).is_err(), "{bad:?}");
        }
        let message = decisions_message(&["resolve"], "http://127.0.0.1:1/pane/1", "t");
        assert!(message.starts_with("could not change decision: resolve wants"), "{message}");
        let message = decisions_message(&["rewrite", "9"], "http://127.0.0.1:1/pane/1", "t");
        assert!(message.contains("--stdin"), "{message}");
    }

    pub(in crate::board) fn decision_board() -> DecisionBoard {
        serde_json::from_str(
            r#"{"project":null,"name":"argus",
                "features":[{"number":7,"slug":"queue","title":"The queue","body":"",
                             "origin_checkout":null,"origin_branch":null,"at":1,"session":null}],
                "decisions":[
                {"id":1,"parent":null,"at":1,"session":null,"checkout":null,"feature":"queue",
                 "chose":"refuse rather than drop","over":"dropping","because":null,
                 "superseded_by":null,"task":12,"context":"a dropped frame is a wrong screen",
                 "detail":"a full queue answers busy\nand the sender retries",
                 "consequences":[{"gain":true,"text":"no torn screens"},
                                 {"gain":false,"text":"senders must retry"}]},
                {"id":2,"parent":1,"at":2,"session":null,"checkout":null,"feature":"queue",
                 "chose":"which bound?","over":null,"because":null,"superseded_by":null,
                 "state":"open","options":["a count","a byte size"]},
                {"id":3,"parent":null,"at":3,"session":null,"checkout":null,"feature":"queue",
                 "chose":"grow without bound","over":null,"because":"memory","superseded_by":null,
                 "state":"rejected"}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn a_board_says_which_decisions_are_questions_and_what_each_was_raised_under() {
        assert_eq!(
            format_decision_board(&decision_board()),
            "Decisions on The queue, newest last:\n\
             1 below marked (open) are questions, not decisions: the person's to answer, not \
             yours.\n\
             #1 refuse rather than drop  (under task #12)\n\
             │     context: a dropped frame is a wrong screen\n\
             │     decision: a full queue answers busy\n\
             │       and the sender retries\n\
             │     over: dropping\n\
             │     + no torn screens\n\
             │     - senders must retry\n\
             └─ #2 which bound?  (open)\n\
             \x20        option: a count\n\
             \x20        option: a byte size\n\
             #3 grow without bound  (rejected)\n\
             \x20     because: memory"
        );
    }
}
