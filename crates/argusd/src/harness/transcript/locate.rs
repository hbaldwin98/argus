//! Where transcripts are on disk, from a harness's path templates: the one
//! a session's id names, and every one kept for a set of checkouts.
//!
//! A template is a path with `~` for the home directory, `{session_id}`
//! and `{checkout}` filled in, `{project}` for a directory the harness
//! named after the checkout's path, and `*` matching any run of characters
//! within one component — enough for every harness Argus knows, which file
//! a session under a directory named after the checkout, or a date, or
//! both. A harness spells a path as a directory name its own way
//! (`C--src-app`, `--src-app--`), so `{project}` matches a name whose runs
//! of letters and digits are the path's, case aside, and Argus never has
//! to spell it as the harness does. A name that only begins with a place's
//! runs may be a directory below it, a worktree since removed among them,
//! which only the transcript's own record of where it ran can settle.

use std::path::{Component, Path, PathBuf};

const SESSION: &str = "{session_id}";
const PROJECT: &str = "{project}";

/// The most entries a listing takes from one template's directories: a
/// harness that files by date keeps every session it ever had together.
const MAX_FOUND: usize = 20_000;

/// The transcript the first template that matches anything names. Where a
/// template matches several files, the most recently written wins: a
/// session resumed in another directory leaves its old file behind.
pub(super) fn find(templates: &[String], session_id: &str, checkout: &Path) -> Option<PathBuf> {
    // The id comes from what a hook reported, or a client sent back.
    if !names_one(session_id) {
        return None;
    }
    templates.iter().find_map(|template| {
        // Any project directory will do: a session id is the harness's
        // own and names one session wherever it was filed.
        let path = expand(&template.replace(PROJECT, "*"), session_id, checkout)?;
        matches(&path)
            .into_iter()
            .filter_map(|path| Some((std::fs::metadata(&path).ok()?.modified().ok()?, path)))
            .max()
            .map(|(_, path)| path)
    })
}

/// Whether an id names one session only: one that could climb out of the
/// directory it is put in, or match more than itself, names nothing.
fn names_one(session_id: &str) -> bool {
    !(session_id.is_empty()
        || session_id.contains(['/', '\\', '*', ':'])
        || session_id.contains(".."))
}

/// One transcript a template names.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Found {
    pub path: PathBuf,
    /// What the template's `{session_id}` matched. Where the name has a
    /// `*` beside it the split is a guess, the longest id that fits, which
    /// the transcript's own record of its id can correct.
    pub session_id: String,
    /// Which of the checkouts the path is named after, where the template
    /// names one; otherwise only the file can say where its session ran.
    pub checkout: Option<usize>,
}

/// Every transcript `template` names for any of `checkouts`, or for a
/// directory below one of `within`. A template without `{session_id}`
/// names no session and lists nothing.
pub(super) fn every(template: &str, checkouts: &[PathBuf], within: &[PathBuf]) -> Vec<Found> {
    if !template.contains(SESSION) {
        return Vec::new();
    }
    let all: Vec<(usize, &Path)> = checkouts.iter().map(PathBuf::as_path).enumerate().collect();
    if !template.contains("{checkout}") {
        let pattern = expand(template, SESSION, Path::new(""));
        return pattern.map_or_else(Vec::new, |pattern| walk(&pattern, &all, within, None));
    }
    all.iter()
        .filter_map(|&(i, checkout)| Some(walk(&expand(template, SESSION, checkout)?, &[(i, checkout)], &[], Some(i))))
        .flatten()
        .collect()
}

/// The files `pattern` names, `*`, `{project}` and `{session_id}` matched
/// against each directory's entries; `projects` are the checkouts a
/// `{project}` may be named after, with their index, and `within` the
/// places a `{project}` may be named after a directory below.
fn walk(pattern: &Path, projects: &[(usize, &Path)], within: &[PathBuf], checkout: Option<usize>) -> Vec<Found> {
    struct Partial {
        path: PathBuf,
        session_id: Option<String>,
        checkout: Option<usize>,
    }
    let mut partials = vec![Partial { path: PathBuf::new(), session_id: None, checkout }];
    for component in pattern.components() {
        let part = component.as_os_str().to_string_lossy();
        let literal = !matches!(component, Component::Normal(_))
            || !(part.contains('*') || part.contains(SESSION) || part == PROJECT);
        if literal {
            partials.iter_mut().for_each(|partial| partial.path.push(component));
            continue;
        }
        let mut next = Vec::new();
        for partial in partials {
            let Ok(entries) = std::fs::read_dir(&partial.path) else {
                continue;
            };
            for entry in entries.flatten().take(MAX_FOUND.saturating_sub(next.len())) {
                let name = entry.file_name().to_string_lossy().into_owned();
                let mut session_id = partial.session_id.clone();
                let mut checkout = partial.checkout;
                if part == PROJECT {
                    match projects.iter().find(|(_, path)| same_place(&name, path)) {
                        Some(&(i, _)) => checkout = Some(i),
                        None if within.iter().any(|path| below(&name, path)) => checkout = None,
                        None => continue,
                    }
                } else if part.contains(SESSION) {
                    let Some(captured) = capture(&part, &name) else {
                        continue;
                    };
                    // Cursor's `{session_id}/{session_id}.jsonl` says the
                    // same id twice.
                    if session_id.as_ref().is_some_and(|id| *id != captured) {
                        continue;
                    }
                    session_id = Some(captured);
                } else if !wildcard(&part, &name) {
                    continue;
                }
                next.push(Partial { path: entry.path(), session_id, checkout });
            }
        }
        partials = next;
    }
    partials
        .into_iter()
        .filter(|partial| partial.path.is_file())
        .filter_map(|partial| {
            Some(Found {
                session_id: partial.session_id?,
                path: partial.path,
                checkout: partial.checkout,
            })
        })
        .collect()
}

/// What `{session_id}` stands for in `name`, matched against `pattern`:
/// the longest run that leaves both sides of it matching.
fn capture(pattern: &str, name: &str) -> Option<String> {
    let (before, after) = pattern.split_once(SESSION)?;
    for (start, _) in name.char_indices().filter(|&(i, _)| wildcard(before, &name[..i])) {
        let end = (start + 1..=name.len())
            .rev()
            .filter(|&end| name.is_char_boundary(end))
            .find(|&end| wildcard(after, &name[end..]));
        if let Some(id) = end.map(|end| &name[start..end]) {
            return names_one(id).then(|| id.to_string());
        }
    }
    None
}

/// Whether a directory name a harness made of a path is that path.
fn same_place(name: &str, path: &Path) -> bool {
    runs(name) == runs(&path.to_string_lossy())
}

/// Whether a directory name a harness made of a path could be a directory
/// below `path`. Could only: `app-wt` begins with the runs of `app` too.
fn below(name: &str, path: &Path) -> bool {
    let (name, path) = (runs(name), runs(&path.to_string_lossy()));
    name.len() > path.len() && name.starts_with(&path)
}

fn runs(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|run| !run.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn expand(template: &str, session_id: &str, checkout: &Path) -> Option<PathBuf> {
    // The id goes in only where the template put it: filled in first, an
    // id spelling `{checkout}` would have the checkout's separators
    // spliced into it.
    let filled = template
        .split(SESSION)
        .map(|part| part.replace("{checkout}", &checkout.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(session_id);
    match filled.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
            Some(home.join(rest.trim_start_matches(['/', '\\'])))
        }
        _ => Some(PathBuf::from(filled)),
    }
}

/// Every file `pattern` names, `*` read as a wildcard in any component.
fn matches(pattern: &Path) -> Vec<PathBuf> {
    let mut found = vec![PathBuf::new()];
    for component in pattern.components() {
        let part = component.as_os_str().to_string_lossy();
        if !matches!(component, Component::Normal(_)) || !part.contains('*') {
            found.iter_mut().for_each(|path| path.push(component));
            continue;
        }
        found = found
            .iter()
            .filter_map(|dir| std::fs::read_dir(dir).ok())
            .flatten()
            .flatten()
            .filter(|entry| wildcard(&part, &entry.file_name().to_string_lossy()))
            .map(|entry| entry.path())
            .collect();
    }
    found.retain(|path| path.is_file());
    found
}

/// Whether `name` is `pattern` with each `*` standing for any run of
/// characters.
fn wildcard(pattern: &str, name: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = name.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wildcard_stands_for_any_run_within_a_name() {
        assert!(wildcard("rollout-*-abc.jsonl", "rollout-2026-10-14T09-00-00-abc.jsonl"));
        assert!(wildcard("*_abc.jsonl", "2026_abc.jsonl"));
        assert!(wildcard("*", "anything"));
        assert!(wildcard("a*b*c", "abc"));
        assert!(!wildcard("rollout-*-abc.jsonl", "rollout-x-abd.jsonl"));
        assert!(!wildcard("abc", "abcd"));
        assert!(!wildcard("a*a", "a"));
    }

    #[test]
    fn a_session_is_found_under_any_directory_a_wildcard_names() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("sessions").join("2026").join("10");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("rollout-0900-s1.jsonl");
        std::fs::write(&file, "{}\n").unwrap();
        std::fs::write(dir.join("rollout-0900-s2.jsonl"), "{}\n").unwrap();

        let template = [format!("{}/sessions/*/*/rollout-*-{{session_id}}.jsonl", root.path().display())];
        assert_eq!(find(&template, "s1", Path::new("/c")), Some(file));
        assert_eq!(find(&template, "s3", Path::new("/c")), None);
    }

    #[test]
    fn a_template_may_name_a_place_in_the_checkout() {
        let checkout = tempfile::tempdir().unwrap();
        let dir = checkout.path().join(".herdr");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("s1.jsonl"), "{}\n").unwrap();
        let found = find(&["{checkout}/.herdr/{session_id}.jsonl".into()], "s1", checkout.path());
        assert_eq!(found, Some(dir.join("s1.jsonl")));
    }

    #[test]
    fn a_session_id_that_could_leave_its_directory_names_nothing() {
        let checkout = tempfile::tempdir().unwrap();
        std::fs::write(checkout.path().join("secret.jsonl"), "{}\n").unwrap();
        let template = ["{checkout}/sub/{session_id}.jsonl".to_string()];
        for id in ["../secret", "..\\secret", "*", ""] {
            assert_eq!(find(&template, id, checkout.path()), None, "{id}");
        }
    }

    #[test]
    fn a_session_id_spelling_a_placeholder_is_only_an_id() {
        // A checkout spelled relative to a project directory would lead
        // the id `{checkout}` out of it, were the id expanded again.
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("projects").join("p")).unwrap();
        std::fs::write(root.path().join("secret.jsonl"), "{}\n").unwrap();
        let template = [format!("{}/projects/*/{{session_id}}.jsonl", root.path().display())];
        assert_eq!(find(&template, "{checkout}", Path::new("../../secret")), None);
    }

    #[test]
    fn a_project_directory_is_its_checkout_however_the_harness_spells_it() {
        assert!(same_place("C--Source-app", Path::new(r"C:\Source\app")));
        assert!(same_place("--home-me-my-app--", Path::new("/home/me/my_app")));
        assert!(same_place("-Users-Me-app", Path::new("/users/me/app")));
        assert!(!same_place("-home-me-app2", Path::new("/home/me/app")));
        assert!(!same_place("-home-me", Path::new("/home/me/app")));
    }

    #[test]
    fn an_id_is_the_longest_run_its_template_leaves() {
        assert_eq!(capture("{session_id}.jsonl", "abc.jsonl").as_deref(), Some("abc"));
        assert_eq!(capture("*_{session_id}.jsonl", "2026-10-14_abc.jsonl").as_deref(), Some("abc"));
        // A guess the transcript's own record corrects.
        assert_eq!(capture("rollout-*-{session_id}.jsonl", "rollout-10-14-a-b.jsonl").as_deref(), Some("14-a-b"));
        assert_eq!(capture("{session_id}.jsonl", "abc.txt"), None);
        assert_eq!(capture("{session_id}", ".."), None);
    }

    #[test]
    fn every_session_is_listed_under_the_checkout_its_directory_is_named_for() {
        let root = tempfile::tempdir().unwrap();
        let (app, other) = (PathBuf::from("/src/app"), PathBuf::from("/src/other"));
        let projects = root.path().join("projects");
        for (dir, file) in [("-src-app", "a1.jsonl"), ("-src-app", "notes.txt"), ("-src-other", "o1.jsonl"), ("-src-gone", "g1.jsonl")] {
            std::fs::create_dir_all(projects.join(dir)).unwrap();
            std::fs::write(projects.join(dir).join(file), "{}\n").unwrap();
        }
        let template = format!("{}/projects/{{project}}/{{session_id}}.jsonl", root.path().display());
        let mut found: Vec<_> = every(&template, &[app, other], &[])
            .into_iter()
            .map(|f| (f.session_id, f.checkout))
            .collect();
        found.sort();
        assert_eq!(found, [("a1".to_string(), Some(0)), ("o1".to_string(), Some(1))]);

        // Finding one session by its id looks in every project directory.
        let one = find(&[template], "g1", Path::new("/src/app"));
        assert_eq!(one, Some(projects.join("-src-gone").join("g1.jsonl")));
    }

    #[test]
    fn a_directory_below_a_place_is_listed_for_its_transcript_to_place() {
        let root = tempfile::tempdir().unwrap();
        let projects = root.path().join("projects");
        for dir in ["-src-app", "-src-app--argus-worktrees-gone", "-src-elsewhere"] {
            std::fs::create_dir_all(projects.join(dir)).unwrap();
            std::fs::write(projects.join(dir).join(format!("{dir}.jsonl")), "{}
").unwrap();
        }
        let template = format!("{}/projects/{{project}}/{{session_id}}.jsonl", root.path().display());
        let app = [PathBuf::from("/src/app")];
        let mut found: Vec<_> = every(&template, &app, &app)
            .into_iter()
            .map(|f| (f.session_id, f.checkout))
            .collect();
        found.sort();
        assert_eq!(
            found,
            [("-src-app".to_string(), Some(0)), ("-src-app--argus-worktrees-gone".to_string(), None)]
        );
    }

    #[test]
    fn a_tilde_is_the_home_directory() {
        let home = directories::BaseDirs::new().unwrap().home_dir().to_path_buf();
        assert_eq!(expand("~/.pi/x", "s", Path::new("/c")), Some(home.join(".pi/x")));
        assert_eq!(expand("~user/x", "s", Path::new("/c")), Some(PathBuf::from("~user/x")));
    }
}
