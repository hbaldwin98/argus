//! What the last search found, marked in the text until a command clears it (`:noh`, or any ex
//! command run), as the library marks the matches of `/`, `?`, `*` and `#` (EDITOR-VIM-7). The
//! library's own style is a wash behind each match; the window's is the attention colour at the
//! share the search results' marks have, since the page's CSS leaves the library's colour.

use argus_desktop_core::code::vim::search::scan;
use gpui_kit::component::input::{RangeDecoration, RangeDecorationStyle};
use gpui_kit::Context;

use super::super::session::Key;
use super::super::CodeEditor;
use crate::theme::Tokens;

impl CodeEditor {
    /// Marks every match of the search Vim holds in the file's text, or takes the marks away while
    /// none is held or Vim is off.
    pub(in crate::code::file::editor) fn vim_matches(&mut self, key: &Key, cx: &mut Context<Self>) {
        let tokens = *Tokens::global(cx);
        let on = self.vim_on;
        let Some(session) = self.sessions.get(key) else { return };
        let (Some(state), Some(look)) = (session.state.clone(), session.look.as_ref()) else { return };
        let matches = look.matches.clone();
        let query = session.vim.as_ref().filter(|_| on).and_then(|vim| vim.highlight(&self.shared));
        let Some(query) = query else {
            if !matches.get_ranges(cx).is_empty() {
                matches.clear(cx);
            }
            return;
        };
        let text = state.read(cx).text().to_string();
        let found = scan::all(query, &text);
        matches.set(found.into_iter().filter(|(from, to)| from < to).map(|(from, to)| RangeDecoration::new(from..to).with_style(RangeDecorationStyle::Fill).with_color(tokens.attention.opacity(tokens.share_hit))).collect(), cx);
    }
}
