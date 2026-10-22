//! The commits column: its header with the fold-all button, a commit's row and the files under it
//! once it is open, or the loading bars and the reason there are none.

use argus_desktop_core::connection::page::{Call, HistoryPage, HistoryRowView};
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, ClickEvent, Context, SharedString};

use super::History;
use crate::kit::tip::tip;
use crate::kit::button::{button, Look};
use crate::kit::ghosts::ghosts;
use crate::kit::icon_button::Node;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::Tokens;

/// The widths of the loading bars of the commit list, as the page draws them.
const LIST_BARS: [f32; 6] = [0.8, 0.7, 0.9, 0.6, 0.75, 0.85];

impl History {
    /// The small header over the commits: what the list is, and the way to
    /// fold it all by mouse. `.history-head`, with `.btn.sm.quiet` for the button.
    pub(super) fn head(&self, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("history-head")
            .flex_shrink_0()
            .pt(px(tokens.sp_6))
            .pr(px(tokens.sp_8))
            .pl(px(tokens.sp_16))
            .gap(px(tokens.sp_8))
            .items_center()
            .child(
                div()
                    .flex_1()
                    .text_size(px(tokens.fs_xs))
                    .weighted(tokens.weight_semibold)
                    .text_color(tokens.fg_muted)
                    .child("Commits"),
            )
            .child(
                button("history-fold-all", "Fold all", Look::Quiet, true, tokens)
                    .tooltip(tip("Fold every commit"))
                    .child(Glyph::FoldAll.at(tokens.icon_xs))
                    .child("Fold all")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.ask(Call::HistoryAction { action: Action::FoldAll, row: None }, cx)
                    })),
            )
    }

    /// `.hrow`: a row of the list, 6px over 8px of padding less the edge the
    /// lit one draws as its mark, the text colour, `--fill` under the
    /// pointer and the selected colour, lifted a step, on the lit one.
    pub(super) fn row(id: impl Into<gpui_kit::ElementId>, on: bool, tokens: &Tokens) -> Node {
        let (fill, selected_hover) = (tokens.fill, tokens.selected_hover);
        h_flex()
            .id(id)
            .test_support()
            .aria_selected(on)
            .flex_shrink_0()
            .gap(px(tokens.sp_8))
            .items_center()
            .min_w_0()
            .py(px(tokens.sp_6 - 1.0))
            .px(px(tokens.sp_8 - 1.0))
            .rounded(px(tokens.r_sm))
            .border_1()
            .border_color(if on { tokens.line_strong } else { gpui_kit::transparent_black() })
            .text_size(px(tokens.fs_sm))
            .text_color(tokens.fg)
            .cursor_pointer()
            .when(on, |row| row.bg(tokens.bg_selected).hover(move |style| style.bg(selected_hover)))
            .when(!on, |row| row.hover(move |style| style.bg(fill)))
    }

    /// A commit: its chevron, short hash, subject and age. A click toggles it,
    /// as the chevron says, and moves the cursor there; the session decides
    /// what it leaves open (HISTORY-4).
    pub(super) fn commit(&self, r: &HistoryRowView, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let (row, expanded) = (r.row, r.expanded);
        let meta = if r.loading {
            Some("loading\u{2026}".to_owned())
        } else if r.expanded {
            r.files.map(|n| format!("{n} file{}", if n == 1 { "" } else { "s" }))
        } else {
            None
        };
        // `.chev`: faint, and the muted of an open commit; the page turns the one
        // glyph a quarter and a GPUI icon swaps for the one already drawn down.
        let chevron = if expanded { Glyph::Down.at(tokens.icon_sm).text_color(tokens.fg_muted) } else { Glyph::Right.at(tokens.icon_sm).text_color(tokens.fg_faint) };
        Self::row(("history-row", row), r.on, tokens)
            .aria_expanded(expanded)
            .aria_label(SharedString::from(format!("{} {}", r.short, r.text)))
            .on_click(cx.listener(move |this, _, _, cx| {
                let action = if expanded { Action::Fold } else { Action::Unfold };
                this.ask(Call::HistoryAction { action, row: Some(row) }, cx);
            }))
            .child(div().flex_shrink_0().child(chevron))
            .child(div().flex_shrink_0().font_family(cx.theme().mono_font_family.clone()).text_color(tokens.fg_faint).child(SharedString::from(r.short.clone())))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when(expanded, |subject| subject.weighted(tokens.weight_semibold))
                    .child(SharedString::from(r.text.clone())),
            )
            .when_some(meta, |line, meta| {
                let meta = SharedString::from(meta);
                line.child(
                    div()
                        .id(("history-meta", row))
                        .test_support()
                        .aria_label(meta.clone())
                        .flex_shrink_0()
                        .typed(tokens.type_small)
                        .text_color(tokens.fg_muted)
                        .child(meta),
                )
            })
            .child(div().flex_shrink_0().typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(r.ago.clone())))
            .into_any_element()
    }

    /// A file of an unfolded commit: the folder dim before its name, and its
    /// counts. A click puts the cursor there and shows its diff; a second one
    /// opens the commit as a review on it, after the first has been sent as
    /// the page's does (HISTORY-5). `.hrow.file` is 32px in, and `--row-h` tall.
    pub(super) fn file(&self, r: &HistoryRowView, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let row = r.row;
        let cut = r.text.rfind('/').map_or(0, |at| at + 1);
        let (dir, name) = r.text.split_at(cut);
        let tooltip = SharedString::from(r.text.clone());
        let mono = cx.theme().mono_font_family.clone();
        Self::row(("history-row", row), r.on, tokens)
            .aria_label(tooltip.clone())
            .min_h(px(tokens.row_h))
            .pl(px(tokens.sp_32 - 1.0))
            .tooltip(tip(tooltip.clone()))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                this.ask(Call::HistoryAction { action: Action::Next, row: Some(row) }, cx);
                if event.click_count() >= 2 {
                    this.ask(Call::HistoryAction { action: Action::Choose, row: None }, cx);
                }
            }))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .when(!dir.is_empty(), |file| {
                        file.child(
                            div()
                                .flex_shrink_0()
                                .font_family(mono.clone())
                                .text_size(px(tokens.fs_xs))
                                .text_color(tokens.fg_faint)
                                .child(SharedString::from(dir.to_owned())),
                        )
                    })
                    .child(div().truncate().child(SharedString::from(name.to_owned()))),
            )
            .child(div().flex_shrink_0().font_family(mono.clone()).text_color(tokens.ok).child(SharedString::from(format!("+{}", r.added))))
            .child(div().flex_shrink_0().font_family(mono).text_color(tokens.danger).child(SharedString::from(format!("\u{2212}{}", r.removed))))
            .into_any_element()
    }

    /// Loading bars, or why there are no commits, or the commits.
    pub(super) fn list(&self, page: &HistoryPage, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        if page.loading {
            return ghosts("history-loading", &LIST_BARS, tokens).into_any_element();
        }
        if let Some(why) = &page.empty {
            return Self::said("history-empty", why, tokens).into_any_element();
        }
        // `.history-list`: 8px in from every edge, 2px between rows.
        v_flex()
            .id("history-list")
            .test_support()
            .aria_label("Commits")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p(px(tokens.sp_8))
            .gap(px(tokens.sp_2))
            .children(page.rows.iter().map(|r| if r.kind == "file" { self.file(r, tokens, cx) } else { self.commit(r, tokens, cx) }))
            .into_any_element()
    }
}
