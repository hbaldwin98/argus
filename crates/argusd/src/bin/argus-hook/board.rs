//! The shared work an agent reads and writes — review comments, the
//! feature it is on, that feature's tasks, and the decision board — each
//! parsed from arguments and formatted for the model to read.

use std::io::Read;

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
        Some(verb @ ("done" | "reopen")) => {
            let Some(slug) = rest.get(1) else {
                return format!("could not change feature: {verb} wants the slug of a feature");
            };
            let (action, said) = if verb == "done" {
                (FeatureAction::Done { slug: slug.to_string() }, "accepted as done")
            } else {
                (FeatureAction::Reopen { slug: slug.to_string() }, "open again")
            };
            let answer = write_feature(action, base_url, token);
            if answer.starts_with("could not") {
                answer
            } else {
                format!("{slug} is {said}")
            }
        }
        Some(verb @ ("retitle" | "brief")) => {
            let (Some(slug), text) = (rest.get(1), rest.get(2..).unwrap_or_default().join(" "))
            else {
                return format!("could not change feature: {verb} wants a slug, then the text");
            };
            if text.trim().is_empty() {
                return format!("could not change feature: {verb} wants the text after the slug");
            }
            let slug = slug.to_string();
            let action = if verb == "retitle" {
                FeatureAction::Retitle { slug, title: text }
            } else {
                FeatureAction::Rewrite { slug, body: text }
            };
            write_feature(action, base_url, token)
        }
        Some("drop") => {
            let Some(slug) = rest.get(1) else {
                return "could not change feature: drop wants the slug of a feature".to_string();
            };
            let answer = write_feature(
                FeatureAction::Drop {
                    slug: slug.to_string(),
                },
                base_url,
                token,
            );
            if answer.starts_with("could not") {
                answer
            } else {
                format!("{slug} is gone")
            }
        }
        Some("hold") => {
            let (Some(slug), reason) = (rest.get(1), rest.get(2..).unwrap_or_default().join(" "))
            else {
                return "could not change feature: hold wants a slug, then why".to_string();
            };
            if reason.trim().is_empty() {
                return "could not change feature: hold wants why after the slug".to_string();
            }
            let action = FeatureAction::Hold {
                slug: slug.to_string(),
                reason: reason.clone(),
            };
            confirmed(write_feature(action, base_url, token), || {
                format!("{slug} is held: {}", reason.trim())
            })
        }
        Some("unhold") => {
            let Some(slug) = rest.get(1) else {
                return "could not change feature: unhold wants the slug of a feature".to_string();
            };
            let action = FeatureAction::Unhold {
                slug: slug.to_string(),
            };
            confirmed(write_feature(action, base_url, token), || {
                format!("{slug} is no longer held")
            })
        }
        Some(verb @ ("wait" | "unwait")) => {
            let (Some(slug), Some(on)) = (rest.get(1), rest.get(2)) else {
                return format!(
                    "could not change feature: {verb} wants the waiting feature's slug, then the \
                     one it waits on"
                );
            };
            let (slug, on) = (slug.to_string(), on.to_string());
            let (action, said) = if verb == "wait" {
                let said = format!("{slug} waits on {on} until it is done");
                (FeatureAction::Wait { slug, on }, said)
            } else {
                let said = format!("{slug} no longer waits on {on}");
                (FeatureAction::Unwait { slug, on }, said)
            };
            confirmed(write_feature(action, base_url, token), || said)
        }
        Some(other) => format!(
            "{other} is not one of list, open, use, note, export, done, reopen, retitle, brief, \
             drop, hold, unhold, wait, unwait — `argus-hook feature use {other}` works on an \
             existing feature"
        ),
    }
}

/// The daemon's answer when it refused, or else what the change did in a
/// line: the whole board again says nothing about the one row that moved.
fn confirmed(answer: String, said: impl FnOnce() -> String) -> String {
    if answer.starts_with("could not") {
        answer
    } else {
        said()
    }
}

pub(super) fn task(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", task_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

/// `task`, `task add`, `task add <title> --under <id>`, `task doing
/// <id>`, `task done <id>`, `task todo <id>`, `task retitle <id> <text>`,
/// `task brief <id> <text>`, `task drop <id>`, and `task move <id>` with one
/// of `--under <id>`, `--top`, `--before <id>`, `--after <id>` or `--to
/// <feature>`.
///
/// The columns are named as verbs rather than hidden behind a state
/// argument, so what an agent types is what a reader of the transcript
/// understands happened; `move` is where a task sits, never its state.
/// `--under` records newly discovered work beneath the task that exposed
/// it.
pub(super) fn task_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let by_id =
        |verb: &str, state: TaskState| match rest.get(1).and_then(|id| id.parse::<i64>().ok()) {
            Some(id) => write_task(TaskAction::Move { id, state }, base_url, token),
            None => format!("could not change task: {verb} wants the number `task` prints"),
        };
    match rest.first().copied() {
        None | Some("list") => write_task(TaskAction::List, base_url, token),
        Some("add") => {
            let write = match parse_task_add_args(&rest[1..]) {
                Ok(write) => write,
                Err(message) => return format!("could not change task: {message}"),
            };
            write_task(TaskAction::Add(write), base_url, token)
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
        Some("move") => match parse_task_move_args(&rest[1..]) {
            Ok((id, place)) => write_task(TaskAction::Place { id, place }, base_url, token),
            Err(message) => format!("could not change task: {message}"),
        },
        Some(other) => format!(
            "could not change task: `{other}` is not one of add, doing, done, todo, retitle, \
             brief, drop, move"
        ),
    }
}

/// `<id>` and then exactly one place: `--under <id>`, `--top`, `--before
/// <id>`, `--after <id>` or `--to <feature>`.
fn parse_task_move_args(args: &[&str]) -> Result<(i64, TaskPlace), String> {
    const USAGE: &str =
        "move wants the task's number, then --under <id>, --top, --before <id>, --after <id> \
         or --to <feature>";
    let Some(id) = args.first().and_then(|id| id.parse::<i64>().ok()) else {
        return Err(USAGE.to_string());
    };
    let task = |raw: Option<&&str>| {
        raw.and_then(|raw| raw.parse::<i64>().ok())
            .filter(|id| *id > 0)
            .ok_or_else(|| USAGE.to_string())
    };
    let place = match args.get(1..).unwrap_or_default() {
        ["--under", other] => TaskPlace::Under(task(Some(other))?),
        ["--top"] => TaskPlace::Top,
        ["--before", other] => TaskPlace::Before(task(Some(other))?),
        ["--after", other] => TaskPlace::After(task(Some(other))?),
        ["--to", feature] if is_slug(feature) => TaskPlace::Feature(feature.to_string()),
        _ => return Err(USAGE.to_string()),
    };
    Ok((id, place))
}

fn parse_task_add_args(args: &[&str]) -> Result<TaskWrite, String> {
    let mut title = Vec::new();
    let mut external = Vec::new();
    let mut parent = None;
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            "--under" => {
                let raw = args
                    .get(index + 1)
                    .ok_or_else(|| "--under needs something after it".to_string())?;
                parent = Some(
                    raw.parse::<i64>()
                        .ok()
                        .filter(|id| *id > 0)
                        .ok_or_else(|| format!("--under wants a task number, not {raw:?}"))?,
                );
                index += 2;
            }
            "--key" => {
                index += 1;
                let start = index;
                while index < args.len() && args[index] != "--under" && args[index] != "--key" {
                    external.push(args[index]);
                    index += 1;
                }
                if index == start {
                    return Err("--key needs something after it".to_string());
                }
            }
            word => {
                title.push(word);
                index += 1;
            }
        }
    }
    Ok(TaskWrite {
        title: title.join(" "),
        external: (!external.is_empty()).then(|| external.join(" ")),
        parent,
    })
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
    for row in list.tree_rows() {
        let task = row.task;
        let key = match &task.external {
            Some(key) => format!(" [{key}]"),
            None => String::new(),
        };
        let who = match (&task.state, &task.claimed_by) {
            (TaskState::Doing, Some(session)) => format!(" — {session}"),
            _ => String::new(),
        };
        lines.push(format!(
            "{}#{:<3} {:<5} {}{key}{who}",
            task_branch(&row),
            task.id,
            task.state,
            task.title
        ));
        if let Some(body) = &task.body {
            let prefix = task_body_prefix(&row);
            lines.extend(body.lines().map(|line| format!("{prefix}{line}")));
        }
    }
    lines.join("\n")
}

const MAX_TASK_INDENT: usize = 24;

fn task_branch(row: &argus_protocol::TaskTreeRow<'_>) -> String {
    if row.depth == 0 {
        return String::new();
    }
    let mut branch = task_ancestor_guides(row, 1);
    branch.push_str(if row.has_next_sibling {
        "├─ "
    } else {
        "└─ "
    });
    branch
}

fn task_continuation(row: &argus_protocol::TaskTreeRow<'_>) -> String {
    if row.depth == 0 {
        return String::new();
    }
    let mut continuation = task_ancestor_guides(row, 1);
    continuation.push_str(if row.has_next_sibling { "│  " } else { "   " });
    continuation
}

fn task_ancestor_guides(row: &argus_protocol::TaskTreeRow<'_>, reserved_slots: usize) -> String {
    let slots = (MAX_TASK_INDENT / 3).saturating_sub(reserved_slots);
    let first = row.ancestor_continuations.len().saturating_sub(slots);
    row.ancestor_continuations[first..]
        .iter()
        .map(|continues| if *continues { "│  " } else { "   " })
        .collect()
}

fn task_body_prefix(row: &argus_protocol::TaskTreeRow<'_>) -> String {
    if row.depth == 0 {
        "      ".to_string()
    } else {
        format!("{}   ", task_continuation(row))
    }
}

pub(super) fn diagram(rest: &[&str]) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", diagram_message(rest, &env_url(), &env_token()));
}

/// `diagram`, `diagram add <title> --stdin`, `diagram drop <id>`.
pub(super) fn diagram_message(rest: &[&str], base_url: &str, token: &str) -> String {
    match rest.first().copied() {
        None | Some("list") => write_diagram(DiagramAction::List, base_url, token),
        Some("add") => {
            let write = match parse_diagram_add_args(&rest[1..]) {
                Ok(write) => write,
                Err(message) => return format!("could not change diagram: {message}"),
            };
            write_diagram(DiagramAction::Add(write), base_url, token)
        }
        Some("drop") => match rest.get(1).and_then(|id| id.parse::<i64>().ok()) {
            Some(id) => write_diagram(DiagramAction::Remove { id }, base_url, token),
            None => "could not change diagram: drop wants the number `diagram` prints".to_string(),
        },
        Some(other) => format!(
            "could not change diagram: `{other}` is not one of add, drop"
        ),
    }
}

/// Title is positional; `--stdin` reads Mermaid source from standard input.
fn parse_diagram_add_args(args: &[&str]) -> Result<DiagramWrite, String> {
    let mut title: Vec<&str> = Vec::new();
    let mut from_stdin = false;
    for arg in args {
        match *arg {
            "--stdin" => from_stdin = true,
            other if other.starts_with("--") => {
                return Err(format!("{other} is not one of --stdin"))
            }
            word => title.push(word),
        }
    }
    if title.is_empty() {
        return Err("add wants a title".into());
    }
    let body = if from_stdin {
        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .map_err(|e| format!("could not read Mermaid source: {e}"))?;
        source
    } else {
        return Err("add wants --stdin with the Mermaid source (sequenceDiagram …)".into());
    };
    DiagramWrite {
        title: title.join(" "),
        body,
    }
    .checked()
    .map_err(str::to_string)
}

pub(super) fn write_diagram(action: DiagramAction, base_url: &str, token: &str) -> String {
    let Ok(body) = serde_json::to_string(&action) else {
        return "could not change diagram: unencodable".to_string();
    };
    let url = endpoint_url(base_url, Endpoint::Diagrams);
    let Some((status, response)) = post_response(&url, token, &body) else {
        return "could not change diagram: daemon unavailable".to_string();
    };
    let response = response.trim();
    if status != 200 {
        return if response.is_empty() {
            "could not change diagram: daemon refused the request".to_string()
        } else {
            format!("could not change diagram: {response}")
        };
    }
    match serde_json::from_str::<DiagramList>(response) {
        Ok(list) => format_diagrams(&list),
        Err(_) => "the diagram list changed".to_string(),
    }
}

pub(super) fn format_diagrams(list: &DiagramList) -> String {
    let Some(feature) = &list.feature else {
        return "this checkout is not on a feature, so it has no sequence diagrams. \
                `argus-hook feature` says where it is."
            .to_string();
    };
    if list.diagrams.is_empty() {
        return format!(
            "No sequence diagrams under {feature} yet. Add one with \
             `argus-hook diagram add \"<title>\" --stdin` and paste Mermaid source."
        );
    }
    let mut lines = vec![format!("Sequence diagrams under {feature}:")];
    for diagram in &list.diagrams {
        lines.push(format!("#{:<3} {}", diagram.id, diagram.title));
        for line in diagram.body.lines() {
            lines.push(format!("      {line}"));
        }
    }
    lines.join("\n")
}

pub(super) fn read_feature_board(
    rest: &[&str],
    base_url: &str,
    token: &str,
) -> Result<FeatureBoard, String> {
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
    lines.extend(parked_lines(current, &board.features));
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
        // Beneath rather than in the parentheses, since a reason is a
        // sentence and runs as long as it needs to.
        if feature.state != argus_protocol::FeatureState::Done {
            if let Some(reason) = &feature.held {
                lines.push(format!("      held: {reason}"));
            }
            let waiting: Vec<&str> = feature.waiting_on(&board.features).collect();
            if !waiting.is_empty() {
                lines.push(format!("      after {}", waiting.join(", ")));
            }
        }
    }
    lines.join("\n")
}

/// Why a feature is to be left alone, for the agent about to work on it:
/// its hold, and every feature it comes after with whether that one is
/// done, so a wait already met can still be found and taken back.
pub(super) fn parked_lines(
    feature: &argus_protocol::Feature,
    features: &[argus_protocol::Feature],
) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(reason) = &feature.held {
        lines.push(format!("Held: {reason}"));
    }
    if !feature.waits_on.is_empty() {
        let after: Vec<String> = feature
            .waits_on
            .iter()
            .map(|slug| {
                let done = features
                    .iter()
                    .any(|f| &f.slug == slug && f.state == argus_protocol::FeatureState::Done);
                format!("{slug} ({})", if done { "done" } else { "open" })
            })
            .collect();
        lines.push(format!("Comes after: {}", after.join(", ")));
    }
    lines
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
    if let Some(verb @ ("withdraw" | "restore")) = rest.first().copied() {
        return change_decision(verb, rest.get(1..).unwrap_or_default(), base_url, token);
    }
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

/// `decisions withdraw <id>` and `decisions restore <id>`: takes a decision
/// recorded in error back, keeping it in the history, and undoes that.
fn change_decision(verb: &str, args: &[&str], base_url: &str, token: &str) -> String {
    let id = match args {
        [id] => id.trim_start_matches('#').parse::<i64>().ok(),
        _ => None,
    };
    let Some(id) = id else {
        return format!("could not change decision: {verb} wants the number `decisions` prints");
    };
    let (change, done) = if verb == "withdraw" {
        (argus_protocol::DecisionChange::Withdraw { id }, "withdrawn")
    } else {
        (argus_protocol::DecisionChange::Restore { id }, "restored")
    };
    let Ok(body) = serde_json::to_string(&change) else {
        return "could not change decision: unencodable".to_string();
    };
    let url = endpoint_url(base_url, Endpoint::DecisionChange);
    let Some((status, response)) = post_response(&url, token, &body) else {
        return "could not change decision: daemon unavailable".to_string();
    };
    let response = response.trim();
    if status != 200 {
        return if response.is_empty() {
            "could not change decision: daemon refused the request".to_string()
        } else {
            format!("could not change decision: {response}")
        };
    }
    format!("decision {id} {done}")
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

pub(super) fn push_decision_lines(
    lines: &mut Vec<String>,
    row: &argus_protocol::DecisionTreeRow<'_>,
) {
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

#[cfg(test)]
mod tests {
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
    fn changing_a_feature_wants_its_slug_and_its_text() {
        let base = "http://127.0.0.1:1/pane/1";
        for args in [&["retitle"][..], &["retitle", "a-slug"], &["brief", "a-slug", " "]] {
            let message = feature_message(args, base, "t");
            assert!(message.starts_with("could not change feature"), "{args:?}: {message}");
        }
        let message = feature_message(&["drop"], base, "t");
        assert!(message.contains("wants the slug"), "{message}");
    }

    #[test]
    fn holding_or_ordering_a_feature_wants_its_slugs_and_why() {
        let base = "http://127.0.0.1:1/pane/1";
        for args in [
            &["hold"][..],
            &["hold", "remote", " "],
            &["unhold"],
            &["wait", "remote"],
            &["unwait"],
        ] {
            let message = feature_message(args, base, "t");
            assert!(message.starts_with("could not change feature"), "{args:?}: {message}");
        }
    }

    /// Three features: one accepted, one open, and one held that comes
    /// after both.
    fn parked_board() -> FeatureBoard {
        serde_json::from_str(
            r#"{"project":null,"project_name":"argus","current":"remote","unfiled":0,
                "decisions":[],"features":[
                {"slug":"handshake","title":"Handshake","body":"","origin_checkout":null,
                 "origin_branch":null,"at":1,"session":null,"state":"done"},
                {"slug":"traffic","title":"Traffic","body":"","origin_checkout":null,
                 "origin_branch":null,"at":2,"session":null},
                {"slug":"remote","title":"Remote","body":"","origin_checkout":null,
                 "origin_branch":null,"at":3,"session":null,
                 "held":"until the user answers","waits_on":["handshake","traffic"]}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn the_list_says_what_holds_a_feature_and_what_it_still_waits_on() {
        let list = format_feature_list(&parked_board());
        assert!(
            list.contains(
                "  remote — Remote (this checkout)\n      held: until the user answers\n      \
                 after traffic"
            ),
            "{list}"
        );
        assert!(!list.contains("after handshake"), "an accepted prerequisite is met: {list}");
    }

    #[test]
    fn reading_a_feature_names_every_prerequisite_and_whether_it_is_met() {
        let text = format_feature(&parked_board());
        assert!(
            text.contains(
                "Held: until the user answers\nComes after: handshake (done), traffic (open)"
            ),
            "{text}"
        );
    }

    #[test]
    fn a_task_moves_to_exactly_one_place() {
        let parse = |args: &[&str]| parse_task_move_args(args);
        assert_eq!(parse(&["4", "--under", "2"]), Ok((4, TaskPlace::Under(2))));
        assert_eq!(parse(&["4", "--top"]), Ok((4, TaskPlace::Top)));
        assert_eq!(parse(&["4", "--before", "7"]), Ok((4, TaskPlace::Before(7))));
        assert_eq!(parse(&["4", "--after", "7"]), Ok((4, TaskPlace::After(7))));
        assert_eq!(
            parse(&["4", "--to", "lean-wire"]),
            Ok((4, TaskPlace::Feature("lean-wire".into())))
        );
        for bad in [
            &[][..],
            &["four", "--top"],
            &["4"],
            &["4", "--under"],
            &["4", "--under", "x"],
            &["4", "--top", "--under", "2"],
            &["4", "--to", "Not A Slug"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn accepting_a_feature_wants_its_slug() {
        let message = feature_message(&["done"], "http://127.0.0.1:1/pane/1", "t");
        assert!(message.contains("wants the slug"), "{message}");
    }

    #[test]
    fn a_board_command_names_its_feature_anywhere_in_its_arguments() {
        let (named, rest) =
            named_feature("task", &["add", "bound", "it", "--feature", "lean-wire", "--under", "4"])
                .unwrap();
        assert_eq!(named, Some("lean-wire"));
        assert_eq!(rest, ["add", "bound", "it", "--under", "4"]);

        let (named, rest) = named_feature("decisions", &[]).unwrap();
        assert_eq!((named, rest.len()), (None, 0));
    }

    #[test]
    fn a_feature_on_its_own_is_read_by_name() {
        let (named, rest) = named_feature("feature", &["protocol-handshake"]).unwrap();
        assert_eq!(named, Some("protocol-handshake"));
        assert!(rest.is_empty());
        let (named, rest) = named_feature("feature", &["list"]).unwrap();
        assert_eq!((named, rest), (None, vec!["list"]), "a subcommand stays one");
    }

    #[test]
    fn a_name_that_is_not_a_slug_never_travels() {
        assert!(named_feature("task", &["--feature"]).is_err());
        assert!(named_feature("task", &["--feature", "Two Words"]).is_err());
        assert!(named_feature("task", &["--feature", "../../etc"]).is_err());
    }

    #[test]
    fn task_add_arguments_keep_the_parent_and_tracker_key() {
        let write = parse_task_add_args(&[
            "bound",
            "the",
            "queue",
            "--key",
            "ORION-412",
            "--under",
            "7",
        ])
        .unwrap();
        assert_eq!(write.title, "bound the queue");
        assert_eq!(write.external.as_deref(), Some("ORION-412"));
        assert_eq!(write.parent, Some(7));
    }

    #[test]
    fn task_add_rejects_a_non_positive_parent_id() {
        let error = parse_task_add_args(&["child", "--under", "0"]).unwrap_err();
        assert!(error.contains("task number"), "{error}");
    }

    #[test]
    fn diagram_add_requires_stdin_for_source() {
        let error = parse_diagram_add_args(&["open", "overlay"]).unwrap_err();
        assert!(error.contains("--stdin"), "{error}");
    }
}
