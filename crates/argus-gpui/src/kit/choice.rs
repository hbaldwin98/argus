//! A row with a box or a ring to tick: a criterion, a reconcile line, an option.
//!
//! The page's `.check` rows are `--row-h` tall with the label inside the press, so the whole row is
//! the target and not the 13px box. Both are controlled, as the kit's are: a row draws the state it
//! is given and asks for the opposite when pressed, and the surface sends that to Rust and draws
//! what Rust says next. Nothing here toggles on its own (CROSS-27).

use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::radio::Radio;
use gpui_kit::component::{h_flex, Disableable as _};
use gpui_kit::{prelude::*, px, App, ElementId, SharedString, Window};

use crate::theme::Tokens;

/// The row around a control, `--row-h` tall at least.
fn row(tokens: &Tokens) -> gpui_kit::Div {
    h_flex().min_h(px(tokens.row_h)).items_center().gap(px(tokens.sp_8))
}

/// `.check`: a box and its label. `off` is a row that cannot be ticked now. `asked` is told the
/// value the press asks for, which is not drawn until the caller draws it.
pub fn check_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    off: bool,
    tokens: &Tokens,
    asked: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = label.into();
    row(tokens).child(
        Checkbox::new(id)
            .label(label.clone())
            .accessibility_label(label)
            .checked(checked)
            .disabled(off)
            .on_change(move |value, window, cx| asked(*value, window, cx)),
    )
}

/// A ring and its label: one option of a few. `asked` is told when it is pressed; a ring already
/// chosen asks for nothing.
pub fn radio_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    chosen: bool,
    off: bool,
    tokens: &Tokens,
    asked: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = label.into();
    row(tokens).child(
        Radio::new(id)
            .label(label.clone())
            .accessibility_label(label)
            .checked(chosen)
            .disabled(off)
            .on_change(move |value, window, cx| {
                if *value {
                    asked(window, cx);
                }
            }),
    )
}

#[cfg(test)]
mod tests;
