//! What a key is to an extension's view that has the keys: one of the keymap's own for a view (a
//! step of the selection, its default action, its move to a page or back), one of the view's action
//! keys, or an extension command heard there.
//!
//! The page decided this in `keys/keymap.ts` (`extensionKeys`, `stageKeys`). The layer that has the
//! key looks up the keymap's action in its places, which is the toolkit's side of the walk, and
//! asks here what the rest means, so the order of the rule is written once.

use super::keys::{action_for, command_in, ExtCommandPage};
use super::views::ExtTable;
use crate::keys::keymap::Action;
use crate::keys::page_key::PageKey;

/// What a view's keys come to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Routed<'a> {
    /// Move the view to a page, or back into its dock.
    Mode { page: bool },
    /// A key of the view's place: the selection stepped, or Enter.
    Key(Action),
    /// An action of the view whose key was typed, by its id.
    Act(&'a str),
    /// An extension command heard in the view or anywhere.
    Command(&'a ExtCommandPage),
    /// An action of the window's own that the layer may run: the page's stage layer hears
    /// `Anywhere` as well as the view's place.
    Window(Action),
}

/// Whether `action` is one of the view's own steps: the selection moved, or Enter.
pub fn moves(action: Action) -> bool {
    matches!(action, Action::Next | Action::Previous | Action::First | Action::Last | Action::Choose)
}

/// What `page` is in `view`, `bound` being the keymap's action for it in the layer's places. A key
/// bound to nothing is first an action's key, then a command heard in this view or anywhere; none
/// of those is `None`, which the layer holds or passes on as it does.
pub fn route<'a>(view: &'a ExtTable, commands: &'a [ExtCommandPage], bound: Option<Action>, page: &PageKey) -> Option<Routed<'a>> {
    match bound {
        Some(Action::ExtensionPage) => Some(Routed::Mode { page: true }),
        Some(Action::ExtensionDock) => Some(Routed::Mode { page: false }),
        Some(action) if moves(action) => Some(Routed::Key(action)),
        Some(action) => Some(Routed::Window(action)),
        None => action_for(view, page)
            .map(|action| Routed::Act(action.id.as_str()))
            .or_else(|| command_in(commands, &["view", "anywhere"], Some(&view.view), page).map(Routed::Command)),
    }
}

#[cfg(test)]
mod tests;
