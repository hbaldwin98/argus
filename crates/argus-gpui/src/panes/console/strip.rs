//! What the console's tab strip and its menus offer, as plain data: which
//! tabs wear a status dot, the one menu that holds split, move and fill the
//! window, a tab's own menu, and the `Call` each item sends.
//!
//! The page's `console_strip.ts` and `console.ts` (`tabMenu`, `stripMenu`)
//! decide this and Rust decides what each does, so an item is only the
//! calls it sends. `panes/console` turns the offers into the kit's menus.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Group, Place};
use argus_desktop_core::layout::docks::{Dock, DocksPage, Panel};
use argus_desktop_core::panes::console::{ConsoleGesture, ConsolePage, ConsolePlace, ConsoleTab};

use crate::keys::caps;
use crate::kit::menu::item::MenuItem;
use crate::layout::dock_menu;

/// An idle shell is the ordinary case, so it wears nothing; a dot is for a
/// console that is doing something, wants someone or went wrong.
pub fn wears_dot(status: &str) -> bool {
    status != "idle"
}

/// What choosing an item does.
#[derive(Debug, Clone)]
pub enum Does {
    /// Sends each call, in order.
    Calls(Vec<Call>),
    /// Opens the rename dialog; the dialog sends the call on Enter.
    Rename { pane: u64, title: String },
}

/// A console menu's items: choices that send calls, or open the rename dialog.
pub type Offers = Vec<MenuItem<Does>>;

fn item(label: impl Into<String>, call: Call) -> MenuItem<Does> {
    MenuItem::choice(label, Does::Calls(vec![call]))
}

fn keyed(label: impl Into<String>, call: Call, keys: Vec<Vec<String>>) -> MenuItem<Does> {
    item(label, call).keyed(keys)
}

fn console(gesture: ConsoleGesture) -> Call {
    Call::Console { gesture }
}

/// A tab's (or a split side's) menu: its name, `Rename...`, on a split side
/// taking it out of the split, then closing it and, when there are others,
/// closing the others one `close` each.
pub fn tab_offers(tab: &ConsoleTab, side: bool, tabs: &[ConsoleTab]) -> Offers {
    let mut offers = vec![
        MenuItem::heading(tab.title.clone()),
        MenuItem::choice("Rename\u{2026}",Does::Rename { pane: tab.pane, title: tab.title.clone() }),
    ];
    if side {
        offers.push(item("Take out of the split", console(ConsoleGesture::CloseSide { pane: tab.pane })));
    }
    offers.push(MenuItem::Rule);
    offers.push(item("Close", console(ConsoleGesture::Close { pane: tab.pane })).danger());
    let others: Vec<Call> =
        tabs.iter().filter(|other| other.pane != tab.pane).map(|other| console(ConsoleGesture::Close { pane: other.pane })).collect();
    if !others.is_empty() {
        offers.push(MenuItem::choice("Close the others", Does::Calls(others)).danger());
    }
    offers
}

/// The strip's more menu for the console as it stands: split or put the
/// split away, move between the Console stage and its dock, fill the window
/// or leave it, then the docks it can be moved to (never floated).
pub fn strip_offers(page: &ConsolePage, docks: &DocksPage, keymap: &[Group]) -> Offers {
    let docked = page.place == ConsolePlace::Docked;
    let dock = dock_menu::dock_of(docks, &Panel::Console).unwrap_or(Dock::Bottom);
    let split = page.beside.is_some();
    let mut offers = vec![
        keyed(
            if split { "Put the split away" } else { "Split" },
            console(ConsoleGesture::Split),
            caps::presses(keymap, Action::SplitConsole, Place::Pane),
        ),
        if docked {
            keyed(
                "Open in the Console stage",
                console(ConsoleGesture::Tab),
                caps::presses(keymap, Action::Console, Place::Anywhere),
            )
        } else {
            item(dock_menu::words(dock), console(ConsoleGesture::Dock))
        },
        keyed(
            if page.place == ConsolePlace::Fullscreen { "Leave fullscreen" } else { "Fullscreen" },
            console(ConsoleGesture::Fullscreen),
            caps::presses(keymap, Action::ConsoleFullscreen, Place::Pane),
        ),
        MenuItem::Rule,
    ];
    offers.extend(dock_menu::moves(&Panel::Console, docked, docks, None).into_iter().map(|entry| entry.map(|call| Does::Calls(vec![call]))));
    offers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::menu::item::{lines, Choice, Hint};
    use argus_desktop_core::layout::docks::{DockPage, DockTab};

    fn tab(pane: u64, title: &str) -> ConsoleTab {
        ConsoleTab { pane, title: title.into(), status: "idle", on: false }
    }

    fn labels(offers: &[MenuItem<Does>]) -> Vec<String> {
        lines(offers)
    }

    fn calls(offer: &MenuItem<Does>) -> Vec<String> {
        match offer.as_choice() {
            Some(Choice { does: Does::Calls(calls), .. }) => calls.iter().map(|call| format!("{call:?}")).collect(),
            other => panic!("not an item that calls: {other:?}"),
        }
    }

    fn page(place: ConsolePlace, beside: Option<u64>) -> ConsolePage {
        ConsolePage { place, beside, ..ConsolePage::default() }
    }

    fn docked_at(dock: Dock) -> DocksPage {
        let mut docks = DocksPage::default();
        let one = DockPage {
            tabs: vec![DockTab { panel: Panel::Console, title: "Console".into() }],
            shown: Some(Panel::Console),
            size: 200,
        };
        match dock {
            Dock::Left => docks.left = one,
            Dock::Right => docks.right = one,
            Dock::Bottom => docks.bottom = one,
        }
        docks
    }

    #[test]
    fn only_a_shell_that_is_doing_something_wears_a_dot() {
        assert!(!wears_dot("idle"));
        for status in ["working", "waiting", "needs-review", "failed", "exited", "done"] {
            assert!(wears_dot(status), "{status}");
        }
    }

    #[test]
    fn a_tabs_menu_renames_closes_and_closes_the_others_one_close_each() {
        let tabs = [tab(5, "pwsh"), tab(6, "cargo"), tab(7, "node")];
        let offers = tab_offers(&tabs[1], false, &tabs);
        assert_eq!(labels(&offers), ["# cargo", "Rename\u{2026}", "---", "!Close", "!Close the others"]);
        assert!(matches!(&offers[1].as_choice().unwrap().does, Does::Rename { pane: 6, title } if title == "cargo"));
        assert_eq!(calls(&offers[3]), ["Console { gesture: Close { pane: 6 } }"]);
        assert_eq!(calls(&offers[4]), ["Console { gesture: Close { pane: 5 } }", "Console { gesture: Close { pane: 7 } }"]);
    }

    #[test]
    fn a_lone_tab_has_no_others_to_close_and_only_a_split_side_leaves_the_split() {
        let tabs = [tab(5, "pwsh")];
        assert_eq!(labels(&tab_offers(&tabs[0], false, &tabs)), ["# pwsh", "Rename\u{2026}", "---", "!Close"]);
        let side = tab_offers(&tabs[0], true, &tabs);
        assert_eq!(labels(&side)[2], "Take out of the split");
        assert_eq!(calls(&side[2]), ["Console { gesture: CloseSide { pane: 5 } }"]);
    }

    #[test]
    fn the_strip_menu_names_the_way_back_once_split_undocked_or_full() {
        let docked = strip_offers(&page(ConsolePlace::Docked, None), &docked_at(Dock::Bottom), &[]);
        assert_eq!(
            labels(&docked),
            [
                "Split",
                "Open in the Console stage",
                "Fullscreen",
                "---",
                "Dock on the left",
                "Dock on the right",
                "Dock at the bottom (here)"
            ]
        );
        let staged = strip_offers(&page(ConsolePlace::Tab, Some(7)), &docked_at(Dock::Right), &[]);
        assert_eq!(labels(&staged)[..3], ["Put the split away", "Dock on the right", "Fullscreen"]);
        assert_eq!(calls(&staged[1]), ["Console { gesture: Dock }"]);
        assert!(!labels(&staged).iter().any(|line| line.contains("(here)")), "off its dock it is in none");
        let full = strip_offers(&page(ConsolePlace::Fullscreen, None), &docked_at(Dock::Left), &[]);
        assert_eq!(labels(&full)[2], "Leave fullscreen");
        assert_eq!(labels(&full)[1], "Dock on the left");
    }

    #[test]
    fn the_strip_menus_split_stage_and_fullscreen_carry_the_keymaps_keys() {
        // SHELL-CONSOLE-8: split is leader `\`, the stage `4`, fullscreen leader `z`.
        let keymap = argus_desktop_core::keys::keymap::groups();
        let offers = strip_offers(&page(ConsolePlace::Docked, None), &docked_at(Dock::Bottom), &keymap);
        let keys = |offer: &MenuItem<Does>| match offer.as_choice().map(|choice| &choice.hint) {
            Some(Hint::Keys(keys)) => keys.iter().map(|presses| presses.join(" ")).collect::<Vec<_>>(),
            Some(Hint::None) => Vec::new(),
            other => panic!("not an item with keys: {other:?}"),
        };
        assert_eq!(keys(&offers[0]), ["Ctrl Space \\"]);
        assert!(keys(&offers[1]).iter().any(|k| k == "4"), "{:?}", keys(&offers[1]));
        assert_eq!(keys(&offers[2]), ["Ctrl Space z"]);
        assert!(keys(&offers[4]).is_empty(), "a dock move has no key");
    }

    #[test]
    fn the_dock_moves_send_the_dock_calls_for_the_console() {
        let offers = strip_offers(&page(ConsolePlace::Docked, None), &docked_at(Dock::Bottom), &[]);
        assert_eq!(calls(&offers[4]), ["Docks { gesture: Move { panel: Console, dock: Left } }"]);
    }
}
