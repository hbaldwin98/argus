//! `:normal keys`: the keys are typed as normal-mode commands, on each line
//! of the range from its start (or where the cursor is, with no range),
//! with an insert they leave open ended where the caret is. `:normal!`
//! types them without the mappings.

use crate::code::vim::key;

use super::range::Parsed;
use super::super::{Mode, Shared, Vim};

impl Vim {
    pub(super) fn ex_normal(&mut self, sh: &mut Shared, p: &Parsed) {
        let arg = p.arg.as_deref().unwrap_or_default();
        let (remap, arg) = match arg.strip_prefix('!') {
            Some(rest) => (false, rest),
            None => (true, arg),
        };
        let arg = arg.trim_start();
        if arg.is_empty() {
            self.message = Some("Argument is required.".into());
            return;
        }
        let keys = key::parse(arg);
        match p.line {
            Some(first) => {
                let last = self.buf.text().last_line();
                let end = p.line_end.unwrap_or(first).min(last);
                for line in first..=end {
                    let t = self.buf.text();
                    self.cursor = t.line_start(line.min(t.last_line()));
                    self.feed_keys(sh, &keys, remap);
                    self.finish_normal(sh);
                }
            }
            None => {
                self.feed_keys(sh, &keys, remap);
                self.finish_normal(sh);
            }
        }
    }

    /// The keys are done: an insert left open ends where the caret is.
    fn finish_normal(&mut self, sh: &mut Shared) {
        if matches!(self.mode, Mode::Insert | Mode::Replace) {
            self.leave_insert(sh, true);
        }
    }
}

#[cfg(test)]
mod tests;
