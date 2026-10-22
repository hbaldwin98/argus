//! What a question an extension puts to this window says and how it is answered: the words of its
//! footer, the answer each kind builds from what was typed, the lit choice of a pick and what a key
//! does in the card.
//!
//! The page held these in `extensions/dialogs.ts`. Which question is open and which waits behind it
//! is `dialog`'s; this is only what the card in front of the person is and means, the same in
//! every window that draws it.

use std::collections::BTreeMap;

use argus_protocol::{ExtAnswer, ExtDialog, ExtDialogKind, ExtField, ExtItem};

use crate::keys::keymap::Action;

/// The line under the title: who asked, and how many more wait behind this question.
pub fn asker(extension: &str, waiting: usize) -> String {
    format!("Asked by the {extension} extension{}", waiting_words(waiting))
}

/// What a count of waiting questions adds to the asker's line.
pub fn waiting_words(waiting: usize) -> String {
    if waiting > 0 {
        format!(" \u{b7} {waiting} more waiting")
    } else {
        String::new()
    }
}

/// What the button that answers says: a confirm's own word (default "Yes"), "Choose", or "OK".
pub fn yes_words(dialog: &ExtDialog) -> String {
    match &dialog.kind {
        ExtDialogKind::Confirm { yes, .. } => yes.clone().unwrap_or_else(|| "Yes".into()),
        ExtDialogKind::Pick { .. } => "Choose".into(),
        ExtDialogKind::Prompt { .. } | ExtDialogKind::Form { .. } => "OK".into(),
    }
}

/// Whether the answering button is drawn as a danger button: only a confirm says so.
pub fn is_danger(dialog: &ExtDialog) -> bool {
    matches!(dialog.kind, ExtDialogKind::Confirm { danger: true, .. })
}

/// What the card says above its controls; a form has none, its fields are labelled.
pub fn message(dialog: &ExtDialog) -> Option<&str> {
    match &dialog.kind {
        ExtDialogKind::Confirm { message, .. } | ExtDialogKind::Prompt { message, .. } | ExtDialogKind::Pick { message, .. } => Some(message),
        ExtDialogKind::Form { .. } => None,
    }
}

/// The answer of a form: each field's name and what it holds, in the order the fields are.
pub fn form_answer(fields: &[ExtField], values: &[String]) -> ExtAnswer {
    let given: BTreeMap<String, String> = fields.iter().zip(values).map(|(field, value)| (field.name.clone(), value.clone())).collect();
    ExtAnswer::Form(given)
}

/// Where the lit choice of a pick goes `by` places, held at both ends.
pub fn step(at: usize, len: usize, by: isize) -> usize {
    at.saturating_add_signed(by).min(len.saturating_sub(1))
}

/// The answer that picks the lit choice, when there is one.
pub fn picked(items: &[ExtItem], lit: usize) -> Option<ExtAnswer> {
    items.get(lit).map(|item| ExtAnswer::Picked(item.key.clone()))
}

/// What a key does while the card has the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// Put the question away unanswered.
    Cancel,
    /// Answer it as the card now stands.
    Submit,
    /// Move the lit choice of a pick.
    Move(isize),
    /// Not the card's: the key goes on to the control that has it.
    Pass,
}

/// What `action`, the key's action in the writing and palette places, does. Enter on a button is
/// that button's click, Cancel's included, so it is the button's and not a submit.
pub fn pressed(action: Option<Action>, on_button: bool, pick: bool) -> Pressed {
    match action {
        Some(Action::CloseLayer) => Pressed::Cancel,
        Some(Action::Choose) if on_button => Pressed::Pass,
        Some(Action::Choose | Action::Save) => Pressed::Submit,
        Some(Action::Next) if pick => Pressed::Move(1),
        Some(Action::Previous) if pick => Pressed::Move(-1),
        _ => Pressed::Pass,
    }
}

#[cfg(test)]
mod tests;
