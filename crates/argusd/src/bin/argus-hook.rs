//! The command Argus's managed agent hooks run, and the one an agent runs
//! itself to say what it is doing.
//!
//! ```text
//! argus-hook title "fixing the pty deadlock"
//! argus-hook status waiting "needs the staging database password"
//! argus-hook status needs-review "ready for review"
//! argus-hook status done "reviewed and complete"
//! argus-hook status working
//! argus-hook checkout                            # reports the current directory
//! argus-hook session <id>                        # records exact resume identity
//! argus-hook comments                            # reads durable review feedback
//! argus-hook context                             # reads the notes for this checkout
//! argus-hook todo add "ported the parser"        # writes to the checkout's note
//! argus-hook todo done 4                         # ticks line 4 of it off
//! argus-hook feature                            # the feature this checkout is on
//! argus-hook feature list                       # every feature of the project
//! argus-hook feature open "decision scoping"    # opens one and works on it
//! argus-hook feature use decision-scoping       # works on one that already exists
//! argus-hook feature note "the board is per feature"   # adds to its document
//! argus-hook decisions                          # this feature's decision board
//! argus-hook decide "sqlite" --over "a file per feature" --because "both need migrations"
//! argus-hook decide "one row per note" --under 3  # hangs under decision 3
//! argus-hook decide "one row per note" --supersedes 7   # replaces decision 7
//! argus-hook say "text"                          # prints, calls nobody
//! argus-hook instructions                        # prints inherited startup context
//! argus-hook <url> <token> [--note-from-stdin] [--title-from-stdin]  # the installed hook form
//! ```
//!
//! The named forms read `ARGUS_HOOK_URL` and `ARGUS_HOOK_TOKEN` from the
//! environment, which every agent pane is handed. That is what makes status
//! harness-agnostic: a CLI that can run one command at some point in its
//! lifecycle needs nothing from Argus but these variables. The explicit form
//! is what Argus writes into a harness's own hook config, where there is no
//! guarantee the environment survives.
//!
//! It **always exits 0**, whatever happens. That is the entire reason it
//! exists instead of a `curl` invocation: a hook command that exits non-zero
//! is reported to the user as a failed turn. A daemon that has since exited —
//! or a port that now belongs to nobody — must degrade to "pane status stops
//! updating", never to an error on every prompt in that directory. `curl`
//! exits 7 on a refused connection, which is exactly what this avoids.
//!
//! Installed hooks write only the JSON the runner needs to let the turn
//! continue — Cursor wants `permission`, Claude wants `decision` — never a
//! human-readable message. Some agent CLIs inject a hook's stdout into the
//! model's context, so staying silent keeps Argus's bookkeeping out of the
//! conversation. The deliberate `say`, `instructions`, `comments`, `context`, `todo`,
//! `feature`, `decisions`, and `decide` commands do return useful output.
//!
//! On Windows it is a GUI-subsystem binary. Not because it has a UI — it
//! has none — but because the agent CLI that runs it decides how it is
//! spawned, and we cannot ask that CLI to pass `CREATE_NO_WINDOW`. A
//! console-subsystem binary spawned from a process without a console gets
//! its own console *window*, which flashes on screen on every hook event.
//! Declaring the GUI subsystem means no console is ever allocated. Safe
//! precisely because this program reads and writes nothing on stdio it was
//! not handed.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use argus_protocol::{
    AgentContext, Decision, DecisionBoard, DecisionWrite, Endpoint, FeatureAction, FeatureBoard,
    FeatureWrite, Report, ReviewComment, TaskAction, TaskList, TaskState, TaskWrite, TodoState,
    TodoWrite, INSTRUCTIONS_COMMAND, INSTRUCTIONS_VAR, NOTE_FLAG,
    OWNS_SESSION_FLAG, SESSION_HEADER, SESSION_KEY_FLAG, TITLE_FLAG, TOKEN_VAR, URL_VAR,
};

const TIMEOUT: Duration = Duration::from_secs(2);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    dispatch(args.first().map(String::as_str), &rest);
}

type NamedHandler = fn(&[&str]);

const NAMED_HANDLERS: &[(&str, NamedHandler)] = &[
    ("say", say),
    (INSTRUCTIONS_COMMAND, instructions),
    ("title", title),
    ("status", status),
    ("checkout", checkout),
    ("session", session),
    ("comments", comments),
    ("context", context),
    ("todo", todo),
    ("feature", feature),
    ("task", task),
    ("decisions", decisions),
    ("decide", decide),
];

fn dispatch(command: Option<&str>, rest: &[&str]) {
    match command {
        // The installed-hook form uses an absolute URL and token because a
        // harness's hook config cannot count on inheriting the environment.
        Some(url) if url.starts_with("http://") => installed_hook(url, rest),
        Some(name) => {
            if let Some((_, handler)) = NAMED_HANDLERS.iter().find(|(key, _)| *key == name) {
                handler(rest);
            }
        }
        None => {}
    }
}

fn say(rest: &[&str]) {
    // Deliberately on stdout: this is context for the model.
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", rest.join(" "));
    let _ = out.flush();
}

fn instructions(_: &[&str]) {
    // Read in the helper, not in a shell command string: multiline context
    // and paths with shell metacharacters must remain data, never shell code.
    say(&[&env_instructions()]);
}

fn title(rest: &[&str]) {
    let text = rest.join(" ");
    if !text.trim().is_empty() {
        let _ = post(
            &endpoint_url(&env_url(), Endpoint::Title),
            &env_token(),
            &text,
        );
    }
}

fn status(rest: &[&str]) {
    // A state the pane API has no name for could only ever be refused at the
    // other end, so it is refused here instead of travelling.
    let Some(report) = rest.first().and_then(|s| Report::parse(s)) else {
        return;
    };
    // Anything after the state is the reason, so
    // `status waiting "needs a password"` reads the way you'd say it.
    let note = rest[1..].join(" ");
    let _ = post(
        &endpoint_url(&env_url(), Endpoint::Status(report)),
        &env_token(),
        &note,
    );
}

fn checkout(rest: &[&str]) {
    if let Some(path) = reported_checkout(rest) {
        let _ = post(
            &endpoint_url(&env_url(), Endpoint::Checkout),
            &env_token(),
            &path.to_string_lossy(),
        );
    }
}

fn session(rest: &[&str]) {
    let id = rest.join(" ");
    if !id.is_empty() {
        let _ = post(
            &endpoint_url(&env_url(), Endpoint::Session),
            &env_token(),
            &id,
        );
    }
}

fn comments(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", comments_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

fn comments_message(rest: &[&str], base_url: &str, token: &str) -> String {
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

/// A read of whatever the human wrote down about this checkout and the
/// project above it. Deliberately on stdout, like `comments`: it is
/// context for the model.
fn context(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", context_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

fn context_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let context: AgentContext = match read_json("context", Endpoint::Context, rest, base_url, token)
    {
        Ok(context) => context,
        Err(message) => return message,
    };
    if context.is_empty() {
        return "no notes for this checkout".to_string();
    }
    let mut sections = Vec::new();
    // The pinned lines are repeated out of the bodies below on purpose.
    // `- [!]` is Argus's spelling, not Markdown's, and an agent reading a
    // note cold has no reason to know it means "standing instruction".
    let pinned: Vec<String> = context
        .pinned()
        .map(|(scope, todo)| format!("- ({}) {}", scope.label(), todo.text))
        .collect();
    if !pinned.is_empty() {
        sections.push(format!(
            "Standing instructions, which apply without being asked for:\n{}",
            pinned.join("\n")
        ));
    }
    for note in &context.notes {
        sections.push(format!(
            "--- {} note: {} ---\n{}",
            note.scope.label(),
            note.name,
            note.body.trim_end()
        ));
    }
    sections.join("\n\n")
}

/// The one command here that changes something, so the one that reports
/// back. Deliberately on stdout for the same reason `context` is: the agent
/// that ran it has to be able to tell the user it was refused.
fn todo(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", todo_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

fn todo_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let Some(write) = parse_todo_args(rest) else {
        return "could not write the note: expected `todo add <text>`, \
                `todo done <line>`, or `todo open <line>`"
            .to_string();
    };
    let body = match serde_json::to_string(&write) {
        Ok(body) => body,
        Err(_) => return "could not write the note: unencodable item".to_string(),
    };
    let url = endpoint_url(base_url, Endpoint::Todo);
    let Some((status, response)) = post_response(&url, token, &body) else {
        return "could not write the note: daemon unavailable".to_string();
    };
    let response = response.trim();
    if status != 200 {
        return if response.is_empty() {
            "could not write the note: daemon refused the request".to_string()
        } else {
            format!("could not write the note: {response}")
        };
    }
    match write {
        TodoWrite::Add { text } => format!("added \"{text}\" — {response}"),
        TodoWrite::Set { state, .. } => format!("marked {} — {response}", state_word(state)),
    }
}

/// The feature this checkout is working on: what it is for, and what has
/// been decided under it.
///
/// One command rather than four because they are one thing from the
/// agent's side — where am I, and how do I say where I am. Bare, it
/// answers the first; `list`, `open`, `use` and `note` answer the second.
fn feature(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", feature_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

fn feature_message(rest: &[&str], base_url: &str, token: &str) -> String {
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
        Some(other) => format!(
            "{other} is not one of list, open, use, note — \
             `argus-hook feature use {other}` works on an existing feature"
        ),
    }
}

fn task(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", task_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

/// `task`, `task add`, `task doing <id>`, `task done <id>`, `task todo
/// <id>`, `task retitle <id> <text>`, `task drop <id>`.
///
/// The columns are named as verbs rather than hidden behind a `move`, so
/// what an agent types is what a reader of the transcript understands
/// happened.
fn task_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let by_id = |verb: &str, state: TaskState| match rest.get(1).and_then(|id| id.parse::<i64>().ok())
    {
        Some(id) => write_task(TaskAction::Move { id, state }, base_url, token),
        None => format!("could not change task: {verb} wants the number `task` prints"),
    };
    match rest.first().copied() {
        None | Some("list") => write_task(TaskAction::List, base_url, token),
        Some("add") => {
            // A trailing `--key ORION-412` is the tracker's own id, kept
            // so an agent can reconcile later. Argus never reads it.
            let words: Vec<&str> = rest[1..].iter().take_while(|a| **a != "--key").copied().collect();
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
        Some("drop") => match rest.get(1).and_then(|id| id.parse::<i64>().ok()) {
            Some(id) => write_task(TaskAction::Remove { id }, base_url, token),
            None => "could not change task: drop wants the number `task` prints".to_string(),
        },
        Some(other) => format!(
            "could not change task: `{other}` is not one of add, doing, done, todo, retitle, drop"
        ),
    }
}

fn write_task(action: TaskAction, base_url: &str, token: &str) -> String {
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
fn format_tasks(list: &TaskList) -> String {
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
    }
    lines.join("\n")
}

fn read_feature_board(rest: &[&str], base_url: &str, token: &str) -> Result<FeatureBoard, String> {
    read_json("the feature", Endpoint::Features, rest, base_url, token)
}

fn write_feature(action: FeatureAction, base_url: &str, token: &str) -> String {
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
fn format_feature(board: &FeatureBoard) -> String {
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
fn no_feature_here(board: &FeatureBoard) -> String {
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
        lines.push("Open one with `argus-hook feature open \"<title>\"`, or work on one of:".into());
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

fn format_feature_list(board: &FeatureBoard) -> String {
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
        lines.push(format!(
            "  {} — {}{branch}{here}",
            feature.slug, feature.title
        ));
    }
    lines.join("\n")
}

/// This feature's decision board, drawn as the tree it is. Read rather than
/// written, and on stdout for the same reason `context` is: it is what an
/// agent picking up a feature is meant to consult before adding to it.
fn decisions(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", decisions_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

fn decisions_message(rest: &[&str], base_url: &str, token: &str) -> String {
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
    for row in board.tree_rows() {
        push_decision_lines(&mut lines, &row);
    }
    lines.join("\n")
}

fn push_decision_lines(lines: &mut Vec<String>, row: &argus_protocol::DecisionTreeRow<'_>) {
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

fn decision_branch(row: &argus_protocol::DecisionTreeRow<'_>) -> String {
    if row.depth == 0 {
        return String::new();
    }
    let mut branch = decision_ancestor_guides(row);
    branch.push_str(if row.has_next_sibling { "├─ " } else { "└─ " });
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
fn decide(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", decide_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

fn decide_message(rest: &[&str], base_url: &str, token: &str) -> String {
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
fn parse_decide_args(rest: &[&str]) -> Result<DecisionWrite, String> {
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

fn id_arg(flag: &str, raw: &str) -> Result<i64, String> {
    raw.parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| format!("{flag} wants a decision number, not {raw:?}"))
}

fn state_word(state: TodoState) -> &'static str {
    match state {
        TodoState::Done => "done",
        _ => "open",
    }
}

/// The line numbers here are the ones a person reads off the note, so they
/// are 1-based on the way in and 0-based on the wire.
fn parse_todo_args(rest: &[&str]) -> Option<TodoWrite> {
    match *rest.first()? {
        "add" => {
            let text = rest[1..].join(" ");
            (!text.trim().is_empty()).then_some(TodoWrite::Add { text })
        }
        verb @ ("done" | "open") => {
            let line: usize = rest.get(1)?.parse().ok()?;
            Some(TodoWrite::Set {
                line: line.checked_sub(1)?,
                state: if verb == "done" {
                    TodoState::Done
                } else {
                    TodoState::Open
                },
            })
        }
        _ => None,
    }
}

/// One JSON read from the pane API, with every way it can fail phrased for
/// the agent that ran the command rather than for a log.
fn read_json<T: serde::de::DeserializeOwned>(
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

fn installed_hook(url: &str, rest: &[&str]) {
    let Some(token) = rest.first() else { return };
    let (key, raw, note, title) = installed_input(rest, std::io::stdin());
    let inherited_url = env_url();
    let inherited_token = env_token();
    let (url, token) = routed_hook(url, token, &inherited_url, &inherited_token);
    let session = key.and_then(|key| raw.as_deref().and_then(|raw| json_string(raw, key)));
    let _ = post_as(&url, &token, &note, session.as_deref());
    if rest.contains(&OWNS_SESSION_FLAG) {
        post_session_id(&url, &token, session.as_deref());
    }
    post_title(&url, &token, &title, session.as_deref());

    let mut out = std::io::stdout();
    let _ = writeln!(
        out,
        "{}",
        hook_reply(
            raw.as_deref(),
            rest.contains(&"--inject-instructions"),
            &env_instructions(),
        )
    );
    let _ = out.flush();
}

/// The JSON a hook runner needs so it does not treat bookkeeping as a
/// denied tool or a blocked prompt. Claude Code keys off `toolCall` and
/// wants `decision`; Cursor keys off `tool_name` and wants `permission`.
fn hook_reply(raw: Option<&str>, inject_instructions: bool, instructions: &str) -> String {
    let raw = raw.unwrap_or("");
    if raw.contains("\"toolCall\"") {
        return r#"{"decision":"allow"}"#.to_string();
    }
    if raw.contains("\"tool_name\"") {
        return r#"{"permission":"allow"}"#.to_string();
    }
    if (raw.contains("\"invocationNum\"") || inject_instructions) && !instructions.is_empty() {
        return serde_json::json!({
            "injectSteps": [{ "ephemeralMessage": instructions }]
        })
        .to_string();
    }
    "{}".to_string()
}

fn env_instructions() -> String {
    std::env::var(INSTRUCTIONS_VAR).unwrap_or_default()
}

fn installed_input<'a>(
    rest: &'a [&str],
    stdin: impl Read,
) -> (Option<&'a str>, Option<String>, String, String) {
    let key = rest
        .iter()
        .position(|arg| *arg == SESSION_KEY_FLAG)
        .and_then(|index| rest.get(index + 1))
        .copied();
    let raw = (rest.contains(&NOTE_FLAG) || rest.contains(&TITLE_FLAG) || key.is_some())
        .then(|| read_hook_input(stdin));
    let note = if rest.contains(&NOTE_FLAG) {
        raw.as_deref().map(note_from).unwrap_or_default()
    } else {
        String::new()
    };
    let title = if rest.contains(&TITLE_FLAG) {
        raw.as_deref().map(title_from).unwrap_or_default()
    } else {
        String::new()
    };
    (key, raw, note, title)
}

/// Records the conversation identity Argus resumes this pane with. Only the
/// event a harness fires when *its own* session starts carries the flag that
/// gets here, so a CLI started from inside the pane cannot claim it.
fn post_session_id(url: &str, token: &str, id: Option<&str>) {
    let Some(id) = id.filter(|id| !id.is_empty()) else {
        return;
    };
    if let Some(base) = pane_base(url) {
        let _ = post_as(&endpoint_url(&base, Endpoint::Session), token, id, Some(id));
    }
}

fn post_title(url: &str, token: &str, title: &str, session: Option<&str>) {
    if title.is_empty() {
        return;
    }
    if let Some(base) = pane_base(url) {
        let _ = post_as(&endpoint_url(&base, Endpoint::Title), token, title, session);
    }
}

fn reported_checkout(args: &[&str]) -> Option<std::path::PathBuf> {
    if args.is_empty() {
        std::env::current_dir().ok()
    } else {
        Some(std::path::PathBuf::from(args.join(" ")))
    }
}

fn env_url() -> String {
    std::env::var(URL_VAR).unwrap_or_default()
}

fn env_token() -> String {
    std::env::var(TOKEN_VAR).unwrap_or_default()
}

/// The message a harness hands its hook on stdin.
///
/// Cursor's runner writes one JSON object and then waits for stdout without
/// closing the pipe. Reading to EOF would deadlock until the hook timeout
/// killed the process — after which the status POST never ran. One complete
/// JSON value is enough; plain text still reads to the end of the stream.
fn read_hook_input(mut reader: impl Read) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Ok(s) = std::str::from_utf8(&buf) {
                    if json_value(s).is_some() {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn json_value(raw: &str) -> Option<serde_json::Value> {
    let trimmed = raw.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let mut de = serde_json::Deserializer::from_str(trimmed);
    serde::Deserialize::deserialize(&mut de).ok()
}

fn json_string(raw: &str, key: &str) -> Option<String> {
    let v = json_value(raw)?;
    if let Some(s) = v
        .get(key)
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
    {
        return Some(s.to_string());
    }
    // Cursor's sessionStart names the same id `session_id`; every other
    // event puts it on `conversation_id`. Asking for either must find both.
    for alias in ["conversation_id", "session_id"] {
        if alias == key {
            continue;
        }
        if let Some(s) = v
            .get(alias)
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
        {
            return Some(s.to_string());
        }
    }
    None
}

/// Repoint a checkout-wide managed hook at the pane-specific URL inherited
/// by this process. Both URLs must name panes on the same loopback listener.
fn rebase_hook_url(configured: &str, inherited: &str) -> Option<String> {
    let configured_base = pane_base(configured)?;
    let inherited_base = pane_base(inherited)?;
    if authority(&configured_base)? != authority(&inherited_base)? {
        return None;
    }
    let suffix = configured.strip_prefix(&configured_base)?;
    (!suffix.is_empty()).then(|| format!("{inherited_base}{suffix}"))
}

fn routed_hook(
    configured_url: &str,
    configured_token: &str,
    inherited_url: &str,
    inherited_token: &str,
) -> (String, String) {
    match (
        rebase_hook_url(configured_url, inherited_url),
        !inherited_token.is_empty(),
    ) {
        (Some(url), true) => (url, inherited_token.to_string()),
        _ => (configured_url.to_string(), configured_token.to_string()),
    }
}

fn authority(url: &str) -> Option<&str> {
    url.strip_prefix("http://")?.split('/').next()
}

/// A pane base (`http://host:port/pane/<id>`) plus the endpoint being asked
/// for. The suffix comes from `argus-protocol` so the daemon parses exactly
/// what is built here.
fn endpoint_url(base: &str, endpoint: Endpoint) -> String {
    format!("{}/{}", base.trim_end_matches('/'), endpoint.suffix())
}

fn pane_base(url: &str) -> Option<String> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = rest.split_once('/')?;
    let mut parts = path.split('/');
    if parts.next()? != "pane" {
        return None;
    }
    parts.next()?.parse::<u64>().ok()?;
    let host = authority.split(':').next()?;
    if host != "127.0.0.1" || authority.rsplit_once(':')?.1.parse::<u16>().is_err() {
        return None;
    }
    Some(format!(
        "http://{authority}/pane/{}",
        path.split('/').nth(1)?
    ))
}

/// Harnesses hand hooks a JSON event where they can. `message` is Claude
/// Code's field for the text of what it is waiting on; a harness that sends
/// plain text instead still gets its first line used.
fn note_from(raw: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(tool) = v.get("toolCall") {
            let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
            if let Some(cmd) = tool
                .get("args")
                .and_then(|a| a.get("CommandLine"))
                .and_then(|v| v.as_str())
            {
                return format!("{name}: {cmd}");
            }
            return name.to_string();
        }
        for key in ["message", "text", "reason", "prompt"] {
            if let Some(s) = v.get(key).and_then(|v| v.as_str()) {
                if !s.trim().is_empty() {
                    return s.trim().to_string();
                }
            }
        }
        // Valid JSON with nothing we recognize is not worth showing raw.
        return String::new();
    }
    raw.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_string()
}

/// The user's prompt, when a harness event carries one. Tool names and
/// session bookkeeping are not titles — a working pane named "Shell" says
/// less than the template already does.
fn title_from(raw: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else {
        return raw
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or_default()
            .to_string();
    };
    for key in ["prompt", "query", "user_message", "userMessage", "text"] {
        if let Some(s) = json_prompt_string(&v, key) {
            return s;
        }
    }
    String::new()
}

fn json_prompt_string(v: &serde_json::Value, key: &str) -> Option<String> {
    let field = v.get(key)?;
    if let Some(s) = field.as_str().map(str::trim).filter(|s| !s.is_empty()) {
        return Some(s.to_string());
    }
    field
        .get("text")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Best-effort POST. Every error is discarded by the caller; the return type
/// exists only so the body can use `?`.
fn post(url: &str, token: &str, body: &str) -> Option<()> {
    post_as(url, token, body, None)
}

fn post_as(url: &str, token: &str, body: &str, session: Option<&str>) -> Option<()> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };

    let addr = authority.parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).ok()?;
    stream.set_write_timeout(Some(TIMEOUT)).ok()?;

    let req = request(path, authority, token, session, body);
    stream.write_all(req.as_bytes()).ok()?;
    // The daemon's reply is deliberately not read: nothing here acts on it,
    // and not waiting keeps the agent's turn from stalling on a slow answer.
    Some(())
}

fn post_response(url: &str, token: &str, body: &str) -> Option<(u16, String)> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr = authority.parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).ok()?;
    stream.set_write_timeout(Some(TIMEOUT)).ok()?;
    stream.set_read_timeout(Some(TIMEOUT)).ok()?;
    stream
        .write_all(request(path, authority, token, None, body).as_bytes())
        .ok()?;

    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let (head, body) = response.split_once("\r\n\r\n")?;
    let status = head.split_whitespace().nth(1)?.parse().ok()?;
    Some((status, body.to_string()))
}

/// Headers are assembled by hand rather than with a client library, so
/// each one must start its own line at column zero: a header the daemon
/// cannot recognize is not an error it can report, only a report that
/// quietly does nothing — a session header it misses files a child's work
/// under its parent's row, and a Content-Length it misses drops the note.
fn request(path: &str, authority: &str, token: &str, session: Option<&str>, body: &str) -> String {
    let session = match session.filter(|id| !id.is_empty()) {
        Some(id) => format!("{SESSION_HEADER}: {id}\r\n"),
        None => String::new(),
    };
    let mut req = String::new();
    req.push_str(&format!("POST {path} HTTP/1.1\r\n"));
    req.push_str(&format!("Host: {authority}\r\n"));
    req.push_str(&format!("Authorization: Bearer {token}\r\n"));
    req.push_str(&session);
    req.push_str(&format!("Content-Length: {}\r\n", body.len()));
    req.push_str("Connection: close\r\n\r\n");
    req.push_str(body);
    req
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_names_the_conversation_it_came_from() {
        // What lets the daemon tell the pane's own agent from a CLI started
        // inside it, which inherits the same URL and token.
        let tagged = request(
            "/pane/1/status/idle",
            "127.0.0.1:4242",
            "tok",
            Some("s-1"),
            "",
        );
        assert!(tagged.contains("\r\nX-Argus-Session: s-1\r\n"), "{tagged}");
        let untagged = request("/pane/1/status/idle", "127.0.0.1:4242", "tok", None, "");
        assert!(!untagged.contains("X-Argus-Session"), "{untagged}");
        assert!(untagged.contains("\r\nContent-Length: 0\r\n"), "{untagged}");
    }

    #[test]
    fn every_header_starts_its_own_line_at_column_zero() {
        // An indented header line is a continuation of the one above it, so
        // a stray space here is invisible on the wire and silently costs the
        // daemon whichever header it swallowed: a session header it misses
        // files a child's report on its parent's row, and a Content-Length
        // it misses drops the note the report was carrying.
        let req = request("/pane/1/title", "127.0.0.1:4242", "tok", Some("s-1"), "hi");
        let (head, body) = req
            .split_once("\r\n\r\n")
            .expect("a blank line ends the headers");
        assert_eq!(body, "hi");
        for line in head.split("\r\n") {
            assert_eq!(line.trim_start(), line, "indented header line: {req:?}");
            assert!(!line.is_empty(), "blank header line: {req:?}");
        }
        assert!(head.contains("\r\nContent-Length: 2\r\n"), "{req:?}");
    }

    #[test]
    fn a_json_event_gives_up_the_message_a_human_would_read() {
        assert_eq!(
            note_from(
                r#"{"session_id":"x","message":"Claude needs your permission to run tests"}"#
            ),
            "Claude needs your permission to run tests"
        );
    }

    #[test]
    fn one_json_event_can_supply_a_note_and_session_id() {
        let raw = r#"{"session_id":"session-123","message":"waiting"}"#;
        assert_eq!(note_from(raw), "waiting");
        assert_eq!(
            json_string(raw, "session_id").as_deref(),
            Some("session-123")
        );
    }

    #[test]
    fn cursor_session_start_names_the_id_session_id() {
        // sessionStart's documented payload uses session_id; other events
        // put the same value on conversation_id. The helper asks for either.
        let start = r#"{"session_id":"conv-9","composer_mode":"agent"}"#;
        assert_eq!(
            json_string(start, "conversation_id").as_deref(),
            Some("conv-9")
        );
        let tool = r#"{"conversation_id":"conv-9","tool_name":"Shell"}"#;
        assert_eq!(json_string(tool, "session_id").as_deref(), Some("conv-9"));
    }

    #[test]
    fn hook_stdin_stops_at_one_json_object_without_waiting_for_eof() {
        struct JsonThenHang {
            data: &'static [u8],
        }
        impl Read for JsonThenHang {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.data.is_empty() {
                    panic!("hook stdin was read past the JSON object");
                }
                let n = self.data.len().min(buf.len());
                buf[..n].copy_from_slice(&self.data[..n]);
                self.data = &self.data[n..];
                Ok(n)
            }
        }
        let raw = read_hook_input(JsonThenHang {
            data: br#"{"conversation_id":"conv-9","tool_name":"Shell"}"#,
        });
        assert_eq!(
            json_string(&raw, "conversation_id").as_deref(),
            Some("conv-9")
        );
    }

    #[test]
    fn hook_stdin_plain_text_still_reads_to_eof() {
        assert_eq!(
            read_hook_input(std::io::Cursor::new("waiting on review\n")),
            "waiting on review\n"
        );
    }

    #[test]
    fn cursor_tool_hooks_allow_with_permission_and_claude_with_decision() {
        assert_eq!(
            hook_reply(
                Some(r#"{"tool_name":"Shell","conversation_id":"c"}"#),
                false,
                ""
            ),
            r#"{"permission":"allow"}"#
        );
        assert_eq!(
            hook_reply(Some(r#"{"toolCall":{"name":"Bash"}}"#), false, ""),
            r#"{"decision":"allow"}"#
        );
        assert_eq!(hook_reply(Some(r#"{"session_id":"c"}"#), false, ""), "{}");
    }

    #[test]
    fn a_checkout_wide_hook_url_rebases_to_the_process_pane() {
        assert_eq!(
            rebase_hook_url(
                "http://127.0.0.1:4242/pane/1/status/idle",
                "http://127.0.0.1:4242/pane/9"
            )
            .as_deref(),
            Some("http://127.0.0.1:4242/pane/9/status/idle")
        );
        assert!(rebase_hook_url(
            "http://127.0.0.1:4242/pane/1/status/idle",
            "http://127.0.0.1:9999/pane/9"
        )
        .is_none());
    }

    #[test]
    fn a_rebased_hook_uses_the_process_token_too() {
        assert_eq!(
            routed_hook(
                "http://127.0.0.1:4242/pane/1/status/idle",
                "configured-token",
                "http://127.0.0.1:4242/pane/9",
                "process-token"
            ),
            (
                "http://127.0.0.1:4242/pane/9/status/idle".to_string(),
                "process-token".to_string()
            )
        );
    }

    #[test]
    fn an_incomplete_or_foreign_process_pair_keeps_the_configured_pair() {
        let configured = "http://127.0.0.1:4242/pane/1/status/idle";
        assert_eq!(
            routed_hook(configured, "configured-token", "", "process-token"),
            (configured.to_string(), "configured-token".to_string())
        );
        assert_eq!(
            routed_hook(
                configured,
                "configured-token",
                "http://127.0.0.1:9999/pane/9",
                "process-token"
            ),
            (configured.to_string(), "configured-token".to_string())
        );
        assert_eq!(
            routed_hook(
                configured,
                "configured-token",
                "http://127.0.0.1:4242/pane/9",
                ""
            ),
            (configured.to_string(), "configured-token".to_string())
        );
    }

    #[test]
    fn plain_text_falls_back_to_its_first_real_line() {
        // A harness that hands its hooks text rather than JSON.
        assert_eq!(
            note_from("\n\n  waiting on review  \nmore"),
            "waiting on review"
        );
    }

    #[test]
    fn json_with_nothing_recognizable_shows_nothing() {
        // Better an empty note than a wall of serialized event under a row.
        assert_eq!(note_from(r#"{"session_id":"x","cwd":"/tmp"}"#), "");
        assert_eq!(note_from(""), "");
    }

    #[test]
    fn a_prompt_event_gives_up_the_text_the_daemon_should_name_the_row() {
        // Cursor beforeSubmitPrompt and Claude UserPromptSubmit both put
        // the user's text on `prompt`. That is what a column of "claude"
        // rows is missing: the task, without waiting for the model to
        // remember `argus-hook title`.
        assert_eq!(
            title_from(
                r#"{"conversation_id":"c","prompt":"fixing the pty deadlock","attachments":[]}"#
            ),
            "fixing the pty deadlock"
        );
        assert_eq!(
            title_from(r#"{"session_id":"s","prompt":"  review split view  "}"#),
            "review split view"
        );
    }

    #[test]
    fn a_tool_start_event_is_not_a_title() {
        // preToolUse stdin names a tool. Using that as the row would label
        // every working pane "Shell".
        assert_eq!(
            title_from(r#"{"conversation_id":"c","tool_name":"Shell","toolCall":{"name":"Bash"}}"#),
            ""
        );
        assert_eq!(title_from(r#"{"session_id":"x","cwd":"/tmp"}"#), "");
    }

    #[test]
    fn a_title_flag_reads_the_prompt_without_turning_it_into_a_note() {
        let raw = r#"{"conversation_id":"c","prompt":"fixing the pty deadlock"}"#;
        let (key, body, note, title) = installed_input(
            &["tok", TITLE_FLAG, SESSION_KEY_FLAG, "conversation_id"],
            std::io::Cursor::new(raw),
        );
        assert_eq!(key, Some("conversation_id"));
        assert_eq!(body.as_deref(), Some(raw));
        assert_eq!(note, "");
        assert_eq!(title, "fixing the pty deadlock");
    }

    #[test]
    fn without_the_title_flag_a_prompt_event_does_not_rename_the_row() {
        let (key, _, note, title) = installed_input(
            &["tok", SESSION_KEY_FLAG, "conversation_id"],
            std::io::Cursor::new(r#"{"conversation_id":"c","prompt":"secret task"}"#),
        );
        assert_eq!(key, Some("conversation_id"));
        assert_eq!(note, "");
        assert_eq!(title, "");
    }

    #[test]
    fn a_prompt_title_posts_to_the_pane_title_endpoint() {
        use std::io::BufRead as _;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            let len = head
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            (head, String::from_utf8(body).unwrap())
        });

        post_title(
            &format!("http://{address}/pane/7/status/working"),
            "tok",
            "fixing the pty deadlock",
            Some("c"),
        );
        let (head, body) = server.join().unwrap();
        assert!(
            head.starts_with("POST /pane/7/title HTTP/1.1\r\n"),
            "{head}"
        );
        assert!(head.contains("\r\nX-Argus-Session: c\r\n"), "{head}");
        assert_eq!(body, "fixing the pty deadlock");
    }

    #[test]
    fn an_explicit_checkout_path_keeps_spaces() {
        assert_eq!(
            reported_checkout(&["C:\\Source\\my", "checkout"]),
            Some(std::path::PathBuf::from("C:\\Source\\my checkout"))
        );
    }

    #[test]
    fn checkout_without_a_path_reports_the_current_directory() {
        assert_eq!(reported_checkout(&[]), std::env::current_dir().ok());
    }

    #[test]
    fn comments_are_read_from_the_daemon_and_rendered_in_order() {
        let comments = vec![ReviewComment {
            id: 4,
            anchor: argus_protocol::ReviewAnchor {
                base: argus_protocol::ReviewBase::Staged,
                commit: None,
                path: "src/main.rs".to_string(),
                old_path: None,
                old_start: Some(9),
                old_end: Some(9),
                new_start: Some(10),
                new_end: Some(10),
                text: vec!["+changed".to_string()],
            },
            body: "fix this".to_string(),
        }];
        let (address, server) = serve_once(&serde_json::to_string(&comments).unwrap());

        let message = comments_message(&[], &format!("http://{address}/pane/4"), "secret");
        let head = server.join().unwrap();

        assert_eq!(message, "#4 [staged] src/main.rs:10 `+changed`: fix this");
        assert!(head.starts_with("POST /pane/4/comments HTTP/1.1\r\n"));
        assert!(head.contains("\r\nAuthorization: Bearer secret\r\n"));
        assert_eq!(
            comments_message(&["extra"], "", ""),
            "could not read comments: comments takes no arguments"
        );
    }

    #[test]
    fn the_item_grammar_is_add_done_and_open() {
        assert_eq!(
            parse_todo_args(&["add", "ported", "the", "parser"]),
            Some(TodoWrite::Add {
                text: "ported the parser".to_string()
            })
        );
        // The line a person reads off the note is 1-based; the wire is not.
        assert_eq!(
            parse_todo_args(&["done", "4"]),
            Some(TodoWrite::Set {
                line: 3,
                state: TodoState::Done
            })
        );
        assert_eq!(
            parse_todo_args(&["open", "1"]),
            Some(TodoWrite::Set {
                line: 0,
                state: TodoState::Open
            })
        );
        for bad in [
            vec![],
            vec!["add"],
            vec!["add", "   "],
            vec!["done"],
            vec!["done", "0"],
            vec!["done", "last"],
            vec!["pin", "1"],
        ] {
            assert_eq!(parse_todo_args(&bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_written_item_reports_what_the_note_now_holds() {
        let (address, server) = serve_status(200, "2 open, 1 done");

        let message = todo_message(
            &["add", "ported the parser"],
            &format!("http://{address}/pane/4"),
            "secret",
        );

        assert_eq!(message, "added \"ported the parser\" — 2 open, 1 done");
        assert!(server
            .join()
            .unwrap()
            .starts_with("POST /pane/4/todo HTTP/1.1\r\n"));
    }

    #[test]
    fn a_refused_item_reports_the_daemons_reason() {
        let refusal = "project proj does not allow agents to write notes";
        let (address, server) = serve_status(409, refusal);

        let message = todo_message(&["done", "2"], &format!("http://{address}/pane/1"), "t");

        assert_eq!(
            message,
            "could not write the note: project proj does not allow agents to write notes"
        );
        let _ = server.join();
        assert_eq!(
            todo_message(&["nonsense"], "", ""),
            "could not write the note: expected `todo add <text>`, \
             `todo done <line>`, or `todo open <line>`"
        );
        assert_eq!(
            todo_message(&["done", "2"], "http://127.0.0.1:1/pane/1", "t"),
            "could not write the note: daemon unavailable"
        );
    }

    #[test]
    fn the_decision_grammar_puts_the_choice_first_and_the_rest_behind_flags() {
        let write = parse_decide_args(&[
            "one",
            "row",
            "per",
            "note",
            "--over",
            "a table per note",
            "--because",
            "the key is durable",
            "--under",
            "3",
        ])
        .unwrap();
        assert_eq!(write.chose, "one row per note");
        assert_eq!(write.over.as_deref(), Some("a table per note"));
        assert_eq!(write.because.as_deref(), Some("the key is durable"));
        assert_eq!(write.under, Some(3));
        assert_eq!(write.supersedes, None);
    }

    #[test]
    fn a_decision_that_could_not_be_recorded_says_which_part_was_wrong() {
        for (bad, expected) in [
            (vec!["--over", "nothing chosen"], "what was chosen"),
            (vec!["x", "--under"], "needs something after it"),
            (vec!["x", "--under", "zero"], "wants a decision number"),
            (vec!["x", "--why", "no"], "is not one of"),
            (
                vec!["x", "--under", "1", "--supersedes", "2"],
                "not both",
            ),
        ] {
            let message = decide_message(&bad, "", "");
            assert!(
                message.starts_with("could not record the decision:")
                    && message.contains(expected),
                "{bad:?} gave {message:?}"
            );
        }
    }

    #[test]
    fn a_recorded_decision_reports_the_id_the_next_one_hangs_off() {
        let recorded = r#"{"id":7,"parent":3,"at":1,"session":null,"checkout":null,
             "chose":"one row per note","over":null,"because":null,"superseded_by":null}"#;
        let (address, server) = serve_once(recorded);

        let message = decide_message(
            &["one", "row", "per", "note", "--under", "3"],
            &format!("http://{address}/pane/4"),
            "secret",
        );

        assert_eq!(
            message,
            "recorded decision #7 under #3: one row per note"
        );
        assert!(server
            .join()
            .unwrap()
            .starts_with("POST /pane/4/decide HTTP/1.1\r\n"));
    }

    #[test]
    fn a_feature_is_read_as_its_brief_and_then_its_reasoning() {
        let board = r#"{"project":null,"project_name":"argus","current":"notes-storage",
            "unfiled":2,
            "features":[{"slug":"notes-storage","title":"Notes storage","body":"keys outlive ids",
                         "origin_checkout":null,"origin_branch":"notes","at":1,"session":null}],
            "decisions":[
              {"id":1,"parent":null,"at":1,"session":null,"checkout":null,
               "feature":"notes-storage","chose":"one row per note","over":"a table per note",
               "because":null,"superseded_by":null}]}"#;
        let (address, server) = serve_once(board);

        let message = feature_message(&[], &format!("http://{address}/pane/4"), "t");
        let _ = server.join();

        assert_eq!(
            message,
            "Feature: Notes storage (notes-storage)\n\
             Started on notes.\n\
             \n\
             keys outlive ids\n\
             \n\
             Decided under it, newest last:\n\
             #1 one row per note\n\
             \x20     over: a table per note"
        );
    }

    #[test]
    fn a_checkout_on_no_feature_is_told_what_to_do_about_it() {
        let board = r#"{"project":null,"project_name":"argus","current":null,"unfiled":3,
            "features":[{"slug":"the-pty-deadlock","title":"The pty deadlock","body":"",
                         "origin_checkout":null,"origin_branch":null,"at":1,"session":null}],
            "decisions":[]}"#;
        let (address, server) = serve_once(board);

        let message = feature_message(&[], &format!("http://{address}/pane/4"), "t");
        let _ = server.join();

        assert!(message.starts_with("This checkout is not on a feature yet"), "{message}");
        assert!(message.contains("the-pty-deadlock — The pty deadlock"), "{message}");
        assert!(message.contains("3 older decision(s)"), "{message}");
    }

    #[test]
    fn a_board_is_drawn_as_the_tree_it_is() {
        let board = r#"{"project":null,"name":"argus","decisions":[
            {"id":1,"parent":null,"at":1,"session":null,"checkout":null,
             "chose":"sqlite","over":"a file per feature",
             "because":"both need migrations","superseded_by":null},
            {"id":2,"parent":1,"at":2,"session":null,"checkout":null,
             "chose":"one table per note","over":null,"because":null,
             "superseded_by":3},
            {"id":3,"parent":1,"at":3,"session":null,"checkout":null,
             "chose":"one row per note","over":null,"because":null,
             "superseded_by":null},
            {"id":4,"parent":2,"at":4,"session":null,"checkout":null,
             "chose":"store the body","over":null,"because":null,
             "superseded_by":null}]}"#;
        let (address, server) = serve_once(board);

        let message = decisions_message(&[], &format!("http://{address}/pane/4"), "t");
        let _ = server.join();

        assert_eq!(
            message,
            "Decisions on argus, newest last:\n\
             #1 sqlite\n\
             │     over: a file per feature\n\
             │     because: both need migrations\n\
             ├─ #2 one table per note  (superseded by #3)\n\
             │  └─ #4 store the body\n\
             └─ #3 one row per note"
        );
    }

    #[test]
    fn an_empty_board_says_so_rather_than_nothing() {
        let (address, server) = serve_once(r#"{"project":null,"name":"argus","decisions":[]}"#);
        let message = decisions_message(&[], &format!("http://{address}/pane/4"), "t");
        let _ = server.join();
        assert_eq!(message, "nothing decided under this feature yet");
    }

    /// Serves one canned response at a chosen status, for the endpoint that
    /// has something to say when it refuses.
    fn serve_status(
        status: u16,
        body: &str,
    ) -> (std::net::SocketAddr, std::thread::JoinHandle<String>) {
        serve_response(&format!(
            "HTTP/1.1 {status} Whatever\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ))
    }

    /// Serves one canned JSON response and hands back the request head, so
    /// a rendering test can also assert what went over the wire.
    fn serve_once(body: &str) -> (std::net::SocketAddr, std::thread::JoinHandle<String>) {
        serve_response(&format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ))
    }

    fn serve_response(response: &str) -> (std::net::SocketAddr, std::thread::JoinHandle<String>) {
        use std::io::BufRead as _;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let response = response.to_string();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            reader.get_mut().write_all(response.as_bytes()).unwrap();
            head
        });
        (address, server)
    }

    fn context_note(
        scope: argus_protocol::ContextScope,
        name: &str,
        body: &str,
    ) -> argus_protocol::ContextNote {
        argus_protocol::ContextNote::new(scope, name.to_string(), body.to_string())
    }

    #[test]
    fn context_renders_standing_instructions_ahead_of_the_notes_they_came_from() {
        use argus_protocol::ContextScope;

        let context = AgentContext {
            notes: vec![
                context_note(ContextScope::Project, "orion", "- [!] house style\n"),
                context_note(ContextScope::Checkout, "/wt/a", "# Branch\n- [ ] a task\n"),
            ],
        };
        let (address, server) = serve_once(&serde_json::to_string(&context).unwrap());

        let message = context_message(&[], &format!("http://{address}/pane/4"), "secret");
        let head = server.join().unwrap();

        assert_eq!(
            message,
            "Standing instructions, which apply without being asked for:\n\
             - (project) house style\n\
             \n\
             --- project note: orion ---\n\
             - [!] house style\n\
             \n\
             --- checkout note: /wt/a ---\n\
             # Branch\n\
             - [ ] a task"
        );
        assert!(head.starts_with("POST /pane/4/context HTTP/1.1\r\n"));
        assert!(head.contains("\r\nAuthorization: Bearer secret\r\n"));
    }

    #[test]
    fn context_with_nothing_written_down_says_so_rather_than_nothing() {
        let (address, server) = serve_once("{\"notes\":[]}");

        let message = context_message(&[], &format!("http://{address}/pane/1"), "t");
        server.join().unwrap();

        assert_eq!(message, "no notes for this checkout");
        assert_eq!(
            context_message(&["extra"], "", ""),
            "could not read context: context takes no arguments"
        );
        assert_eq!(
            context_message(&[], "http://127.0.0.1:1/pane/1", "t"),
            "could not read context: daemon unavailable"
        );
    }
}
