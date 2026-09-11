//! The shared work an agent reads and writes — review comments, the
//! feature it is on, that feature's tasks, and the decision board — each
//! parsed from arguments and formatted for the model to read.

use super::*;

pub(super) fn comments(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", comments_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

pub(super) fn comments_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let comments: Vec<ReviewComment> =
        match read_json("comments", Endpoint::Comments, rest, base_url, token) {
            Ok(comments) => comments,
            Err(message) => return message,
        };
    if comments.is_empty() {
        return "no review comments".to_string();
    }
    comments
        .iter()
        .map(|comment| {
            format!(
                "#{} [{}] {}",
                comment.id,
                comment.anchor.base.label(),
                comment.anchor.notification(&comment.body)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The feature this checkout is working on: what it is for, and what has
/// been decided under it.
///
/// One command rather than four because they are one thing from the
/// agent's side — where am I, and how do I say where I am. Bare, it
/// answers the first; `list`, `open`, `use` and `note` answer the second.
pub(super) fn feature(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", feature_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

pub(super) fn feature_message(rest: &[&str], base_url: &str, token: &str) -> String {
    match rest.first().copied() {
        None => match read_feature_board(&[], base_url, token) {
            Ok(board) => format_feature(&board),
            Err(message) => message,
        },
        Some("list") => match read_feature_board(&rest[1..], base_url, token) {
            Ok(board) => format_feature_list(&board),
            Err(message) => message,
        },
        Some("open") => {
            let title = rest[1..]
                .iter()
                .take_while(|a| **a != "--body")
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
            let body = rest[1..]
                .iter()
                .position(|a| *a == "--body")
                .map(|at| rest[2 + at..].join(" "));
            write_feature(
                FeatureAction::Open(FeatureWrite { title, body }),
                base_url,
                token,
            )
        }
        Some("use") => match rest.get(1) {
            Some(slug) => write_feature(
                FeatureAction::Select {
                    slug: (*slug).to_string(),
                },
                base_url,
                token,
            ),
            None => "could not change feature: use wants the slug of a feature".to_string(),
        },
        Some("note") => write_feature(
            FeatureAction::Append {
                text: rest[1..].join(" "),
            },
            base_url,
            token,
        ),
        Some("export") => match read_feature_board(&rest[1..], base_url, token) {
            Ok(board) => format_export(&board),
            Err(message) => message,
        },
        Some(other) => format!(
            "{other} is not one of list, open, use, note, export — \
             `argus-hook feature use {other}` works on an existing feature"
        ),
    }
}

pub(super) fn task(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", task_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

/// `task`, `task add`, `task doing <id>`, `task done <id>`, `task todo
/// <id>`, `task retitle <id> <text>`, `task brief <id> <text>`, `task drop <id>`.
///
/// The columns are named as verbs rather than hidden behind a `move`, so
/// what an agent types is what a reader of the transcript understands
/// happened.
pub(super) fn task_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let by_id =
        |verb: &str, state: TaskState| match rest.get(1).and_then(|id| id.parse::<i64>().ok()) {
            Some(id) => write_task(TaskAction::Move { id, state }, base_url, token),
            None => format!("could not change task: {verb} wants the number `task` prints"),
        };
    match rest.first().copied() {
        None | Some("list") => write_task(TaskAction::List, base_url, token),
        Some("add") => {
            // A trailing `--key ORION-412` is the tracker's own id, kept
            // so an agent can reconcile later. Argus never reads it.
            let words: Vec<&str> = rest[1..]
                .iter()
                .take_while(|a| **a != "--key")
                .copied()
                .collect();
            let external = rest[1..]
                .iter()
                .position(|a| *a == "--key")
                .map(|at| rest[2 + at..].join(" "));
            write_task(
                TaskAction::Add(TaskWrite {
                    title: words.join(" "),
                    external,
                }),
                base_url,
                token,
            )
        }
        Some("doing") => by_id("doing", TaskState::Doing),
        Some("done") => by_id("done", TaskState::Done),
        Some("todo") => by_id("todo", TaskState::Todo),
        Some("retitle") => match rest.get(1).and_then(|id| id.parse::<i64>().ok()) {
            Some(id) => write_task(
                TaskAction::Retitle {
                    id,
                    title: rest[2..].join(" "),
                },
                base_url,
                token,
            ),
            None => "could not change task: retitle wants the number `task` prints".to_string(),
        },
        Some("brief") => match rest.get(1).and_then(|id| id.parse::<i64>().ok()) {
            Some(id) => write_task(
                TaskAction::SetBody {
                    id,
                    body: rest[2..].join(" "),
                },
                base_url,
                token,
            ),
            None => "could not change task: brief wants the number `task` prints".to_string(),
        },
        Some("drop") => match rest.get(1).and_then(|id| id.parse::<i64>().ok()) {
            Some(id) => write_task(TaskAction::Remove { id }, base_url, token),
            None => "could not change task: drop wants the number `task` prints".to_string(),
        },
        Some(other) => format!(
            "could not change task: `{other}` is not one of add, doing, done, todo, retitle, brief, drop"
        ),
    }
}

pub(super) fn write_task(action: TaskAction, base_url: &str, token: &str) -> String {
    let Ok(body) = serde_json::to_string(&action) else {
        return "could not change task: unencodable".to_string();
    };
    let url = endpoint_url(base_url, Endpoint::Tasks);
    let Some((status, response)) = post_response(&url, token, &body) else {
        return "could not change task: daemon unavailable".to_string();
    };
    let response = response.trim();
    if status != 200 {
        return if response.is_empty() {
            "could not change task: daemon refused the request".to_string()
        } else {
            format!("could not change task: {response}")
        };
    }
    match serde_json::from_str::<TaskList>(response) {
        Ok(list) => format_tasks(&list),
        Err(_) => "the task list changed".to_string(),
    }
}

/// The list as an agent reads it: the number it needs to name a task, the
/// column it is in, and the tracker key if it came from one.
pub(super) fn format_tasks(list: &TaskList) -> String {
    let Some(feature) = &list.feature else {
        return "this checkout is not on a feature, so it has no tasks. \
                `argus-hook feature` says where it is."
            .to_string();
    };
    if list.tasks.is_empty() {
        return format!(
            "Nothing to do under {feature} yet. Add the first with \
             `argus-hook task add \"<what to do>\"`."
        );
    }
    let mut lines = vec![format!("Tasks under {feature}:")];
    for task in &list.tasks {
        let key = match &task.external {
            Some(key) => format!(" [{key}]"),
            None => String::new(),
        };
        let who = match (&task.state, &task.claimed_by) {
            (TaskState::Doing, Some(session)) => format!(" — {session}"),
            _ => String::new(),
        };
        lines.push(format!(
            "#{:<3} {:<5} {}{key}{who}",
            task.id, task.state, task.title
        ));
        if let Some(body) = &task.body {
            lines.extend(body.lines().map(|line| format!("      {line}")));
        }
    }
    lines.join("\n")
}

pub(super) fn read_feature_board(rest: &[&str], base_url: &str, token: &str) -> Result<FeatureBoard, String> {
    read_json("the feature", Endpoint::Features, rest, base_url, token)
}

pub(super) fn write_feature(action: FeatureAction, base_url: &str, token: &str) -> String {
    let Ok(body) = serde_json::to_string(&action) else {
        return "could not change feature: unencodable".to_string();
    };
    let url = endpoint_url(base_url, Endpoint::Feature);
    let Some((status, response)) = post_response(&url, token, &body) else {
        return "could not change feature: daemon unavailable".to_string();
    };
    let response = response.trim();
    if status != 200 {
        return if response.is_empty() {
            "could not change feature: daemon refused the request".to_string()
        } else {
            format!("could not change feature: {response}")
        };
    }
    match serde_json::from_str::<FeatureBoard>(response) {
        Ok(board) => format_feature(&board),
        // The change landed; only the account of it did not.
        Err(_) => "the feature changed".to_string(),
    }
}

/// What the agent is meant to read before it starts: the brief, then the
/// reasoning underneath it. Deliberately one answer — a decision without
/// what the feature is for explains half of itself.
pub(super) fn format_feature(board: &FeatureBoard) -> String {
    let Some(current) = board
        .current
        .as_ref()
        .and_then(|slug| board.features.iter().find(|f| &f.slug == slug))
    else {
        return no_feature_here(board);
    };
    let mut lines = vec![format!("Feature: {} ({})", current.title, current.slug)];
    if let Some(branch) = &current.origin_branch {
        lines.push(format!("Started on {branch}."));
    }
    if !current.body.trim().is_empty() {
        lines.push(String::new());
        lines.push(current.body.trim().to_string());
    }
    lines.push(String::new());
    if board.decisions.is_empty() {
        lines.push("Nothing decided under it yet.".to_string());
    } else {
        lines.push("Decided under it, newest last:".to_string());
        let tree = DecisionBoard {
            project: board.project,
            name: board.project_name.clone(),
            features: board.features.clone(),
            decisions: board.decisions.clone(),
        };
        for row in tree.tree_rows() {
            push_decision_lines(&mut lines, &row);
        }
    }
    lines.join("\n")
}

/// The answer that has to teach, because it is what an agent hits first on
/// a checkout nobody has scoped yet.
pub(super) fn no_feature_here(board: &FeatureBoard) -> String {
    let mut lines = vec![
        "This checkout is not on a feature yet, so there is nothing to decide under.".to_string(),
    ];
    if board.features.is_empty() {
        lines.push(
            "Open one with `argus-hook feature open \"<title>\"` when you know what you are \
             building."
                .to_string(),
        );
    } else {
        lines
            .push("Open one with `argus-hook feature open \"<title>\"`, or work on one of:".into());
        for feature in &board.features {
            lines.push(format!("  {} — {}", feature.slug, feature.title));
        }
        lines.push("with `argus-hook feature use <slug>`.".to_string());
    }
    if board.unfiled > 0 {
        lines.push(format!(
            "({} older decision(s) predate features and are on no board.)",
            board.unfiled
        ));
    }
    lines.join("\n")
}

pub(super) fn format_feature_list(board: &FeatureBoard) -> String {
    if board.features.is_empty() {
        return format!("no features on {} yet", board.project_name);
    }
    let mut lines = vec![format!("Features of {}:", board.project_name)];
    for feature in &board.features {
        let here = if board.current.as_deref() == Some(feature.slug.as_str()) {
            " (this checkout)"
        } else {
            ""
        };
        let branch = feature
            .origin_branch
            .as_deref()
            .map(|b| format!(" [{b}]"))
            .unwrap_or_default();
        // How its tasks stand, and whether it has been accepted. An agent
        // picking one off this list needs to know it is not choosing work
        // somebody has already finished.
        let mut about = Vec::new();
        if feature.state == argus_protocol::FeatureState::Done {
            about.push("done".to_string());
        }
        if feature.tasks.total() > 0 {
            about.push(format!(
                "{}/{} tasks",
                feature.tasks.done,
                feature.tasks.total()
            ));
        }
        let about = match about.is_empty() {
            true => String::new(),
            false => format!(" ({})", about.join(", ")),
        };
        lines.push(format!(
            "  {} — {}{branch}{about}{here}",
            feature.slug, feature.title
        ));
    }
    lines.join("\n")
}

/// The whole feature handed to the agent as something to write up.
///
/// Deliberately not a rendered document. Argus can only reorder what the
/// board already holds, and a template that did would produce the board
/// again with more punctuation. What a reader actually wants out of a
/// decision tree is prose that says why it has this shape, and the only
/// thing in the room that can write that is the model reading this. So
/// the command prints the commission and then the material, and the
/// writing happens where the judgement is.
pub(super) fn format_export(board: &FeatureBoard) -> String {
    if board.current.is_none() {
        return no_feature_here(board);
    }
    format!(
        "{EXPORT_BRIEF}

--- the material ---

{}",
        format_feature(board)
    )
}

const EXPORT_BRIEF: &str = "Write this feature up as a document somebody can read end to end: what was built, and why it has the shape it does. Everything below is the whole record — the brief, then every decision taken under it, in the tree they were recorded in. Nothing else about this feature is written down.

How to write it:
  - Open with what the feature is and where it stands, in a paragraph or two. Someone who has never seen this code should be able to stop there and still know what it is.
  - Then follow the tree. A decision's children are the choices it forced, so they belong under it, and the shape of the tree is the shape of the argument.
  - `over` is the road not taken, and it is usually the more interesting half. A decision that names one is only half explained until you say what the alternative was and what ruled it out.
  - A node marked superseded was replaced by the decision it names. It is what was believed at the time, not a mistake to tidy away — say what it was and what changed.
  - Write prose, in the register of the material. Bullets that restate chose/over/because are the board again, and the board can already be read.
  - Do not invent a reason a decision does not give. `the board does not say` is a better sentence than a plausible one you made up.

Save it where the human asked for it, and if they did not say, as a markdown file named after the feature in this checkout.";

/// This feature's decision board, drawn as the tree it is. Read rather than
/// written, and on stdout for the same reason `context` is: it is what an
/// agent picking up a feature is meant to consult before adding to it.
pub(super) fn decisions(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", decisions_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

pub(super) fn decisions_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let board: DecisionBoard =
        match read_json("decisions", Endpoint::Decisions, rest, base_url, token) {
            Ok(board) => board,
            Err(message) => return message,
        };
    if board.decisions.is_empty() {
        return "nothing decided under this feature yet".to_string();
    }
    format_decision_board(&board)
}

pub(super) fn format_decision_board(board: &DecisionBoard) -> String {
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
    for row in board.tree_rows() {
        push_decision_lines(&mut lines, &row);
    }
    lines.join("\n")
}

pub(super) fn push_decision_lines(lines: &mut Vec<String>, row: &argus_protocol::DecisionTreeRow<'_>) {
    let decision = row.decision;
    // The id leads because it is what the next decision is hung off,
    // and the branch is what says which one that would be under.
    let mark = match decision.superseded_by {
        Some(by) => format!("  (superseded by #{by})"),
        None => String::new(),
    };
    lines.push(format!(
        "{}#{} {}{mark}",
        decision_branch(row),
        decision.id,
        decision.chose
    ));
    let continuation = decision_continuation(row);
    if let Some(over) = &decision.over {
        lines.push(format!("{continuation}   over: {over}"));
    }
    if let Some(because) = &decision.because {
        lines.push(format!("{continuation}   because: {because}"));
    }
}

pub(super) fn decision_branch(row: &argus_protocol::DecisionTreeRow<'_>) -> String {
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

pub(super) fn decision_continuation(row: &argus_protocol::DecisionTreeRow<'_>) -> String {
    let mut continuation = decision_ancestor_guides(row);
    if row.depth > 0 {
        continuation.push_str(if row.has_next_sibling { "│  " } else { "   " });
    }
    continuation.push_str(if row.has_children { "│  " } else { "   " });
    continuation
}

pub(super) fn decision_ancestor_guides(row: &argus_protocol::DecisionTreeRow<'_>) -> String {
    row.ancestor_continuations
        .iter()
        .map(|continues| if *continues { "│  " } else { "   " })
        .collect()
}

/// Appends one decision. Reports the id it was given, because that id is
/// the only part of the answer the agent has to keep.
pub(super) fn decide(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", decide_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

pub(super) fn decide_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let write = match parse_decide_args(rest) {
        Ok(write) => write,
        Err(message) => return format!("could not record the decision: {message}"),
    };
    let body = match serde_json::to_string(&write) {
        Ok(body) => body,
        Err(_) => return "could not record the decision: unencodable".to_string(),
    };
    let url = endpoint_url(base_url, Endpoint::Decide);
    let Some((status, response)) = post_response(&url, token, &body) else {
        return "could not record the decision: daemon unavailable".to_string();
    };
    let response = response.trim();
    if status != 200 {
        return if response.is_empty() {
            "could not record the decision: daemon refused the request".to_string()
        } else {
            format!("could not record the decision: {response}")
        };
    }
    match serde_json::from_str::<Decision>(response) {
        Ok(decision) => {
            let place = match (decision.parent, write.supersedes) {
                (_, Some(old)) => format!(" replacing #{old}"),
                (Some(parent), None) => format!(" under #{parent}"),
                (None, None) => String::new(),
            };
            format!(
                "recorded decision #{}{place}: {}",
                decision.id, decision.chose
            )
        }
        // The write landed; only the account of it did not.
        Err(_) => "recorded the decision".to_string(),
    }
}

/// `decide <what was chosen> [--over <what against>] [--because <why>]
/// [--under <id>] [--supersedes <id>]`.
///
/// The chosen thing is positional because it is the one field a decision
/// cannot be recorded without, and an agent writing the common case should
/// not have to name it.
pub(super) fn parse_decide_args(rest: &[&str]) -> Result<DecisionWrite, String> {
    let mut write = DecisionWrite::default();
    let mut chose: Vec<&str> = Vec::new();
    let mut args = rest.iter().copied();
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| format!("{arg} needs something after it"))
        };
        match arg {
            "--over" => write.over = Some(value()?.to_string()),
            "--because" => write.because = Some(value()?.to_string()),
            "--under" => {
                write.under = Some(id_arg(arg, value()?)?);
            }
            "--supersedes" => {
                write.supersedes = Some(id_arg(arg, value()?)?);
            }
            other if other.starts_with("--") => {
                return Err(format!(
                    "{other} is not one of --over, --because, --under, --supersedes"
                ))
            }
            word => chose.push(word),
        }
    }
    write.chose = chose.join(" ");
    write.checked().map_err(str::to_string)
}

pub(super) fn id_arg(flag: &str, raw: &str) -> Result<i64, String> {
    raw.parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| format!("{flag} wants a decision number, not {raw:?}"))
}

/// One JSON read from the pane API, with every way it can fail phrased for
/// the agent that ran the command rather than for a log.
pub(super) fn read_json<T: serde::de::DeserializeOwned>(
    what: &str,
    endpoint: Endpoint,
    rest: &[&str],
    base_url: &str,
    token: &str,
) -> Result<T, String> {
    if !rest.is_empty() {
        return Err(format!("could not read {what}: {what} takes no arguments"));
    }
    let Some((status, body)) = post_response(&endpoint_url(base_url, endpoint), token, "") else {
        return Err(format!("could not read {what}: daemon unavailable"));
    };
    if status != 200 {
        let reason = body.trim();
        return Err(if reason.is_empty() {
            format!("could not read {what}: daemon refused the request")
        } else {
            format!("could not read {what}: {reason}")
        });
    }
    serde_json::from_str(&body)
        .map_err(|_| format!("could not read {what}: invalid daemon response"))
}
