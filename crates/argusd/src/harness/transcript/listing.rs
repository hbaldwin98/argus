//! Which sessions the harnesses kept for a repository's checkouts, newest
//! first, each read only as far as a list needs.
//!
//! Files are ordered by when they were last written before any is opened,
//! so a listing opens the ones it shows and not the thousands behind them.
//! A transcript filed under a directory named for its checkout is placed
//! by its path; any other is opened to read where it ran — one filed by
//! date (Codex), or under a directory named for one below a checkout or
//! the worktree root, which may be a worktree since removed — and only
//! the newest few hundred of those are.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use argus_protocol::sessions::{Session, MAX_SESSIONS};
use argus_protocol::CheckoutId;

use super::glance::Head;
use super::{locate, Source};

/// The most transcripts a listing opens only to learn where they ran.
const MAX_UNPLACED: usize = 300;

/// Where a repository's sessions may have run.
#[derive(Debug, Clone)]
pub struct Places {
    pub checkouts: Vec<(CheckoutId, PathBuf)>,
    /// Where Argus puts the repository's worktrees, so where one it has
    /// since removed was.
    pub worktree_root: Option<PathBuf>,
    /// The checkout a session from a removed worktree is listed under:
    /// the one place of the repository certain to stay.
    pub primary: CheckoutId,
}

/// Where a transcript's session ran, as a listing places it.
enum Place {
    Checkout(usize),
    /// A worktree of the repository that is gone, as the transcript spells it.
    Removed(String),
}

/// The sessions `harnesses` kept for any of `places`, newest first and at
/// most [`MAX_SESSIONS`]. A harness is named with where it keeps its
/// transcripts; the first to list a session is the one it is listed under.
pub fn sessions(harnesses: &[(String, Source)], places: &Places) -> Vec<Session> {
    let paths: Vec<PathBuf> = places.checkouts.iter().map(|(_, path)| path.clone()).collect();
    let within: Vec<PathBuf> = paths.iter().chain(&places.worktree_root).cloned().collect();
    let mut found = Vec::new();
    for (harness, (_, source)) in harnesses.iter().enumerate() {
        for template in &source.paths {
            for file in locate::every(template, &paths, &within) {
                if let Ok(modified) = std::fs::metadata(&file.path).and_then(|m| m.modified()) {
                    found.push((modified, harness, file));
                }
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));

    // Compared canonical, as a harness records the directory it ran in
    // however the shell spelled it.
    let checkouts: Vec<PathBuf> = paths.iter().map(|path| canonical(path)).collect();
    let root = places.worktree_root.as_deref().map(canonical);
    let mut seen = HashSet::new();
    let mut unplaced = 0;
    let mut listed = Vec::new();
    for (modified, harness, file) in found {
        if listed.len() >= MAX_SESSIONS {
            break;
        }
        let (name, source) = &harnesses[harness];
        let Ok(mut head) = Head::open(source.format, &file.path) else {
            continue;
        };
        let place = match file.checkout {
            Some(checkout) => Place::Checkout(checkout),
            None if unplaced < MAX_UNPLACED => {
                unplaced += 1;
                let Ok(Some(cwd)) = head.cwd() else {
                    continue;
                };
                match place(cwd, &checkouts, root.as_deref()) {
                    Some(place) => place,
                    None => continue,
                }
            }
            None => continue,
        };
        let Ok(glance) = head.finish() else {
            continue;
        };
        // The id the file records, where the name only half spells it:
        // `rollout-<time>-<id>` splits more than one way.
        let session_id = glance
            .session_id
            .filter(|id| file.session_id.ends_with(id.as_str()))
            .unwrap_or(file.session_id);
        if !seen.insert((harness, session_id.clone())) {
            continue;
        }
        let (checkout, removed_worktree) = match place {
            Place::Checkout(i) => (places.checkouts[i].0, None),
            Place::Removed(path) => (places.primary, Some(path)),
        };
        listed.push(Session {
            harness: name.clone(),
            session_id,
            checkout,
            removed_worktree,
            title: glance.meta.title,
            started: glance.meta.started,
            ended: glance.meta.ended,
            modified: millis(modified),
            turns: glance.whole.then_some(glance.meta.turns),
            pane: None,
        });
    }
    listed
}

/// Which of `checkouts` a session that ran in `cwd` belongs to: the
/// deepest holding it, while it is there. Once it is gone, a directory is
/// the repository's only where worktrees are put — the worktree root, or a
/// tool's `.<tool>/worktrees` in a checkout — since anything else below a
/// checkout that is gone was never a checkout of its own.
fn place(cwd: &str, checkouts: &[PathBuf], root: Option<&Path>) -> Option<Place> {
    let at = canonical(Path::new(cwd));
    if at.exists() {
        return checkouts
            .iter()
            .enumerate()
            .filter(|(_, checkout)| at.starts_with(checkout))
            .max_by_key(|(_, checkout)| checkout.components().count())
            .map(|(i, _)| Place::Checkout(i));
    }
    let in_root = root.is_some_and(|root| at.starts_with(root) && at != root);
    let in_tools = checkouts.iter().any(|checkout| {
        let mut rest = at.strip_prefix(checkout).into_iter().flat_map(Path::components);
        let tool = rest.next().is_some_and(|c| c.as_os_str().to_string_lossy().starts_with('.'));
        tool && rest.next().is_some_and(|c| c.as_os_str() == "worktrees") && rest.next().is_some()
    });
    (in_root || in_tools).then(|| Place::Removed(cwd.to_string()))
}

/// `path` canonical as far as it exists, the rest as spelled: a removed
/// worktree is compared with the checkouts it was beside, and those are
/// canonical.
fn canonical(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut at = path;
    loop {
        if let Ok(found) = at.canonicalize() {
            return missing.iter().rev().fold(found, |path, part| path.join(part));
        }
        match (at.parent(), at.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name.to_os_string());
                at = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

fn millis(at: SystemTime) -> Option<u64> {
    let since = at.duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(since.as_millis()).ok()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::harness::transcript::Format;

    /// A transcript written at `minute` past an arbitrary hour, so which is
    /// newer does not depend on how fast the test writes them.
    fn write(path: &Path, lines: &[serde_json::Value], minute: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
        std::fs::write(path, text).unwrap();
        let at = UNIX_EPOCH + Duration::from_secs(1_800_000_000 + minute * 60);
        std::fs::File::options().write(true).open(path).unwrap().set_modified(at).unwrap();
    }

    /// The directory name Claude Code makes of a path.
    fn mangled(path: &Path) -> String {
        path.to_string_lossy().replace(|c: char| !c.is_alphanumeric(), "-")
    }

    fn claude(root: &Path) -> (String, Source) {
        let template = format!("{}/projects/{{project}}/{{session_id}}.jsonl", root.display());
        ("claude".into(), Source { format: Format::Claude, paths: vec![template] })
    }

    fn asked(text: &str) -> serde_json::Value {
        json!({"type": "user", "message": {"role": "user", "content": text}, "timestamp": "2026-10-14T09:00:00Z"})
    }

    fn asked_in(cwd: &Path, text: &str) -> serde_json::Value {
        let mut record = asked(text);
        record["cwd"] = json!(cwd);
        record
    }

    fn places(checkouts: &[(CheckoutId, PathBuf)]) -> Places {
        Places { checkouts: checkouts.to_vec(), worktree_root: None, primary: checkouts[0].0 }
    }

    #[test]
    fn a_repositorys_sessions_are_its_checkouts_newest_first() {
        let root = tempfile::tempdir().unwrap();
        let (main, worktree) = (root.path().join("app"), root.path().join("app-wt"));
        let projects = root.path().join("projects");
        write(&projects.join(mangled(&main)).join("old.jsonl"), &[asked("Fix the build")], 1);
        write(&projects.join(mangled(&worktree)).join("new.jsonl"), &[asked("Add a stop command")], 5);
        write(&projects.join(mangled(&root.path().join("other"))).join("elsewhere.jsonl"), &[asked("no")], 9);

        let listed = sessions(&[claude(root.path())], &places(&[(CheckoutId(1), main), (CheckoutId(2), worktree)]));
        let rows: Vec<_> = listed
            .iter()
            .map(|s| (s.session_id.as_str(), s.checkout, s.title.as_deref(), s.turns))
            .collect();
        assert_eq!(
            rows,
            [
                ("new", CheckoutId(2), Some("Add a stop command"), Some(1)),
                ("old", CheckoutId(1), Some("Fix the build"), Some(1)),
            ]
        );
        assert_eq!(listed[0].harness, "claude");
        assert_eq!(listed[0].removed_worktree, None);
        assert_eq!(listed[0].started.as_deref(), Some("2026-10-14T09:00:00Z"));
        assert!(listed[0].modified > listed[1].modified);
    }

    #[test]
    fn a_session_from_a_removed_worktree_is_listed_and_flagged() {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("app");
        std::fs::create_dir_all(app.join("crates")).unwrap();
        let worktrees = root.path().join("trees");
        let projects = root.path().join("projects");
        let gone = worktrees.join("feat-x");
        let tool_gone = app.join(".claude").join("worktrees").join("agent-1");
        let dir_gone = app.join("scratch");
        let sibling = root.path().join("app-old");
        std::fs::create_dir_all(&sibling).unwrap();
        write(&projects.join(mangled(&gone)).join("g1.jsonl"), &[asked_in(&gone, "Gone")], 1);
        write(&projects.join(mangled(&tool_gone)).join("g2.jsonl"), &[asked_in(&tool_gone, "Agent")], 2);
        // Below a checkout that is still there: that checkout's.
        let crates = app.join("crates");
        write(&projects.join(mangled(&crates)).join("c1.jsonl"), &[asked_in(&crates, "Sub")], 3);
        // Gone, but never where a worktree is put.
        write(&projects.join(mangled(&dir_gone)).join("d1.jsonl"), &[asked_in(&dir_gone, "Scratch")], 4);
        // Named with the runs of `app`, but another directory.
        write(&projects.join(mangled(&sibling)).join("s1.jsonl"), &[asked_in(&sibling, "Other")], 5);

        let places = Places {
            checkouts: vec![(CheckoutId(1), app)],
            worktree_root: Some(worktrees),
            primary: CheckoutId(1),
        };
        let listed = sessions(&[claude(root.path())], &places);
        let rows: Vec<_> = listed
            .iter()
            .map(|s| (s.session_id.as_str(), s.checkout, s.removed_worktree.is_some()))
            .collect();
        assert_eq!(rows, [("c1", CheckoutId(1), false), ("g2", CheckoutId(1), true), ("g1", CheckoutId(1), true)]);
        assert_eq!(listed[2].removed_worktree.as_deref(), Some(&*gone.to_string_lossy()));
    }

    #[test]
    fn a_session_filed_by_date_is_placed_by_where_it_says_it_ran() {
        let root = tempfile::tempdir().unwrap();
        let checkout = root.path().join("app");
        std::fs::create_dir_all(&checkout).unwrap();
        let day = root.path().join("sessions").join("2026-10-14");
        let id = "0199a-b2";
        let meta = |cwd: &Path, id: &str| {
            json!({"timestamp": "2026-10-14T09:00:00Z", "type": "session_meta", "payload": {"id": id, "cwd": cwd}})
        };
        write(&day.join(format!("rollout-2026-10-14T09-00-{id}.jsonl")), &[meta(&checkout, id)], 1);
        write(&day.join("rollout-2026-10-14T10-00-c3.jsonl"), &[meta(&root.path().join("other"), "c3")], 2);

        let template = format!("{}/sessions/*/rollout-*-{{session_id}}.jsonl", root.path().display());
        let codex = ("codex".to_string(), Source { format: Format::Codex, paths: vec![template] });
        let listed = sessions(&[codex], &places(&[(CheckoutId(7), checkout)]));
        let rows: Vec<_> = listed.iter().map(|s| (s.session_id.as_str(), s.checkout)).collect();
        // The name alone splits `2026-…-0199a-b2` more than one way; the
        // record says which is the id.
        assert_eq!(rows, [(id, CheckoutId(7))]);
    }

    #[test]
    fn a_transcript_two_templates_reach_is_one_session() {
        let root = tempfile::tempdir().unwrap();
        let checkout = root.path().join("app");
        let projects = root.path().join("projects").join(mangled(&checkout));
        write(&projects.join("s1.jsonl"), &[asked("hi")], 1);
        let (name, mut source) = claude(root.path());
        source.paths.push(source.paths[0].clone());
        assert_eq!(sessions(&[(name, source)], &places(&[(CheckoutId(1), checkout)])).len(), 1);
    }
}
