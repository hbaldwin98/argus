//! Where Down, Up, `j` and `k` leave a menu's highlight: on a choice that can be chosen, never a
//! heading (SHELL-MENU-4).
//!
//! The page focuses the first enabled item when a menu opens and steps among enabled items only,
//! wrapping (ts:kit/menu.ts). The kit's menu steps among them too, except that with nothing
//! highlighted and when wrapping past the end it goes to item 0, which is a heading in the dock
//! and console tab menus. Its highlight is private, so this keeps its own account of where it is
//! (from the keys it saw) and, when the kit would land on a heading, presses Down once more to
//! reach the first enabled choice. Pointing at a row makes the kit move the highlight where this
//! cannot see, so the account is dropped then and the kit steps alone until the menu is opened
//! again.

use std::cell::RefCell;

use gpui_kit::base::actions::SelectDown;
use gpui_kit::{App, Keystroke, Window};

/// The kit's name for the context its menu holds the keys in.
const MENU: &str = "PopupMenu";

/// What this knows of the kit's highlight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spot {
    Nothing,
    At(usize),
    /// Moved by the pointer, which this does not see.
    Unknown,
}

struct Open {
    /// The kit's index of every item that can be chosen, in order.
    enabled: Vec<usize>,
    spot: Spot,
}

thread_local! {
    /// The menu most recently opened: the one whose keys these are.
    static OPEN: RefCell<Option<Open>> = const { RefCell::new(None) };
}

/// A menu has opened with these items (by the kit's index) able to be chosen.
pub fn opened(enabled: Vec<usize>) {
    OPEN.with(|open| *open.borrow_mut() = Some(Open { enabled, spot: Spot::Nothing }));
}

/// The pointer is on a row, so the kit's highlight is wherever it put it.
pub fn pointed() {
    OPEN.with(|open| {
        if let Some(open) = open.borrow_mut().as_mut() {
            open.spot = Spot::Unknown;
        }
    });
}

/// Where Down goes from `spot`, and whether the kit would go there by itself.
fn down(enabled: &[usize], spot: Spot) -> Option<(usize, bool)> {
    let first = *enabled.first()?;
    let (wanted, kit) = match spot {
        Spot::At(at) => match enabled.iter().find(|&&ix| ix > at) {
            Some(&next) => (next, next),
            None => (first, 0),
        },
        _ => (first, 0),
    };
    Some((wanted, wanted == kit))
}

fn up(enabled: &[usize], spot: Spot) -> Option<usize> {
    let last = *enabled.last()?;
    match spot {
        Spot::At(at) => Some(enabled.iter().rev().find(|&&ix| ix < at).copied().unwrap_or(last)),
        _ => Some(last),
    }
}

/// Whether a menu of the kit holds the keys now.
pub fn is_open(window: &Window) -> bool {
    window.context_stack().iter().any(|context| context.contains(MENU))
}

/// A key while a menu is open (the Menu layer hears it, `keys/layers/menu`): steps the highlight
/// where the kit would land on a heading. Whether it was taken; a key not taken goes on to the
/// kit's own bindings.
pub fn step(keystroke: &Keystroke, window: &mut Window, cx: &mut App) -> bool {
    if keystroke.modifiers.modified() || !is_open(window) {
        return false;
    }
    let key = keystroke.key.as_str();
    let going_down = matches!(key, "down" | "j");
    let going_up = matches!(key, "up" | "k");
    if !going_down && !going_up {
        return false;
    }
    let presses = OPEN.with(|open| {
        let mut open = open.borrow_mut();
        let open = open.as_mut()?;
        if open.spot == Spot::Unknown {
            return None;
        }
        if going_up {
            open.spot = up(&open.enabled, open.spot).map_or(open.spot, Spot::At);
            return None;
        }
        // Nothing to land on: the kit would still move to a heading, so it is not asked.
        let Some((wanted, direct)) = down(&open.enabled, open.spot) else { return Some(0) };
        open.spot = Spot::At(wanted);
        Some(if direct { 1 } else { 2 })
    });
    let Some(presses) = presses else { return false };
    for _ in 0..presses {
        window.dispatch_action(Box::new(SelectDown), cx);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // Headings at 0 and 3, choices at 1, 2 and 4.
    const ENABLED: [usize; 3] = [1, 2, 4];

    #[test]
    fn down_from_nothing_goes_to_the_first_choice_by_a_second_press() {
        assert_eq!(down(&ENABLED, Spot::Nothing), Some((1, false)));
    }

    #[test]
    fn down_steps_past_a_heading_as_the_kit_does() {
        assert_eq!(down(&ENABLED, Spot::At(2)), Some((4, true)));
    }

    #[test]
    fn down_wraps_to_the_first_choice_not_the_heading() {
        assert_eq!(down(&ENABLED, Spot::At(4)), Some((1, false)));
    }

    #[test]
    fn down_from_nothing_is_one_press_when_item_zero_is_a_choice() {
        assert_eq!(down(&[0, 2], Spot::Nothing), Some((0, true)));
        assert_eq!(down(&[0, 2], Spot::At(2)), Some((0, true)));
    }

    #[test]
    fn up_wraps_from_the_first_choice_to_the_last() {
        assert_eq!(up(&ENABLED, Spot::At(1)), Some(4));
        assert_eq!(up(&ENABLED, Spot::Nothing), Some(4));
        assert_eq!(up(&ENABLED, Spot::At(4)), Some(2));
    }

    #[test]
    fn a_menu_with_nothing_to_choose_goes_nowhere() {
        assert_eq!(down(&[], Spot::Nothing), None);
        assert_eq!(up(&[], Spot::Nothing), None);
    }
}
