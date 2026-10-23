//! A draft of the review, under the line it was written on: who wrote it and where, its words, and
//! the edit and the delete it offers.
//!
//! The words are the session's (`DraftView`); an edit is written in the card's own box and sent,
//! and the card shows the new words once the daemon's drafts carry them, which is also when the
//! box shuts. Nothing here changes the draft on screen ahead of that answer.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::review::drafts::{DraftGesture, DraftView};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Context, Entity, Pixels, SharedString, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::button::{button, Look};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The card's head, a line of its words, and the room about it, in the page's pixels.
const HEAD: f32 = 30.0;
const LINE: f32 = 19.0;
const ROOM: f32 = 34.0;
/// The edit box and its buttons, while the words are being changed.
const EDITING: f32 = 128.0;
/// About how many characters of the body face fit a line of a card at its widest, to guess how
/// many lines its words wrap to.
const PER_LINE: usize = 96;

/// How tall the card for `draft` is, at `zoom`, editing or not.
pub fn height(draft: &DraftView, editing: bool, zoom: f32) -> Pixels {
    let lines: usize = draft.body.lines().map(|line| 1 + line.chars().count() / PER_LINE).sum::<usize>().max(1);
    let body = if editing { EDITING } else { lines as f32 * LINE };
    px((HEAD + body + ROOM) * zoom)
}

pub struct DraftCard {
    heard: Entity<Heard>,
    caller: Caller,
    draft: DraftView,
    /// The edit box, while the words are being changed.
    editing: Option<Entity<TextareaState>>,
    /// How far in the card stands, so its words begin where the rows' code does.
    left: f32,
}

impl DraftCard {
    pub fn new(heard: Entity<Heard>, caller: Caller, draft: DraftView) -> Self {
        Self { heard, caller, draft, editing: None, left: 0.0 }
    }

    pub fn editing(&self) -> bool {
        self.editing.is_some()
    }

    /// The draft as the daemon now has it: new words shut an open edit, as they are its answer.
    pub fn follow(&mut self, draft: &DraftView, left: f32, cx: &mut Context<Self>) {
        if draft.body != self.draft.body {
            self.editing = None;
        }
        if *draft != self.draft || left != self.left {
            self.draft = draft.clone();
            self.left = left;
            cx.notify();
        }
    }

    fn ask(&self, gesture: DraftGesture, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(Call::Drafts { gesture });
        }
    }

    fn edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = self.draft.body.clone();
        let text = cx.new(|cx| TextareaState::new(window, cx).rows(3));
        text.update(cx, |text, cx| {
            text.set_value(body, window, cx);
            text.focus(window, cx);
        });
        self.editing = Some(text);
        cx.notify();
    }
}

impl Render for DraftCard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let id = self.draft.id;
        let head = h_flex()
            .gap(rem(tokens.sp_8))
            .items_center()
            .px(rem(tokens.sp_12))
            .h(rem(HEAD))
            .border_b_1()
            .border_color(tokens.line)
            .typed(tokens.type_small)
            .text_color(tokens.fg_faint)
            .child(div().text_color(tokens.fg).weighted(tokens.weight_semibold).child("You"))
            .child(
                div()
                    .px(rem(tokens.sp_6))
                    .rounded(rem(tokens.r_pill))
                    .bg(tokens.accent.opacity(0.14))
                    .text_color(tokens.accent)
                    .text_size(rem(tokens.fs_xs))
                    .child("Pending"),
            )
            .child(div().font_family(SharedString::from(tokens.font_mono)).text_size(rem(tokens.fs_xs)).child(SharedString::from(self.draft.at.clone())))
            .child(div().flex_1())
            .when(self.editing.is_none(), |head| {
                head.child(
                    button(("draft-edit", id as usize), "Edit", Look::Plain, true, &tokens)
                        .child("Edit")
                        .on_click(cx.listener(|this, _, window, cx| this.edit(window, cx))),
                )
                .child(
                    button(("draft-delete", id as usize), "Delete", Look::Plain, true, &tokens)
                        .child("Delete")
                        .on_click(cx.listener(move |this, _, _, cx| this.ask(DraftGesture::Delete { id }, cx))),
                )
            });
        let body = match &self.editing {
            Some(text) => {
                let text = text.clone();
                v_flex()
                    .gap(rem(tokens.sp_8))
                    .p(rem(tokens.sp_8))
                    .child(Textarea::new(&text).w_full().aria_label("Edit the comment"))
                    .child(
                        h_flex()
                            .gap(rem(tokens.sp_6))
                            .justify_end()
                            .child(
                                button(("draft-cancel", id as usize), "Cancel", Look::Plain, true, &tokens)
                                    .child("Cancel")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.editing = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                button(("draft-save", id as usize), "Save", Look::Primary, true, &tokens)
                                    .child("Save")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let Some(body) = this.editing.as_ref().map(|t| t.read(cx).value().to_string()) else { return };
                                        this.ask(DraftGesture::Edit { id, body }, cx);
                                    })),
                            ),
                    )
                    .into_any_element()
            }
            None => div()
                .px(rem(tokens.sp_12))
                .py(rem(tokens.sp_6))
                .typed(tokens.type_body)
                .text_color(tokens.fg)
                .child(SharedString::from(self.draft.body.clone()))
                .into_any_element(),
        };
        div()
            .id(("draft-card", id as usize))
            .test_support()
            .aria_label(SharedString::from(format!("Pending comment on {}", self.draft.at)))
            .size_full()
            .pl(rem(self.left))
            .pr(rem(tokens.sp_12))
            .py(rem(tokens.sp_6))
            .bg(tokens.bg_chrome)
            .font_family(SharedString::from(tokens.font_ui))
            .child(
                v_flex()
                    .max_w(rem(720.0))
                    .rounded(rem(tokens.r_md))
                    .border_1()
                    .border_color(tokens.line_strong)
                    .bg(tokens.bg_surface)
                    .overflow_hidden()
                    .child(head)
                    .child(body),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(body: &str) -> DraftView {
        DraftView { id: 1, path: "a.rs".into(), at: "a.rs:3".into(), body: body.into(), staged: false, row: Some(3) }
    }

    #[test]
    fn a_card_grows_with_its_lines_and_with_the_edit_box() {
        let one = height(&draft("why?"), false, 1.0);
        let three = height(&draft("one\ntwo\nthree"), false, 1.0);
        assert!(three > one);
        assert_eq!(f32::from(three - one), 2.0 * LINE);
        assert!(height(&draft("why?"), true, 1.0) > three);
        assert_eq!(height(&draft("why?"), false, 2.0), one * 2.0, "measured at the zoom");
    }
}
