//! A menu drawn once: the model's headings, rules and choices as the kit's popup menu.
//!
//! The kit's menu supplies what is not ours to write: it sits under a button or at a pointer kept
//! 8px inside the window (SHELL-MENU-2), moves its highlight with Up and Down and wraps
//! (SHELL-MENU-4, with `j` and `k` bound by `keys` and the first and the wrap kept off a heading
//! by `stepping`), closes on Escape or a press outside, and
//! gives the keys back to what had them. A choice is drawn here, since the kit's has no danger
//! colour, no reason and no hint: its label, then its reason if it has one, else its hint.
//! Choosing runs `act` with what the choice does after the menu has gone (SHELL-MENU-3).
//!
//! A fold is the kit's submenu, a flyout; its label is a plain string, so its note (the workspace or host in use) is not drawn on the row and the flyout's ticked row says it. `stepping` keeps its account of the top menu only, so a
//! menu with a fold leaves the stepping to the kit.

use std::rc::Rc;

use gpui_kit::component::h_flex;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{div, prelude::*, App, Context, SharedString, TestSupportExt as _, Window};

use super::item::{Choice, Hint, MenuItem, Tone};
use super::stepping;
use crate::kit::keycap::keycaps;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The words at a row's edge, small and quiet unless a tone says otherwise.
fn said(words: &str, colour: gpui_kit::Hsla, tokens: &Tokens) -> gpui_kit::Div {
    div().text_size(rem(tokens.fs_xs)).text_color(colour).child(SharedString::from(words.to_owned()))
}

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
        (Some(reason), _) => Some(said(reason, tokens.fg_faint, tokens)),
        (None, Hint::Text(words)) => Some(said(words, tokens.fg_faint, tokens)),
        (None, Hint::Toned(words, tone)) => {
            let colour = match tone {
                Tone::Loud => tokens.attention,
                Tone::Active => tokens.accent,
                Tone::Muted => tokens.fg_faint,
            };
            Some(said(words, colour, tokens).font_weight(tokens.scale.type_label.weight()))
        }
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
        .gap(rem(tokens.sp_16))
        .items_center()
        .justify_between()
        .text_color(colour)
        .child(
            h_flex()
                .min_w_0()
                .gap(rem(tokens.sp_8))
                .items_center()
                // The check column keeps its room when it is not ticked, so the labels line up.
                .when_some(choice.checked, |row, on| {
                    row.child(div().id(("menu-check", ix)).test_support().aria_label(if on { "checked" } else { "" }).w(rem(tokens.icon_sm)).flex_shrink_0().text_color(tokens.accent).child(if on { "\u{2713}" } else { "" }))
                })
                .when_some(choice.lead, |row, mark| {
                    row.child(div().w(rem(tokens.icon_sm)).flex_shrink_0().text_color(tokens.fg_faint).child(mark))
                })
                .child(div().min_w_0().truncate().child(SharedString::from(choice.label.clone()))),
        )
        .children(side)
}

/// What a choice does once the menu has gone.
type Act<A> = dyn Fn(&A, &mut Window, &mut App);

/// What a menu needs to draw its items: how a choice is held off and what it does.
struct Ways<A> {
    tokens: Tokens,
    held: Rc<dyn Fn(&App) -> bool>,
    act: Rc<Act<A>>,
}

/// `menu` with `items` in it. A choice that has a reason is disabled, and so is every choice while
/// `held` says the daemon is gone; `held` is asked again when one is picked, since a menu opened
/// before the link dropped must send nothing after it did.
pub fn fill<A: Clone + 'static>(
    menu: PopupMenu,
    items: &[MenuItem<A>],
    tokens: Tokens,
    held: impl Fn(&App) -> bool + 'static,
    act: impl Fn(&A, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let ways = Rc::new(Ways { tokens, held: Rc::new(held), act: Rc::new(act) });
    let now = (ways.held)(cx);
    let folded = items.iter().any(|item| matches!(item, MenuItem::Fold(_)));
    let enabled = (0..items.len()).filter(|&ix| matches!(&items[ix], MenuItem::Choice(choice) if !now && choice.reason.is_none())).collect();
    let menu = fill_items(menu, items, &ways, now, window, cx);
    // After the folds are built: a fold's own menu is built with it, and none of it is this one's.
    stepping::opened(if folded { Vec::new() } else { enabled });
    if folded {
        stepping::pointed();
    }
    menu
}

fn fill_items<A: Clone + 'static>(mut menu: PopupMenu, items: &[MenuItem<A>], ways: &Rc<Ways<A>>, now: bool, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let tokens = ways.tokens;
    for (ix, item) in items.iter().enumerate() {
        menu = match item {
            MenuItem::Heading(text) => menu.label(text.clone()),
            MenuItem::Rule => menu.separator(),
            MenuItem::Fold(fold) => {
                let (inner, ways) = (fold.items.clone(), ways.clone());
                // The kit's submenu row takes a plain string for its label, so the note cannot sit at
                // the row's right edge as the page's meta text does; the flyout ticks the row in use.
                menu.submenu(fold.label.clone(), window, cx, move |sub, window, cx| fill_items(sub, &inner, &ways, now, window, cx))
            }
            MenuItem::Choice(choice) => {
                let shown = choice.clone();
                let (held, act, does) = (ways.held.clone(), ways.act.clone(), choice.does.clone());
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
