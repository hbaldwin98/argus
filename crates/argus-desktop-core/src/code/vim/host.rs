//! What the host asks of Vim and tells it between keys: the text, the cursor,
//! the selection and the line being typed to show, and the view, the wrap
//! width, the comment token and the read-only flag to set.

use ropey::Rope;

use super::key::Key;
use super::screen::View;
use super::{region, search, Mode, Shared, Vim};

impl Vim {
    pub fn doc(&self) -> &Rope {
        self.buf.doc()
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The cursor, as a byte of the text.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The selected span in visual modes, `from..to` with the character at
    /// the far end included, whole lines in visual-line mode, and the span
    /// that bounds the rows of a block.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let t = self.buf.text();
        match self.mode {
            Mode::Visual => {
                let r = region::of_selection(t, self.anchor, self.cursor, false);
                Some((r.from, r.to))
            }
            Mode::VisualLine => {
                let r = region::of_selection(t, self.anchor, self.cursor, true);
                Some((r.from, r.to))
            }
            Mode::VisualBlock => {
                let rows = self.block_rows();
                let from = rows.iter().map(|r| r.0).min()?;
                let to = rows.iter().map(|r| r.1).max()?;
                Some((from, to))
            }
            _ => None,
        }
    }

    /// The pieces of the selection: one for a character or line selection,
    /// a row each for a block, a row too short to reach the block empty.
    pub fn selections(&self) -> Vec<(usize, usize)> {
        match self.mode {
            Mode::VisualBlock => self.block_rows(),
            _ => self.selection().into_iter().collect(),
        }
    }

    /// The keys of a command not yet finished, for the host to show.
    pub fn pending(&self) -> &[Key] {
        &self.pending
    }

    /// The `:` line as typed, while in command mode.
    pub fn command_line(&self) -> Option<&str> {
        self.prompt().filter(|(prefix, _, _)| *prefix == ':').map(|(_, text, _)| text)
    }

    /// The line being typed after `:`, `/` or `?`: its first character, what
    /// is typed, and the caret in characters.
    pub fn prompt(&self) -> Option<(char, &str, usize)> {
        let p = self.prompt.as_ref()?;
        Some((p.prefix(), p.text.as_str(), p.caret))
    }

    /// The line the last key left at the bottom, if any.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// The file is read-only (EDITOR-VIM-18): no edit is made and insert
    /// mode is not entered.
    pub fn set_read_only(&mut self, on: bool) {
        self.buf.set_read_only(on);
    }

    /// The lines the window shows, `top` to `bottom` inclusive, for `H`,
    /// `M`, `L`, paging and the scroll commands. With none told, the whole
    /// text is in view.
    pub fn set_view(&mut self, top: usize, bottom: usize) {
        self.view = View { top, bottom: bottom.max(top) };
    }

    /// How many characters a row of a wrapped line holds, or none if lines
    /// do not wrap, for `gj`, `gk`, `g0`, `g^` and `g$`.
    pub fn set_wrap(&mut self, width: Option<usize>) {
        self.wrap = width.filter(|w| *w > 0);
    }

    /// The token a line comment begins with in this file's language, for
    /// `gc`. The page's editor loads no language, so there `gc` changes
    /// nothing; with a token it toggles the lines it is given.
    pub fn set_comment_token(&mut self, token: Option<&str>) {
        self.comment = token.map(str::to_owned);
    }

    /// The matches of the last search, for the window to mark, until a
    /// command that clears them (`:noh`, or any ex command run).
    pub fn highlight<'a>(&self, sh: &'a Shared) -> Option<&'a search::pattern::Query> {
        sh.search.highlight.then_some(sh.search.query.as_ref()).flatten()
    }
}
