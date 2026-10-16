//! Which extension views the window holds, which are listed where the person
//! is, each one's selected row, the question an action waits on, and what
//! their gestures send.
//!
//! The daemon owns the views. Like an agent's view, one changes only when
//! its extension publishes it again: an action is sent as it stands, and
//! nothing here moves until the extension answers with a new view. Where a
//! view is — which dock, or a page — is `docks`'s, and the page up is
//! `stage`'s; the extension's `place` is only where it goes the first time
//! it opens.

use std::collections::BTreeMap;

use argus_protocol::{CheckoutId, ClientMsg, ExtPlace, ExtScope, ExtView, PaneId, RepositoryId};
use serde::{Deserialize, Serialize};

use crate::docks::{Dock, Panel, Place};
use crate::extension_keys::{ExtCommandPage, ExtGroup};
use crate::keymap::Action;
use crate::page::Side;
use crate::settings::ExtensionsLayout;

/// Where the person is, which decides the views a scoped one is listed in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Here {
    pub repository: Option<RepositoryId>,
    pub checkout: Option<CheckoutId>,
}

impl Here {
    fn holds(&self, scope: ExtScope) -> bool {
        match scope {
            ExtScope::Global => true,
            ExtScope::Repository(id) => self.repository == Some(id),
            ExtScope::Checkout(id) => self.checkout == Some(id),
        }
    }
}

/// Where an extension says its view goes the first time it opens.
pub fn first_place(place: ExtPlace) -> Place {
    match place {
        ExtPlace::Panel | ExtPlace::Right => Place::Dock(Dock::Right),
        ExtPlace::Left => Place::Dock(Dock::Left),
        ExtPlace::Bottom => Place::Dock(Dock::Bottom),
        ExtPlace::Stage => Place::Stage,
    }
}

/// What the running extensions put in the window: the views listed where
/// the person is, those on screen, their commands, and their
/// keys as the sheet lists them.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtensionsPage {
    /// Every view listed where the person is, for the sidebar's
    /// Extensions.
    pub views: Vec<ExtTab>,
    /// Every view on screen: the one its dock shows, and the page.
    pub tables: Vec<ExtTable>,
    /// The question an action asks before it is sent.
    pub confirm: Option<ExtConfirm>,
    /// Whether the sidebar's Extensions are unfolded.
    pub listed: bool,
    pub commands: Vec<ExtCommandPage>,
    /// A heading of the sheet per extension.
    pub groups: Vec<ExtGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtTab {
    /// The panel it is, which a drag from the sidebar moves.
    pub panel: Panel,
    pub extension: String,
    pub view: String,
    pub title: String,
    /// One of `EXT_ICONS`, or a name the page draws as its generic icon.
    pub icon: Option<String>,
    /// It opens as a page rather than docked.
    pub page: bool,
    /// It is on screen, docked or as the page.
    pub on: bool,
}

/// An extension's view as the panel draws it: its table, the row selected,
/// and what can be done.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtTable {
    pub panel: Panel,
    pub extension: String,
    pub view: String,
    pub title: String,
    /// Open as a page, filling the stage, rather than docked.
    pub page: bool,
    pub columns: Vec<argus_protocol::Column>,
    pub rows: Vec<ExtTableRow>,
    pub actions: Vec<ExtTableAction>,
    /// The selected row's key.
    pub selected: Option<String>,
    /// The row action a double-click or Enter on a row takes.
    pub default: Option<String>,
    /// Set for a view that is a console: the page draws a terminal where
    /// the table would be, and the columns and rows are empty.
    pub console: Option<ExtTableConsole>,
}

/// A console view as the page draws it. What the daemon says of it is here
/// as it said it; the screen and the keys are the window's.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtTableConsole {
    /// The screen drawing it, once the window has given it one; until then
    /// the page has nowhere to send a size.
    pub side: Option<Side>,
    /// The pane running it, which the daemon fills in a moment after it
    /// takes the view.
    pub pane: Option<u64>,
    /// How it ended, while its last screen stays up.
    pub ended: Option<ExtConsoleEnd>,
    /// This console's screen has the keys.
    pub keyed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtConsoleEnd {
    /// The exit code; none when a signal ended it.
    pub code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtTableRow {
    pub key: String,
    pub cells: Vec<argus_protocol::Value>,
    pub status: Option<argus_protocol::Chip>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtTableAction {
    pub id: String,
    pub label: String,
    pub key: Option<String>,
    pub view_wide: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtConfirm {
    /// The extension that asks.
    pub extension: String,
    /// The action's label, for the yes button.
    pub label: String,
    pub question: String,
}

/// What was done in one extension's view.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum ExtensionGesture {
    /// A row clicked.
    Select { key: String },
    /// One of the view's actions, by its id.
    Act { action: String },
    /// A double click or Enter on `row`: the view's default action on it.
    /// The row rides along, so the default is taken on the row clicked
    /// whether or not its select has landed yet.
    Default { row: String },
    /// A key of the view: the selection stepped, or Enter.
    Key { action: Action },
}

#[derive(Default)]
pub struct ExtensionViews {
    views: Vec<ExtView>,
    /// Each view's selected row, while one is chosen.
    selected: BTreeMap<Panel, String>,
    /// An action waiting on a yes: the view it was taken in, the question,
    /// and what the yes sends.
    confirm: Option<(Panel, ExtConfirm, ClientMsg)>,
    /// The view last opened or used, which the palette's page and dock
    /// commands move.
    last: Option<Panel>,
    /// Whether the sidebar's Extensions are unfolded.
    listed: bool,
    unsaved: bool,
}

impl ExtensionViews {
    pub fn new(layout: ExtensionsLayout) -> ExtensionViews {
        ExtensionViews { listed: layout.open, ..ExtensionViews::default() }
    }

    pub fn receive(&mut self, views: Vec<ExtView>) {
        self.views = views;
        // A question about a row or an action the extension has since taken
        // away, or a view it no longer publishes, is no longer one to answer.
        let still = self.confirm.as_ref().is_none_or(|(_, _, msg)| {
            let ClientMsg::ExtensionAction { extension, view, row, action } = msg else { return false };
            self.find(extension, view).is_some_and(|v| v.offers(action, row.as_deref()).is_ok())
        });
        if !still {
            self.confirm = None;
        }
    }

    /// Every view a daemon that went had published goes with it; what the
    /// person chose of each stays, for when they come again.
    pub fn clear(&mut self) {
        self.views.clear();
    }

    fn find(&self, extension: &str, view: &str) -> Option<&ExtView> {
        self.views.iter().find(|v| v.extension == extension && v.name == view)
    }

    /// The view `panel` names, while it is published and in scope here.
    pub fn get(&self, panel: &Panel, here: &Here) -> Option<&ExtView> {
        let (extension, view) = panel.extension_view()?;
        self.find(extension, view).filter(|v| here.holds(v.scope))
    }

    pub fn views(&self) -> &[ExtView] {
        &self.views
    }

    /// The panel of every view listed here, for the docks to show.
    pub fn present(&self, here: &Here) -> impl Iterator<Item = Panel> + '_ {
        let here = *here;
        self.views.iter().filter(move |v| here.holds(v.scope)).map(|v| Panel::extension(&v.extension, &v.name))
    }

    /// The view last opened or used, which the palette's commands move.
    pub fn last(&self) -> Option<&Panel> {
        self.last.as_ref()
    }

    pub fn touch(&mut self, panel: &Panel) {
        self.last = Some(panel.clone());
    }

    /// A gesture in `panel`'s view where the person is: what to send now,
    /// or why nothing is. Any gesture makes it the view last used; the view
    /// itself changes only when its extension publishes it again.
    pub fn gesture(&mut self, panel: &Panel, here: &Here, gesture: ExtensionGesture) -> Result<Option<ClientMsg>, String> {
        self.touch(panel);
        match gesture {
            ExtensionGesture::Select { key } => self.select(panel, key),
            ExtensionGesture::Act { action } => return self.act(panel, here, &action),
            ExtensionGesture::Default { row } => {
                self.select(panel, row);
                return self.take_default(panel, here);
            }
            ExtensionGesture::Key { action } => match action {
                Action::Next => self.step(panel, here, 1),
                Action::Previous => self.step(panel, here, -1),
                Action::First => self.select_end(panel, here, false),
                Action::Last => self.select_end(panel, here, true),
                Action::Choose => return self.take_default(panel, here),
                _ => {}
            },
        }
        Ok(None)
    }

    /// The view's default action on its selected row; a view without one
    /// leaves Enter and a double click doing nothing, said nowhere.
    fn take_default(&mut self, panel: &Panel, here: &Here) -> Result<Option<ClientMsg>, String> {
        match self.get(panel, here).and_then(|v| v.default.clone()) {
            Some(action) => self.act(panel, here, &action),
            None => Ok(None),
        }
    }

    fn select(&mut self, panel: &Panel, key: String) {
        self.selected.insert(panel.clone(), key);
    }

    /// The selected row's key: the one chosen while the view holds it,
    /// else the first.
    fn selected_in(&self, panel: &Panel, view: &ExtView) -> Option<String> {
        let chosen = self.selected.get(panel);
        view.rows.iter().find(|r| Some(&r.key) == chosen).or(view.rows.first()).map(|r| r.key.clone())
    }

    /// Moves the selection `by` rows, held at either end.
    fn step(&mut self, panel: &Panel, here: &Here, by: isize) {
        let Some(view) = self.get(panel, here) else { return };
        let current = self.selected_in(panel, view);
        let at = view.rows.iter().position(|r| Some(&r.key) == current.as_ref()).unwrap_or(0);
        let next = at.saturating_add_signed(by).min(view.rows.len().saturating_sub(1));
        if let Some(row) = view.rows.get(next).map(|r| r.key.clone()) {
            self.select(panel, row);
        }
    }

    /// The first row, or the last.
    fn select_end(&mut self, panel: &Panel, here: &Here, last: bool) {
        let Some(view) = self.get(panel, here) else { return };
        let row = if last { view.rows.last() } else { view.rows.first() };
        if let Some(row) = row.map(|r| r.key.clone()) {
            self.select(panel, row);
        }
    }

    /// The action `id` of the view, on its selected row unless it is
    /// view-wide: what to send now, or nothing when it asks first.
    fn act(&mut self, panel: &Panel, here: &Here, id: &str) -> Result<Option<ClientMsg>, String> {
        let view = self.get(panel, here).ok_or("that extension's view is not shown here")?;
        let picked = self.selected_in(panel, view);
        let taken = view.offers(id, picked.as_deref())?;
        let msg = ClientMsg::ExtensionAction {
            extension: view.extension.clone(),
            view: view.name.clone(),
            row: taken.row.map(str::to_string),
            action: taken.action.id.clone(),
        };
        let question = taken.action.confirm.clone().map(|question| ExtConfirm {
            extension: view.extension.clone(),
            label: taken.action.label.clone(),
            question,
        });
        match question {
            Some(confirm) => {
                self.confirm = Some((panel.clone(), confirm, msg));
                Ok(None)
            }
            None => Ok(Some(msg)),
        }
    }

    /// The person answered the question: a yes is what it asked about.
    pub fn answer(&mut self, yes: bool) -> Option<ClientMsg> {
        let (_, _, msg) = self.confirm.take()?;
        yes.then_some(msg)
    }

    pub fn confirm(&self) -> Option<ExtConfirm> {
        self.confirm.as_ref().map(|(_, confirm, _)| confirm.clone())
    }

    pub fn toggle_listed(&mut self) {
        self.listed = !self.listed;
        self.unsaved = true;
    }

    pub fn listed(&self) -> bool {
        self.listed
    }

    /// What to keep between runs, once something in it changed.
    pub fn take_unsaved(&mut self) -> bool {
        std::mem::take(&mut self.unsaved)
    }

    pub fn layout(&self) -> ExtensionsLayout {
        ExtensionsLayout { open: self.listed, modes: BTreeMap::new() }
    }

    /// The views listed here, each saying whether it opens as a page and
    /// whether it is on screen, as `on` says.
    pub fn tabs(&self, here: &Here, page: impl Fn(&Panel) -> bool, on: impl Fn(&Panel) -> bool) -> Vec<ExtTab> {
        self.views
            .iter()
            .filter(|v| here.holds(v.scope))
            .map(|v| {
                let panel = Panel::extension(&v.extension, &v.name);
                ExtTab {
                    panel: panel.clone(),
                    extension: v.extension.clone(),
                    view: v.name.clone(),
                    title: v.title.clone(),
                    icon: v.icon.clone(),
                    page: page(&panel),
                    on: on(&panel),
                }
            })
            .collect()
    }

    /// The console views on screen, as `on` says, each by its name and the
    /// pane the daemon runs it in, once it has. The window keeps one screen
    /// for each; a view that is off screen costs the daemon nothing to send.
    pub fn consoles_on(&self, here: &Here, on: impl Fn(&Panel) -> bool) -> Vec<(String, Option<PaneId>)> {
        self.views
            .iter()
            .filter(|v| here.holds(v.scope))
            .filter(|v| on(&Panel::extension(&v.extension, &v.name)))
            .filter_map(|v| Some((format!("{}/{}", v.extension, v.name), v.console.as_ref()?.pane)))
            .collect()
    }

    /// The view `panel` names, as its dock or the page draws it.
    pub fn table(&self, panel: &Panel, here: &Here, page: bool) -> Option<ExtTable> {
        let view = self.get(panel, here)?;
        Some(ExtTable {
            panel: panel.clone(),
            extension: view.extension.clone(),
            view: view.name.clone(),
            title: view.title.clone(),
            page,
            columns: view.columns.clone(),
            rows: view
                .rows
                .iter()
                .map(|r| ExtTableRow { key: r.key.clone(), cells: r.cells.clone(), status: r.status.clone() })
                .collect(),
            actions: view
                .actions
                .iter()
                .map(|a| ExtTableAction { id: a.id.clone(), label: a.label.clone(), key: a.key.clone(), view_wide: a.view_wide })
                .collect(),
            selected: self.selected_in(panel, view),
            default: view.default.clone(),
            // The window fills in its screen and keys: only it knows them.
            console: view.console.as_ref().map(|c| ExtTableConsole {
                pane: c.pane.map(|p| p.0),
                ended: c.ended.map(|e| ExtConsoleEnd { code: e.code }),
                ..ExtTableConsole::default()
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn published() -> Vec<ExtView> {
        let json = r#"[
            {"extension": "docker", "name": "containers", "title": "Containers", "place": "stage", "icon": "box",
             "columns": [{"title": "Name"}],
             "rows": [{"key": "web", "cells": [{"kind": "text", "text": "web"}], "status": {"label": "running", "tone": "good"}},
                      {"key": "db", "cells": [{"kind": "text", "text": "db"}]}],
             "actions": [{"id": "restart", "label": "Restart", "key": "r", "confirm": "Restart it?"},
                         {"id": "logs", "label": "Logs", "key": "o"},
                         {"id": "prune", "label": "Prune", "view_wide": true}]},
            {"extension": "docker", "name": "images", "title": "Images", "scope": {"repository": 4},
             "columns": [{"title": "Tag"}],
             "rows": [{"key": "a", "cells": [{"kind": "text", "text": "a"}]},
                      {"key": "b", "cells": [{"kind": "text", "text": "b"}]}]}
        ]"#;
        serde_json::from_str(json).unwrap()
    }

    fn held() -> (ExtensionViews, Here) {
        let mut views = ExtensionViews::default();
        views.receive(published());
        (views, Here::default())
    }

    fn containers() -> Panel {
        Panel::extension("docker", "containers")
    }

    fn images() -> Panel {
        Panel::extension("docker", "images")
    }

    fn key(action: Action) -> ExtensionGesture {
        ExtensionGesture::Key { action }
    }

    /// The views held, the containers' naming `logs` its default.
    fn defaulted() -> ExtensionViews {
        let mut views = ExtensionViews::default();
        let mut published = published();
        published[0].default = Some("logs".into());
        views.receive(published);
        views
    }

    fn sent_row(sent: Option<ClientMsg>) -> Option<String> {
        match sent {
            Some(ClientMsg::ExtensionAction { row, action, .. }) if action == "logs" => row,
            other => panic!("not the default: {other:?}"),
        }
    }

    #[test]
    fn a_key_steps_the_selection_and_enter_takes_the_default_action() {
        let (mut views, here) = (defaulted(), Here::default());
        assert!(views.gesture(&containers(), &here, key(Action::Next)).unwrap().is_none());
        assert_eq!(views.table(&containers(), &here, false).unwrap().selected.as_deref(), Some("db"));
        assert!(views.gesture(&containers(), &here, key(Action::First)).unwrap().is_none());
        assert_eq!(views.table(&containers(), &here, false).unwrap().selected.as_deref(), Some("web"));
        views.gesture(&containers(), &here, key(Action::Last)).unwrap();
        let sent = views.gesture(&containers(), &here, key(Action::Choose)).unwrap();
        assert_eq!(sent_row(sent).as_deref(), Some("db"));
    }

    #[test]
    fn a_default_is_taken_on_the_row_it_rides_with() {
        let (mut views, here) = (defaulted(), Here::default());
        let sent = views.gesture(&containers(), &here, ExtensionGesture::Default { row: "db".into() }).unwrap();
        assert_eq!(sent_row(sent).as_deref(), Some("db"), "not the first row, selected until now");
    }

    #[test]
    fn enter_on_a_view_that_names_no_default_sends_nothing() {
        let (mut views, here) = held();
        assert!(views.gesture(&containers(), &here, key(Action::Choose)).unwrap().is_none());
        let double = ExtensionGesture::Default { row: "db".into() };
        assert!(views.gesture(&containers(), &here, double).unwrap().is_none());
        assert_eq!(views.table(&containers(), &here, false).unwrap().selected.as_deref(), Some("db"), "only selected");
    }

    #[test]
    fn every_gesture_makes_its_view_the_one_the_palette_moves() {
        let (mut views, _) = held();
        let repo = Here { repository: Some(RepositoryId(4)), checkout: None };
        for gesture in [
            ExtensionGesture::Select { key: "a".into() },
            ExtensionGesture::Act { action: "none".into() },
            ExtensionGesture::Default { row: "a".into() },
            key(Action::Choose),
        ] {
            views.touch(&containers());
            let _ = views.gesture(&images(), &repo, gesture);
            assert_eq!(views.last(), Some(&images()));
        }
    }

    #[test]
    fn a_gesture_on_a_view_not_shown_here_is_refused() {
        let (mut views, nowhere) = held();
        let act = ExtensionGesture::Act { action: "logs".into() };
        assert_eq!(views.gesture(&images(), &nowhere, act).unwrap_err(), "that extension's view is not shown here");
    }

    #[test]
    fn a_scoped_view_is_listed_only_where_its_scope_is() {
        let (views, nowhere) = held();
        assert_eq!(views.tabs(&nowhere, |_| false, |_| false).len(), 1);
        let here = Here { repository: Some(RepositoryId(4)), checkout: None };
        let names: Vec<String> = views.tabs(&here, |_| false, |_| false).into_iter().map(|t| t.view).collect();
        assert_eq!(names, ["containers", "images"]);
        assert_eq!(views.present(&here).collect::<Vec<_>>(), [containers(), images()]);
    }

    #[test]
    fn a_view_goes_where_its_extension_says_the_first_time() {
        let (views, here) = held();
        assert_eq!(first_place(views.get(&containers(), &here).unwrap().place), Place::Stage);
        let repo = Here { repository: Some(RepositoryId(4)), checkout: None };
        assert_eq!(first_place(views.get(&images(), &repo).unwrap().place), Place::Dock(Dock::Right), "the default");
        assert_eq!(first_place(ExtPlace::Bottom), Place::Dock(Dock::Bottom));
    }

    #[test]
    fn an_action_on_the_selected_row_is_sent_as_it_stands() {
        let (mut views, here) = held();
        views.gesture(&containers(), &here, key(Action::Next)).unwrap();
        let sent = views.act(&containers(), &here, "logs").unwrap().unwrap();
        assert!(matches!(
            sent,
            ClientMsg::ExtensionAction { ref row, ref action, .. } if row.as_deref() == Some("db") && action == "logs"
        ));
        let wide = views.act(&containers(), &here, "prune").unwrap().unwrap();
        assert!(matches!(wide, ClientMsg::ExtensionAction { row: None, .. }));
        assert_eq!(views.table(&containers(), &here, false).unwrap().rows.len(), 2, "nothing predicted");
    }

    #[test]
    fn each_view_keeps_its_own_selected_row() {
        let (mut views, _) = held();
        let repo = Here { repository: Some(RepositoryId(4)), checkout: None };
        views.gesture(&containers(), &repo, key(Action::Next)).unwrap();
        views.gesture(&images(), &repo, key(Action::Last)).unwrap();
        assert_eq!(views.table(&containers(), &repo, false).unwrap().selected.as_deref(), Some("db"));
        assert_eq!(views.table(&images(), &repo, false).unwrap().selected.as_deref(), Some("b"));
    }

    #[test]
    fn an_action_that_asks_first_waits_for_a_yes() {
        let (mut views, here) = held();
        assert!(views.act(&containers(), &here, "restart").unwrap().is_none());
        let asked = views.confirm().unwrap();
        assert_eq!((asked.question.as_str(), asked.extension.as_str()), ("Restart it?", "docker"));
        assert!(views.answer(false).is_none());
        assert!(views.confirm().is_none());
        views.act(&containers(), &here, "restart").unwrap();
        assert!(matches!(views.answer(true), Some(ClientMsg::ExtensionAction { .. })));
    }

    #[test]
    fn a_question_about_a_row_since_taken_away_is_dropped() {
        let (mut views, here) = held();
        views.act(&containers(), &here, "restart").unwrap();
        let mut republished = published();
        republished[0].rows.remove(0);
        views.receive(republished);
        assert!(views.answer(true).is_none());
    }

    #[test]
    fn an_action_the_view_does_not_offer_is_refused_by_its_title() {
        let (mut views, here) = held();
        assert_eq!(views.act(&containers(), &here, "delete").unwrap_err(), "Containers offers no delete");
    }

    #[test]
    fn a_question_about_an_action_since_taken_away_is_dropped() {
        let (mut views, here) = held();
        views.act(&containers(), &here, "restart").unwrap();
        let mut republished = published();
        republished[0].actions.retain(|a| a.id != "restart");
        views.receive(republished);
        assert!(views.answer(true).is_none());
    }

    #[test]
    fn a_view_out_of_scope_or_gone_is_not_shown() {
        let (mut views, here) = held();
        assert!(views.get(&images(), &here).is_none(), "out of scope");
        assert!(views.get(&containers(), &here).is_some());
        views.receive(Vec::new());
        assert!(views.table(&containers(), &here, true).is_none());
        assert!(views.act(&containers(), &here, "logs").is_err());
    }
}
