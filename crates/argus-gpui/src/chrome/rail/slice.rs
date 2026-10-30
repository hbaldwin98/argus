//! The part of what the window has heard that the rail draws, kept to compare: the rail redraws
//! when this changes and not for every event the session sends.
//!
//! `Heard` is one model every view observes, and a frame or a dialog's page notifies it as much as
//! a tree does. The rail copies the slice it reads, and says it changed only when the copy
//! differs, so a pane's frame (which does not notify) and the console's every key (which does) do
//! not rebuild the whole list. The `Rail` itself is shared, so keeping it costs no copy until it
//! changes.

use std::collections::HashSet;
use std::rc::Rc;

use argus_desktop_core::connection::page::Rail;
use argus_desktop_core::extensions::views::ExtTab;
use argus_desktop_core::keys::keymap::{Action, Place};
use argus_desktop_core::panes::console::ConsolePlace;
use argus_desktop_core::views::nav::{self, NavView};

use super::counts::Counts;
use crate::chrome::nav::Nav;
use crate::chrome::server::{Console, Server};
use crate::chrome::stage::route::frame;
use crate::connection::heard::Heard;
use crate::keys::caps;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Slice {
    pub rail: Rc<Rail>,
    pub git_up: bool,
    pub held: bool,
    /// A fullscreen console puts the strip away and keeps the panel.
    pub furniture: bool,
    /// Each nav item's words with the keys the keymap names for it, in `Nav::ALL`'s order: its tooltip.
    pub titles: Vec<String>,
    pub counts: Counts,
    pub server: Server,
    /// The Extensions fold: every view listed where the person is, and whether it is unfolded.
    pub extensions: Vec<ExtTab>,
    pub extensions_listed: bool,
    /// The Views fold: every saved view, and the ones a panel in a dock shows now.
    pub saved: Vec<NavView>,
    pub lit: HashSet<String>,
    /// The panes that are browsers, which the tree's rows do not say: their rows wear a globe and
    /// are told apart from a shell.
    pub browsers: Vec<u64>,
}

impl Slice {
    /// What `heard` says now, sharing the rail of `before` while it has not changed.
    pub fn of(heard: &Heard, before: Option<&Slice>) -> Slice {
        let rail = match before {
            Some(before) if *before.rail == heard.rail => before.rail.clone(),
            _ => Rc::new(heard.rail.clone()),
        };
        let titles = Nav::ALL
            .iter()
            .map(|nav| match nav.action() {
                Some(action) => caps::titled(nav.label(), &heard.keymap, action, Place::Anywhere),
                None => nav.label().to_owned(),
            })
            .collect();
        let console = Console { shown: heard.console.place != ConsolePlace::Hidden, count: heard.console.tabs.len() };
        let words = if console.shown { "Hide the console" } else { "Show the console" };
        let status = heard.status.as_ref();
        let server = Server {
            link: heard.link(),
            alarm: status.and_then(|s| s.alarm.clone()),
            chime: status.is_none_or(|s| s.chime),
            console,
            console_title: caps::titled(words, &heard.keymap, Action::ToggleConsole, Place::Anywhere),
            settings_title: caps::titled("Settings", &heard.keymap, Action::Settings, Place::Anywhere),
        };
        Slice {
            furniture: frame(&heard.rail, heard.console.place).furniture,
            git_up: heard.git_up(),
            held: heard.held(),
            titles,
            counts: Counts::of(heard, &rail),
            server,
            extensions: heard.extension_views.clone(),
            extensions_listed: heard.extensions_listed,
            saved: heard.saved_views.clone(),
            lit: nav::lit(&heard.saved_panels, &heard.docks),
            browsers: heard.browsers.browsers.iter().map(|browser| browser.pane).collect(),
            rail,
        }
    }
}
