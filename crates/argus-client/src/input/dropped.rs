//! A file dropped on the terminal, recognized in what it pasted.
//!
//! A terminal has no way to hand a program a file; dropping one types its
//! path, quoted or escaped the way the terminal's shell would want it.
//! That path is on this machine, and when the daemon is on another the
//! pane cannot open it — so a paste that is one such path to a file here
//! is the file, not the text, and crosses as bytes.

use std::path::PathBuf;

/// What a dropped file may be, by extension: the kinds a harness attaches
/// rather than reads as text, which are the ones worth carrying across.
/// Everything else pastes as the path it always did.
const CARRIED: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "pdf"];

/// What a paste that is one path to a carried file names.
#[derive(Debug, PartialEq, Eq)]
pub enum Dropped {
    /// A file on this machine, ready to cross.
    Here(PathBuf),
    /// A path spelled as one machine's, to a file this one does not have:
    /// a desktop's screenshot dropped on a terminal that is ssh'd into
    /// here. Named so the user is told where the client would have to
    /// run, rather than left with an agent that cannot find the file.
    Elsewhere(String),
}

/// The file a paste names, when it names exactly one.
pub fn file(text: &str) -> Option<Dropped> {
    let typed = unquote(text.trim());
    let one_line = !typed.contains('\n');
    if !one_line || !looks_absolute(&typed) || !is_carried(&typed) {
        return None;
    }
    let path = PathBuf::from(&typed);
    if path.is_file() {
        return Some(Dropped::Here(path));
    }
    // Split on either separator: a Windows path is one component to a
    // Unix `Path`, and its name is the part the user will recognize.
    let name = typed
        .rsplit(['\\', '/'])
        .next()
        .map(str::to_string)
        .unwrap_or(typed);
    Some(Dropped::Elsewhere(name))
}

/// Absolute on any machine's spelling, not only this one's: a Windows
/// drive path dropped into an ssh session reaches a Unix client, and
/// `Path::is_absolute` there does not know it.
fn looks_absolute(typed: &str) -> bool {
    let bytes = typed.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    typed.starts_with('/') || typed.starts_with("~/") || typed.starts_with("\\\\") || drive
}

/// By the text after the last dot, not `Path::extension`, which does not
/// find one in a Windows path on a Unix client.
fn is_carried(typed: &str) -> bool {
    typed
        .rsplit('.')
        .next()
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
    use std::path::Path;

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

        assert_eq!(file(&typed), Some(Dropped::Here(shot.clone())));
        assert_eq!(
            file(&format!("{typed}\n")),
            Some(Dropped::Here(shot.clone())),
            "a drop may end in a newline"
        );
        assert_eq!(file(&format!("'{typed}'")), Some(Dropped::Here(shot.clone())));
        assert_eq!(file(&format!("\"{typed}\"")), Some(Dropped::Here(shot)));
    }

    #[test]
    fn an_image_path_from_another_machine_is_named_as_elsewhere() {
        // A desktop's screenshot dropped on a terminal ssh'd into here:
        // spelled absolutely, in the desktop's own way, and not here.
        let elsewhere = |name: &str| Some(Dropped::Elsewhere(name.to_string()));
        assert_eq!(file("C:\\Users\\me\\Pictures\\shot.png"), elsewhere("shot.png"));
        assert_eq!(file("\"C:\\Users\\me\\a b.PNG\""), elsewhere("a b.PNG"));
        assert_eq!(file("/Users/me/Desktop/shot.png"), elsewhere("shot.png"));
        assert_eq!(file("~/Desktop/shot.png"), elsewhere("shot.png"));
        assert_eq!(file("/nowhere/notes.txt"), None, "only what would have crossed");
    }

    #[test]
    fn a_space_escaped_the_terminal_way_is_a_space() {
        let dir = tempfile::tempdir().unwrap();
        let shot = image_at(dir.path(), "a b.png");
        let typed = shot.to_string_lossy().replace(' ', "\\ ");

        assert_eq!(file(&typed), Some(Dropped::Here(shot)));
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
