//! `argus-hook feature`: the feature a checkout is on, read as its brief and
//! reasoning, listed, or exported, and every write that opens, moves or
//! links one.

use crate::*;

use super::decisions::{open_question_lead, push_decision_lines};

/// The feature this checkout is working on: what it is for, and what has
/// been decided under it.
///
/// One command rather than four because they are one thing from the
/// agent's side — where am I, and how do I say where I am. Bare, it
/// answers the first; `list`, `open`, `use` and `note` answer the second.
pub(crate) fn feature(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", feature_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

pub(crate) fn feature_message(rest: &[&str], base_url: &str, token: &str) -> String {
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
                (FeatureAction::Done { slug: slug.to_string(), detail: None }, "accepted as done")
            } else {
                (FeatureAction::Reopen { slug: slug.to_string(), detail: None }, "open again")
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
        Some("link") => {
            let Some(slug) = rest.get(1) else {
                return "could not change feature: link wants a slug, then the ticket's URL".to_string();
            };
            let link = match parse_link_args(rest.get(2..).unwrap_or_default()) {
                Ok(link) => link,
                Err(message) => return format!("could not change feature: {message}"),
            };
            // Read here only to say what it will be called; the daemon
            // reads it again with the same function, so the two agree.
            let said = match link.clone().checked() {
                Ok(read) => format!("{slug} is linked to {} {}", read.tracker, read.key),
                Err(message) => return format!("could not change feature: {message}"),
            };
            let action = FeatureAction::Link {
                slug: slug.to_string(),
                link,
            };
            confirmed(write_feature(action, base_url, token), || said)
        }
        Some("unlink") => {
            let (Some(slug), Some(key)) = (rest.get(1), rest.get(2)) else {
                return "could not change feature: unlink wants a slug, then the ticket's key".to_string();
            };
            let action = FeatureAction::Unlink {
                slug: slug.to_string(),
                key: key.to_string(),
            };
            confirmed(write_feature(action, base_url, token), || {
                format!("{slug} is no longer linked to {key}")
            })
        }
        Some(other) => format!(
            "{other} is not one of list, open, use, note, export, done, reopen, retitle, brief, \
             drop, hold, unhold, wait, unwait, link, unlink — `argus-hook feature use {other}` \
             works on an existing feature"
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

/// `<url> [--tracker <name>] [--key <key>]`: the URL, and what the person
/// knows better than the URL does. A tracker's name may be several words.
fn parse_link_args(args: &[&str]) -> Result<LinkWrite, String> {
    let mut link = LinkWrite::default();
    let mut index = 0;
    while index < args.len() {
        match args[index] {
            flag @ ("--tracker" | "--key" | "--role") => {
                index += 1;
                let start = index;
                while index < args.len() && !args[index].starts_with("--") {
                    index += 1;
                }
                if index == start {
                    return Err(format!("{flag} needs something after it"));
                }
                let value = Some(args[start..index].join(" "));
                match flag {
                    "--tracker" => link.tracker = value,
                    "--key" => link.key = value,
                    _ => link.role = value,
                }
            }
            url if link.url.is_empty() => {
                link.url = url.to_string();
                index += 1;
            }
            extra => return Err(format!("link takes one URL; {extra:?} is a second")),
        }
    }
    if link.url.is_empty() {
        return Err("link wants the ticket's URL after the slug".to_string());
    }
    Ok(link)
}

pub(crate) fn read_feature_board(
    rest: &[&str],
    base_url: &str,
    token: &str,
) -> Result<FeatureBoard, String> {
    read::<hook::call::Features>("the feature", rest, base_url, token)
}

fn write_feature(action: FeatureAction, base_url: &str, token: &str) -> String {
    match call::<hook::call::ChangeFeature>(base_url, token, &action) {
        Ok(board) => format_feature(&board),
        Err(Refusal::Unreadable) => "the feature changed".to_string(),
        Err(refusal) => refusal.said("could not change feature"),
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
    let mut lines = vec![format!(
        "Feature{}: {} ({})",
        feature_number(current),
        current.title,
        current.slug
    )];
    if let Some(branch) = &current.origin_branch {
        lines.push(format!("Started on {branch}."));
    }
    lines.extend(parked_lines(current, &board.features));
    for link in &current.links {
        let role = if link.role.is_empty() { String::new() } else { format!(" ({})", link.role) };
        lines.push(format!("Linked to {} {}{role}: {}", link.tracker, link.key, link.url));
    }
    if let Some(reconcile) = &current.reconcile {
        lines.push(format!(
            "A reconcile with its tickets is under way ({}, {}); `argus-hook reconcile` reads it.",
            reconcile.stage().as_str(),
            reconcile.by
        ));
    }
    if !current.body.trim().is_empty() {
        lines.push(String::new());
        lines.push(current.body.trim().to_string());
    }
    lines.push(String::new());
    if board.decisions.is_empty() {
        lines.push("Nothing decided under it yet.".to_string());
    } else {
        lines.extend(open_question_lead(&board.decisions));
        lines.push("Decided under it, newest last:".to_string());
        let tree = DecisionBoard {
            project: board.project,
            name: board.project_name.clone(),
            features: board.features.clone(),
            decisions: board.decisions.clone(),
            ..DecisionBoard::default()
        };
        for row in tree.tree_rows() {
            push_decision_lines(&mut lines, &row);
        }
    }
    lines.join("\n")
}

/// ` #7`, what a person calls the feature, or nothing from a daemon that
/// predates numbers.
pub(crate) fn feature_number(feature: &argus_protocol::Feature) -> String {
    match feature.number {
        0 => String::new(),
        number => format!(" #{number}"),
    }
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
        let number = match feature.number {
            0 => String::new(),
            number => format!("#{number} "),
        };
        lines.push(format!(
            "  {number}{} — {}{branch}{about}{here}",
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
pub(crate) fn parked_lines(
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
pub(crate) fn format_export(board: &FeatureBoard) -> String {
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
  - A node marked open is a question nobody has answered, and its options are what is being weighed. Write it as open; do not pick one. A node marked rejected is an answer that was turned down.
  - Write prose, in the register of the material. Bullets that restate chose/over/because are the board again, and the board can already be read.
  - Do not invent a reason a decision does not give. `the board does not say` is a better sentence than a plausible one you made up.

Save it where the human asked for it, and if they did not say, as a markdown file named after the feature in this checkout.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::tests::serve_once;
    use crate::board::decisions::tests::decision_board;

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
    fn accepting_a_feature_wants_its_slug() {
        let message = feature_message(&["done"], "http://127.0.0.1:1/pane/1", "t");
        assert!(message.contains("wants the slug"), "{message}");
    }

    #[test]
    fn link_arguments_take_one_url_and_what_the_person_knows_better() {
        let write = parse_link_args(&[
            "https://tracker.example/t/9",
            "--tracker",
            "Azure",
            "DevOps",
            "--key",
            "AB-9",
        ])
        .unwrap();
        assert_eq!(write.url, "https://tracker.example/t/9");
        assert_eq!(write.tracker.as_deref(), Some("Azure DevOps"));
        assert_eq!(write.key.as_deref(), Some("AB-9"));

        assert!(parse_link_args(&[]).is_err());
        assert!(parse_link_args(&["https://a.example/1", "https://b.example/2"]).is_err());
        assert!(parse_link_args(&["https://a.example/1", "--key"]).is_err());
    }

    #[test]
    fn a_link_that_reads_as_no_ticket_is_refused_before_it_travels() {
        let answer = feature_message(
            &["link", "remote", "file:///etc/passwd"],
            "http://127.0.0.1:1/pane/1/",
            "token",
        );
        assert!(answer.starts_with("could not change feature"), "{answer}");
        assert!(!answer.contains("unavailable"), "refused here, not by the daemon: {answer}");
    }

    #[test]
    fn a_feature_is_read_and_listed_by_the_number_a_person_calls_it() {
        let tree = decision_board();
        let board = FeatureBoard {
            project: None,
            project_name: "argus".into(),
            features: tree.features,
            current: Some("queue".into()),
            decisions: tree.decisions,
            unfiled: 0,
        };
        let read = format_feature(&board);
        assert!(read.starts_with("Feature #7: The queue (queue)\n"), "{read}");
        assert!(
            read.contains("\n1 below marked (open) are questions, not decisions"),
            "{read}"
        );
        assert!(format_feature_list(&board).contains("  #7 queue — The queue"));
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

        assert!(
            message.starts_with("This checkout is not on a feature yet"),
            "{message}"
        );
        assert!(
            message.contains("the-pty-deadlock — The pty deadlock"),
            "{message}"
        );
        assert!(message.contains("3 older decision(s)"), "{message}");
    }

    fn exportable_board() -> FeatureBoard {
        let feature = argus_protocol::Feature {
            slug: "streaming-the-pty".into(),
            title: "streaming the pty".into(),
            body: "the reader thread owns the handle.".into(),
            origin_checkout: None,
            origin_branch: Some("pty-stream".into()),
            at: 0,
            session: None,
            state: Default::default(),
            checkouts: Vec::new(),
            tasks: Default::default(),
            held: None,
            waits_on: Vec::new(),
            ..Default::default()
        };
        FeatureBoard {
            project: None,
            project_name: "argus".into(),
            features: vec![feature],
            current: Some("streaming-the-pty".into()),
            decisions: vec![Decision {
                id: 4,
                parent: None,
                at: 0,
                session: None,
                checkout: None,
                feature: Some("streaming-the-pty".into()),
                chose: "one reader thread".into(),
                over: Some("polling the handle".into()),
                because: Some("a poll cannot see a burst".into()),
                superseded_by: None,
                withdrawn_at: None,
                ..Default::default()
            }],
            unfiled: 0,
        }
    }

    #[test]
    fn an_export_hands_over_the_whole_feature_and_asks_for_prose() {
        let message = format_export(&exportable_board());

        // The commission first: without it this is the board again, and
        // the point of the command is what the model does with it.
        assert!(message.starts_with("Write this feature up"), "{message}");
        assert!(message.contains("the road not taken"));
        // Then every part of the record a document would need.
        assert!(message.contains("Feature: streaming the pty (streaming-the-pty)"));
        assert!(message.contains("the reader thread owns the handle."));
        assert!(message.contains("#4 one reader thread"));
        assert!(message.contains("over: polling the handle"));
        assert!(message.contains("because: a poll cannot see a burst"));
    }

    #[test]
    fn a_checkout_on_no_feature_is_told_that_rather_than_asked_to_write() {
        let mut board = exportable_board();
        board.current = None;
        board.decisions.clear();

        let message = format_export(&board);

        assert!(
            message.starts_with("This checkout is not on a feature"),
            "{message}"
        );
        assert!(
            message.contains("streaming-the-pty"),
            "it still offers the ones that exist"
        );
    }
}
