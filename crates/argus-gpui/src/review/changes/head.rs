//! The head over the diff: the kit's file head (`kit::file_head`), one 44px line with the file's
//! status, name and folder, what changed in it (its counts, how many hunks) and its tools — the
//! layout, the whole file, the editor, the review's drafts, what is picked, and the stage of the
//! whole file. Too narrow for the tools, the lesser ones fold into a menu.
//!
//! Every press sends the call the review's keys send; nothing on screen changes until the session
//! says so, but the turn back to the files, which is the window's own.

use argus_desktop_core::connection::page::{Call, ReviewPage, ReviewReading};
use argus_desktop_core::keys::keymap::Action;
use argus_desktop_core::review::staging::StageGesture;
use gpui_kit::component::h_flex;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Context, Role, SharedString};

use super::lists::{ghost, status};
use super::Changes;
use crate::kit::file_head::{file_head, Head};
use crate::kit::icon_button::Node;
use crate::kit::icons::Glyph;
use crate::kit::menu::item::MenuItem;
use crate::kit::segment;
use crate::kit::strip;
use crate::kit::tip::tip;
use crate::kit::tools;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// Below this many CSS pixels across the diff, its lesser tools are a menu: the full set, with the
/// review button and the stage button, is about 540px and the head must stay one line.
pub const TOOLS_FOLD: f32 = 740.0;

/// How many hunks the diff on screen holds.
pub fn hunks(page: &ReviewPage) -> usize {
    page.rows.iter().filter(|row| row.kind == "hunk").count()
}

/// What the whole file's button does to it: from the index's diff it comes out, else it goes in.
pub fn stage_words(page: &ReviewPage) -> Option<(&'static str, bool)> {
    let staging = page.staging.as_ref()?;
    page.file?;
    Some(if staging.staged { ("Unstage file", true) } else { ("Stage file", false) })
}

/// The tools folded into the menu, as the calls each sends.
fn folded(split: bool) -> Vec<MenuItem<Call>> {
    let review = |action| Call::ReviewAction { action };
    vec![
        MenuItem::choice(if split { "Unified diff" } else { "Split diff" }, review(Action::Split)),
        MenuItem::choice("Whole file", review(Action::UnfoldAll)),
        MenuItem::Rule,
        MenuItem::choice("Open in Code", review(Action::OpenCode)),
        MenuItem::choice("Open in your editor", review(Action::OpenEditor)),
    ]
}

/// A path as the head says it: its folder, without the last slash, and its name.
fn split_path(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

impl Changes {
    /// `back` is whether the files and the diff take turns, and `fold` whether the lesser tools
    /// are a menu.
    pub(super) fn head(&self, page: &ReviewPage, back: bool, fold: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let file = page.file.and_then(|i| page.files.get(i));
        let layout = |id: &'static str, label: &'static str, on: bool, split: bool, cx: &mut Context<Self>| {
            segment::switch_option(id, Role::RadioButton, on, tokens).child(label).on_click(cx.listener(move |this, _, _, cx| {
                // The one already on sends nothing.
                if super::page_split(&this.heard, cx) != split {
                    this.ask(Call::ReviewAction { action: Action::Split }, cx);
                }
            }))
        };
        let tool = |id: &'static str, words: &'static str, key: Option<&str>, action: Action, cx: &mut Context<Self>| {
            tools::ghost(id, words, key, false, tokens).tooltip(tip(words)).on_click(cx.listener(move |this, _, _, cx| this.ask(Call::ReviewAction { action }, cx)))
        };
        let mut bar: Vec<AnyElement> = Vec::new();
        if fold {
            let (caller, heard, items, tokens) = (self.caller.clone(), self.heard.clone(), folded(page.split), *tokens);
            bar.push(
                div()
                    .flex_shrink_0()
                    .child(
                        strip::menu_button("review-more", Glyph::More, 14.0, &tokens, cx)
                            .tooltip("More")
                            .dropdown_menu(move |menu, window, cx| crate::layout::dock_menu::build(menu, &items, &caller, &heard, tokens, window, cx)),
                    )
                    .into_any_element(),
            );
        } else {
            bar.push(
                segment::track("review-view", "Layout", Role::RadioGroup, tokens)
                    .child(layout("review-view-unified", "Unified", !page.split, false, cx))
                    .child(layout("review-view-split", "Split", page.split, true, cx))
                    .into_any_element(),
            );
            bar.push(tool("review-whole", "Whole file", None, Action::UnfoldAll, cx).into_any_element());
            bar.push(tool("review-open", "Open in editor", Some("E"), Action::OpenEditor, cx).into_any_element());
        }
        bar.extend(self.review_button(page, tokens, cx).map(IntoElement::into_any_element));
        if let Some(((words, staged), file)) = stage_words(page).zip(file) {
            let path = file.path.clone();
            bar.push(
                tools::primary("review-stage-file", words, tokens)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.ask(Call::Stage { gesture: StageGesture::File { staged, path: path.clone() } }, cx);
                    }))
                    .into_any_element(),
            );
        }
        let n = hunks(page);
        let (letter, colour, _) = file.map_or(("M", tokens.changed, ""), |f| status(f.kind, tokens));
        let (dir, name) = file.map_or(("", ""), |f| split_path(&f.path));
        let head = Head {
            status: file.map(|_| (letter, colour)),
            name,
            dir,
            counts: file.map(|f| (f.added, f.removed)),
            // What is picked is a part of the one meta, not a tool: a pill among the tools pushed
            // the stage button off the line.
            meta: match (n > 0, page.pick.as_ref()) {
                (true, Some(pick)) => Some(SharedString::from(format!("{n} hunk{} \u{b7} {pick} picked", if n == 1 { "" } else { "s" }))),
                (true, None) => Some(SharedString::from(format!("{n} hunk{}", if n == 1 { "" } else { "s" }))),
                (false, Some(pick)) => Some(SharedString::from(format!("{pick} picked"))),
                (false, None) => None,
            },
        };
        self.with_back(back, file_head("review-diff-head", &head, bar, tokens), tokens, cx)
    }

    /// The head, with the way back to the files in front of it while they and the diff take turns.
    fn with_back(&self, back: bool, head: Node, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        if !back {
            return head.into_any_element();
        }
        h_flex()
            .flex_shrink_0()
            .w_full()
            .min_w_0()
            .items_center()
            .border_b_1()
            .border_color(tokens.line)
            .child(div().flex_none().pl(rem(tokens.sp_8)).child(self.back(tokens, cx)))
            .child(head.flex_1().border_b_0().pl(rem(tokens.sp_8)))
            .into_any_element()
    }

    /// The way back to the files, while they and the diff take turns.
    fn back(&self, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        ghost("review-back", "Back to the files", tokens).tooltip(tip("Back to the files")).child(Glyph::Back.at(14.0)).on_click(cx.listener(|this, _, _, cx| {
            this.narrow.back();
            cx.notify();
        }))
    }

    /// The head over a file with no change, read whole: its path, that it is unchanged, and the
    /// way to read it in Code.
    pub(super) fn reading_head(&self, reading: &ReviewReading, back: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (dir, name) = split_path(&reading.path);
        let head = Head { status: None, name, dir, counts: None, meta: None };
        let unchanged = div()
            .id("review-unchanged")
            .test_support()
            .aria_label("Unchanged")
            .flex_shrink_0()
            .px(rem(tokens.sp_8))
            .rounded(rem(tokens.r_pill))
            .border_1()
            .border_color(tokens.line_strong)
            .typed(tokens.type_small)
            .text_color(tokens.fg_muted)
            .child("Unchanged");
        let open = tools::ghost("review-open", "Open in Code", None, false, tokens)
            .tooltip(tip("Read it in Code"))
            .on_click(cx.listener(|this, _, _, cx| this.ask(Call::ReviewAction { action: Action::OpenCode }, cx)));
        self.with_back(back, file_head("review-diff-head", &head, vec![unchanged.into_any_element(), open.into_any_element()], tokens), tokens, cx)
    }

    /// `.review-btn`: the review's drafts, counted, which opens the panel that sends them.
    fn review_button(&self, page: &ReviewPage, tokens: &Tokens, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let pending = page.drafts.as_ref()?.drafts.len();
        Some(
            h_flex()
                .id("review-open-submit")
                .test_support()
                .aria_label(SharedString::from(format!("Review, {pending} pending")))
                .flex_shrink_0()
                .gap(rem(7.0))
                .items_center()
                .h(rem(tokens.icon_btn))
                .pl(rem(10.0))
                .pr(rem(tokens.sp_8))
                .rounded(rem(tokens.r_sm))
                .border_1()
                .border_color(if pending > 0 { tokens.accent.opacity(0.6) } else { tokens.line_strong })
                .typed(tokens.type_small)
                .weighted(tokens.weight_medium)
                .text_color(tokens.fg)
                .cursor_pointer()
                .hover(|button| button.bg(tokens.fill))
                .child("Review")
                .child(
                    div()
                        .min_w(rem(18.0))
                        .px(rem(5.0))
                        .rounded(rem(tokens.r_pill))
                        .bg(if pending > 0 { tokens.accent } else { tokens.fill })
                        .text_color(if pending > 0 { tokens.fg_on_accent } else { tokens.fg_muted })
                        .text_size(rem(10.5))
                        .text_center()
                        .child(SharedString::from(pending.to_string())),
                )
                .on_click(cx.listener(|this, _, _, cx| this.submit.update(cx, |submit, cx| submit.toggle(cx)))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::connection::page::DiffRow;
    use argus_desktop_core::review::staging::StagingPage;

    #[test]
    fn the_head_counts_the_hunks_and_stages_the_file_toward_the_other_side() {
        let row = |kind: &'static str| DiffRow { row: 0, kind, text: String::new(), hidden: None, step: None, left: None, right: None, cursor: false, picked: false };
        let mut page = ReviewPage { rows: vec![row("hunk"), row("line"), row("hunk"), row("fold")], file: Some(0), ..ReviewPage::default() };
        assert_eq!(hunks(&page), 2);
        assert_eq!(stage_words(&page), None, "a review that cannot stage has no button");
        page.staging = Some(StagingPage { staged: false, busy: false, committing: false, commits: 0, can_commit: false });
        assert_eq!(stage_words(&page), Some(("Stage file", false)));
        page.staging = Some(StagingPage { staged: true, busy: false, committing: false, commits: 0, can_commit: true });
        assert_eq!(stage_words(&page), Some(("Unstage file", true)));
        page.file = None;
        assert_eq!(stage_words(&page), None, "no file on screen");
    }

    #[test]
    fn a_path_splits_into_its_folder_and_its_name() {
        assert_eq!(split_path("crates/a/src/mod.rs"), ("crates/a/src", "mod.rs"));
        assert_eq!(split_path("Cargo.toml"), ("", "Cargo.toml"));
    }
}
