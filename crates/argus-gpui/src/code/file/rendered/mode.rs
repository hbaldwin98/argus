//! Whether a markdown file is read as a document or as its text, and which each file was left in.
//!
//! The choice is the window's own, kept by path, since it says how this window draws a file and
//! nothing the session or another window should follow. A file nobody has chosen for is read
//! rendered: that is what a person opening a README came to see.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Rendered,
    Source,
}

/// Whether a file is markdown, by its extension; the choice is offered for these alone.
pub fn is_markdown(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(stem, ext)| !stem.is_empty() && (ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown")))
}

#[derive(Default)]
pub struct Modes(HashMap<String, Mode>);

impl Modes {
    pub fn of(&self, path: &str) -> Mode {
        self.0.get(path).copied().unwrap_or(Mode::Rendered)
    }

    pub fn choose(&mut self, path: &str, mode: Mode) {
        self.0.insert(path.to_owned(), mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_is_told_by_extension_and_not_by_a_bare_dotfile() {
        for yes in ["README.md", "docs/guide.MD", "a/b/notes.markdown"] {
            assert!(is_markdown(yes), "{yes}");
        }
        for no in ["src/lib.rs", "md", ".md", "docs.md/lib.rs", "notes.mdx", "Makefile"] {
            assert!(!is_markdown(no), "{no}");
        }
    }

    #[test]
    fn a_file_is_rendered_until_it_is_chosen_otherwise_and_each_keeps_its_own() {
        let mut modes = Modes::default();
        assert_eq!(modes.of("a.md"), Mode::Rendered);
        modes.choose("a.md", Mode::Source);
        assert_eq!((modes.of("a.md"), modes.of("b.md")), (Mode::Source, Mode::Rendered));
    }
}
