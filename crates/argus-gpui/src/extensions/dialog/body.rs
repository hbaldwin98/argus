//! What the card of an extension's question draws: the title with who asked under it, the message and
//! the control of the kind (the field, the list of choices, a labelled field for each of a form's),
//! and the foot with Cancel and the button that answers (EXTDIALOG-2, -3, -6).

use argus_desktop_core::extensions::ask::{asker, is_danger, message, yes_words};
use argus_protocol::ExtDialogKind;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Context, Role, SharedString, Window};

use super::card::AskView;
use crate::kit::button::{button, Look};
use crate::kit::dialog::{esc, field_label, foot};
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl AskView {
    /// The card's width, and how tall it is estimated to be, in the page's pixels: the page centres its
    /// dialogs, and the kit's sits near the top unless told how far down to go. A form is the wider
    /// card, as the page's `:has(.label)` makes it.
    pub(super) fn size(&self, tokens: &Tokens) -> (f32, f32) {
        let line = tokens.type_body.size * 1.4;
        let head = tokens.sp_24 + tokens.sp_32.max(line + tokens.fs_sm * 1.4) + tokens.sp_16;
        let foot = 2.0 * tokens.sp_16 + tokens.control_h + tokens.hairline;
        let (wide, inside) = match &self.dialog.kind {
            ExtDialogKind::Confirm { .. } => (false, line),
            ExtDialogKind::Prompt { .. } => (false, line + tokens.sp_12 + tokens.control_h),
            ExtDialogKind::Pick { items, .. } => (false, line + tokens.sp_12 + (items.len() as f32 * tokens.row_h).min(tokens.ext_pick_max_h)),
            ExtDialogKind::Form { fields } => (true, fields.len() as f32 * (line + tokens.control_h + tokens.sp_12)),
        };
        (if wide { tokens.dialog_w } else { tokens.dialog_w_sm }, head + inside + tokens.sp_24 + foot)
    }

    fn head(&self, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let waiting = self.waiting(cx);
        let title = SharedString::from(self.dialog.title.clone());
        let who = SharedString::from(asker(&self.extension, waiting));
        h_flex()
            .flex_shrink_0()
            .items_start()
            .gap(rem(tokens.sp_12))
            .px(rem(tokens.sp_24))
            .pt(rem(tokens.sp_24))
            .pb(rem(tokens.sp_16))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().id("ext-dialog-title").test_support().aria_label(title.clone()).typed(tokens.type_title).text_color(tokens.fg).child(title))
                    // Who asked sits under the title, where the question is read, not in the footer.
                    .child(div().id("ext-dialog-asker").test_support().aria_label(who.clone()).typed(tokens.type_small).text_size(rem(tokens.fs_sm)).text_color(tokens.fg_muted).child(who)),
            )
            .child(
                icon_button("ext-dialog-close", "Close", false, tokens)
                    .on_click(cx.listener(|this, _, window, cx| this.cancel(window, cx)))
                    .child(Glyph::Close.at(tokens.icon_sm)),
            )
    }

    /// The message over the control the kind has.
    fn inside(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let said = message(&self.dialog).map(|words| {
            let words = SharedString::from(words.to_owned());
            div().id("ext-dialog-message").test_support().aria_label(words.clone()).text_color(tokens.fg_muted).child(words)
        });
        let control: Option<AnyElement> = match &self.dialog.kind {
            ExtDialogKind::Confirm { .. } => None,
            ExtDialogKind::Prompt { message, .. } => self.fields.first().map(|field| {
                div().id("ext-dialog-field").test_support().aria_label(SharedString::from(message.clone())).child(Input::new(field.input()).aria_label(SharedString::from(message.clone())).h(rem(tokens.control_h))).into_any_element()
            }),
            ExtDialogKind::Pick { items, .. } => Some(self.pick(items, tokens, cx)),
            ExtDialogKind::Form { fields } => Some(
                v_flex()
                    .gap(rem(tokens.sp_12))
                    .children(fields.iter().zip(&self.fields).map(|(spec, field)| {
                        v_flex()
                            .gap(rem(tokens.sp_6))
                            .child(field_label(SharedString::from(spec.label.clone()), tokens))
                            .child(
                                div()
                                    .id(SharedString::from(format!("ext-dialog-field-{}", spec.name)))
                                    .test_support()
                                    .aria_label(SharedString::from(spec.label.clone()))
                                    .child(Input::new(field.input()).aria_label(SharedString::from(spec.label.clone())).h(rem(tokens.control_h))),
                            )
                    }))
                    .into_any_element(),
            ),
        };
        v_flex()
            .gap(rem(tokens.sp_12))
            .px(rem(tokens.sp_24))
            .pb(rem(tokens.sp_24))
            .typed(tokens.type_body)
            .children(said)
            .children(control)
            .into_any_element()
    }

    /// The choices: a row each, the lit one marked, a press answering at once.
    fn pick(&self, items: &[argus_protocol::ExtItem], tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let fill = tokens.fill;
        v_flex()
            .id("ext-dialog-pick")
            .role(Role::ListBox)
            .test_support()
            .max_h(rem(tokens.ext_pick_max_h))
            .overflow_y_scroll()
            .p(rem(tokens.sp_4))
            .rounded(rem(tokens.r_md))
            .border_1()
            .border_color(tokens.line)
            .children(items.iter().enumerate().map(|(at, item)| {
                let on = at == self.lit;
                div()
                    .id(("ext-dialog-choice", at))
                    .role(Role::ListBoxOption)
                    .test_support()
                    .aria_label(SharedString::from(item.label.clone()))
                    .aria_selected(on)
                    .track_focus(&self.rows[at])
                    .h(rem(tokens.row_h))
                    .px(rem(tokens.sp_8))
                    .flex()
                    .items_center()
                    .rounded(rem(tokens.r_sm))
                    .text_color(tokens.fg)
                    .cursor_pointer()
                    .when(on, |row| row.bg(fill))
                    .hover(move |style| style.bg(fill))
                    .on_click(cx.listener(move |this, _, window, cx| this.choose(at, window, cx)))
                    .child(SharedString::from(item.label.clone()))
            }))
            .into_any_element()
    }

    fn foot(&self, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let words = SharedString::from(yes_words(&self.dialog));
        let look = if is_danger(&self.dialog) { Look::Danger } else { Look::Primary };
        foot(tokens)
            .justify_end()
            .child(
                button("ext-dialog-cancel", "Cancel", Look::Plain, false, tokens)
                    .track_focus(&self.cancel)
                    .child("Cancel")
                    .child(esc(tokens))
                    .on_click(cx.listener(|this, _, window, cx| this.cancel(window, cx))),
            )
            .child(
                button("ext-dialog-yes", words.clone(), look, false, tokens)
                    .track_focus(&self.yes)
                    .child(words)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            )
    }
}

impl Render for AskView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        v_flex()
            .id("ext-dialog")
            .role(Role::Dialog)
            .test_support()
            .aria_label(SharedString::from(format!("Question from {}", self.extension)))
            .key_context("ExtensionDialog")
            .track_focus(&self.card)
            .child(self.head(&tokens, cx))
            .child(self.inside(&tokens, cx))
            .child(self.foot(&tokens, cx))
    }
}
