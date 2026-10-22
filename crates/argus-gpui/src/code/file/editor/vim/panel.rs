//! The panel under the editor where Vim speaks: the `:` or `/` line as it is typed, an error, and
//! `recording @q`. The page's library puts these in `.cm-vim-panel`, mono at `--fs-sm` on the
//! surface colour (`css:6258-6270`), and the panel is there only while there is something to say.
//! The mode and the keys of a command begun stay in the bar, as the window's own.

use gpui_kit::{div, prelude::*, AnyElement, SharedString, TestSupportExt as _};

use super::super::CodeEditor;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl CodeEditor {
    /// The panel, or nothing while Vim has no line to show.
    pub(in crate::code::file::editor) fn vim_panel(&self, tokens: &Tokens) -> Option<AnyElement> {
        let line = self.vim_said().map(|said| said.line).filter(|line| !line.is_empty())?;
        let line = SharedString::from(line);
        Some(
            div()
                .id("vim-line")
                .test_support()
                .aria_label(line.clone())
                .flex_shrink_0()
                .w_full()
                .px(rem(tokens.sp_12))
                .py(rem(tokens.sp_2))
                .bg(tokens.bg_surface)
                .border_t_1()
                .border_color(tokens.line)
                .typed(tokens.type_small)
                .font_family(SharedString::from(tokens.font_mono))
                .text_size(rem(tokens.fs_sm))
                .text_color(tokens.fg)
                .child(line)
                .into_any_element(),
        )
    }
}
