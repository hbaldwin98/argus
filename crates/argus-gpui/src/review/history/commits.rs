//! The commits column (`project/GitHistory.dc.html`): the kit panel head, the filter field, a
//! commit's two-line row and the files under it once it is open as kit rows, or the loading bars
//! and the reason there are none.

use argus_desktop_core::code::file::path_parts;
use argus_desktop_core::connection::page::{Call, HistoryPage, HistoryRowView};
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, ClickEvent, Context, SharedString};

use super::History;
use crate::kit::counts::counts;
use crate::kit::ghosts::ghosts;
use crate::kit::icons::Glyph;
use crate::kit::panel_head::panel_head;
use crate::kit::row::{nest, row, Slots};
use crate::kit::selected::selected;
use crate::kit::tip::tip;
use crate::kit::tools;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The widths of the loading bars of the commit list, as the page draws them.
const LIST_BARS: [f32; 6] = [0.8, 0.7, 0.9, 0.6, 0.75, 0.85];

impl History {
    /// The kit panel head, `Commits <branch>` with the way to fold it all by mouse, over the
    /// filter field (which the stage owns, and the keys reach).
    pub(super) fn head(&self, page: &HistoryPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let fold = tools::ghost("history-fold-all", "Fold all", None, false, tokens)
            .tooltip(tip("Fold every commit"))
            .on_click(cx.listener(|this, _, _, cx| this.ask(Call::HistoryAction { action: Action::FoldAll, row: None }, cx)))
            .into_any_element();
        v_flex()
            .flex_shrink_0()
            .child(panel_head("history-head", "Commits", Some(SharedString::from(page.branch.clone())), vec![fold], tokens))
            .child(div().p(rem(10.0)).child(self.filter.clone()))
    }

    /// `.cm`: a commit, two lines (subject, then who and when) with its chevron before them and
    /// its short hash at the end. A click toggles it, as the chevron says, and moves the cursor
    /// there; the session decides what it leaves open (HISTORY-4).
    pub(super) fn commit(&self, r: &HistoryRowView, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let (id, expanded, on) = (r.row, r.expanded, r.on);
        let by = if r.author.is_empty() { r.ago.clone() } else { format!("{} \u{b7} {}", r.author, r.ago) };
        let who = if r.loading {
            format!("{by} \u{b7} loading\u{2026}")
        } else if expanded {
            match r.files {
                Some(n) => format!("{by} \u{b7} {n} file{}", if n == 1 { "" } else { "s" }),
                None => by,
            }
        } else {
            by
        };
        let chevron = if expanded { Glyph::Down.at(tokens.icon_xs).text_color(tokens.accent) } else { Glyph::Right.at(tokens.icon_xs).text_color(tokens.fg_faint) };
        let (fill, over) = (tokens.fill, tokens.selected_hover);
        let line = h_flex()
            .id(("history-row", id))
            .test_support()
            .aria_selected(on)
            .aria_expanded(expanded)
            .aria_label(SharedString::from(format!("{} {}", r.short, r.text)))
            .flex_shrink_0()
            .min_w_0()
            .mx(rem(tokens.sp_6))
            .py(rem(6.0))
            .pl(rem(tokens.sp_4))
            .pr(rem(tokens.sp_8))
            .gap(rem(7.0))
            .items_start()
            .rounded(rem(tokens.r_sm))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                let action = if expanded { Action::Fold } else { Action::Unfold };
                this.ask(Call::HistoryAction { action, row: Some(id) }, cx);
            }))
            .map(|line| if on { line.hover(move |style| style.bg(over)) } else { line.hover(move |style| style.bg(fill)) });
        selected(line, on, tokens)
            // The chevron sits on the subject's line, not centred between the two.
            .child(div().flex_none().w(rem(tokens.icon_xs)).mt(rem(tokens.sp_2)).flex().justify_center().child(chevron))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(rem(tokens.sp_2))
                    .child(
                        div()
                            .truncate()
                            .text_size(rem(tokens.fs_md))
                            .text_color(if on || expanded { tokens.fg } else { tokens.fg_muted.blend(tokens.fg.opacity(0.6)) })
                            .when(expanded, |subject| subject.weighted(tokens.weight_semibold))
                            .child(SharedString::from(r.text.clone())),
                    )
                    .child(
                        div()
                            .id(("history-meta", id))
                            .test_support()
                            .aria_label(SharedString::from(who.clone()))
                            .truncate()
                            .typed(tokens.type_small)
                            .text_color(tokens.fg_muted)
                            .child(SharedString::from(who)),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .pl(rem(tokens.sp_8))
                    .font_family(cx.theme().mono_font_family.clone())
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_muted)
                    .child(SharedString::from(r.short.clone())),
            )
            .into_any_element()
    }

    /// A file of an unfolded commit: the kit row, name first and its folder after it faint, with
    /// its counts at the end, on the commit's guide line. A click puts the cursor there and shows
    /// its diff; a second one opens the commit as a review on it, after the first has been sent
    /// as the page's does (HISTORY-5).
    pub(super) fn file(&self, r: &HistoryRowView, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let id = r.row;
        let (name, dir) = path_parts(&r.text);
        let full = SharedString::from(r.text.clone());
        let slots = Slots {
            icon: Some(Glyph::FileSm.at(tokens.icon_xs).into_any_element()),
            folder: Some(SharedString::from(dir.to_owned())),
            end: Some(counts(r.added, r.removed, tokens).into_any_element()),
            on: r.on,
            ..Slots::new(name.to_owned())
        };
        let file = row(("history-row", id), slots, tokens)
            .aria_label(full.clone())
            .tooltip(tip(full))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                this.ask(Call::HistoryAction { action: Action::Next, row: Some(id) }, cx);
                if event.click_count() >= 2 {
                    this.ask(Call::HistoryAction { action: Action::Choose, row: None }, cx);
                }
            }));
        // One guide segment a row: the rows touch, so the segments read as one line.
        nest(("history-nest", id), vec![file.into_any_element()], tokens).ml(rem(tokens.sp_16 + 6.0)).into_any_element()
    }

    /// Loading bars, or why there are no commits, or the commits.
    pub(super) fn list(&self, page: &HistoryPage, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        if page.loading {
            return ghosts("history-loading", &LIST_BARS, tokens).into_any_element();
        }
        if let Some(why) = &page.empty {
            return Self::said("history-empty", why, tokens).into_any_element();
        }
        // The rows carry their own side margin, and no gap breaks a commit's guide line.
        v_flex()
            .id("history-list")
            .test_support()
            .aria_label("Commits")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .pb(rem(tokens.sp_8))
            .children(page.rows.iter().map(|r| if r.kind == "file" { self.file(r, tokens, cx) } else { self.commit(r, tokens, cx) }))
            .into_any_element()
    }
}
