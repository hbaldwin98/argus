//! The one element each saved panel is, held by the window so it is the same element whether the
//! panel is docked or floating, and keeps its scroll and what it was showing when it moves
//! (CHECKOUTS-CODE-VIEWS-SAVED-12; the page keeps a panel's element and parks it between docks).

use std::collections::HashMap;

use argus_desktop_core::layout::docks::Panel;
use argus_desktop_core::views::saved::SavedViewsPage;
use gpui_kit::{App, AppContext as _, Entity, Global};

use super::panel::SavedPanel;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;

#[derive(Default)]
struct Held(HashMap<String, Entity<SavedPanel>>);

impl Global for Held {}

/// The element of the saved panel `panel`, made the first time it is asked for. A panel that is not a
/// saved one has none.
pub fn element(panel: &Panel, heard: &Entity<Heard>, caller: &Caller, cx: &mut App) -> Option<Entity<SavedPanel>> {
    let Panel::Saved(_) = panel else { return None };
    let name = panel.to_string();
    if let Some(held) = cx.default_global::<Held>().0.get(&name) {
        return Some(held.clone());
    }
    let made = cx.new(|cx| SavedPanel::new(panel.clone(), heard.clone(), caller.clone(), cx));
    cx.default_global::<Held>().0.insert(name, made.clone());
    Some(made)
}

/// Lets go of the element of `panel` once the page no longer lists it, so a panel opened again
/// starts at the top and the left, as the page's `drawSavedPanels` deletes the element of a panel
/// that is gone. Nothing is dropped before the page has arrived: it lists no panel yet.
pub fn forget_if_closed(panel: &Panel, page: Option<&SavedViewsPage>, cx: &mut App) {
    let Some(page) = page else { return };
    if page.panels.iter().all(|trail| &trail.panel != panel) {
        cx.default_global::<Held>().0.remove(&panel.to_string());
    }
}
