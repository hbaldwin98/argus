//! Which extension views the window holds, which are listed where the person
//! is, the one out in the side panel beside the agents' views, its selected
//! row and the question an action waits on — and what its gestures send.
//!
//! The daemon owns the views. Like an agent's view, one changes only when
//! its extension publishes it again: an action is sent as it stands, and
//! nothing here moves until the extension answers with a new view.

use argus_protocol::{CheckoutId, ClientMsg, ExtScope, ExtView, RepositoryId};

use crate::page::{ExtConfirm, ExtTab, ExtTable, ExtTableAction, ExtTableRow};

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

#[derive(Default)]
pub struct ExtensionViews {
    views: Vec<ExtView>,
    /// The view out in the side panel, by its extension and its name.
    open: Option<(String, String)>,
    /// The row chosen, by key.
    selected: Option<String>,
    /// An action waiting on a yes, and what the yes sends.
    confirm: Option<(ExtConfirm, ClientMsg)>,
}

impl ExtensionViews {
    pub fn receive(&mut self, views: Vec<ExtView>) {
        self.views = views;
        // A question about a row the extension has since taken away, or a
        // view it no longer publishes, is no longer one to answer.
        if let Some((_, ClientMsg::ExtensionAction { extension, view, row, .. })) = &self.confirm {
            let still = self
                .find(extension, view)
                .is_some_and(|v| row.as_ref().is_none_or(|row| v.rows.iter().any(|r| &r.key == row)));
            if !still {
                self.confirm = None;
            }
        }
    }

    /// Every view a daemon that went had published goes with it.
    pub fn clear(&mut self) {
        *self = ExtensionViews { open: self.open.take(), ..ExtensionViews::default() };
    }

    fn find(&self, extension: &str, view: &str) -> Option<&ExtView> {
        self.views.iter().find(|v| v.extension == extension && v.name == view)
    }

    pub fn views(&self) -> &[ExtView] {
        &self.views
    }

    /// Brings `view` out in the panel, from its first row when it was not
    /// the one out already. Says whether there is such a view here.
    pub fn open(&mut self, extension: &str, view: &str, here: &Here) -> bool {
        if !self.find(extension, view).is_some_and(|v| here.holds(v.scope)) {
            return false;
        }
        let wanted = (extension.to_string(), view.to_string());
        if self.open.as_ref() != Some(&wanted) {
            self.open = Some(wanted);
            self.selected = None;
            self.confirm = None;
        }
        true
    }

    pub fn hide(&mut self) {
        self.open = None;
        self.confirm = None;
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The view out, while it is still published and in scope.
    pub fn shown(&self, here: &Here) -> Option<&ExtView> {
        let (extension, view) = self.open.as_ref()?;
        self.find(extension, view).filter(|v| here.holds(v.scope))
    }

    pub fn select(&mut self, key: String) {
        self.selected = Some(key);
    }

    /// The selected row's key: the one chosen while the view holds it,
    /// else the first.
    fn selected_in(&self, view: &ExtView) -> Option<String> {
        view.rows
            .iter()
            .find(|r| Some(&r.key) == self.selected.as_ref())
            .or(view.rows.first())
            .map(|r| r.key.clone())
    }

    /// Moves the selection `by` rows, held at either end.
    pub fn step(&mut self, here: &Here, by: isize) {
        let Some(view) = self.shown(here) else { return };
        let current = self.selected_in(view);
        let at = view.rows.iter().position(|r| Some(&r.key) == current.as_ref()).unwrap_or(0);
        let next = at.saturating_add_signed(by).min(view.rows.len().saturating_sub(1));
        self.selected = view.rows.get(next).map(|r| r.key.clone());
    }

    /// The first row, or the last.
    pub fn select_end(&mut self, here: &Here, last: bool) {
        let Some(view) = self.shown(here) else { return };
        let row = if last { view.rows.last() } else { view.rows.first() };
        self.selected = row.map(|r| r.key.clone());
    }

    /// The action `id` of the view out, on its selected row unless it is
    /// view-wide: what to send now, or nothing when it asks first.
    pub fn act(&mut self, here: &Here, id: &str) -> Result<Option<ClientMsg>, String> {
        let view = self.shown(here).ok_or("no extension's view is out")?;
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
        match &action.confirm {
            Some(question) => {
                let confirm = ExtConfirm { label: action.label.clone(), question: question.clone() };
                self.confirm = Some((confirm, msg));
                Ok(None)
            }
            None => Ok(Some(msg)),
        }
    }

    /// The person answered the question: a yes is what it asked about.
    pub fn answer(&mut self, yes: bool) -> Option<ClientMsg> {
        let (_, msg) = self.confirm.take()?;
        yes.then_some(msg)
    }

    /// The views listed here, for the panel's tabs and the launcher's
    /// count.
    pub fn tabs(&self, here: &Here) -> Vec<ExtTab> {
        self.views
            .iter()
            .filter(|v| here.holds(v.scope))
            .map(|v| ExtTab { extension: v.extension.clone(), view: v.name.clone(), title: v.title.clone() })
            .collect()
    }

    /// The view out, as the panel draws it.
    pub fn table(&self, here: &Here) -> Option<ExtTable> {
        let view = self.shown(here)?;
        Some(ExtTable {
            extension: view.extension.clone(),
            view: view.name.clone(),
            title: view.title.clone(),
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
            confirm: self.confirm.as_ref().map(|(c, _)| c.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn published() -> Vec<ExtView> {
        let json = r#"[
            {"extension": "docker", "name": "containers", "title": "Containers",
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
    fn an_action_on_the_selected_row_is_sent_as_it_stands() {
        let (mut views, here) = held();
        assert!(views.open("docker", "containers", &here));
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
    fn a_view_out_of_scope_or_gone_is_not_out() {
        let (mut views, here) = held();
        assert!(!views.open("docker", "images", &here), "out of scope");
        assert!(views.open("docker", "containers", &here));
        views.receive(Vec::new());
        assert!(views.shown(&here).is_none());
        assert!(views.act(&here, "logs").is_err());
    }
}
