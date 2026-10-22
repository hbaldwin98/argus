//! A menu drawn once: the model's headings, rules and choices as the kit's popup menu.
//!
//! The kit's menu supplies what is not ours to write: it sits under a button or at a pointer kept
//! 8px inside the window (SHELL-MENU-2), moves its highlight with Up and Down and wraps
//! (SHELL-MENU-4, with `j` and `k` bound by `keys` and the first and the wrap kept off a heading
//! by `stepping`), closes on Escape or a press outside, and
//! gives the keys back to what had them. A choice is drawn here, since the kit's has no danger
//! colour, no reason and no hint: its label, then its reason if it has one, else its hint.
//! Choosing runs `act` with what the choice does after the menu has gone (SHELL-MENU-3).

use std::rc::Rc;

use gpui_kit::component::h_flex;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{div, prelude::*, px, App, SharedString, TestSupportExt as _, Window};

use super::item::{Choice, Hint, MenuItem};
use super::stepping;
use crate::kit::keycap::keycaps;
use crate::theme::Tokens;

/// The row of a choice: its label, and at the right edge why it cannot be chosen, or its hint.
fn row<A>(ix: usize, choice: &Choice<A>, held: bool, tokens: &Tokens) -> impl IntoElement {
    let off = held || choice.reason.is_some();
    let colour = if off {
        tokens.fg_disabled
    } else if choice.danger {
        tokens.danger
    } else {
        tokens.fg
    };
    let side = match (&choice.reason, &choice.hint) {
        (Some(reason), _) => Some(div().text_size(px(tokens.fs_xs)).text_color(tokens.fg_faint).child(SharedString::from(reason.clone()))),
        (None, Hint::Text(words)) => Some(div().text_size(px(tokens.fs_xs)).text_color(tokens.fg_faint).child(SharedString::from(words.clone()))),
        (None, Hint::Keys(keys)) => Some(keycaps(keys, tokens)),
        (None, Hint::None) => None,
    };
    h_flex()
        .id(("menu-choice", ix))
        .test_support()
        .aria_label(SharedString::from(choice.label.clone()))
        .on_hover(|hovered, _, _| {
            if *hovered {
                stepping::pointed();
            }
        })
        .w_full()
        .gap(px(tokens.sp_16))
        .items_center()
        .justify_between()
        .text_color(colour)
        .child(
            h_flex()
                .min_w_0()
                .gap(px(tokens.sp_8))
                .items_center()
                // The check column keeps its room when it is not ticked, so the labels line up.
                .when_some(choice.checked, |row, on| {
                    row.child(div().id(("menu-check", ix)).test_support().aria_label(if on { "checked" } else { "" }).w(px(tokens.icon_sm)).flex_shrink_0().text_color(tokens.accent).child(if on { "\u{2713}" } else { "" }))
                })
                .child(div().min_w_0().truncate().child(SharedString::from(choice.label.clone()))),
        )
        .children(side)
}

/// `menu` with `items` in it. A choice that has a reason is disabled, and so is every choice while
/// `held` says the daemon is gone; `held` is asked again when one is picked, since a menu opened
/// before the link dropped must send nothing after it did.
pub fn fill<A: Clone + 'static>(
    mut menu: PopupMenu,
    items: &[MenuItem<A>],
    tokens: Tokens,
    held: impl Fn(&App) -> bool + Clone + 'static,
    act: impl Fn(&A, &mut Window, &mut App) + Clone + 'static,
    cx: &App,
) -> PopupMenu {
    let now = held(cx);
    let enabled = (0..items.len()).filter(|&ix| matches!(&items[ix], MenuItem::Choice(choice) if !now && choice.reason.is_none())).collect();
    stepping::opened(enabled);
    let act = Rc::new(act);
    for (ix, item) in items.iter().enumerate() {
        menu = match item {
            MenuItem::Heading(text) => menu.label(text.clone()),
            MenuItem::Rule => menu.separator(),
            MenuItem::Choice(choice) => {
                let shown = choice.clone();
                let (held, act, does) = (held.clone(), act.clone(), choice.does.clone());
                let drawn = PopupMenuItem::element(move |_, _| row(ix, &shown, now, &tokens))
                    .disabled(now || choice.reason.is_some())
                    .on_click(move |_, window, cx| {
                        if held(cx) {
                            return;
                        }
                        // After the menu has gone and given the keys back, so what a choice opens
                        // (a dialog) is not given them back to what the menu interrupted.
                        let (held, act, does) = (held.clone(), act.clone(), does.clone());
                        window.defer(cx, move |window, cx| {
                            if !held(cx) {
                                act(&does, window, cx);
                            }
                        });
                    });
                menu.item(drawn)
            }
        };
    }
    menu
}

#[cfg(test)]
mod tests;
