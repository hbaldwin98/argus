//! The head over the diff, as the prototype's `.view-head` draws it: the way back to the files
//! while they take turns with the diff, the file's path with its folders faint, what changed in it
//! (its status, its counts, how many hunks), and its tools — the layout, the whole file, the
//! editor, the review's drafts, what is picked, and the stage of the whole file. Too narrow for
//! the tools, the lesser ones fold into a menu.
//!
//! Every press sends the call the review's keys send; nothing on screen changes until the session
//! says so, but the turn back to the files, which is the window's own.

use argus_desktop_core::connection::page::{Call, ReviewPage};
use argus_desktop_core::keys::keymap::Action;
use argus_desktop_core::review::staging::StageGesture;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{h_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, Role, SharedString};

use super::lists::{badge, ghost, status};
use super::Changes;
use crate::kit::icons::Glyph;
use crate::kit::menu::item::MenuItem;
use crate::kit::segment;
use crate::kit::strip;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// Below this many CSS pixels across the diff, its lesser tools are a menu.
pub const TOOLS_FOLD: f32 = 620.0;

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

impl Changes {
    /// `.view-head`. `back` is whether the files and the diff take turns, and `fold` whether the
    /// lesser tools are a menu.
    pub(super) fn head(&self, page: &ReviewPage, back: bool, fold: bool, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let file = page.file.and_then(|i| page.files.get(i));
        let mono = cx.theme().mono_font_family.clone();
        let layout = |id: &'static str, label: &'static str, on: bool, split: bool, cx: &mut Context<Self>| {
            segment::pill(id, Role::RadioButton, on, tokens).child(label).on_click(cx.listener(move |this, _, _, cx| {
                // The one already on sends nothing.
                if super::page_split(&this.heard, cx) != split {
                    this.ask(Call::ReviewAction { action: Action::Split }, cx);
                }
            }))
        };
        let tool = |id: &'static str, words: &'static str, action: Action, cx: &mut Context<Self>| {
            ghost(id, words, tokens).tooltip(tip(words)).on_click(cx.listener(move |this, _, _, cx| this.ask(Call::ReviewAction { action }, cx)))
        };
        let (caller, heard) = (self.caller.clone(), self.heard.clone());
        let items = folded(page.split);
        h_flex()
            .id("review-diff-head")
            .flex_shrink_0()
            .flex_wrap()
            .min_h(rem(tokens.sp_44))
            .py(rem(tokens.sp_8))
            .pl(rem(tokens.sp_12))
            .pr(rem(10.0))
            .gap(rem(tokens.sp_8))
            .items_center()
            .bg(tokens.bg_surface)
            .border_b_1()
            .border_color(tokens.line)
            .when(back, |head| {
                head.child(
                    ghost("review-back", "Back to the files", tokens).tooltip(tip("Back to the files")).child(Glyph::Back.at(14.0)).on_click(cx.listener(|this, _, _, cx| {
                        this.narrow.back();
                        cx.notify();
                    })),
                )
            })
            // `.path`: the folders faint, cut short from the end when there is no room, and the
            // file's own name bright, never cut.
            .when_some(file, |head, file| {
                let dir = file.dir.trim_end_matches('/');
                head.child(
                    h_flex()
                        .id("review-path")
                        .test_support()
                        .aria_label(SharedString::from(file.path.clone()))
                        .flex_1()
                        .min_w(rem(120.0))
                        .gap(rem(tokens.sp_4))
                        .items_center()
                        .overflow_hidden()
                        .typed(tokens.type_heading)
                        .tooltip(tip(SharedString::from(file.path.clone())))
                        .when(!dir.is_empty(), |path| {
                            path.child(
                                div()
                                    .min_w_0()
                                    .flex_shrink(1.0)
                                    .truncate()
                                    .text_color(tokens.fg_faint)
                                    .weighted(tokens.weight_regular)
                                    .child(SharedString::from(dir.replace('/', " / "))),
                            )
                            .child(div().flex_shrink_0().text_color(tokens.fg_fainter).child("/"))
                        })
                        .child(div().flex_shrink_0().text_color(tokens.fg).child(SharedString::from(file.name.clone()))),
                )
            })
            // `.meta`: the status, the counts, and how many hunks.
            .when_some(file, |head, file| {
                let (letter, colour, said) = status(file.kind, tokens);
                let n = hunks(page);
                head.child(
                    h_flex()
                        .flex_shrink_0()
                        .gap(rem(tokens.sp_8))
                        .items_center()
                        .font_family(mono.clone())
                        .text_size(rem(11.5))
                        .child(badge("review-file-on-status", letter, colour, said, tokens, cx))
                        .child(div().text_color(tokens.ok).child(SharedString::from(format!("+{}", file.added))))
                        .child(div().text_color(tokens.danger).child(SharedString::from(format!("\u{2212}{}", file.removed))))
                        .when(!fold && n > 0, |meta| {
                            meta.child(div().text_color(tokens.fg_faint).child(SharedString::from(format!("{n} hunk{}", if n == 1 { "" } else { "s" }))))
                        }),
                )
            })
            .child(
                h_flex()
                    .id("review-tools")
                    .flex_shrink_0()
                    .ml_auto()
                    .gap(rem(tokens.sp_6))
                    .items_center()
                    .when(!fold, |tools| {
                        tools
                            .child(
                                segment::track("review-view", "Layout", Role::RadioGroup, tokens)
                                    .child(layout("review-view-unified", "Unified", !page.split, false, cx))
                                    .child(layout("review-view-split", "Split", page.split, true, cx)),
                            )
                            .child(tool("review-whole", "Whole file", Action::UnfoldAll, cx).child("Whole file"))
                            .child(tool("review-open", "Read it in Code, at this line", Action::OpenCode, cx).child(Glyph::Code.at(14.0)).child("Open"))
                    })
                    .when(fold, |tools| {
                        let tokens = *tokens;
                        tools.child(
                            div().flex_shrink_0().child(
                                strip::menu_button("review-more", Glyph::More, 14.0, &tokens, cx)
                                    .tooltip("More")
                                    .dropdown_menu(move |menu, window, cx| crate::layout::dock_menu::build(menu, &items, &caller, &heard, tokens, window, cx)),
                            ),
                        )
                    })
                    .children(self.review_button(page, tokens, cx))
                    // `.ktag`: what is picked, as a pill.
                    .when_some(page.pick.clone(), |tools, pick| {
                        tools.child(
                            div()
                                .id("review-pick")
                                .test_support()
                                .aria_label(SharedString::from(format!("{pick} picked")))
                                .flex_shrink_0()
                                .px(rem(tokens.sp_8))
                                .rounded(rem(tokens.r_pill))
                                .border_1()
                                .border_color(tokens.line_strong)
                                .typed(tokens.type_small)
                                .text_color(tokens.fg_muted)
                                .whitespace_nowrap()
                                .child(SharedString::from(format!("{pick} picked"))),
                        )
                    })
                    .when_some(stage_words(page).zip(file), |tools, ((words, staged), file)| {
                        let path = file.path.clone();
                        tools.child(
                            crate::kit::button::button("review-stage-file", words, crate::kit::button::Look::Plain, true, tokens)
                                .child(words)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.ask(Call::Stage { gesture: StageGesture::File { staged, path: path.clone() } }, cx);
                                })),
                        )
                    }),
            )
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
                .h(rem(tokens.control_h_sm))
                .pl(rem(10.0))
                .pr(rem(tokens.sp_8))
                .rounded(rem(tokens.r_sm))
                .border_1()
                .border_color(if pending > 0 { tokens.accent.opacity(0.6) } else { tokens.line_strong })
                .bg(tokens.bg_surface)
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
}
