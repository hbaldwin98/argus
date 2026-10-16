//! Which extension views the window holds, which are listed where the person
//! is, each one's selected row, the question an action waits on, the view
//! open as a page, and what their gestures send.
//!
//! The daemon owns the views. Like an agent's view, one changes only when
//! its extension publishes it again: an action is sent as it stands, and
//! nothing here moves until the extension answers with a new view. Where a
//! view is — which dock, or a page — is `docks`'s; the extension's `place`
//! is only where it goes the first time it opens.

use std::collections::BTreeMap;

use argus_protocol::{CheckoutId, ClientMsg, ExtPlace, ExtScope, ExtView, PaneId, RepositoryId};

use crate::docks::{Dock, Panel, Place};
use crate::page::{ExtConfirm, ExtConsoleEnd, ExtTab, ExtTable, ExtTableAction, ExtTableConsole, ExtTableRow};
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

/// The view open as a page, and its title when it opened, to say what went
/// once it has gone.
struct Paged {
    panel: Panel,
    extension: String,
    title: String,
}

#[derive(Default)]
pub struct ExtensionViews {
    views: Vec<ExtView>,
    /// Each view's selected row, while one is chosen.
    selected: BTreeMap<Panel, String>,
    /// An action waiting on a yes: the view it was taken in, the question,
    /// and what the yes sends.
    confirm: Option<(Panel, ExtConfirm, ClientMsg)>,
    page: Option<Paged>,
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

    /// Puts `panel` on the stage as a page, or takes the page away.
    pub fn set_page(&mut self, panel: Option<&Panel>) {
        self.page = panel.and_then(|panel| {
            let (extension, name) = panel.extension_view()?;
            let title = self.find(extension, name).map(|v| v.title.clone()).unwrap_or_default();
            Some(Paged { panel: panel.clone(), extension: extension.to_string(), title })
        });
    }

    pub fn page(&self) -> Option<&Panel> {
        self.page.as_ref().map(|p| &p.panel)
    }

    /// What the page was called and who published it, to say it went.
    pub fn page_title(&self) -> Option<(String, String)> {
        self.page.as_ref().map(|p| (p.extension.clone(), p.title.clone()))
    }

    pub fn select(&mut self, panel: &Panel, key: String) {
        self.selected.insert(panel.clone(), key);
    }

    /// The selected row's key: the one chosen while the view holds it,
    /// else the first.
    fn selected_in(&self, panel: &Panel, view: &ExtView) -> Option<String> {
        let chosen = self.selected.get(panel);
        view.rows.iter().find(|r| Some(&r.key) == chosen).or(view.rows.first()).map(|r| r.key.clone())
    }

    /// Moves the selection `by` rows, held at either end.
    pub fn step(&mut self, panel: &Panel, here: &Here, by: isize) {
        let Some(view) = self.get(panel, here) else { return };
        let current = self.selected_in(panel, view);
        let at = view.rows.iter().position(|r| Some(&r.key) == current.as_ref()).unwrap_or(0);
        let next = at.saturating_add_signed(by).min(view.rows.len().saturating_sub(1));
        if let Some(row) = view.rows.get(next).map(|r| r.key.clone()) {
            self.select(panel, row);
        }
    }

    /// The first row, or the last.
    pub fn select_end(&mut self, panel: &Panel, here: &Here, last: bool) {
        let Some(view) = self.get(panel, here) else { return };
        let row = if last { view.rows.last() } else { view.rows.first() };
        if let Some(row) = row.map(|r| r.key.clone()) {
            self.select(panel, row);
        }
    }

    /// The action `id` of the view, on its selected row unless it is
    /// view-wide: what to send now, or nothing when it asks first.
    pub fn act(&mut self, panel: &Panel, here: &Here, id: &str) -> Result<Option<ClientMsg>, String> {
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

    /// The id of the view's default row action, when it names one.
    pub fn default_action(&self, panel: &Panel, here: &Here) -> Option<String> {
        self.get(panel, here)?.default.clone()
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
        views.step(&containers(), &here, 1);
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
        views.step(&containers(), &repo, 1);
        views.select_end(&images(), &repo, true);
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
        views.set_page(Some(&containers()));
        views.receive(Vec::new());
        assert!(views.table(&containers(), &here, true).is_none());
        assert_eq!(views.page_title(), Some(("docker".into(), "Containers".into())), "to say what went");
        assert!(views.act(&containers(), &here, "logs").is_err());
    }
}
