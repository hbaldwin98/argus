//! `gc`: line comments toggled. With the token a line comment begins with
//! in the file's language, the lines it is given are commented at their
//! common indent, or uncommented if every one already is. The page's editor
//! has no language loaded, so there `gc` finds no token and changes
//! nothing; the host gives one with `Vim::set_comment_token`.

use crate::code::change::Change;

use super::Vim;

impl Vim {
    /// Toggles the comment of lines `first` to `last`.
    pub(super) fn toggle_comment(&mut self, first: usize, last: usize) {
        let Some(token) = self.comment.clone() else { return };
        if self.buf.read_only() {
            return;
        }
        let t = self.buf.text();
        let blank = |l: usize| t.line_text(l).trim().is_empty();
        let mut lines: Vec<usize> = (first..=last.min(t.last_line())).filter(|l| !blank(*l)).collect();
        if lines.is_empty() {
            lines = (first..=last.min(t.last_line())).collect();
        }
        let commented = |l: usize| t.line_text(l).trim_start().starts_with(&token);
        let mut edits = Vec::new();
        if lines.iter().all(|l| commented(*l)) {
            for &l in &lines {
                let start = t.first_nonblank(l);
                let mut end = start + token.len();
                if t.char_at(end) == Some(' ') {
                    end += 1;
                }
                edits.push(Change { from: start, to: end, insert: String::new() });
            }
        } else {
            let col = lines
                .iter()
                .map(|l| t.line_text(*l).chars().take_while(|c| c.is_whitespace()).count())
                .min()
                .unwrap_or(0);
            for &l in &lines {
                let at = t.at(l, col);
                edits.push(Change { from: at, to: at, insert: format!("{token} ") });
            }
        }
        self.buf.edit(&edits);
        let t = self.buf.text();
        self.cursor = t.line_start(first.min(t.last_line()));
        self.want = 0;
    }
}

#[cfg(test)]
mod tests;
