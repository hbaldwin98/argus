//! A form block: fields the person fills in, then the submit action's button (FORM-1 to FORM-4).
//!
//! What is typed is kept in `views/memory` by the form's path and the field's name, so an agent that
//! shows the view again does not put the defaults back under the person's fingers; core says what a
//! field shows and what a press sends (`views::blocks::form`). The boxes a field is typed in are
//! kept (`kept`) and named by how many times the form has been sent, so a sent form starts clean as
//! the page's redrawn one does. Enter does not submit, and nothing here asks for a call but the
//! submit button, through the action button's own rules.

use std::rc::Rc;

use argus_desktop_core::views::blocks::flow::{columns, Fill};
use argus_desktop_core::views::blocks::form::{answers, inline, shown_flag, shown_text, wide, Typed};
use argus_protocol::{FormField, FormInput, ViewAction};
use gpui_kit::component::date_picker::DatePicker;
use gpui_kit::component::input::{Input, NumberInput, Textarea};
use gpui_kit::component::select::Select;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString};

use super::pen::Pen;
use crate::kit::choice::check_row;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::actions::{action_button, PressWith};
use crate::views::ctx::ViewCtx;
use crate::views::memory::Shared;

/// `.vform`: the fields in a grid of columns at least `--form-col-min` wide, a long text across all
/// of them, and the submit button at the right of a foot.
pub fn form(id: &str, fields: &[FormField], submit: &ViewAction, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}-form", ctx.path);
    let prefix = format!("{}|", ctx.path);
    let sent = pen.memory.borrow().sent(&prefix);
    let measured = format!("{}|form", ctx.path);
    let n = columns(pen.width(&measured), tokens.form_col_min, tokens.sp_12, fields.len(), Fill::Fit);
    let cells: Vec<AnyElement> = fields.iter().enumerate().map(|(k, field)| cell(k, field, &key, &prefix, sent, pen)).collect();

    // A press reads what the fields say at the moment of it, and the sent form's drafts go.
    let with = {
        let (memory, fields, prefix, form) = (pen.memory.clone(), fields.to_vec(), prefix.clone(), id.to_owned());
        Rc::new(move || {
            let said = answers(&fields, |name| memory.borrow().draft(&format!("{prefix}{name}")).cloned());
            memory.borrow_mut().clear_drafts(&prefix);
            PressWith { rows: Vec::new(), form: Some(form.clone()), answers: said }
        })
    };
    v_flex()
        .id(SharedString::from(key.clone()))
        .role(Role::Form)
        .test_support()
        .relative()
        .min_w_0()
        .gap(rem(tokens.sp_12))
        .child(pen.watch(measured))
        .child(div().grid().grid_cols(n as u16).gap(rem(tokens.sp_12)).children(cells))
        .child(h_flex().justify_end().child(action_button(submit, ctx, with, false, pen)))
        .into_any_element()
}

/// One field: its label over its control, or for a toggle the box then the label on one line.
fn cell(k: usize, field: &FormField, key: &str, prefix: &str, sent: u32, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let (id, draft) = (format!("{key}-field{k}"), format!("{prefix}{}", field.name));
    let held = pen.memory.borrow().draft(&draft).cloned();
    let control = control(field, &id, &draft, held.as_ref(), sent, pen);
    v_flex()
        .id(SharedString::from(id))
        .test_support()
        .min_w_0()
        .gap(rem(tokens.sp_4))
        .when(wide(field), |cell| cell.col_span_full())
        .when(!inline(field), |cell| {
            cell.child(div().typed(tokens.type_label).text_color(tokens.fg_muted).child(SharedString::from(field.label.clone())))
        })
        .child(control)
        .into_any_element()
}

fn control(field: &FormField, id: &str, draft: &str, held: Option<&Typed>, sent: u32, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let (memory, repaint, label) = (pen.memory.clone(), pen.repaint, SharedString::from(field.label.clone()));
    let remember = move |memory: Shared, draft: String| move |text: &str, _: &mut gpui_kit::App| memory.borrow_mut().set_draft(&draft, Typed::Text(text.to_owned()));
    let text = shown_text(&field.input, held);
    let kept = format!("{id}#{sent}");
    match &field.input {
        FormInput::Text { placeholder, multiline: true, .. } => {
            let area = pen.area(&kept, placeholder.as_deref().unwrap_or_default(), &text, remember(memory, draft.to_owned()));
            boxed(id, Textarea::new(&area).w_full().aria_label(label))
        }
        FormInput::Text { placeholder, .. } => {
            let input = pen.field(&kept, placeholder.as_deref().unwrap_or_default(), &text, remember(memory, draft.to_owned()));
            boxed(id, Input::new(&input).aria_label(label).h(rem(tokens.control_h)))
        }
        FormInput::Date { .. } => {
            let picker = pen.date(&kept, &text, remember(memory, draft.to_owned()));
            boxed(id, DatePicker::new(&picker).placeholder("YYYY-MM-DD").cleanable(true).w_full().h(rem(tokens.control_h)))
        }
        FormInput::Number { min, max, .. } => {
            let input = pen.number(&kept, &text, (*min, *max), remember(memory, draft.to_owned()));
            boxed(id, NumberInput::new(&input).accessibility_label(label).w_full().h(rem(tokens.control_h)))
        }
        FormInput::Choice { options, .. } => {
            let select = pen.choice(&kept, options, &text, remember(memory, draft.to_owned()));
            boxed(id, Select::new(&select).id(SharedString::from(format!("{id}-select"))).accessibility_label(label).w_full().h(rem(tokens.control_h)))
        }
        FormInput::Toggle { .. } => {
            let (memory, draft) = (memory.clone(), draft.to_owned());
            check_row(SharedString::from(format!("{id}-toggle")), label, shown_flag(&field.input, held), false, tokens, move |on, _, cx| {
                memory.borrow_mut().set_draft(&draft, Typed::Flag(on));
                cx.notify(repaint);
            })
            .into_any_element()
        }
    }
}

/// A box a field is typed in, named so it can be found and pressed.
fn boxed(id: &str, field: impl IntoElement) -> AnyElement {
    div().id(SharedString::from(format!("{id}-box"))).test_support().child(field).into_any_element()
}
