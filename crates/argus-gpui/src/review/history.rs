//! The History tab of the Git stage: a checkout's newest commits, each
//! unfolding in place to the files it touched, beside the diff of the file
//! under the cursor. It replaces the history dialog, drawn as a full stage.
//!
//! Everything it draws is the session's `HistoryPage`: the rows with the cursor
//! and which commits are open, why there is no list or no diff, and the rows of
//! the diff of the file under the cursor. A press sends the call the Tauri page
//! sends (`review/history.ts`) and changes nothing on screen; the history
//! changes when the next `History` event says so, so a click on a hidden or
//! refused row leaves the list as the session holds it. A commit's diff
//! arrives only in that event, so none is drawn before it.
//!
//! The diff is the shared view with no cursor, no picking and no folds: Rust
//! filters the folds out, and the view asked for none takes no clicks
//! (HISTORY-9). What a body shows is chosen in the page's order: the list's
//! loading bars, then why it is empty, then rows; the diff's reason before its
//! loading bars (SHARED-3). The split between the commits and the diff is the
//! window's own, never sent anywhere. The filter field is in the stage's strip
//! (`review/filter`); its keys and the history's are heard by the stage.
use argus_desktop_core::connection::page::{Call, HistoryPage, HistoryRowView};
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, ClickEvent, Context, Entity, ScrollHandle, SharedString, Subscription, Window};

use crate::chrome::button::{button, Look};
use crate::chrome::icon_button::Node;
use crate::chrome::keycap::keycap;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::diff::view::DiffView;
use crate::icons::Glyph;
use crate::review::ghosts::ghosts;
use crate::review::hints;
use crate::review::split::{self, Split};
use crate::style::Typed as _;
use crate::theme::Tokens;

/// The width the commits open at, and how far the person may drag it.
const COMMITS: f32 = 520.0;
const COMMITS_MIN: f32 = 300.0;
const COMMITS_MAX: f32 = 820.0;

/// The widths of the loading bars of the commit list, and of a diff, as the page draws them.
const LIST_BARS: [f32; 6] = [0.8, 0.7, 0.9, 0.6, 0.75, 0.85];
const DIFF_BARS: [f32; 5] = [0.4, 0.9, 0.8, 0.85, 0.6];

/// The keys the footer names, in order. Bound and unhinted: `o`, `c`, Home and End.
const HINTS: [hints::Hint; 6] = [
    (&["J", "K"], "move"),
    (&["L", "H"], "open / fold"),
    (&["Shift H"], "fold all"),
    (&["Enter"], "as a review"),
    (&["/"], "filter"),
    (&["Esc"], "close"),
];

pub struct History {
    heard: Entity<Heard>,
    caller: Caller,
    diff: Entity<DiffView>,
    split: Entity<Split>,
    scroll: ScrollHandle,
    /// The row lit when the list last scrolled to it, and where in the list it stood.
    lit: Option<(usize, usize)>,
    _resized: Subscription,
}

impl History {
    /// Copies the diff's selected text; whether there was any.
    pub fn copy(&self, cx: &mut Context<Self>) -> bool {
        self.diff.update(cx, |diff, cx| diff.copy(cx))
    }

    /// The diff on screen, for the stage's sideways keys.
    pub fn diff(&self) -> &Entity<DiffView> {
        &self.diff
    }

    pub fn new(heard: Entity<Heard>, caller: Caller, cx: &mut Context<Self>) -> Self {
        let diff = cx.new(|cx| DiffView::new("history-diff", false, false, cx));
        let split = cx.new(|cx| Split::new("history-split", "Resize the commits", COMMITS, COMMITS_MIN, COMMITS_MAX, cx));
        let resized = cx.observe(&split, |_, _, cx| cx.notify());
        Self { heard, caller, diff, split, scroll: ScrollHandle::new(), lit: None, _resized: resized }
    }

    /// Follows the session: the diff's rows, and the lit row brought into view
    /// when it moved, by id or by place in the list: a filter or a fold that
    /// takes rows from above a lit row that keeps its id moves it with no
    /// event of its own (SHARED-5, HISTORY-18). A row that stays lit where it
    /// was leaves the scroll where it is.
    pub fn follow(&mut self, page: &HistoryPage, cx: &mut Context<Self>) {
        self.diff.update(cx, |diff, cx| diff.show(&page.diff, false, None, cx));
        let at = page.rows.iter().position(|row| row.on);
        let now = at.map(|at| (page.rows[at].row, at));
        if now != self.lit {
            self.lit = now;
            if let Some((_, at)) = now {
                self.scroll.scroll_to_item(at);
            }
        }
        cx.notify();
    }

    fn ask(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    /// The small header over the commits: what the list is, and the way to
    /// fold it all by mouse. `.history-head`, with `.btn.sm.quiet` for the button.
    fn head(&self, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .tooltip(|window, cx| Tooltip::new("Fold every commit").build(window, cx))
                    .child(Glyph::FoldAll.at(12.0))
                    .child("Fold all")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.ask(Call::HistoryAction { action: Action::FoldAll, row: None }, cx)
                    })),
            )
    }

    /// `.hrow`: a row of the list, 6px over 8px of padding less the edge the
    /// lit one draws as its mark, the text colour, `--fill` under the
    /// pointer and the selected colour, lifted a step, on the lit one.
    fn row(id: impl Into<gpui_kit::ElementId>, on: bool, tokens: &Tokens) -> Node {
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
    fn commit(&self, r: &HistoryRowView, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
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
    fn file(&self, r: &HistoryRowView, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let row = r.row;
        let cut = r.text.rfind('/').map_or(0, |at| at + 1);
        let (dir, name) = r.text.split_at(cut);
        let tooltip = SharedString::from(r.text.clone());
        let mono = cx.theme().mono_font_family.clone();
        Self::row(("history-row", row), r.on, tokens)
            .aria_label(tooltip.clone())
            .min_h(px(tokens.row_h))
            .pl(px(tokens.sp_32 - 1.0))
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
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
    fn list(&self, page: &HistoryPage, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
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

    /// A quiet line standing where a pane is not drawn: the reason there is nothing to show.
    /// `.review-empty.fg-muted`: 24px about a line in the body type.
    fn said(id: &'static str, words: &str, tokens: &Tokens) -> impl IntoElement {
        div()
            .id(id)
            .test_support()
            .aria_label(SharedString::from(words.to_owned()))
            .size_full()
            .p(px(tokens.sp_24))
            .typed(tokens.type_body)
            .text_color(tokens.fg_muted)
            .child(SharedString::from(words.to_owned()))
    }

    /// The file under the cursor and its commit, with the way to open it as a
    /// review. `.fhdr`: the chrome colour over a hairline.
    fn diff_head(&self, page: &HistoryPage, path: &str, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let cut = path.rfind('/');
        let name = SharedString::from(path[cut.map_or(0, |at| at + 1)..].to_owned());
        let dir = cut.map(|at| SharedString::from(path[..at].to_owned()));
        let full = SharedString::from(path.to_owned());
        let commit = SharedString::from(page.commit.clone().unwrap_or_default());
        let mono = cx.theme().mono_font_family.clone();
        h_flex()
            .id("history-diff-head")
            .test_support()
            .aria_label(full)
            .flex_shrink_0()
            .h(px(tokens.sp_44))
            .px(px(tokens.sp_12))
            .gap(px(tokens.sp_12))
            .items_center()
            .bg(tokens.bg_chrome)
            .border_b_1()
            .border_color(tokens.line)
            .child(
                div()
                    .id("history-commit")
                    .test_support()
                    .aria_label(commit.clone())
                    .flex_shrink_0()
                    .font_family(mono.clone())
                    .text_color(tokens.fg_faint)
                    .tooltip(|window, cx| Tooltip::new("Diffed against its first parent").build(window, cx))
                    .child(commit),
            )
            // The name leads and the folder after it is what gives way: a path
            // cut at its end loses the name.
            .child(
                h_flex()
                    .min_w_0()
                    .gap(px(tokens.sp_8))
                    .overflow_hidden()
                    .font_family(mono)
                    .child(div().flex_shrink_0().weighted(tokens.weight_semibold).text_color(tokens.fg).child(name))
                    .when_some(dir, |head, dir| head.child(div().min_w_0().truncate().text_color(tokens.fg_faint).child(dir))),
            )
            .child(div().flex_1())
            .child(
                button("history-as-review", "Open as a review", Look::Plain, true, tokens)
                    .child("Open as a review")
                    .child(keycap("O", SharedString::from(tokens.font_mono), tokens))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.ask(Call::HistoryAction { action: Action::AsReview, row: None }, cx)
                    })),
            )
    }

    /// Which file, then the diff's reason before its loading bars, then its rows.
    fn diff_body(&self, page: &HistoryPage, tokens: &Tokens) -> gpui_kit::AnyElement {
        if page.path.is_none() {
            return Self::said("history-pick", "Open a commit and pick a file.", tokens).into_any_element();
        }
        if let Some(why) = &page.diff_empty {
            return Self::said("history-diff-empty", why, tokens).into_any_element();
        }
        if page.diff_loading {
            return div().size_full().bg(tokens.bg_sunken).child(ghosts("history-diff-loading", &DIFF_BARS, tokens)).into_any_element();
        }
        div().id("history-rows").test_support().size_full().min_h_0().child(self.diff.clone()).into_any_element()
    }
}

impl Render for History {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.heard.read(cx).history.clone() else {
            return div().into_any_element();
        };
        let commits = v_flex()
            .id("history-commits")
            .size_full()
            .min_w_0()
            .child(self.head(&tokens, cx))
            .child(div().flex_1().min_h_0().child(self.list(&page, &tokens, cx)));
        let diff = v_flex()
            .id("history-diff-pane")
            .size_full()
            .min_w_0()
            .when_some(page.path.clone(), |pane, path| pane.child(self.diff_head(&page, &path, &tokens, cx)))
            .child(div().flex_1().min_h_0().bg(tokens.bg_sunken).child(self.diff_body(&page, &tokens)));
        v_flex()
            .id("history")
            .test_support()
            .size_full()
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(split::side(&self.split, "history-column", commits, &tokens, cx))
                    .child(div().flex_1().min_w_0().h_full().child(diff)),
            )
            .child(hints::bar("history-hints", &HINTS, &tokens))
            .into_any_element()
    }
}
