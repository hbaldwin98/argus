//! `argus-hook task`: the current feature's task tree, its writes parsed from
//! arguments and the tree drawn for the model to read.

use crate::*;

pub(crate) fn task(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", task_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

/// `task`, `task add`, `task add <title> --under <id>`, `task doing
/// <id>`, `task done <id>`, `task todo <id>`, `task retitle <id> <text>`,
/// `task brief <id> <text>`, `task drop <id>`, `task move <id>` with one
/// of `--under <id>`, `--top`, `--before <id>`, `--after <id>` or `--to
/// <feature>`, `task criterion add <id> <text>`, `task criterion check|
/// uncheck|drop <id> <n>`, and `task note <id> <text>`.
///
/// The columns are named as verbs rather than hidden behind a state
/// argument, so what an agent types is what a reader of the transcript
/// understands happened; `move` is where a task sits, never its state.
/// `--under` records newly discovered work beneath the task that exposed
/// it.
pub(crate) fn task_message(rest: &[&str], base_url: &str, token: &str) -> String {
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
        Some("criterion") => match parse_task_criterion_args(&rest[1..]) {
            Ok(action) => write_task(action, base_url, token),
            Err(message) => format!("could not change task: {message}"),
        },
        Some("note") => match rest.get(1).and_then(|id| id.parse::<i64>().ok()) {
            Some(id) => write_task(
                TaskAction::Note {
                    id,
                    text: rest[2..].join(" "),
                },
                base_url,
                token,
            ),
            None => "could not change task: note wants the number `task` prints".to_string(),
        },
        Some(other) => format!(
            "could not change task: `{other}` is not one of add, doing, done, todo, retitle, \
             brief, drop, move, criterion, note"
        ),
    }
}

/// `add <id> <text>`, or `check`, `uncheck` or `drop` with `<id> <n>`.
///
/// `n` is the number `task` prints beside the criterion, counted from one
/// because a person reads it; the daemon is told its place in the list.
fn parse_task_criterion_args(args: &[&str]) -> Result<TaskAction, String> {
    const USAGE: &str = "criterion wants add <task> <text>, or check, uncheck or drop with \
                         <task> <n> as `task` numbers them";
    let (Some(verb), Some(id)) = (
        args.first().copied(),
        args.get(1).and_then(|id| id.parse::<i64>().ok()),
    ) else {
        return Err(USAGE.to_string());
    };
    if verb == "add" {
        return Ok(TaskAction::AddCriterion {
            id,
            text: args[2..].join(" "),
        });
    }
    let [_, _, n] = args else {
        return Err(USAGE.to_string());
    };
    let index = n
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .ok_or_else(|| format!("{verb} wants the criterion's number, counted from 1, not {n:?}"))?;
    match verb {
        "check" => Ok(TaskAction::SetCriterion {
            id,
            index,
            done: true,
        }),
        "uncheck" => Ok(TaskAction::SetCriterion {
            id,
            index,
            done: false,
        }),
        "drop" => Ok(TaskAction::RemoveCriterion { id, index }),
        _ => Err(USAGE.to_string()),
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

fn write_task(action: TaskAction, base_url: &str, token: &str) -> String {
    match call::<hook::call::Tasks>(base_url, token, &action) {
        Ok(list) => format_tasks(&list),
        Err(Refusal::Unreadable) => "the task list changed".to_string(),
        Err(refusal) => refusal.said("could not change task"),
    }
}

/// The list as an agent reads it: the number it needs to name a task, the
/// column it is in, and the tracker key if it came from one.
pub(crate) fn format_tasks(list: &TaskList) -> String {
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
        let who = match (&task.state, &task.claimed_by, task.claimed_pane) {
            (TaskState::Doing, Some(session), Some(pane)) => format!(" — {session} (pane {pane})"),
            (TaskState::Doing, Some(session), None) => format!(" — {session}"),
            (TaskState::Doing, None, Some(pane)) => format!(" — pane {pane}"),
            _ => String::new(),
        };
        lines.push(format!(
            "{}#{:<3} {:<5} {}{key}{who}",
            task_branch(&row),
            task.id,
            task.state,
            task.title
        ));
        let prefix = task_body_prefix(&row);
        if let Some(body) = &task.body {
            lines.extend(body.lines().map(|line| format!("{prefix}{line}")));
        }
        // Numbered, because the number is what `criterion check` takes.
        for (index, criterion) in task.criteria.iter().enumerate() {
            let mark = if criterion.done { 'x' } else { ' ' };
            lines.push(format!("{prefix}[{mark}] {}. {}", index + 1, criterion.text));
        }
        let earlier = task.notes.len().saturating_sub(SHOWN_TASK_NOTES);
        if earlier > 0 {
            lines.push(format!("{prefix}({earlier} earlier note(s))"));
        }
        for note in &task.notes[earlier..] {
            let who = match (&note.session, note.pane) {
                (Some(session), _) => session.clone(),
                (None, Some(pane)) => format!("pane {pane}"),
                (None, None) => "a person".to_string(),
            };
            lines.push(format!("{prefix}note, {who}: {}", note.text));
        }
    }
    lines.join("\n")
}

/// The notes printed under a task: where the work stands is the last few
/// lines, and the list is read at the start of every piece of work.
const SHOWN_TASK_NOTES: usize = 3;

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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_criterion_is_named_by_the_number_a_person_reads() {
        let parse = |args: &[&str]| parse_task_criterion_args(args);
        assert_eq!(
            parse(&["add", "12", "a", "full", "queue", "refuses"]),
            Ok(TaskAction::AddCriterion { id: 12, text: "a full queue refuses".into() })
        );
        assert_eq!(parse(&["check", "12", "1"]), Ok(TaskAction::SetCriterion { id: 12, index: 0, done: true }));
        assert_eq!(
            parse(&["uncheck", "12", "3"]),
            Ok(TaskAction::SetCriterion { id: 12, index: 2, done: false })
        );
        assert_eq!(parse(&["drop", "12", "2"]), Ok(TaskAction::RemoveCriterion { id: 12, index: 1 }));
        for bad in [
            &[][..],
            &["check"],
            &["check", "twelve", "1"],
            &["check", "12"],
            &["check", "12", "0"],
            &["check", "12", "first"],
            &["check", "12", "1", "2"],
            &["tick", "12", "1"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        let message = task_message(&["note"], "http://127.0.0.1:1/pane/1", "t");
        assert!(message.contains("note wants the number"), "{message}");
    }

    #[test]
    fn a_task_is_printed_with_its_criteria_checked_and_its_latest_notes() {
        let list: TaskList = serde_json::from_str(
            r#"{"project_name":"argus","feature":"queue","tasks":[
                {"id":12,"feature":"queue","parent":null,"title":"bound the queue","body":null,
                 "state":"doing","claimed_by":"sess-1","claimed_pane":4,"external":null,
                 "position":0,"at":1,"session":null,
                 "criteria":[{"text":"a full queue refuses","done":true},
                             {"text":"a drained one accepts"}],
                 "notes":[{"at":1,"session":"sess-1","pane":4,"text":"first"},
                          {"at":2,"session":"sess-1","pane":4,"text":"second"},
                          {"at":3,"session":null,"pane":4,"text":"third"},
                          {"at":4,"session":null,"text":"ship the bound first"}]},
                {"id":13,"feature":"queue","parent":12,"title":"under load","body":null,
                 "state":"doing","claimed_by":null,"claimed_pane":5,"external":null,
                 "position":0,"at":1,"session":null,
                 "criteria":[{"text":"holds at 10k","done":false}]}
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            format_tasks(&list),
            "Tasks under queue:\n\
             #12  doing bound the queue — sess-1 (pane 4)\n\
             \x20     [x] 1. a full queue refuses\n\
             \x20     [ ] 2. a drained one accepts\n\
             \x20     (1 earlier note(s))\n\
             \x20     note, sess-1: second\n\
             \x20     note, pane 4: third\n\
             \x20     note, a person: ship the bound first\n\
             └─ #13  doing under load — pane 5\n\
             \x20     [ ] 1. holds at 10k"
        );
    }
}
