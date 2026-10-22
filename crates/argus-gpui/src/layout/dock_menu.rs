//! What a panel's dock menu offers and the call each item sends.
//!
//! Two menus share one list of moves (`layout/docks.ts`, `moveItems`): the
//! one a panel head's dock button opens, and a dock tab's context menu, which
//! adds the tab's name above and its close, or hide, below. Which items there
//! are is decided here from what Rust said of the panel (`PresentPanel`);
//! what each does is Rust's, so an item is only the `Call` it sends.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::{Dock, DockGesture, DocksPage, Panel, PresentPanel};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{App, Entity};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::menu::item::MenuItem;
use crate::kit::menu::popup;
use crate::theme::Tokens;

/// Said in place of a hint by the dock a panel is already in (`layout/docks.ts`, `moveItems`).
const HERE: &str = "here";

fn gesture(gesture: DockGesture) -> Call {
    Call::Docks { gesture }
}

/// "Dock on the right", for a menu item.
pub fn words(dock: Dock) -> &'static str {
    match dock {
        Dock::Left => "Dock on the left",
        Dock::Right => "Dock on the right",
        Dock::Bottom => "Dock at the bottom",
    }
}

/// The dock `panel` is in, or `None` for a page or a panel no dock lists.
pub fn dock_of(docks: &DocksPage, panel: &Panel) -> Option<Dock> {
    [(Dock::Left, &docks.left), (Dock::Right, &docks.right), (Dock::Bottom, &docks.bottom)]
        .into_iter()
        .find(|(_, page)| page.tabs.iter().any(|tab| &tab.panel == panel))
        .map(|(dock, _)| dock)
}

/// The moves to each dock, the one the panel is shown in marked, and for a
/// panel that floats, floating it as a dialog. `docked` is whether the panel
/// is on screen in a dock now; a panel on the stage or floating is in none.
pub fn moves(panel: &Panel, docked: bool, docks: &DocksPage, present: Option<&PresentPanel>) -> Vec<MenuItem<Call>> {
    let own = dock_of(docks, panel);
    let mut items: Vec<MenuItem<Call>> = [Dock::Left, Dock::Right, Dock::Bottom]
        .into_iter()
        .map(|dock| {
            MenuItem::choice(words(dock), gesture(DockGesture::Move { panel: panel.clone(), dock }))
                .refused_when(docked && own == Some(dock), HERE)
        })
        .collect();
    if present.is_some_and(|present| present.floats) {
        let floating = docks.floating.iter().any(|float| &float.panel == panel);
        items.push(MenuItem::Rule);
        items.push(
            MenuItem::choice("Float as a dialog", gesture(DockGesture::Float { panel: panel.clone() })).refused_when(floating, HERE),
        );
    }
    items
}

/// A dock tab's context menu: its name, the moves, and its close, which only a
/// panel that `closes` has, else the hide that puts its dock away.
pub fn tab(panel: &Panel, title: &str, docks: &DocksPage, present: Option<&PresentPanel>) -> Vec<MenuItem<Call>> {
    let mut items = vec![MenuItem::heading(title)];
    items.extend(moves(panel, true, docks, present));
    items.push(MenuItem::Rule);
    if present.is_some_and(|present| present.closes) {
        items.push(MenuItem::choice("Close", gesture(DockGesture::Close { panel: panel.clone() })));
    } else {
        items.push(MenuItem::choice("Hide", gesture(DockGesture::Hide { panel: panel.clone() })));
    }
    items
}

/// The kit's menu for `items`: each choice sends its call through `caller`.
/// While the daemon is gone every choice is disabled, and a click that still
/// arrives (a menu opened before the link dropped) sends nothing: a menu is
/// held like the tabs and the sash (`Heard::held`).
pub fn build(menu: PopupMenu, items: &[MenuItem<Call>], caller: &Caller, heard: &Entity<Heard>, tokens: Tokens, cx: &App) -> PopupMenu {
    let (heard, caller) = (heard.clone(), caller.clone());
    popup::fill(menu, items, tokens, move |cx| heard.read(cx).held(), move |call, _, _| caller.call(call.clone()), cx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::menu::item::lines;
    use argus_desktop_core::layout::docks::{DockPage, DockTab, FloatBounds, FloatPage, PanelKind};

    fn present(panel: &Panel, floats: bool, closes: bool) -> PresentPanel {
        PresentPanel { panel: panel.clone(), kind: PanelKind::Views, floats, stage: false, closes }
    }

    fn docks_with(panel: &Panel, right: bool) -> DocksPage {
        let tab = DockTab { panel: panel.clone(), title: "Views".into() };
        let page = DockPage { tabs: vec![tab], shown: Some(panel.clone()), size: 400 };
        let mut docks = DocksPage::default();
        if right {
            docks.right = page;
        }
        docks
    }

    fn shown(entries: &[MenuItem<Call>]) -> Vec<String> {
        lines(entries)
    }

    #[test]
    fn a_docked_panels_own_dock_is_marked_and_the_others_move_it() {
        let views = Panel::Views;
        let docks = docks_with(&views, true);
        let entries = moves(&views, true, &docks, Some(&present(&views, false, false)));
        assert_eq!(shown(&entries), ["Dock on the left", "Dock on the right (here)", "Dock at the bottom"]);
        let call = &entries[0].as_choice().expect("a choice").does;
        assert_eq!(format!("{call:?}"), "Docks { gesture: Move { panel: Views, dock: Left } }");
    }

    #[test]
    fn a_panel_not_shown_in_its_dock_has_no_place_marked() {
        let views = Panel::Views;
        let docks = docks_with(&views, true);
        let entries = moves(&views, false, &docks, Some(&present(&views, false, false)));
        assert!(!shown(&entries).iter().any(|line| line.contains("(here)")));
    }

    #[test]
    fn only_a_panel_that_floats_offers_to_and_a_floating_one_is_marked() {
        let saved = Panel::Saved("plan".into());
        let docks = DocksPage::default();
        let fixed = moves(&saved, false, &docks, Some(&present(&saved, false, true)));
        assert_eq!(fixed.len(), 3);
        let floats = moves(&saved, false, &docks, Some(&present(&saved, true, true)));
        assert_eq!(shown(&floats)[3..], ["---", "Float as a dialog"]);

        let mut floating = DocksPage::default();
        floating.floating.push(FloatPage {
            panel: saved.clone(),
            title: "Plan".into(),
            bounds: FloatBounds { x: 0, y: 0, width: 300, height: 200 },
        });
        let marked = moves(&saved, false, &floating, Some(&present(&saved, true, true)));
        assert_eq!(shown(&marked)[4], "Float as a dialog (here)");
    }

    #[test]
    fn a_tabs_menu_names_it_and_ends_in_close_or_hide() {
        let views = Panel::Views;
        let docks = docks_with(&views, true);
        let hide = tab(&views, "Views", &docks, Some(&present(&views, false, false)));
        let lines = shown(&hide);
        assert_eq!(lines[0], "# Views");
        assert_eq!(lines[lines.len() - 2..], ["---", "Hide"]);
        let call = &hide.last().and_then(MenuItem::as_choice).expect("a choice").does;
        assert_eq!(format!("{call:?}"), "Docks { gesture: Hide { panel: Views } }");

        let close = tab(&views, "Views", &docks, Some(&present(&views, false, true)));
        assert_eq!(shown(&close).last().map(String::as_str), Some("Close"));
    }
}
