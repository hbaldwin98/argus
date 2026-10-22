//! The ex commands that act on the editor, not on lines: `:w` asks the
//! host to write, `:startinsert` enters insert mode (`!` at the line's
//! end), and a bare line number moves the cursor there.

use crate::code::vim::key::Key;

use super::range::Parsed;
use super::super::{Effect, Shared, Vim};

impl Vim {
    /// `:w`, whatever follows it: the page's own callback takes no argument.
    pub(super) fn ex_write(&mut self) {
        self.effects.push(Effect::Write);
    }

    /// `:startinsert` is `i`; `:startinsert!` is `A`.
    pub(super) fn ex_start_insert(&mut self, sh: &mut Shared, p: &Parsed) {
        let key = if p.arg.as_deref() == Some("!") { 'A' } else { 'i' };
        self.feed_keys(sh, &[Key::ch(key)], false);
    }

    /// `:N`: the cursor goes to the first non-blank of line `N`, 0-based here.
    pub(super) fn goto_line(&mut self, line: usize) {
        let t = self.buf.text();
        let line = line.min(t.last_line());
        let p = t.first_nonblank(line);
        self.cursor = if p >= t.line_end(line) { t.last_char(line) } else { p };
    }
}

#[cfg(test)]
mod tests;
