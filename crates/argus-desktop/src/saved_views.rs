//! The views agents saved, as this window holds them, and the trail of
//! columns each saved panel shows.
//!
//! The daemon owns the saved views: refreshing, marking stale, deleting and
//! pressing send the request and change nothing here until the daemon says
//! it changed. What is the window's own is the trail: a panel is named by
//! the view it opened with and holds a row of view ids, one column each,
//! and following a link from a column keeps the columns up to it and
//! replaces the rest with the target. Where the panel sits is
//! `layout::docks`'s.

use std::collections::{BTreeMap, HashMap, HashSet};

use argus_protocol::{
    ClientMsg, FeatureRef, ProjectInfo, RepositoryId, SavedView, ViewPress, ViewScope, ViewSource,
};
use serde::{Deserialize, Serialize};

use crate::agent_views;
use crate::layout::docks::Panel;
use crate::connection::page::PageDiagram;
use crate::connection::sent::Sent;

/// The column of a saved panel a link was followed in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SavedColumn {
    pub panel: Panel,
    pub column: usize,
}

/// Where the person is, which decides the saved views listed as here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Whereabouts {
    pub repository: Option<RepositoryId>,
    /// The feature open on the Feature stage, by its number.
    pub feature: Option<i64>,
}

/// What closing a column came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Closed {
    /// The first column is the panel: it goes, trail and all.
    Panel,
    Column,
    Nothing,
}

#[derive(Default)]
pub struct SavedViews {
    views: Vec<SavedView>,
    /// Each panel's columns, by the id the panel is named with. A panel
    /// that has none is its one first column.
    trails: BTreeMap<String, Vec<String>>,
    refreshing: HashSet<String>,
    /// Action ids pressed in a view and not answered by its being saved again.
    pressed: HashMap<String, Vec<String>>,
    sent: Sent<SavedViewsPage>,
}

impl SavedViews {
    /// A new connection replays the whole list, so what this window held
    /// is dropped rather than doubled. The trails stay: they only name
    /// views, and name them again once the list is back.
    pub fn connected(&mut self, out: &mut Vec<ClientMsg>) {
        self.clear();
        out.push(ClientMsg::WatchSavedViews);
    }

    /// A daemon that cannot keep saved views has none to show.
    pub fn clear(&mut self) {
        self.views.clear();
        self.refreshing.clear();
        self.pressed.clear();
    }

    pub fn receive_all(&mut self, views: Vec<SavedView>) {
        self.views = views;
        let held: Vec<String> = self.views.iter().map(|v| v.id.clone()).collect();
        self.refreshing.retain(|id| held.contains(id));
        self.pressed.retain(|id, _| held.contains(id));
    }

    /// A view was saved again, or for the first time: what waited on it is
    /// answered.
    pub fn changed(&mut self, view: SavedView) {
        self.refreshing.remove(&view.id);
        self.pressed.remove(&view.id);
        match self.views.iter_mut().find(|v| v.id == view.id) {
            Some(held) => *held = view,
            None => self.views.push(view),
        }
    }

    /// A view was deleted. Its panel is the session's to take from the
    /// docks; the columns showing it elsewhere close.
    pub fn removed(&mut self, id: &str) {
        self.views.retain(|v| v.id != id);
        self.refreshing.remove(id);
        self.pressed.remove(id);
        self.trails.remove(id);
        for trail in self.trails.values_mut() {
            trail.retain(|column| column != id);
        }
    }

    pub fn get(&self, id: &str) -> Option<&SavedView> {
        self.views.iter().find(|v| v.id == id)
    }

    /// What a panel or a notice calls a view: its title, else its id.
    pub fn title(&self, id: &str) -> String {
        self.get(id).map(|v| v.view.title.clone()).unwrap_or_else(|| id.to_string())
    }

    /// The panel of every saved view, for the docks to show the ones the
    /// person has placed.
    pub fn present(&self) -> impl Iterator<Item = Panel> + '_ {
        self.views.iter().map(|v| Panel::Saved(v.id.clone()))
    }

    // ---- The trail ----------------------------------------------------------

    /// The panel's columns, left to right.
    pub fn columns(&self, panel: &str) -> Vec<String> {
        self.trails.get(panel).cloned().unwrap_or_else(|| vec![panel.to_string()])
    }

    /// Starts the panel named `panel` if it has no trail yet.
    pub fn open(&mut self, panel: &str) {
        self.trails.entry(panel.to_string()).or_insert_with(|| vec![panel.to_string()]);
    }

    /// Following a link from `column` of the panel: the columns after it
    /// give way to the target. Says whether `column` was there to follow
    /// from.
    pub fn follow(&mut self, panel: &str, column: usize, target: String) -> bool {
        let mut trail = self.columns(panel);
        if column >= trail.len() {
            return false;
        }
        trail.truncate(column + 1);
        trail.push(target);
        self.trails.insert(panel.to_string(), trail);
        true
    }

    /// Closes one column; the first is the panel itself.
    pub fn close_column(&mut self, panel: &str, column: usize) -> Closed {
        let mut trail = self.columns(panel);
        match column {
            _ if column >= trail.len() => Closed::Nothing,
            0 => {
                self.trails.remove(panel);
                Closed::Panel
            }
            _ => {
                trail.remove(column);
                self.trails.insert(panel.to_string(), trail);
                Closed::Column
            }
        }
    }

    /// Takes a column out of the trail to be a panel of its own, giving
    /// its view's id. The first column is the panel already.
    pub fn pop_out(&mut self, panel: &str, column: usize) -> Option<String> {
        let mut trail = self.columns(panel);
        if column == 0 || column >= trail.len() {
            return None;
        }
        let id = trail.remove(column);
        self.trails.insert(panel.to_string(), trail);
        Some(id)
    }

    /// The panels showing the view in a column, the one named for it first.
    pub fn showing(&self, id: &str) -> Vec<String> {
        let mut panels: Vec<String> = self.trails.iter().filter(|(_, t)| t.iter().any(|c| c == id)).map(|(p, _)| p.clone()).collect();
        panels.sort_by_key(|p| p != id);
        if !panels.iter().any(|p| p == id) {
            panels.insert(0, id.to_string());
        }
        panels
    }

    // ---- What the person asks of the daemon ---------------------------------

    pub fn refresh(&mut self, id: String, out: &mut Vec<ClientMsg>) {
        if self.get(&id).is_some() {
            self.refreshing.insert(id.clone());
            out.push(ClientMsg::RefreshSavedView { id });
        }
    }

    pub fn mark_stale(&self, id: String, out: &mut Vec<ClientMsg>) {
        if self.get(&id).is_some() {
            out.push(ClientMsg::MarkSavedViewStale { id });
        }
    }

    pub fn delete(&self, id: String, out: &mut Vec<ClientMsg>) {
        if self.get(&id).is_some() {
            out.push(ClientMsg::DeleteSavedView { id });
        }
    }

    /// Sends a press and holds its action pending until the view is saved
    /// again, as an agent's view does.
    pub fn press(&mut self, id: String, press: ViewPress, out: &mut Vec<ClientMsg>) {
        if self.get(&id).is_some() {
            let pending = self.pressed.entry(id.clone()).or_default();
            if !pending.contains(&press.action) {
                pending.push(press.action.clone());
            }
            out.push(ClientMsg::PressSavedView { id, press });
        }
    }

    /// The daemon could not do what was asked of a view: nothing waits on
    /// it any more. Gives the sentence to say, led by the view's title.
    pub fn failed(&mut self, id: &str, reason: &str) -> String {
        self.refreshing.remove(id);
        self.pressed.remove(id);
        match self.get(id) {
            Some(held) => format!("{}: {reason}", held.view.title),
            None => reason.to_string(),
        }
    }

    // ---- What the page is told ----------------------------------------------

    /// What to send the page, if it has not heard it. `placed` names the
    /// panels the docks hold, whose columns the page draws.
    pub fn take(&mut self, here: Whereabouts, tree: &[ProjectInfo], placed: &[String], can_press: bool) -> Option<SavedViewsPage> {
        let page = self.page(here, tree, placed, can_press);
        // Empty is the default, which `Sent` says only once views have been
        // and gone, so a window the daemon kept none for hears nothing.
        self.sent.take((page != SavedViewsPage::default()).then_some(page))
    }

    pub fn forget(&mut self) {
        self.sent.forget();
    }

    fn page(&self, here: Whereabouts, tree: &[ProjectInfo], placed: &[String], can_press: bool) -> SavedViewsPage {
        let mut views: Vec<(u8, PageSaved)> = self
            .views
            .iter()
            .map(|v| {
                let (rank, label) = scope_label(&v.scope, tree);
                let saved = PageSaved {
                    id: v.id.clone(),
                    panel: Panel::Saved(v.id.clone()),
                    scope: PageScope { rank, label },
                    here: is_here(v, here),
                    features: attached(v),
                    recipe: v.recipe.clone(),
                    sources: v.sources.iter().filter_map(source).collect(),
                    built_at: v.built_at,
                    built_commit: v.built_commit.clone(),
                    stale: v.stale,
                    view: v.view.clone(),
                    diagrams: agent_views::diagrams(&v.view.blocks),
                    refreshing: self.refreshing.contains(&v.id),
                    pressed: self.pressed.get(&v.id).cloned().unwrap_or_default(),
                    under: None,
                };
                (rank, saved)
            })
            .collect();
        views.sort_by(|(a, x), (b, y)| (a, &x.scope.label, &x.view.title).cmp(&(b, &y.scope.label, &y.view.title)));
        let mut views: Vec<PageSaved> = views.into_iter().map(|(_, v)| v).collect();
        nest(&mut views);
        SavedViewsPage {
            views,
            panels: placed.iter().map(|panel| SavedPanel { panel: Panel::Saved(panel.clone()), columns: self.columns(panel) }).collect(),
            can_press,
        }
    }
}

/// Sets each view's `under`, so a view saved to be reached from another
/// waits under it in the sidebar rather than crowding its group. Only
/// within a scope, so a view is never listed under another group's
/// label. Views that only reach each other, in a ring, would have no top;
/// the ring is cut at its first member in the list's order.
fn nest(views: &mut [PageSaved]) {
    let links: Vec<Vec<String>> = views.iter().map(|v| argus_protocol::linked_views(&v.view.blocks)).collect();
    let mut under: Vec<Option<usize>> = (0..views.len())
        .map(|i| {
            (0..views.len()).find(|&p| {
                p != i && views[p].scope.label == views[i].scope.label && links[p].contains(&views[i].id)
            })
        })
        .collect();
    for start in 0..views.len() {
        let mut ring = vec![start];
        let mut at = start;
        while let Some(parent) = under[at] {
            if let Some(pos) = ring.iter().position(|&r| r == parent) {
                let cut = *ring[pos..].iter().min().unwrap();
                under[cut] = None;
                break;
            }
            ring.push(parent);
            at = parent;
        }
    }
    for (i, parent) in under.into_iter().enumerate() {
        views[i].under = parent.map(|p| views[p].id.clone());
    }
}

/// Where a saved view belongs, in words, and how scopes sort: the global
/// ones first, then a repository's, then a feature's.
fn scope_label(scope: &ViewScope, tree: &[ProjectInfo]) -> (u8, String) {
    let repository = |id: RepositoryId| {
        tree.iter().flat_map(|p| &p.repositories).find(|r| r.id == id).map_or_else(|| format!("repository {id}"), |r| r.name.clone())
    };
    match scope {
        ViewScope::Global => (0, "Global".into()),
        ViewScope::Repository { repository: id } => (1, repository(*id)),
        ViewScope::Feature { repository: id, number } => (2, format!("{} #{number}", repository(*id))),
    }
}

/// Whether the view is about where the person is: a global one always,
/// a repository's or a feature's while its repository is the chosen one
/// and, with a feature open, that feature is the one it names.
fn is_here(view: &SavedView, here: Whereabouts) -> bool {
    let on = |repository: RepositoryId, number: Option<i64>| {
        here.repository == Some(repository) && number.is_none_or(|n| here.feature.is_none_or(|f| f == n))
    };
    let own = match view.scope {
        ViewScope::Global => true,
        ViewScope::Repository { repository } => on(repository, None),
        ViewScope::Feature { repository, number } => on(repository, Some(number)),
    };
    own || view.features.iter().any(|f| on(f.repository, Some(f.number)))
}

/// The features a view is scoped to or attached to.
fn attached(view: &SavedView) -> Vec<PageFeature> {
    let scoped = match view.scope {
        ViewScope::Feature { repository, number } => Some(FeatureRef { repository, number }),
        _ => None,
    };
    scoped.iter().chain(&view.features).map(|f| PageFeature { repository: f.repository.0, number: f.number }).collect()
}

fn source(source: &ViewSource) -> Option<PageSource> {
    let (kind, text) = match source {
        ViewSource::File { path } => ("file", path),
        ViewSource::Commit { sha } => ("commit", sha),
        ViewSource::Url { url } => ("url", url),
        ViewSource::Note { text } => ("note", text),
        ViewSource::Unknown => return None,
    };
    Some(PageSource { kind, text: text.clone() })
}

/// The saved views, and the columns of each panel the docks hold.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SavedViewsPage {
    /// Grouped by scope, then by title.
    pub views: Vec<PageSaved>,
    pub panels: Vec<SavedPanel>,
    /// Whether the daemon takes presses in a saved view.
    pub can_press: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SavedPanel {
    pub panel: Panel,
    /// The view ids, left to right; one may be a view the daemon no longer
    /// holds, which the page draws as missing.
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PageSaved {
    pub id: String,
    /// The panel it opens in on its own, which a drag from the sidebar moves.
    pub panel: Panel,
    pub scope: PageScope,
    /// About where the person is, which the sidebar's filter keeps.
    pub here: bool,
    /// The features it is scoped or attached to, for each one's stage.
    pub features: Vec<PageFeature>,
    pub recipe: Option<String>,
    pub sources: Vec<PageSource>,
    /// Unix seconds, by the daemon's clock.
    pub built_at: i64,
    pub built_commit: Option<String>,
    pub stale: bool,
    pub view: argus_protocol::AgentView,
    pub diagrams: Vec<PageDiagram>,
    /// A refresh was asked for and the agent has not saved the view again.
    pub refreshing: bool,
    pub pressed: Vec<String>,
    /// The view the sidebar lists this one under: the first in the list's
    /// order, in the same scope, that links to it. `None` for a view at
    /// the top of its group.
    pub under: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PageScope {
    /// 0 global, 1 repository, 2 feature.
    pub rank: u8,
    /// What the group is called: `Global`, `argus`, `argus #7`.
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PageFeature {
    pub repository: u64,
    pub number: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PageSource {
    /// `file`, `commit`, `url` or `note`.
    #[cfg_attr(test, ts(type = "\"file\" | \"commit\" | \"url\" | \"note\""))]
    pub kind: &'static str,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, project, repository};
    use argus_protocol::AgentView;

    fn saved(id: &str, scope: ViewScope) -> SavedView {
        SavedView {
            id: id.into(),
            scope,
            features: Vec::new(),
            recipe: None,
            sources: Vec::new(),
            built_at: 1,
            built_commit: None,
            stale: false,
            view: AgentView { id: id.into(), title: id.into(), ..Default::default() },
        }
    }

    fn held(ids: &[&str]) -> SavedViews {
        let mut views = SavedViews::default();
        views.receive_all(ids.iter().map(|id| saved(id, ViewScope::Global)).collect());
        views
    }

    fn tree() -> Vec<ProjectInfo> {
        vec![project(1, "src", vec![repository(10, "argus", None, vec![checkout(100, "main", true, vec![])])])]
    }

    #[test]
    fn a_panel_with_no_trail_is_its_one_first_column() {
        let views = held(&["a"]);
        assert_eq!(views.columns("a"), ["a"]);
    }

    #[test]
    fn following_a_link_keeps_the_columns_up_to_it_and_drops_the_rest() {
        let mut views = held(&["a", "b", "c", "d"]);
        assert!(views.follow("a", 0, "b".into()));
        assert!(views.follow("a", 1, "c".into()));
        assert_eq!(views.columns("a"), ["a", "b", "c"]);
        assert!(views.follow("a", 0, "d".into()), "from the first column again");
        assert_eq!(views.columns("a"), ["a", "d"]);
        assert!(!views.follow("a", 5, "b".into()), "no such column to follow from");
        assert_eq!(views.columns("a"), ["a", "d"]);
    }

    #[test]
    fn closing_a_column_takes_that_one_and_the_first_takes_the_panel() {
        let mut views = held(&["a", "b", "c"]);
        views.follow("a", 0, "b".into());
        views.follow("a", 1, "c".into());
        assert_eq!(views.close_column("a", 1), Closed::Column);
        assert_eq!(views.columns("a"), ["a", "c"]);
        assert_eq!(views.close_column("a", 9), Closed::Nothing);
        assert_eq!(views.close_column("a", 0), Closed::Panel);
        assert_eq!(views.columns("a"), ["a"], "a panel opened again starts over");
    }

    #[test]
    fn popping_a_column_out_leaves_the_others_and_never_pops_the_first() {
        let mut views = held(&["a", "b", "c"]);
        views.follow("a", 0, "b".into());
        views.follow("a", 1, "c".into());
        assert_eq!(views.pop_out("a", 0), None);
        assert_eq!(views.pop_out("a", 1).as_deref(), Some("b"));
        assert_eq!(views.columns("a"), ["a", "c"]);
    }

    #[test]
    fn a_view_shown_in_a_trail_is_found_there_and_its_own_panel_comes_first() {
        let mut views = held(&["a", "b"]);
        views.follow("a", 0, "b".into());
        assert_eq!(views.showing("b"), ["b", "a"]);
        views.removed("b");
        assert_eq!(views.columns("a"), ["a"], "a deleted view's column closes");
        assert_eq!(views.showing("b"), ["b"]);
    }

    #[test]
    fn a_refresh_waits_until_the_view_is_saved_again_or_fails() {
        let mut views = held(&["a"]);
        let mut out = Vec::new();
        views.refresh("a".into(), &mut out);
        views.refresh("nope".into(), &mut out);
        assert!(matches!(&out[..], [ClientMsg::RefreshSavedView { id }] if id == "a"));
        assert!(views.page(Whereabouts::default(), &[], &[], true).views[0].refreshing);
        views.changed(saved("a", ViewScope::Global));
        assert!(!views.page(Whereabouts::default(), &[], &[], true).views[0].refreshing);
        views.refresh("a".into(), &mut out);
        assert_eq!(views.failed("a", "its agent is gone"), "a: its agent is gone");
        assert!(!views.page(Whereabouts::default(), &[], &[], true).views[0].refreshing);
    }

    #[test]
    fn views_are_grouped_by_scope_and_here_follows_the_repository_and_feature() {
        let mut views = SavedViews::default();
        let repo = RepositoryId(10);
        let mut attached = saved("attached", ViewScope::Repository { repository: RepositoryId(99) });
        attached.features.push(FeatureRef { repository: repo, number: 7 });
        views.receive_all(vec![
            saved("seven", ViewScope::Feature { repository: repo, number: 7 }),
            saved("repo", ViewScope::Repository { repository: repo }),
            saved("elsewhere", ViewScope::Repository { repository: RepositoryId(99) }),
            saved("all", ViewScope::Global),
            attached,
        ]);
        let tree = tree();
        let at = |feature| views.page(Whereabouts { repository: Some(repo), feature }, &tree, &[], true);
        let page = at(None);
        let order: Vec<_> = page.views.iter().map(|v| (v.id.as_str(), v.scope.label.as_str())).collect();
        assert_eq!(order, [("all", "Global"), ("repo", "argus"), ("attached", "repository 99"), ("elsewhere", "repository 99"), ("seven", "argus #7")]);
        let here = |page: &SavedViewsPage| page.views.iter().filter(|v| v.here).map(|v| v.id.clone()).collect::<Vec<_>>();
        assert_eq!(here(&page), ["all", "repo", "attached", "seven"]);
        assert_eq!(here(&at(Some(8))), ["all", "repo"], "another feature is open");
        assert_eq!(here(&at(Some(7))), ["all", "repo", "attached", "seven"]);
    }

    fn linking(id: &str, to: &[&str]) -> SavedView {
        let mut view = saved(id, ViewScope::Global);
        view.view.blocks = to.iter().map(|t| argus_protocol::Block::Embed { view: t.to_string() }).collect();
        view
    }

    fn under(views: &SavedViews) -> Vec<(String, Option<String>)> {
        let page = views.page(Whereabouts::default(), &[], &[], true);
        page.views.into_iter().map(|v| (v.id, v.under)).collect()
    }

    #[test]
    fn a_view_another_links_to_is_listed_under_the_first_that_does() {
        let mut views = SavedViews::default();
        views.receive_all(vec![linking("a", &["b", "c"]), linking("b", &["c"]), linking("c", &[]), linking("d", &["d"])]);
        let s = |x: &str| Some(x.to_string());
        assert_eq!(
            under(&views),
            [("a".into(), None), ("b".into(), s("a")), ("c".into(), s("a")), ("d".into(), None)],
            "a link to itself nests nothing"
        );
    }

    #[test]
    fn views_that_only_reach_each_other_keep_the_first_at_the_top() {
        let mut views = SavedViews::default();
        views.receive_all(vec![linking("a", &["b"]), linking("b", &["c"]), linking("c", &["a"])]);
        let s = |x: &str| Some(x.to_string());
        assert_eq!(under(&views), [("a".into(), None), ("b".into(), s("a")), ("c".into(), s("b"))]);
    }

    #[test]
    fn a_view_is_not_nested_under_one_of_another_scope() {
        let mut views = SavedViews::default();
        let mut sub = linking("sub", &[]);
        sub.scope = ViewScope::Repository { repository: RepositoryId(99) };
        views.receive_all(vec![linking("top", &["sub"]), sub]);
        assert_eq!(under(&views), [("top".into(), None), ("sub".into(), None)]);
    }

    #[test]
    fn the_page_hears_of_saved_views_only_once_there_have_been_some() {
        let mut views = SavedViews::default();
        assert_eq!(views.take(Whereabouts::default(), &[], &[], false), None);
        views.changed(saved("a", ViewScope::Global));
        assert_eq!(views.take(Whereabouts::default(), &[], &[], false).unwrap().views.len(), 1);
        assert_eq!(views.take(Whereabouts::default(), &[], &[], false), None);
        views.removed("a");
        assert_eq!(views.take(Whereabouts::default(), &[], &[], false), Some(SavedViewsPage::default()));
    }

    #[test]
    fn a_saved_view_names_the_panel_it_opens_in() {
        let views = held(&["plan"]);
        let page = views.page(Whereabouts::default(), &[], &[], true);
        assert_eq!(page.views[0].panel, Panel::Saved("plan".into()), "the sidebar's drag moves the panel the docks know");
    }
}
