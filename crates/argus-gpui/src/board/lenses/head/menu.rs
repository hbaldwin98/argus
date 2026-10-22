//! The feature menu at the end of an open feature's title line (BOARD-HEAD-7, -8, -10): the brief, the
//! tickets, the reconcile with what it waits on, and the views that name the feature.
//!
//! What the menu says of the reconcile is core's (`FeatureView::reconcile_entry`), built from the
//! counts and not unpicked from a sentence; the views are the agent views the feature names and the
//! saved views scoped or attached to it (`views::offered`). A choice names what it sends; the feature
//! changes when the session says so.

use argus_desktop_core::connection::page::{Call, FeatureView};
use argus_desktop_core::views::saved::PageSaved;

use crate::kit::menu::item::MenuItem;

/// What a choice of the menu does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Does {
    /// Opens the brief to be written, once the menu has let go of the keys.
    Brief,
    Link,
    Reconcile,
    /// An agent's view that names the feature.
    View { pane: u64, id: String },
    /// A saved view scoped or attached to it.
    Saved { id: String },
}

impl Does {
    /// The call a choice other than the brief sends.
    pub fn call(&self) -> Option<Call> {
        match self {
            Self::Brief => None,
            Self::Link => Some(Call::OpenLink { map: false }),
            Self::Reconcile => Some(Call::OpenReconcile),
            Self::View { pane, id } => Some(Call::OpenView { pane: *pane, id: id.clone() }),
            Self::Saved { id } => Some(Call::OpenSavedView { id: id.clone(), from: None }),
        }
    }
}

/// How the button says itself to a reader that cannot see it: a waiting proposal is said too, since
/// only the dot on the button shows it.
pub fn button_label(view: &FeatureView) -> &'static str {
    match &view.reconcile_entry {
        Some(entry) if entry.attention => "Feature menu: Reconcile, a proposal waits",
        _ => "Feature menu",
    }
}

/// The menu: Edit the brief, Link a ticket, Reconcile when the feature has a ticket, then a rule and
/// the views when any name it.
pub fn items(view: &FeatureView, saved: &[&PageSaved]) -> Vec<MenuItem<Does>> {
    let mut items = vec![
        MenuItem::choice("Edit the brief", Does::Brief).keyed(vec![vec!["B".into()]]),
        MenuItem::choice("Link a ticket\u{2026}", Does::Link).keyed(vec![vec!["T".into()]]),
    ];
    if let Some(entry) = &view.reconcile_entry {
        items.push(MenuItem::choice(entry.label.clone(), Does::Reconcile).noted(entry.hint.clone()));
    }
    let views: Vec<MenuItem<Does>> = view
        .views
        .iter()
        .map(|link| MenuItem::choice(link.title.clone(), Does::View { pane: link.pane, id: link.id.clone() }).noted("agent view"))
        .chain(saved.iter().map(|view| {
            MenuItem::choice(view.view.title.clone(), Does::Saved { id: view.id.clone() }).noted(if view.stale { "saved, out of date" } else { "saved view" })
        }))
        .collect();
    if !views.is_empty() {
        items.push(MenuItem::Rule);
        items.extend(views);
    }
    items
}

#[cfg(test)]
mod tests;
