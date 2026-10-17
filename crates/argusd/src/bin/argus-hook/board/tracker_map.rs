//! `argus-hook tracker-map`: how this repository maps to its trackers, as
//! the person approved it, and an agent's draft of one.

use std::io::Read;

use crate::*;

pub(crate) fn tracker_map(rest: &[&str]) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", tracker_map_message(rest, &env_url(), &env_token(), std::io::stdin()));
}

/// `tracker-map`, `tracker-map draft --stdin`.
fn tracker_map_message(
    rest: &[&str],
    base_url: &str,
    token: &str,
    mut stdin: impl Read,
) -> String {
    let draft = match rest {
        [] => String::new(),
        ["draft", "--stdin"] => {
            let mut body = String::new();
            if let Err(e) = stdin.read_to_string(&mut body) {
                return format!("could not draft the tracker map: {e}");
            }
            match argus_protocol::checked_map(&body) {
                Ok(body) if !body.trim().is_empty() => body,
                Ok(_) => return "could not draft the tracker map: the draft on stdin is empty".to_string(),
                Err(why) => return format!("could not draft the tracker map: {why}"),
            }
        }
        ["draft", ..] => return "could not draft the tracker map: draft wants --stdin with the map".to_string(),
        [other, ..] => return format!("tracker-map: `{other}` is not draft"),
    };

    match call::<hook::call::TrackerMap>(base_url, token, &draft) {
        Ok(map) => format_tracker_map(&map, !draft.is_empty()),
        Err(Refusal::Unreadable) => "the tracker map changed".to_string(),
        Err(refusal) => refusal.said("could not read the tracker map"),
    }
}

fn format_tracker_map(map: &TrackerMap, drafted: bool) -> String {
    let mut lines = Vec::new();
    if drafted {
        lines.push(
            "Drafted. It is not the map until the person approves it, which they do from the \
             feature index with M; until then, work from the map below."
                .to_string(),
        );
    }
    if map.body.is_empty() {
        lines.push(
            "No tracker map yet. Draft one, when asked, from what you can read of the trackers: \
             `argus-hook tracker-map draft --stdin`."
                .to_string(),
        );
    } else {
        lines.push("The tracker map, as the person approved it:".to_string());
        lines.extend(map.body.lines().map(|line| format!("  {line}")));
    }
    if let Some(waiting) = map.draft.as_ref().filter(|_| !drafted) {
        lines.push(format!(
            "A draft from {} waits for the person; it is a proposal, not the map:",
            waiting.by
        ));
        lines.extend(waiting.body.lines().map(|line| format!("  {line}")));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_or_unflagged_map_draft_never_travels() {
        let url = "http://127.0.0.1:1/pane/1/";
        for (args, stdin) in [(&["draft", "--stdin"][..], "  \n"), (&["draft"][..], "Linear: x")] {
            let answer = tracker_map_message(args, url, "token", stdin.as_bytes());
            assert!(answer.starts_with("could not draft"), "{answer}");
            assert!(!answer.contains("unavailable"), "refused here, not by the daemon: {answer}");
        }
    }

    #[test]
    fn a_waiting_draft_reads_as_a_proposal_beside_the_map() {
        let map = TrackerMap {
            body: "Linear: a feature is an issue".into(),
            draft: Some(argus_protocol::MapDraft {
                body: "Linear: a feature is a project".into(),
                by: "pane #32".into(),
                at: 1,
            }),
        };
        let read = format_tracker_map(&map, false);
        assert!(read.contains("as the person approved it:\n  Linear: a feature is an issue"), "{read}");
        assert!(read.contains("A draft from pane #32 waits"), "{read}");

        let drafted = format_tracker_map(&TrackerMap::default(), true);
        assert!(drafted.starts_with("Drafted. It is not the map"), "{drafted}");
        assert!(drafted.contains("No tracker map yet"), "{drafted}");
    }
}
