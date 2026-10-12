//! What an agent reads before it starts: the pane's review comments, and
//! which feature the checkout is on — one answer, sized to sit in the model's
//! context for the whole session.
//!
//! Review comments are printed whole because they are addressed to this
//! agent. The feature is only named, with how much is filed under it: a
//! brief sits in every request of a session whether or not the work touches
//! it, so `feature` and `task` show it to the agent that asks. Its open
//! questions are the exception, and are listed.

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

    let open: Vec<&Decision> = board.decisions.iter().filter(|d| d.open()).collect();
    let mut feature = vec![format!(
        "Feature{}: {} ({}), {}, {} decision(s).",
        super::board::feature_number(current),
        current.title,
        current.slug,
        match &tasks {
            Ok(list) => open_task_count(list),
            Err(message) => message.clone(),
        },
        board.decisions.len() - open.len()
    )];
    feature.extend(super::board::parked_lines(current, &board.features));
    // Named here, where the brief is not, because this is the one thing on
    // the board an agent can get wrong by not having read it: a question
    // left for a person looks, from inside the work, like a choice to make.
    if !open.is_empty() {
        feature.push(format!(
            "{} open question(s), the person's to answer — do not decide them alone:",
            open.len()
        ));
        feature.extend(open.iter().map(|d| format!("  #{} {}", d.id, d.chose)));
    }
    feature.push(
        "When the work touches it, `\"$ARGUS_HOOK\" feature` shows its brief and decisions \
         and `\"$ARGUS_HOOK\" task` its tasks."
            .to_string(),
    );
    sections.push(feature.join("\n"));
    sections.join("\n\n")
}

fn open_task_count(list: &TaskList) -> String {
    let open = list
        .tasks
        .iter()
        .filter(|task| task.state != TaskState::Done)
        .count();
    format!("{open} of {} task(s) open", list.tasks.len())
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
    fn context_names_the_feature_and_leaves_its_brief_to_be_asked_for() {
        let text = format_context("no review comments", Ok(board(Some("notes"))), Ok(tasks()));
        assert_eq!(
            text,
            "Argus pane context, as of now (`\"$ARGUS_HOOK\" context` refreshes it):\n\
             \n\
             Review comments:\n\
             no review comments\n\
             \n\
             Feature: Notes (notes), 2 of 3 task(s) open, 1 decision(s).\n\
             When the work touches it, `\"$ARGUS_HOOK\" feature` shows its brief and decisions \
             and `\"$ARGUS_HOOK\" task` its tasks."
        );
        assert!(!text.contains("keys outlive ids"), "the brief stays out");
    }

    #[test]
    fn context_says_what_holds_the_feature() {
        let mut board = board(Some("notes"));
        board.features[0].held = Some("until the user answers".to_string());
        let text = format_context("no review comments", Ok(board), Ok(tasks()));
        assert!(
            text.contains("1 decision(s).\nHeld: until the user answers\nWhen the work"),
            "{text}"
        );
    }

    #[test]
    fn context_lists_the_questions_an_agent_must_not_answer_alone() {
        let mut board = board(Some("notes"));
        board.features[0].number = 7;
        let mut question = board.decisions[0].clone();
        question.id = 2;
        question.chose = "which store?".to_string();
        question.state = argus_protocol::DecisionState::Open;
        board.decisions.push(question);
        let text = format_context("no review comments", Ok(board), Ok(tasks()));
        assert!(
            text.contains(
                "Feature #7: Notes (notes), 2 of 3 task(s) open, 1 decision(s).\n\
                 1 open question(s), the person's to answer — do not decide them alone:\n\
                 \x20 #2 which store?\n\
                 When the work"
            ),
            "{text}"
        );
    }

    #[test]
    fn context_without_a_feature_points_at_the_list() {
        let text = format_context("no review comments", Ok(board(None)), Ok(tasks()));
        assert!(text.ends_with("`\"$ARGUS_HOOK\" feature list` shows the project's features."));
        assert!(!text.contains("task(s) open"));
    }

    #[test]
    fn context_outside_a_pane_is_silent() {
        assert_eq!(context_message("", ""), "");
    }
}
