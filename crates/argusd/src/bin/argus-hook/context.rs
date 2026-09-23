//! What an agent reads before it starts: the pane's review comments, the
//! checkout's feature brief, and the tasks still open under it — one answer,
//! sized to sit in the model's context for the whole session.
//!
//! One command rather than the three reads it replaces because every tool
//! call re-reads the conversation so far; a context hook that prints this at
//! session start costs the agent no calls at all. Deliberately a summary:
//! finished tasks, task briefs and the decision tree are counted rather than
//! printed, and `feature` and `task` still show them in full.

use super::*;

/// Prints `preface` (the harness's instructions, when a hook passes them)
/// and then the live context.
pub(super) fn context(preface: &[&str]) {
    let preface = preface.join(" ");
    let body = context_message(&env_url(), &env_token());
    let text = [preface.trim(), body.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() {
        return;
    }
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{text}");
    let _ = out.flush();
}

pub(super) fn context_message(base_url: &str, token: &str) -> String {
    // No pane to read for: say nothing rather than three refusals, since a
    // context hook also fires in checkouts where Argus is not running.
    if base_url.is_empty() {
        return String::new();
    }
    let comments = comments_message(&[], base_url, token);
    let board = read_feature_board(&[], base_url, token);
    let tasks = read_tasks(base_url, token);
    format_context(&comments, board, tasks)
}

fn read_tasks(base_url: &str, token: &str) -> Result<TaskList, String> {
    let body = serde_json::to_string(&TaskAction::List).unwrap_or_default();
    let url = endpoint_url(base_url, Endpoint::Tasks);
    let Some((status, response)) = post_response(&url, token, &body) else {
        return Err("could not read tasks: daemon unavailable".to_string());
    };
    if status != 200 {
        return Err("could not read tasks: daemon refused the request".to_string());
    }
    serde_json::from_str(&response)
        .map_err(|_| "could not read tasks: invalid daemon response".to_string())
}

pub(super) fn format_context(
    comments: &str,
    board: Result<FeatureBoard, String>,
    tasks: Result<TaskList, String>,
) -> String {
    let mut sections = vec![
        "Argus pane context, as of now (`\"$ARGUS_HOOK\" context` refreshes it):".to_string(),
        format!("Review comments:\n{comments}"),
    ];
    let board = match board {
        Ok(board) => board,
        Err(message) => {
            sections.push(message);
            return sections.join("\n\n");
        }
    };
    let Some(current) = board
        .current
        .as_ref()
        .and_then(|slug| board.features.iter().find(|f| &f.slug == slug))
    else {
        sections.push(
            "This checkout is not on a feature. `\"$ARGUS_HOOK\" feature list` shows the \
             project's features."
                .to_string(),
        );
        return sections.join("\n\n");
    };

    let mut feature = vec![format!("Feature: {} ({})", current.title, current.slug)];
    if !current.body.trim().is_empty() {
        feature.push(current.body.trim().to_string());
    }
    if !board.decisions.is_empty() {
        feature.push(format!(
            "{} decision(s) recorded; `\"$ARGUS_HOOK\" feature` shows them.",
            board.decisions.len()
        ));
    }
    sections.push(feature.join("\n"));

    sections.push(match tasks {
        Ok(list) => format_open_tasks(&list),
        Err(message) => message,
    });
    sections.join("\n\n")
}

/// Open tasks only, one line each. Parents stay in tree order so a subtask
/// still reads beneath the work it belongs to, and a finished parent of an
/// open subtask is named rather than listed.
fn format_open_tasks(list: &TaskList) -> String {
    let rows = list.tree_rows();
    let done = rows
        .iter()
        .filter(|row| row.task.state == TaskState::Done)
        .count();
    let open: Vec<String> = rows
        .iter()
        .filter(|row| row.task.state != TaskState::Done)
        .map(|row| {
            let under = match row.task.parent {
                Some(parent) => format!(" (under #{parent})"),
                None => String::new(),
            };
            format!(
                "#{} {} {}{under}",
                row.task.id, row.task.state, row.task.title
            )
        })
        .collect();
    if rows.is_empty() {
        return "No tasks under this feature yet.".to_string();
    }
    if open.is_empty() {
        return format!("All {done} task(s) under this feature are done.");
    }
    format!(
        "Open tasks ({done} of {} done):\n{}",
        rows.len(),
        open.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tasks() -> TaskList {
        serde_json::from_str(
            r#"{"project_name":"argus","feature":"notes","tasks":[
                {"id":1,"feature":"notes","parent":null,"title":"root","body":"long brief",
                 "state":"done","claimed_by":null,"external":null,"position":0,"at":1,"session":null},
                {"id":2,"feature":"notes","parent":1,"title":"child","body":null,
                 "state":"doing","claimed_by":null,"external":null,"position":0,"at":2,"session":null},
                {"id":3,"feature":"notes","parent":null,"title":"other","body":null,
                 "state":"todo","claimed_by":null,"external":null,"position":1,"at":3,"session":null}
            ]}"#,
        )
        .unwrap()
    }

    fn board(current: Option<&str>) -> FeatureBoard {
        let current = match current {
            Some(slug) => format!("\"{slug}\""),
            None => "null".to_string(),
        };
        serde_json::from_str(&format!(
            r#"{{"project":null,"project_name":"argus","current":{current},"unfiled":0,
                "features":[{{"slug":"notes","title":"Notes","body":"keys outlive ids",
                              "origin_checkout":null,"origin_branch":null,"at":1,"session":null}}],
                "decisions":[{{"id":1,"parent":null,"at":1,"session":null,"checkout":null,
                   "feature":"notes","chose":"one row","over":null,"because":null,
                   "superseded_by":null}}]}}"#
        ))
        .unwrap()
    }

    #[test]
    fn context_carries_the_brief_and_only_open_tasks() {
        let text = format_context("no review comments", Ok(board(Some("notes"))), Ok(tasks()));
        assert_eq!(
            text,
            "Argus pane context, as of now (`\"$ARGUS_HOOK\" context` refreshes it):\n\
             \n\
             Review comments:\n\
             no review comments\n\
             \n\
             Feature: Notes (notes)\n\
             keys outlive ids\n\
             1 decision(s) recorded; `\"$ARGUS_HOOK\" feature` shows them.\n\
             \n\
             Open tasks (1 of 3 done):\n\
             #2 doing child (under #1)\n\
             #3 todo other"
        );
        assert!(!text.contains("long brief"), "task briefs stay out");
    }

    #[test]
    fn context_without_a_feature_points_at_the_list() {
        let text = format_context("no review comments", Ok(board(None)), Ok(tasks()));
        assert!(text.ends_with("`\"$ARGUS_HOOK\" feature list` shows the project's features."));
        assert!(!text.contains("Open tasks"));
    }

    #[test]
    fn context_outside_a_pane_is_silent() {
        assert_eq!(context_message("", ""), "");
    }
}
