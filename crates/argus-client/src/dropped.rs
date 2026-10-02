//! A file dropped on the terminal, recognized in what it pasted.
//!
//! A terminal has no way to hand a program a file; dropping one types its
//! path, quoted or escaped the way the terminal's shell would want it.
//! That path is on this machine, and when the daemon is on another the
//! pane cannot open it — so a paste that is one such path to a file here
//! is the file, not the text, and crosses as bytes.

use std::path::{Path, PathBuf};

/// What a dropped file may be, by extension: the kinds a harness attaches
/// rather than reads as text, which are the ones worth carrying across.
/// Everything else pastes as the path it always did.
const CARRIED: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "pdf"];

/// The file a paste names, when it names exactly one that is here.
pub fn file(text: &str) -> Option<PathBuf> {
    let path = PathBuf::from(unquote(text.trim()));
    if !path.is_absolute() || !is_carried(&path) {
        return None;
    }
    path.is_file().then_some(path)
}

fn is_carried(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| CARRIED.iter().any(|c| ext.eq_ignore_ascii_case(c)))
}

/// The path as typed, without the quoting a terminal put on it: a pair of
/// single or double quotes around the whole (Windows Terminal, GNOME
/// Terminal), or a backslash before each space (macOS Terminal, iTerm2).
fn unquote(text: &str) -> String {
    let quoted = text.len() >= 2
        && (text.starts_with('\'') && text.ends_with('\'')
            || text.starts_with('"') && text.ends_with('"'));
    if quoted {
        return text[1..text.len() - 1].to_string();
    }
    text.replace("\\ ", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_at(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"png").unwrap();
        path
    }

    #[test]
    fn a_dropped_image_path_is_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let shot = image_at(dir.path(), "shot.PNG");
        let typed = shot.to_string_lossy().into_owned();

        assert_eq!(file(&typed), Some(shot.clone()));
        assert_eq!(file(&format!("{typed}\n")), Some(shot.clone()), "a drop may end in a newline");
        assert_eq!(file(&format!("'{typed}'")), Some(shot.clone()));
        assert_eq!(file(&format!("\"{typed}\"")), Some(shot));
    }

    #[test]
    fn a_space_escaped_the_terminal_way_is_a_space() {
        let dir = tempfile::tempdir().unwrap();
        let shot = image_at(dir.path(), "a b.png");
        let typed = shot.to_string_lossy().replace(' ', "\\ ");

        assert_eq!(file(&typed), Some(shot));
    }

    #[test]
    fn text_that_is_not_one_file_here_pastes_as_text() {
        let dir = tempfile::tempdir().unwrap();
        let shot = image_at(dir.path(), "shot.png");
        let typed = shot.to_string_lossy().into_owned();

        assert_eq!(file("look at shot.png"), None, "a sentence");
        assert_eq!(file("shot.png"), None, "relative: whose?");
        assert_eq!(file(&format!("{typed} and more")), None);
        assert_eq!(file(&format!("{typed}\n{typed}")), None, "two files are two drops");
        assert_eq!(file(&dir.path().join("missing.png").to_string_lossy()), None);
        assert_eq!(file(&dir.path().to_string_lossy()), None, "a directory");
    }

    #[test]
    fn a_text_file_pastes_as_its_path() {
        // A source file dropped on the terminal is meant to be read where
        // the pane is; carrying it would hand the agent a copy to edit.
        let dir = tempfile::tempdir().unwrap();
        let src = image_at(dir.path(), "main.rs");

        assert_eq!(file(&src.to_string_lossy()), None);
    }
}
