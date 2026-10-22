//! What the window shows, as far as Vim needs to know it: the first and
//! last line in view, from which `H`, `M` and `L` pick a line, paging and
//! half-page scrolling say how far to go, and `Ctrl-e`, `Ctrl-y` and the `z`
//! commands say where the view should be put. The window tells Vim the view
//! (`Vim::set_view`) and does the scrolling Vim asks of it (`Effect::Reveal`).

use super::{Effect, Vim};

/// The lines in view, `bottom` included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View {
    pub top: usize,
    pub bottom: usize,
}

impl Default for View {
    /// With no window to ask, the whole text is in view.
    fn default() -> View {
        View { top: 0, bottom: usize::MAX }
    }
}

/// Where in the view a line is put.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Top,
    Center,
    Bottom,
}

impl View {
    /// The view held to a text of lines `0..=last`.
    pub fn within(self, last: usize) -> View {
        let top = self.top.min(last);
        View { top, bottom: self.bottom.min(last).max(top) }
    }

    pub fn rows(self) -> usize {
        self.bottom - self.top + 1
    }

    /// `H`: the `count`th line from the top.
    pub fn nth_from_top(self, count: usize) -> usize {
        (self.top + count.max(1) - 1).min(self.bottom)
    }

    /// `M`: the middle line.
    pub fn middle(self) -> usize {
        (self.top + self.bottom) / 2
    }

    /// `L`: the `count`th line from the bottom.
    pub fn nth_from_bottom(self, count: usize) -> usize {
        self.bottom.saturating_sub(count.max(1) - 1).max(self.top)
    }

    /// Where a page forward or back puts the cursor from `line`, a page
    /// being the lines in view.
    pub fn paged(self, line: usize, count: usize, forward: bool, last: usize) -> usize {
        let by = self.rows().saturating_mul(count.max(1));
        if forward {
            line.saturating_add(by).min(last)
        } else {
            line.saturating_sub(by)
        }
    }

    /// Where `Ctrl-d` and `Ctrl-u` put the cursor: `count` lines if one was
    /// given, else half the lines in view.
    pub fn halved(self, line: usize, count: Option<usize>, forward: bool, last: usize) -> usize {
        let by = count.unwrap_or_else(|| (self.rows() / 2).max(1)).max(1);
        if forward {
            line.saturating_add(by).min(last)
        } else {
            line.saturating_sub(by)
        }
    }
}

impl Vim {
    /// `Ctrl-e` and `Ctrl-y`: the view moves `count` lines, and the cursor
    /// goes along with it if it would be left out.
    pub(super) fn scroll_lines(&mut self, forward: bool, count: usize) {
        let t = self.buf.text();
        let last = t.last_line();
        let view = self.view.within(last);
        let line = t.line_of(self.cursor);
        let n = count.max(1);
        let (to, reveal) = if forward {
            let top = (view.top + n).min(last);
            (line.max(top), Effect::Reveal { line: top, at: Place::Top })
        } else {
            let bottom = view.bottom.saturating_sub(n);
            (line.min(bottom), Effect::Reveal { line: bottom, at: Place::Bottom })
        };
        if to != line {
            self.cursor = t.at_col(to, self.want);
        }
        self.effects.push(reveal);
    }

    /// `zt`, `zz`, `zb` and the `z.`, `z<CR>`, `z-` that also go to the
    /// first non-blank: the cursor's line is put at `at` in the view.
    pub(super) fn reveal(&mut self, at: Place, first_nonblank: bool) {
        let t = self.buf.text();
        let line = t.line_of(self.cursor);
        if first_nonblank {
            self.cursor = t.first_nonblank(line).min(t.last_char(line).max(t.line_start(line)));
            self.want = t.col(self.cursor);
        }
        self.effects.push(Effect::Reveal { line, at });
    }
}

#[cfg(test)]
mod tests;
