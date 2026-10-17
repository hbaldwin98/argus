//! Appending to a transcript in the neutral format, for a harness that
//! keeps none of its own: its hooks, or the agent itself, run
//! `argus-hook transcript` at each turn, and a `[[harness]]` block's
//! `transcript = { format = "neutral", path = … }` names the same file, so
//! its sessions preview like any other.
//!
//! Each line is the protocol's `Entry` as JSON, so what is written here is
//! what the daemon reads back. It talks to no daemon: the file is the
//! harness's, kept wherever its config says.

use std::path::PathBuf;

use argus_protocol::agents::transcript::{rfc3339, Entry, TITLE_KIND};
use serde_json::json;

use super::*;

/// `argus-hook transcript FILE user|agent|title [TEXT…] [--stdin]`, or
/// `argus-hook transcript FILE tool NAME [SUMMARY…] [--failed]`.
pub(super) fn transcript(rest: &[&str]) {
    let stdin = rest.contains(&"--stdin").then(|| {
        let mut text = String::new();
        let _ = std::io::stdin().read_to_string(&mut text);
        text
    });
    let Some((file, line)) = line(rest, stdin, argus_protocol::ago::now()) else {
        return;
    };
    let _ = append(&file, &line);
}

/// The file `rest` names and the line it says to append to it, stamped
/// `now`. `None` for one that says nothing: an empty turn is not a turn.
fn line(rest: &[&str], stdin: Option<String>, now: u64) -> Option<(PathBuf, String)> {
    let failed = rest.contains(&"--failed");
    let mut args = rest.iter().copied().filter(|arg| !matches!(*arg, "--stdin" | "--failed"));
    let file = home(args.next()?)?;
    let kind = args.next()?;
    let at = Some(rfc3339(now));
    let line = if kind == "tool" {
        let tool = args.next().filter(|tool| !tool.trim().is_empty())?.to_string();
        let summary = stdin.unwrap_or_else(|| args.collect::<Vec<_>>().join(" "));
        serde_json::to_string(&Entry::Tool { tool, summary: summary.trim().to_string(), at, failed })
    } else {
        let text = stdin.unwrap_or_else(|| args.collect::<Vec<_>>().join(" "));
        let text = text.trim().to_string();
        if text.is_empty() {
            return None;
        }
        match kind {
            "user" => serde_json::to_string(&Entry::User { text, at }),
            "agent" => serde_json::to_string(&Entry::Agent { text, at }),
            TITLE_KIND => serde_json::to_string(&json!({ "kind": TITLE_KIND, "text": text })),
            _ => return None,
        }
    };
    Some((file, line.ok()?))
}

/// `path` with a leading `~` read as the home directory, as a transcript
/// template spells it: a hook config may run this with no shell to expand
/// it.
fn home(path: &str) -> Option<PathBuf> {
    if path.is_empty() {
        return None;
    }
    match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
            Some(home.join(rest.trim_start_matches(['/', '\\'])))
        }
        _ => Some(PathBuf::from(path)),
    }
}

/// One line on the end of `file`, written in one call so two hooks
/// appending at once do not interleave inside a line.
fn append(file: &std::path::Path, line: &str) -> std::io::Result<()> {
    if let Some(dir) = file.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut out = std::fs::OpenOptions::new().create(true).append(true).open(file)?;
    out.write_all(format!("{line}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_791_968_400;

    fn written(rest: &[&str], stdin: Option<&str>) -> Option<serde_json::Value> {
        let (_, line) = line(rest, stdin.map(str::to_string), NOW)?;
        Some(serde_json::from_str(&line).unwrap())
    }

    #[test]
    fn each_kind_is_a_neutral_line_stamped_with_the_time() {
        assert_eq!(
            written(&["f.jsonl", "user", "Tidy", "the", "notes"], None).unwrap(),
            json!({ "kind": "user", "text": "Tidy the notes", "at": "2026-10-14T09:00:00Z" })
        );
        assert_eq!(
            written(&["f.jsonl", "agent", "--stdin"], Some("Tidied.\n")).unwrap(),
            json!({ "kind": "agent", "text": "Tidied.", "at": "2026-10-14T09:00:00Z" })
        );
        assert_eq!(
            written(&["f.jsonl", "tool", "shell", "npm", "run", "lint", "--failed"], None).unwrap(),
            json!({ "kind": "tool", "tool": "shell", "summary": "npm run lint", "at": "2026-10-14T09:00:00Z", "failed": true })
        );
        assert_eq!(
            written(&["f.jsonl", "title", "Release notes"], None).unwrap(),
            json!({ "kind": "title", "text": "Release notes" })
        );
    }

    #[test]
    fn a_line_that_says_nothing_is_not_written() {
        assert_eq!(written(&["f.jsonl", "user", "  "], None), None);
        assert_eq!(written(&["f.jsonl", "tool"], None), None);
        assert_eq!(written(&["f.jsonl", "memo", "hi"], None), None);
        assert_eq!(written(&["f.jsonl"], None), None);
        assert_eq!(written(&[], None), None);
    }

    #[test]
    fn lines_are_appended_to_the_file_creating_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sessions").join("s1.jsonl");
        let file_arg = file.to_string_lossy().into_owned();
        for rest in [[file_arg.as_str(), "user", "one"], [file_arg.as_str(), "agent", "two"]] {
            let (path, line) = line(&rest, None, NOW).unwrap();
            append(&path, &line).unwrap();
        }
        let kinds: Vec<String> = std::fs::read_to_string(&file)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["kind"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(kinds, ["user", "agent"]);
    }

    #[test]
    fn a_tilde_is_the_home_directory() {
        let home_dir = directories::BaseDirs::new().unwrap().home_dir().to_path_buf();
        assert_eq!(home("~/.herdr/s1.jsonl"), Some(home_dir.join(".herdr/s1.jsonl")));
        assert_eq!(home("rel/s1.jsonl"), Some(PathBuf::from("rel/s1.jsonl")));
    }
}
