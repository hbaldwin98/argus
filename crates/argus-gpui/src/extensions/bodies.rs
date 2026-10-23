//! Which element draws each extension view: one per panel, made the first time something asks and
//! kept, so a view moved between its dock and the stage's page is the same element with the same
//! focus and, for a console, the same terminal (EXTVIEW-35).
//!
//! A dock, a floating frame and the stage each ask for the panel they show; none builds a view of its
//! own. The key layers ask which of them has the keys.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::layout::docks::Panel;
use gpui_kit::{prelude::*, AnyView, App, Entity, FocusHandle, Global, Window};

use super::view::ExtensionView;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;

/// The views made so far. A handle: clones share one list.
#[derive(Clone, Default)]
pub struct Bodies(Rc<RefCell<Vec<Made>>>);

/// A view and the panel it draws.
type Made = (Panel, Entity<ExtensionView>);

impl Global for Bodies {}

impl Bodies {
    pub fn of(cx: &mut App) -> Self {
        cx.default_global::<Bodies>().clone()
    }

    /// The view drawing `panel`, made on the first ask. Only an extension's view has one.
    pub fn view(&self, panel: &Panel, heard: &Entity<Heard>, caller: &Caller, window: &mut Window, cx: &mut App) -> Option<Entity<ExtensionView>> {
        panel.extension_view()?;
        // A view belongs to the window that heard it: the app may have had another before.
        let found = self.0.borrow().iter().find(|(held, view)| held == panel && view.read(cx).heard == *heard).map(|(_, view)| view.clone());
        if let Some(view) = found {
            return Some(view);
        }
        self.0.borrow_mut().retain(|(held, _)| held != panel);
        let (heard, caller, made) = (heard.clone(), caller.clone(), panel.clone());
        let view = cx.new(|cx| ExtensionView::new(heard, caller, made, window, cx));
        self.0.borrow_mut().push((panel.clone(), view.clone()));
        Some(view)
    }

    /// The view that is the stage's page, if one is up: whichever extension view the session says is a
    /// page, drawn by the same element its dock would draw it with.
    pub fn page(&self, heard: &Entity<Heard>, caller: &Caller, window: &mut Window, cx: &mut App) -> Option<AnyView> {
        let panel = heard.read(cx).extension_tables.iter().find(|table| table.page).map(|table| table.panel.clone())?;
        self.view(&panel, heard, caller, window, cx).map(Into::into)
    }

    /// Whether `handle` is the screen of a console view, which the panes' focus moves on from when the
    /// session gives the keys back to a pane.
    pub fn holds_screen(&self, handle: &FocusHandle, cx: &App) -> bool {
        self.0.borrow().iter().filter_map(|(_, view)| view.read(cx).console.terminal().cloned()).any(|terminal| terminal.read(cx).focus_handle() == handle)
    }

    /// The view whose body has the keys, if one does: a click in it gave them.
    pub fn focused(&self, window: &Window, cx: &App) -> Option<Entity<ExtensionView>> {
        self.0.borrow().iter().find(|(_, view)| view.read(cx).has_keys(window, cx)).map(|(_, view)| view.clone())
    }
}

#[cfg(test)]
impl Bodies {
    /// The view already made for `panel`, for a test to read what it measured.
    pub(crate) fn made(&self, panel: &Panel) -> Option<Entity<ExtensionView>> {
        self.0.borrow().iter().find(|(held, _)| held == panel).map(|(_, view)| view.clone())
    }
}
