//! The list column: a Live heading and its rows, then the Ended group, held under one row that folds
//! until it is opened or a search or filter asks for what is in it (SESSIONS-2..7, -22).
//!
//! The rows, their order and which one the cursor is on are core's and are drawn as they come, never
//! sorted here. A click moves the cursor (`next` with the row) and a second click goes on to `choose`
//! (the first of a double click is the single). The cursor row is brought into view nearest when it
//! moves, and the list keeps its scroll otherwise.

use argus_desktop_core::agents::said::{ended_summary, folds, row_meta, row_tip};
use argus_desktop_core::agents::sessions_dialog::{SessionRowView, SessionsPage};
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, ClickEvent, Context, SharedString, TestSupportExt as _, Window};

use super::{said, SessionsView};
use crate::agents::ask::ask;
use crate::kit::ghosts::ghosts;
use crate::kit::icons::Glyph;
use crate::kit::selected::selected;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::chrome::stage::route::narrow;
use crate::theme::scale::{css, rem, Type};
use crate::theme::Tokens;

/// The widths of the loading bars of the list, as the page draws them.
const BARS: [f32; 6] = [0.8, 0.7, 0.9, 0.6, 0.75, 0.85];

fn act(action: Action, row: Option<usize>) -> Call {
    Call::SessionsAction { action, row }
}

impl SessionsView {
    /// `.history-list` as the sessions use it: the history's 520px, narrowing to 360 in a window of 1100 or less.
    pub(super) fn list_column(&mut self, page: &SessionsPage, window: &Window, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = narrow(css(window.viewport_size().width, window.rem_size()), tokens.history_narrow_below);
        let body = self.list_body(page, tokens, cx);
        div()
            .id("sessions-list")
            .test_support()
            .aria_label("Sessions")
            .flex_shrink_0()
            .w(rem(if narrow { tokens.history_col_narrow_w } else { tokens.history_col_w }))
            .h_full()
            .border_r_1()
            .border_color(tokens.line)
            .child(body)
    }

    fn list_body(&mut self, page: &SessionsPage, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        if page.loading {
            return div().id("sessions-loading-list").test_support().aria_label("Loading sessions").size_full().child(ghosts("sessions-loading", &BARS, tokens)).into_any_element();
        }
        if let Some(why) = &page.empty {
            return said("sessions-empty", why, tokens).into_any_element();
        }
        let (children, lit) = self.rows(page, tokens, cx);
        // The cursor row is brought into view nearest when it moved, by id or by place.
        let now = lit.map(|at| (page.rows.iter().find(|row| row.on).map_or(0, |row| row.row), at));
        if now != self.lit {
            self.lit = now;
            if let Some((_, at)) = now {
                self.list.scroll_to_item(at);
            }
        }
        v_flex().id("sessions-rows").size_full().overflow_y_scroll().track_scroll(&self.list).p(rem(tokens.sp_8)).gap(rem(tokens.sp_2)).children(children).into_any_element()
    }

    /// The group headings and rows, and where among them the cursor row is.
    fn rows(&self, page: &SessionsPage, tokens: &Tokens, cx: &mut Context<Self>) -> (Vec<AnyElement>, Option<usize>) {
        let mut children: Vec<AnyElement> = Vec::new();
        let mut lit = None;
        let live: Vec<&SessionRowView> = page.rows.iter().filter(|row| row.live).collect();
        let ended: Vec<&SessionRowView> = page.rows.iter().filter(|row| !row.live).collect();
        if !live.is_empty() {
            children.push(heading("sessions-live", "Live", tokens).into_any_element());
        }
        for row in live {
            if row.on {
                lit = Some(children.len());
            }
            children.push(self.row(row, tokens, cx));
        }
        if page.ended_count > 0 {
            children.push(if folds(page) { self.fold(page, tokens, cx).into_any_element() } else { heading("sessions-ended", "Ended", tokens).into_any_element() });
        }
        for row in ended {
            if row.on {
                lit = Some(children.len());
            }
            children.push(self.row(row, tokens, cx));
        }
        (children, lit)
    }

    /// One session: its title over what ran where, and for an ended one its age at the right (the
    /// group says which are live). A removed worktree's is dimmed to the muted colour, no further.
    fn row(&self, row: &SessionRowView, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let (at, on) = (row.row, row.on);
        let colour = if row.removed { tokens.fg_muted } else { tokens.fg };
        let title = if row.title.is_empty() { "Untitled session".to_owned() } else { row.title.clone() };
        let meta = SharedString::from(row_meta(row));
        let tooltip = row_tip(row);
        selected(
            h_flex()
                .id(("sessions-row", at))
                .test_support()
                .aria_label(SharedString::from(title.clone()))
                .aria_selected(on)
                .flex_shrink_0()
                .gap(rem(tokens.sp_8))
                .items_center()
                .min_w_0()
                .py(rem(tokens.sp_6 - tokens.hairline))
                .px(rem(tokens.sp_8 - tokens.hairline))
                .rounded(rem(tokens.r_sm))
                .text_size(rem(tokens.fs_sm))
                .text_color(tokens.fg)
                .cursor_pointer()
                .when(!on, |row| row.hover(move |style| style.bg(tokens.fill)))
                .when_some(tooltip, |row, words| row.tooltip(tip(words)))
                .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                    ask(&this.heard, &this.caller, act(Action::Next, Some(at)), cx);
                    if event.click_count() >= 2 {
                        ask(&this.heard, &this.caller, act(Action::Choose, None), cx);
                    }
                }))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(rem(tokens.sp_2))
                        .child(div().min_w_0().truncate().text_color(colour).child(SharedString::from(title)))
                        .child(div().id(("sessions-meta", at)).test_support().aria_label(meta.clone()).min_w_0().truncate().text_size(rem(tokens.fs_xs)).text_color(tokens.fg_muted).child(meta)),
                )
                .children((!row.live).then(|| div().flex_shrink_0().typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(row.ago.clone())))),
            on,
            tokens,
        )
        .into_any_element()
    }

    /// `.sgroup.sfold`: the Ended group as one row, a chevron, its count and how long since the newest
    /// was written. It is drawn the same once opened, with the rows under it, so a second click folds
    /// it back.
    fn fold(&self, page: &SessionsPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let open = !page.ended_folded;
        let summary = SharedString::from(ended_summary(page));
        h_flex()
            .id("sessions-ended")
            .test_support()
            .aria_label("Ended")
            .aria_expanded(open)
            .flex_shrink_0()
            .mt(rem(tokens.sp_8))
            .gap(rem(tokens.sp_8))
            .items_center()
            .p(rem(tokens.sp_8))
            .border_t_1()
            .border_color(tokens.line)
            .typed(Type::new(tokens.weight_semibold, tokens.fs_sm, 1.4))
            .text_color(tokens.fg_muted)
            .cursor_pointer()
            .hover(move |style| style.bg(tokens.fill))
            .on_click(cx.listener(|this, _, _, cx| ask(&this.heard, &this.caller, act(Action::ToggleEnded, None), cx)))
            .child(div().flex_shrink_0().text_color(tokens.fg_faint).child(if open { Glyph::Down } else { Glyph::Right }.at(tokens.icon_sm)))
            .child("Ended")
            .child(div().id("sessions-ended-summary").test_support().aria_label(summary.clone()).typed(tokens.type_small).weighted(tokens.weight_regular).text_color(tokens.fg_muted).child(summary))
    }
}

/// `.mlabel.sgroup`: a group's name, 600 at the small size, in the muted colour.
fn heading(id: &'static str, words: &'static str, tokens: &Tokens) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .aria_label(words)
        .flex_shrink_0()
        .pt(rem(tokens.sp_6))
        .px(rem(tokens.sp_8))
        .pb(rem(tokens.sp_2))
        .typed(Type::new(tokens.weight_semibold, tokens.fs_sm, 1.4))
        .text_color(tokens.fg_muted)
        .child(words)
}
