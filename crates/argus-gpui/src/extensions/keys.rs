//! The two layers of the key walk an extension's view has: a view clicked (`Layer::Extension`), up
//! while its body has the keys and no menu or question is over it, and the page on the stage
//! (`Layer::ExtensionPage`), up as long as the stage is the page (EXTVIEW-10 to -16).
//!
//! The keymap's action for the key in the layer's places is looked up here, which is the toolkit's
//! side of the walk; what the key then comes to is core's (`extensions/route`): the view's own step
//! or mode, one of its actions by its key, or a command heard in the view or anywhere. A clicked
//! view passes a key it has no use for to the layer under it; the page holds every key, as every
//! other stage layer does, so none is typed into the pane under it.

use argus_desktop_core::connection::page::{Call, Stage};
use argus_desktop_core::extensions::route::{route, Routed};
use argus_desktop_core::extensions::views::{ExtTable, ExtensionGesture};
use argus_desktop_core::keys::keymap::Place;
use gpui_kit::{App, Entity, Window};

use super::bodies::Bodies;
use crate::connection::heard::Heard;
use crate::keys::extension::send;
use crate::keys::layer::{action_for, Handler, Layers, Press, Verdict};
use crate::keys::layers::menu::Menu;
use crate::keys::run;

/// The view with the keys and what it says: its table, as the session last sent it.
fn table_with_keys(window: &Window, cx: &App) -> Option<(ExtTable, Entity<Heard>)> {
    let bodies = cx.try_global::<Bodies>()?.clone();
    let view = bodies.focused(window, cx)?;
    let view = view.read(cx);
    view.table(cx).map(|table| (table, view.heard.clone()))
}

/// What the key comes to in `view`, run; whether anything ran it. `bound` is the keymap's action for
/// it in the layer's places.
fn run_key(view: &ExtTable, heard: &Entity<Heard>, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> bool {
    let bound = action_for(places, press.page);
    let (commands, pick) = {
        let heard = heard.read(cx);
        (heard.extension_commands.clone(), heard.rail.cursor.pick)
    };
    let panel = view.panel.clone();
    let layers = Layers::of(cx);
    match route(view, &commands, bound, press.page) {
        Some(Routed::Mode { page }) => layers.call(Call::ExtensionMode { panel, page }),
        Some(Routed::Key(action)) => layers.call(Call::Extension { panel, gesture: ExtensionGesture::Key { action } }),
        Some(Routed::Act(action)) => layers.call(Call::Extension { panel, gesture: ExtensionGesture::Act { action: action.to_owned() } }),
        Some(Routed::Command(command)) => {
            let (extension, name) = (command.extension.clone(), command.command.clone());
            send(extension, name, pick.map(|pick| pick.checkout), pick.and_then(|pick| pick.pane), cx)
        }
        Some(Routed::Window(action)) => run::run(action, window, cx),
        None => false,
    }
}

/// A view whose body was clicked.
pub struct Clicked;

impl Handler for Clicked {
    fn open(&self, window: &Window, cx: &App) -> bool {
        !Menu.open(window, cx) && table_with_keys(window, cx).is_some()
    }

    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        let Some((table, heard)) = table_with_keys(window, cx) else { return Verdict::Pass };
        if run_key(&table, &heard, press, places, window, cx) {
            Verdict::Taken
        } else {
            Verdict::Pass
        }
    }
}

/// The view that is the stage's page.
pub struct Page {
    pub heard: gpui_kit::WeakEntity<Heard>,
}

impl Handler for Page {
    fn open(&self, _: &Window, cx: &App) -> bool {
        self.heard.upgrade().is_some_and(|heard| heard.read(cx).rail.stage == Stage::Extension)
    }

    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        let Some(heard) = self.heard.upgrade() else { return Verdict::Pass };
        let table = heard.read(cx).extension_tables.iter().find(|table| table.page).cloned();
        let Some(table) = table else { return Verdict::Held };
        if run_key(&table, &heard, press, places, window, cx) {
            Verdict::Taken
        } else {
            Verdict::Held
        }
    }
}
