//! The bar over the editor (`.ed-bar`): one line of the file's status in its colour; at its right
//! the grammar on offer with Install and Not now, or while a clash is held the two buttons that say
//! whose text stands (EDITOR-STATE, EDITOR-CLASH-4, EDITOR-GRAM-7); and Vim's mode, the keys of a
//! command begun and the line being typed, which the page draws in the library's own panel.

use argus_desktop_core::code::bar::{said, tone, Tone};
use argus_desktop_core::code::grammar::slot::Shown;
use argus_desktop_core::code::live::Status;
use argus_desktop_core::code::sync::Keep;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, AnyElement, Context, SharedString, TestSupportExt as _};

use super::CodeEditor;
use crate::kit::button::{button, Look};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl CodeEditor {
    /// The bar of the file on screen, or a blank bar before any file is shown.
    pub(super) fn bar(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let status = self.shown.as_ref().and_then(|key| self.lives.get(key.0, &key.1)).map(|live| live.status().clone()).unwrap_or(Status::Opening);
        let colour = match tone(&status) {
            Tone::Faint => tokens.fg_faint,
            Tone::Muted => tokens.fg_muted,
            Tone::Danger => tokens.danger,
            Tone::Attention => tokens.attention,
        };
        let clash = status == Status::Clash;
        let offer = self.shown.as_ref().and_then(|key| self.sessions.get(key)).and_then(|session| session.grammar.shown(clash));
        let vim = self.vim_said();
        h_flex()
            .id("editor-bar")
            .test_support()
            .flex_shrink_0()
            .w_full()
            .min_h(rem(26.0))
            .px(rem(tokens.sp_12))
            .py(rem(tokens.sp_2))
            .gap(rem(tokens.sp_8))
            .items_center()
            .border_b_1()
            .border_color(tokens.line)
            .typed(tokens.type_small)
            .text_size(rem(tokens.fs_xs))
            .child(div().id("editor-state").test_support().aria_label(SharedString::from(said(&status))).text_color(colour).child(SharedString::from(said(&status))))
            .children(vim.as_ref().filter(|vim| !vim.line.is_empty()).map(|vim| {
                div().id("vim-line").test_support().aria_label(SharedString::from(vim.line.clone())).font_family(SharedString::from(tokens.font_mono)).text_color(tokens.fg_muted).child(SharedString::from(vim.line.clone()))
            }))
            .child(div().flex_1())
            .when(clash, |bar| {
                bar.child(button("editor-keep-mine", "Keep mine", Look::Plain, true, tokens).child("Keep mine").on_click(cx.listener(|this, _, window, cx| this.resolve(Keep::Mine, window, cx))))
                    .child(button("editor-take-theirs", "Take the agent's", Look::Primary, true, tokens).child("Take the agent's").on_click(cx.listener(|this, _, window, cx| this.resolve(Keep::Theirs, window, cx))))
            })
            .children(match offer {
                Some(Shown::Ask(ask)) => vec![
                    div()
                        .id("editor-grammar")
                        .min_w_0()
                        .flex_shrink_1()
                        .test_support()
                        .aria_label(SharedString::from(format!("{}{}", ask.question, ask.from)))
                        .text_color(tokens.fg_faint)
                        .child(SharedString::from(ask.question))
                        .child(div().font_family(SharedString::from(tokens.font_mono)).child(SharedString::from(ask.from)))
                        .into_any_element(),
                    button("editor-install", "Install", Look::Primary, true, tokens).child("Install").on_click(cx.listener(|this, _, _, cx| this.install_grammar(cx))).into_any_element(),
                    button("editor-not-now", "Not now", Look::Plain, true, tokens).child("Not now").on_click(cx.listener(|this, _, window, cx| this.not_now(window, cx))).into_any_element(),
                ],
                Some(Shown::Text(text)) => vec![div().id("editor-grammar").min_w_0().flex_shrink_1().test_support().aria_label(SharedString::from(text.clone())).text_color(tokens.fg_faint).child(SharedString::from(text)).into_any_element()],
                None => Vec::new(),
            })
            .children(vim.as_ref().filter(|vim| !vim.pending.is_empty()).map(|vim| {
                div().id("vim-pending").test_support().aria_label(SharedString::from(vim.pending.clone())).font_family(SharedString::from(tokens.font_mono)).text_color(tokens.fg_muted).child(SharedString::from(vim.pending.clone()))
            }))
            .children(vim.map(|vim| div().id("vim-mode").test_support().aria_label(SharedString::from(vim.mode)).text_color(tokens.fg_faint).child(SharedString::from(vim.mode))))
            .into_any_element()
    }
}
