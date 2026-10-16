//! `argus-hook view`: showing the person a view of something the agent read
//! with its own tools, taking it down, and keeping one in the store.
//!
//! The view's JSON is the agent's to write and the daemon's to check, so
//! this only wraps it for the `view` or `saved-view` endpoint and relays
//! the daemon's refusal, which names the path of the block at fault, as the
//! answer. Like every deliberate command it exits 0 and says on stdout how
//! it went.

use std::io::{Read, Write};

use argus_protocol::{
    AgentView, Block, SavedViewChange, SavedViewEntry, SavedViewWrite, ViewChange, WriteScope,
};

use super::*;

pub(super) fn view(rest: &[&str]) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", view_message(rest, &env_url(), &env_token(), std::io::stdin()));
}

const USAGE: &str = "view: use `view show --stdin`, `view patch <id> <path> --stdin`, `view close <id>`, \
                     `view save --stdin`, `view saved`, `view get <id>`, `view delete <id>` or `view stale <id>`";

/// `view show --stdin`, `view patch <id> <path> --stdin`, `view close <id>`,
/// and the saved views' `save --stdin`, `saved`, `get <id>`, `delete <id>`
/// and `stale <id>`.
pub(super) fn view_message(
    rest: &[&str],
    base_url: &str,
    token: &str,
    mut stdin: impl Read,
) -> String {
    if let ["save" | "saved" | "get" | "delete" | "stale", ..] = rest {
        return saved_view_message(rest, base_url, token, stdin);
    }
    let (change, done) = match rest {
        ["show", "--stdin"] => {
            let mut body = String::new();
            if let Err(e) = stdin.read_to_string(&mut body) {
                return format!("could not show the view: {e}");
            }
            let view: AgentView = match serde_json::from_str(&body) {
                Ok(view) => view,
                Err(e) => return format!("could not show the view: not a view: {e}"),
            };
            let done = format!("shown: {}, {} blocks", view.id.trim(), view.blocks.len());
            (ViewChange::Show { view }, done)
        }
        ["show", ..] => return "could not show the view: show wants --stdin with the view's JSON".into(),
        ["patch", id, path, "--stdin"] if !id.trim().is_empty() && !path.trim().is_empty() => {
            let mut body = String::new();
            if let Err(e) = stdin.read_to_string(&mut body) {
                return format!("could not patch the view: {e}");
            }
            let block: Block = match serde_json::from_str(&body) {
                Ok(block) => block,
                Err(e) => return format!("could not patch the view: not a block: {e}"),
            };
            let done = format!("patched: {id} {path}");
            (ViewChange::Patch { id: id.to_string(), path: path.to_string(), block }, done)
        }
        ["patch", ..] => {
            return "could not patch the view: patch wants the view's id, the block's path and --stdin with \
                    the block's JSON"
                .into()
        }
        ["close", id] if !id.trim().is_empty() => {
            (ViewChange::Close { id: id.to_string() }, format!("closed: {id}"))
        }
        ["close", ..] => return "could not close the view: close wants the view's id".into(),
        _ => return USAGE.into(),
    };
    match call::<hook_call::ChangeView>(base_url, token, &change) {
        Ok(()) => done,
        Err(refusal) => refusal.said("could not change the view"),
    }
}

/// The saved-view half of `view`. The daemon's answer to a list or a read
/// is JSON; a list is printed a line a view, and a read as the JSON itself,
/// which `save --stdin` takes back.
fn saved_view_message(rest: &[&str], base_url: &str, token: &str, mut stdin: impl Read) -> String {
    let change = match rest {
        ["save", "--stdin"] => {
            let mut body = String::new();
            if let Err(e) = stdin.read_to_string(&mut body) {
                return format!("could not save the view: {e}");
            }
            match serde_json::from_str::<SavedViewWrite>(&body) {
                Ok(view) => SavedViewChange::Save { view: Box::new(view) },
                Err(e) => return format!("could not save the view: not a saved view: {e}"),
            }
        }
        ["save", ..] => {
            return "could not save the view: save wants --stdin with {\"scope\", \"features\", \"recipe\", \
                    \"sources\", \"view\"} as JSON"
                .into()
        }
        ["saved"] => SavedViewChange::List,
        ["get", id] if !id.trim().is_empty() => SavedViewChange::Get { id: id.to_string() },
        ["delete", id] if !id.trim().is_empty() => SavedViewChange::Delete { id: id.to_string() },
        ["stale", id] if !id.trim().is_empty() => SavedViewChange::Stale { id: id.to_string() },
        _ => return USAGE.into(),
    };
    // The answer's shape is the change's, so it is read once as JSON and
    // taken apart below; one that is not JSON still means the change landed.
    let reply = match call::<hook_call::SavedView>(base_url, token, &change) {
        Ok(reply) => Some(reply),
        Err(Refusal::Unreadable) => None,
        Err(refusal) => return refusal.said("could not change the saved view"),
    };
    match change {
        SavedViewChange::Save { view } => {
            let entry: Option<SavedViewEntry> = reply.and_then(|reply| serde_json::from_value(reply).ok());
            let id = entry.map_or_else(|| view.view.id.trim().to_string(), |e| e.id);
            format!("saved: {id}, {} blocks", view.view.blocks.len())
        }
        SavedViewChange::List => match reply.map(serde_json::from_value::<Vec<SavedViewEntry>>) {
            Some(Ok(entries)) if entries.is_empty() => "no saved views".into(),
            Some(Ok(entries)) => entries.iter().map(entry_line).collect::<Vec<_>>().join("\n"),
            _ => "could not read the saved views".into(),
        },
        SavedViewChange::Get { .. } => match reply {
            Some(record) => serde_json::to_string_pretty(&record).unwrap_or_else(|_| record.to_string()),
            None => "could not read the saved view".into(),
        },
        SavedViewChange::Delete { id } => format!("deleted: {id}"),
        SavedViewChange::Stale { id } => format!("marked stale: {id}"),
    }
}

/// `id  scope  stale  title`, for an agent deciding which to refresh.
fn entry_line(entry: &SavedViewEntry) -> String {
    let scope = match entry.scope {
        WriteScope::Global => "global".to_string(),
        WriteScope::Repository => "repository".to_string(),
        WriteScope::Feature { number } => format!("feature #{number}"),
    };
    let stale = if entry.stale { "stale" } else { "current" };
    format!("{}  {scope}  {stale}  {}", entry.id, entry.title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const BASE: &str = "http://127.0.0.1:1/pane/1";
    const VIEW: &str = r#"{"id":"eng-1","title":"ENG-1","blocks":[{"kind":"markdown","text":"hi"}]}"#;

    #[test]
    fn showing_a_view_wraps_it_for_the_endpoint_and_counts_its_blocks() {
        let (base, server) = answering("200 OK", "");
        let said = view_message(&["show", "--stdin"], &base, "t", Cursor::new(VIEW));
        assert_eq!(said, "shown: eng-1, 1 blocks");
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /pane/1/view"), "{request}");
        assert!(request.contains(r#""action":"show""#) && request.contains(r#""id":"eng-1""#), "{request}");
    }

    #[test]
    fn the_daemons_refusal_is_the_answer() {
        let (base, server) = answering("409 Conflict", "blocks[0]: a table is at most 200 rows");
        let said = view_message(&["show", "--stdin"], &base, "t", Cursor::new(VIEW));
        server.join().unwrap();
        assert_eq!(said, "could not change the view: blocks[0]: a table is at most 200 rows");
    }

    #[test]
    fn closing_a_view_sends_its_id() {
        let (base, server) = answering("200 OK", "");
        assert_eq!(view_message(&["close", "eng-1"], &base, "t", Cursor::new("")), "closed: eng-1");
        let request = server.join().unwrap();
        assert!(request.contains(r#"{"action":"close","id":"eng-1"}"#), "{request}");
    }

    #[test]
    fn patching_a_view_sends_the_path_and_the_block() {
        let (base, server) = answering("200 OK", "");
        let block = r#"{"kind":"markdown","text":"newer"}"#;
        let args = ["patch", "eng-1", "blocks[1].columns[0][2]", "--stdin"];
        let said = view_message(&args, &base, "t", Cursor::new(block));
        assert_eq!(said, "patched: eng-1 blocks[1].columns[0][2]");
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /pane/1/view"), "{request}");
        assert!(request.contains(r#""action":"patch""#), "{request}");
        assert!(request.contains(r#""path":"blocks[1].columns[0][2]""#), "{request}");
        assert!(request.contains(r#""text":"newer""#), "{request}");
    }

    #[test]
    fn the_daemons_refusal_of_a_patch_is_the_answer() {
        let (base, server) = answering("409 Conflict", "blocks[7]: blocks[7] is out of range, there are 3 blocks");
        let block = r#"{"kind":"markdown","text":"x"}"#;
        let said = view_message(&["patch", "eng-1", "blocks[7]", "--stdin"], &base, "t", Cursor::new(block));
        server.join().unwrap();
        assert!(said.starts_with("could not change the view: blocks[7]: "), "{said}");
    }

    #[test]
    fn a_malformed_command_is_refused_before_the_daemon_is_asked() {
        for (args, stdin) in [
            (&["show"][..], VIEW),
            (&["show", "--stdin"], "not json"),
            (&["show", "--stdin"], r#"{"id":"x"}"#),
            (&["patch", "eng-1", "blocks[0]"], r#"{"kind":"divider"}"#),
            (&["patch", "eng-1", "--stdin"], r#"{"kind":"divider"}"#),
            (&["patch", " ", "blocks[0]", "--stdin"], r#"{"kind":"divider"}"#),
            (&["patch", "eng-1", "blocks[0]", "--stdin"], "not json"),
            (&["close"], ""),
            (&["close", " "], ""),
            (&[], ""),
            (&["wipe"], ""),
        ] {
            let said = view_message(args, BASE, "t", Cursor::new(stdin));
            assert!(!said.contains("unavailable") && !said.starts_with("shown"), "{args:?}: {said}");
        }
    }

    #[test]
    fn saving_a_view_wraps_it_for_the_saved_view_endpoint() {
        let entry = r#"{"id":"eng-1","title":"ENG-1","scope":{"kind":"repository"},"stale":false,"built_at":1}"#;
        let (base, server) = answering("200 OK", entry);
        let body = format!(r#"{{"scope":{{"kind":"global"}},"recipe":"read it","view":{VIEW}}}"#);
        let said = view_message(&["save", "--stdin"], &base, "t", Cursor::new(body));
        assert_eq!(said, "saved: eng-1, 1 blocks");
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /pane/1/saved-view"), "{request}");
        assert!(request.contains(r#""action":"save""#) && request.contains(r#""recipe":"read it""#), "{request}");
    }

    #[test]
    fn the_saved_views_print_a_line_each() {
        let list = r#"[{"id":"a","title":"A","scope":{"kind":"feature","number":3},"stale":true,"built_at":1},
                       {"id":"b","title":"B","scope":{"kind":"global"},"stale":false,"built_at":2}]"#;
        let (base, server) = answering("200 OK", list);
        let said = view_message(&["saved"], &base, "t", Cursor::new(""));
        assert_eq!(said, "a  feature #3  stale  A\nb  global  current  B");
        assert!(server.join().unwrap().contains(r#"{"action":"list"}"#));
        let (base, server) = answering("200 OK", "[]");
        assert_eq!(view_message(&["saved"], &base, "t", Cursor::new("")), "no saved views");
        server.join().unwrap();
    }

    #[test]
    fn getting_a_saved_view_prints_the_json_a_save_takes_back() {
        let (base, server) = answering("200 OK", r#"{"id":"a","view":{"id":"a","title":"A","blocks":[]}}"#);
        let said = view_message(&["get", "a"], &base, "t", Cursor::new(""));
        assert!(said.contains("\"title\": \"A\""), "{said}");
        assert!(server.join().unwrap().contains(r#"{"action":"get","id":"a"}"#));
    }

    #[test]
    fn deleting_and_marking_stale_send_the_id() {
        let (base, server) = answering("200 OK", "null");
        assert_eq!(view_message(&["delete", "a"], &base, "t", Cursor::new("")), "deleted: a");
        assert!(server.join().unwrap().contains(r#"{"action":"delete","id":"a"}"#));
        let (base, server) = answering("200 OK", "null");
        assert_eq!(view_message(&["stale", "a"], &base, "t", Cursor::new("")), "marked stale: a");
        assert!(server.join().unwrap().contains(r#"{"action":"stale","id":"a"}"#));
    }

    #[test]
    fn the_daemons_refusal_of_a_saved_view_is_the_answer() {
        let (base, server) = answering("409 Conflict", "recipe: at most 4096 bytes");
        let said = view_message(&["save", "--stdin"], &base, "t", Cursor::new(format!(r#"{{"view":{VIEW}}}"#)));
        server.join().unwrap();
        assert_eq!(said, "could not change the saved view: recipe: at most 4096 bytes");
    }

    #[test]
    fn a_malformed_saved_view_command_is_refused_before_the_daemon_is_asked() {
        for (args, stdin) in [
            (&["save"][..], ""),
            (&["save", "--stdin"], "not json"),
            (&["save", "--stdin"], r#"{"scope":{"kind":"global"}}"#),
            (&["get"], ""),
            (&["get", " "], ""),
            (&["delete"], ""),
            (&["stale", "a", "b"], ""),
            (&["saved", "extra"], ""),
        ] {
            let said = view_message(args, BASE, "t", Cursor::new(stdin));
            assert!(!said.contains("unavailable") && !said.starts_with("saved:"), "{args:?}: {said}");
        }
    }
}
