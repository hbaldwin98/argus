//! Which extension views the window holds, which are listed where the person
//! is, the one open — docked in the side panel or as a page — with its
//! selected row and the question an action waits on, where the person chose
//! each view opens, and what its gestures send.
//!
//! The daemon owns the views. Like an agent's view, one changes only when
//! its extension publishes it again: an action is sent as it stands, and
//! nothing here moves until the extension answers with a new view. Where a
//! view opens is the window's own: the extension's `place` is only where it
//! opens until the person moves it.

use std::collections::BTreeMap;

use argus_protocol::{CheckoutId, ClientMsg, ExtPlace, ExtScope, ExtView, RepositoryId};

use crate::page::{ExtConfirm, ExtTab, ExtTable, ExtTableAction, ExtTableRow};
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

/// The view open, by its extension and its name, and what the person did
/// to it there.
struct Opened {
    extension: String,
    view: String,
    /// Its title when it opened, to say what went once it has gone.
    title: String,
    selected: Option<String>,
    /// An action waiting on a yes, and what the yes sends.
    confirm: Option<(ExtConfirm, ClientMsg)>,
}

#[derive(Default)]
pub struct ExtensionViews {
    views: Vec<ExtView>,
    open: Option<Opened>,
    /// Docked or a page, by `extension/view`, as the person chose.
    modes: BTreeMap<String, ExtPlace>,
    /// Whether the sidebar's Extensions are unfolded.
    listed: bool,
    unsaved: bool,
}

fn key(extension: &str, view: &str) -> String {
    format!("{extension}/{view}")
}

impl ExtensionViews {
    pub fn new(layout: ExtensionsLayout) -> ExtensionViews {
        ExtensionViews { modes: layout.modes, listed: layout.open, ..ExtensionViews::default() }
    }

    pub fn receive(&mut self, views: Vec<ExtView>) {
        self.views = views;
        // A question about a row the extension has since taken away, or a
        // view it no longer publishes, is no longer one to answer.
        let still = self.open.as_ref().and_then(|open| {
            let (_, ClientMsg::ExtensionAction { extension, view, row, .. }) = open.confirm.as_ref()? else {
                return None;
            };
            Some(self.find(extension, view).is_some_and(|v| row.as_ref().is_none_or(|row| v.rows.iter().any(|r| &r.key == row))))
        });
        if still == Some(false) {
            if let Some(open) = &mut self.open {
                open.confirm = None;
            }
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

    pub fn views(&self) -> &[ExtView] {
        &self.views
    }

    /// Where `view` opens: where the person last put it, else where its
    /// extension says.
    pub fn mode_of(&self, view: &ExtView) -> ExtPlace {
        self.modes.get(&key(&view.extension, &view.name)).copied().unwrap_or(view.place)
    }

    /// Opens `view` where it opens, from its first row when it was not the
    /// one open already: that place, or `None` when there is no such view
    /// here.
    pub fn open(&mut self, extension: &str, view: &str, here: &Here) -> Option<ExtPlace> {
        let found = self.find(extension, view).filter(|v| here.holds(v.scope))?;
        let (mode, title) = (self.mode_of(found), found.title.clone());
        let same = self.open.as_ref().is_some_and(|o| o.extension == extension && o.view == view);
        if !same {
            self.open = Some(Opened {
                extension: extension.to_string(),
                view: view.to_string(),
                title,
                selected: None,
                confirm: None,
            });
        }
        Some(mode)
    }

    /// Moves the view open to `mode`, and remembers it there. Says whether
    /// a view was open to move.
    pub fn set_mode(&mut self, mode: ExtPlace) -> bool {
        let Some(open) = &self.open else { return false };
        let at = key(&open.extension, &open.view);
        if self.modes.get(&at) != Some(&mode) {
            self.modes.insert(at, mode);
            self.unsaved = true;
        }
        true
    }

    /// Where the view open is, while it is still published.
    pub fn open_mode(&self) -> Option<ExtPlace> {
        let open = self.open.as_ref()?;
        self.find(&open.extension, &open.view).map(|v| self.mode_of(v))
    }

    /// What the view open was called and who published it, to say it went.
    pub fn open_title(&self) -> Option<(String, String)> {
        self.open.as_ref().map(|o| (o.extension.clone(), o.title.clone()))
    }

    pub fn hide(&mut self) {
        self.open = None;
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The view open, while it is still published and in scope.
    pub fn shown(&self, here: &Here) -> Option<&ExtView> {
        let open = self.open.as_ref()?;
        self.find(&open.extension, &open.view).filter(|v| here.holds(v.scope))
    }

    pub fn select(&mut self, key: String) {
        if let Some(open) = &mut self.open {
            open.selected = Some(key);
        }
    }

    /// The selected row's key: the one chosen while the view holds it,
    /// else the first.
    fn selected_in(&self, view: &ExtView) -> Option<String> {
        let chosen = self.open.as_ref().and_then(|o| o.selected.as_ref());
        view.rows.iter().find(|r| Some(&r.key) == chosen).or(view.rows.first()).map(|r| r.key.clone())
    }

    /// Moves the selection `by` rows, held at either end.
    pub fn step(&mut self, here: &Here, by: isize) {
        let Some(view) = self.shown(here) else { return };
        let current = self.selected_in(view);
        let at = view.rows.iter().position(|r| Some(&r.key) == current.as_ref()).unwrap_or(0);
        let next = at.saturating_add_signed(by).min(view.rows.len().saturating_sub(1));
        let row = view.rows.get(next).map(|r| r.key.clone());
        if let Some(row) = row {
            self.select(row);
        }
    }

    /// The first row, or the last.
    pub fn select_end(&mut self, here: &Here, last: bool) {
        let Some(view) = self.shown(here) else { return };
        let row = if last { view.rows.last() } else { view.rows.first() };
        if let Some(row) = row.map(|r| r.key.clone()) {
            self.select(row);
        }
    }

    /// The action `id` of the view open, on its selected row unless it is
    /// view-wide: what to send now, or nothing when it asks first.
    pub fn act(&mut self, here: &Here, id: &str) -> Result<Option<ClientMsg>, String> {
        let view = self.shown(here).ok_or("no extension's view is open")?;
        let action = view.actions.iter().find(|a| a.id == id).ok_or_else(|| format!("{} offers no {id}", view.title))?;
        let row = if action.view_wide {
            None
        } else {
            Some(self.selected_in(view).ok_or_else(|| format!("{} needs a row", action.label))?)
        };
        let msg = ClientMsg::ExtensionAction {
            extension: view.extension.clone(),
            view: view.name.clone(),
            row,
            action: action.id.clone(),
        };
        let question = action.confirm.clone().map(|question| ExtConfirm { label: action.label.clone(), question });
        match (question, &mut self.open) {
            (Some(confirm), Some(open)) => {
                open.confirm = Some((confirm, msg));
                Ok(None)
            }
            _ => Ok(Some(msg)),
        }
    }

    /// The person answered the question: a yes is what it asked about.
    pub fn answer(&mut self, yes: bool) -> Option<ClientMsg> {
        let (_, msg) = self.open.as_mut()?.confirm.take()?;
        yes.then_some(msg)
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
        ExtensionsLayout { open: self.listed, modes: self.modes.clone() }
    }

    /// The views listed here, each saying where it opens and whether it is
    /// the one open.
    pub fn tabs(&self, here: &Here) -> Vec<ExtTab> {
        let open = self.shown(here).map(|v| (v.extension.as_str(), v.name.as_str()));
        self.views
            .iter()
            .filter(|v| here.holds(v.scope))
            .map(|v| ExtTab {
                extension: v.extension.clone(),
                view: v.name.clone(),
                title: v.title.clone(),
                icon: v.icon.clone(),
                page: self.mode_of(v) == ExtPlace::Stage,
                on: open == Some((v.extension.as_str(), v.name.as_str())),
            })
            .collect()
    }

    /// The view open, as the panel or the page draws it.
    pub fn table(&self, here: &Here) -> Option<ExtTable> {
        let view = self.shown(here)?;
        Some(ExtTable {
            extension: view.extension.clone(),
            view: view.name.clone(),
            title: view.title.clone(),
            page: self.mode_of(view) == ExtPlace::Stage,
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
            selected: self.selected_in(view),
            confirm: self.open.as_ref().and_then(|o| o.confirm.as_ref()).map(|(c, _)| c.clone()),
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
             "columns": [{"title": "Tag"}]}
        ]"#;
        serde_json::from_str(json).unwrap()
    }

    fn held() -> (ExtensionViews, Here) {
        let mut views = ExtensionViews::default();
        views.receive(published());
        (views, Here::default())
    }

    #[test]
    fn a_scoped_view_is_listed_only_where_its_scope_is() {
        let (views, nowhere) = held();
        assert_eq!(views.tabs(&nowhere).len(), 1);
        let here = Here { repository: Some(RepositoryId(4)), checkout: None };
        let names: Vec<String> = views.tabs(&here).into_iter().map(|t| t.view).collect();
        assert_eq!(names, ["containers", "images"]);
    }

    #[test]
    fn a_view_opens_where_its_extension_says_until_the_person_moves_it() {
        let (mut views, here) = held();
        let repo = Here { repository: Some(RepositoryId(4)), checkout: None };
        assert_eq!(views.open("docker", "images", &repo), Some(ExtPlace::Panel), "the default");
        assert_eq!(views.open("docker", "containers", &here), Some(ExtPlace::Stage), "as it asked");
        assert!(views.set_mode(ExtPlace::Panel));
        assert!(views.take_unsaved());
        views.receive(published());
        assert_eq!(views.open_mode(), Some(ExtPlace::Panel), "kept across a republish");
        let again = ExtensionViews::new(views.layout());
        let mut again = again;
        again.receive(published());
        assert_eq!(again.open("docker", "containers", &here), Some(ExtPlace::Panel), "and across runs");
    }

    #[test]
    fn an_action_on_the_selected_row_is_sent_as_it_stands() {
        let (mut views, here) = held();
        assert!(views.open("docker", "containers", &here).is_some());
        views.step(&here, 1);
        let sent = views.act(&here, "logs").unwrap().unwrap();
        assert!(matches!(
            sent,
            ClientMsg::ExtensionAction { ref row, ref action, .. } if row.as_deref() == Some("db") && action == "logs"
        ));
        let wide = views.act(&here, "prune").unwrap().unwrap();
        assert!(matches!(wide, ClientMsg::ExtensionAction { row: None, .. }));
        assert_eq!(views.table(&here).unwrap().rows.len(), 2, "nothing predicted");
    }

    #[test]
    fn an_action_that_asks_first_waits_for_a_yes() {
        let (mut views, here) = held();
        views.open("docker", "containers", &here);
        assert!(views.act(&here, "restart").unwrap().is_none());
        assert_eq!(views.table(&here).unwrap().confirm.unwrap().question, "Restart it?");
        assert!(views.answer(false).is_none());
        assert!(views.table(&here).unwrap().confirm.is_none());
        views.act(&here, "restart").unwrap();
        assert!(matches!(views.answer(true), Some(ClientMsg::ExtensionAction { .. })));
    }

    #[test]
    fn a_question_about_a_row_since_taken_away_is_dropped() {
        let (mut views, here) = held();
        views.open("docker", "containers", &here);
        views.act(&here, "restart").unwrap();
        let mut republished = published();
        republished[0].rows.remove(0);
        views.receive(republished);
        assert!(views.answer(true).is_none());
    }

    #[test]
    fn a_view_out_of_scope_or_gone_is_not_shown() {
        let (mut views, here) = held();
        assert!(views.open("docker", "images", &here).is_none(), "out of scope");
        assert!(views.open("docker", "containers", &here).is_some());
        views.receive(Vec::new());
        assert!(views.shown(&here).is_none());
        assert_eq!(views.open_mode(), None);
        assert!(views.act(&here, "logs").is_err());
    }
}
