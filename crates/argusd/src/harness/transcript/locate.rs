//! Where a session's transcript is on disk, from a harness's path
//! templates.
//!
//! A template is a path with `~` for the home directory, `{session_id}`
//! and `{checkout}` filled in, and `*` matching any run of characters
//! within one component — enough for every harness Argus knows, which file
//! a session under a directory named after the checkout, or a date, or
//! both, and none of which Argus should have to spell the way the harness
//! does.

use std::path::{Component, Path, PathBuf};

/// The transcript the first template that matches anything names. Where a
/// template matches several files, the most recently written wins: a
/// session resumed in another directory leaves its old file behind.
pub(super) fn find(templates: &[String], session_id: &str, checkout: &Path) -> Option<PathBuf> {
    // The id comes from what a hook reported. One that could climb out of
    // the directory it is put in, or match more than itself, names nothing.
    if session_id.is_empty()
        || session_id.contains(['/', '\\', '*', ':'])
        || session_id.contains("..")
    {
        return None;
    }
    templates.iter().find_map(|template| {
        let path = expand(template, session_id, checkout)?;
        matches(&path)
            .into_iter()
            .filter_map(|path| Some((std::fs::metadata(&path).ok()?.modified().ok()?, path)))
            .max()
            .map(|(_, path)| path)
    })
}

fn expand(template: &str, session_id: &str, checkout: &Path) -> Option<PathBuf> {
    // The id goes in last: filled in first, an id spelling `{checkout}`
    // would have the checkout's separators spliced into it.
    let filled = template
        .replace("{checkout}", &checkout.to_string_lossy())
        .replace("{session_id}", session_id);
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
    fn a_tilde_is_the_home_directory() {
        let home = directories::BaseDirs::new().unwrap().home_dir().to_path_buf();
        assert_eq!(expand("~/.pi/x", "s", Path::new("/c")), Some(home.join(".pi/x")));
        assert_eq!(expand("~user/x", "s", Path::new("/c")), Some(PathBuf::from("~user/x")));
    }
}
