//! Which views agents showed, as this window holds them, and the one their
//! panel shows.
//!
//! The daemon owns the views: a person closing one or asking for a refresh
//! sends the request and changes nothing here until the daemon says it
//! changed. Only which one is chosen, and that a refresh is pending, are
//! the window's own; whether the panel is on screen is `docks`'s.

use std::collections::HashSet;

use argus_protocol::{ClientMsg, PaneId, ProjectInfo, RepositoryId, ShownView};

use argus_protocol::flow::{self, FlowRow};
use argus_protocol::Block;

use crate::page::{FeatureViewLink, FlowLine, Link, PageDiagram, PageParticipant, PageView, ViewRef, ViewsPage};
use crate::sent::Sent;
use crate::view_refs;

#[derive(Default)]
pub struct AgentViews {
    /// Newest first.
    views: Vec<ShownView>,
    /// The view the panel shows; the newest while none is chosen.
    open: Option<(PaneId, String)>,
    /// The view open as a page. It is the window's to hold, as the stage
    /// is, and ends when the view closes.
    popped: Option<(PaneId, String)>,
    refreshing: HashSet<(PaneId, String)>,
    sent: Sent<ViewsPage>,
}

impl AgentViews {
    /// A new connection replays every view the daemon holds, so what this
    /// window had is dropped rather than doubled.
    pub fn connected(&mut self, out: &mut Vec<ClientMsg>) {
        self.views.clear();
        self.refreshing.clear();
        self.popped = None;
        out.push(ClientMsg::WatchViews);
    }

    /// A view came. It comes out by itself, saying so, only when its pane
    /// is the one on screen and the panel is not out already: the person is
    /// watching that agent, and anywhere else it would take the panel from
    /// under them.
    pub fn shown(&mut self, view: ShownView, on_screen: Option<PaneId>, out: bool) -> bool {
        let key = (view.pane, view.view.id.clone());
        self.refreshing.remove(&key);
        self.views.retain(|v| (v.pane, v.view.id.as_str()) != (key.0, key.1.as_str()));
        let comes_out = !out && on_screen == Some(view.pane);
        if comes_out {
            self.open = Some(key);
        }
        self.views.insert(0, view);
        comes_out
    }

    /// A view closed, saying whether it was the one open as a page.
    pub fn closed(&mut self, pane: PaneId, id: &str) -> bool {
        self.views.retain(|v| (v.pane, v.view.id.as_str()) != (pane, id));
        self.refreshing.remove(&(pane, id.to_string()));
        if self.open.as_ref().is_some_and(|(p, i)| (*p, i.as_str()) == (pane, id)) {
            self.open = None;
        }
        let was_popped = self.popped.as_ref().is_some_and(|(p, i)| (*p, i.as_str()) == (pane, id));
        if was_popped {
            self.popped = None;
        }
        was_popped
    }

    /// Chooses a view, saying whether the window holds it. The page it was
    /// open as, if it was, is over: this is the way back to the dock.
    pub fn open(&mut self, pane: PaneId, id: String) -> bool {
        let held = self.find(pane, &id).is_some();
        if held {
            self.open = Some((pane, id));
            self.popped = None;
        }
        held
    }

    /// Opens a view as a page, saying whether the window holds it.
    pub fn pop_out(&mut self, pane: PaneId, id: String) -> bool {
        let held = self.find(pane, &id).is_some();
        if held {
            self.open = Some((pane, id.clone()));
            self.popped = Some((pane, id));
        }
        held
    }

    pub fn popped(&self) -> bool {
        self.popped.is_some()
    }

    /// The page is over, because the person went to another stage.
    pub fn end_pop_out(&mut self) {
        self.popped = None;
    }

    /// The views that name a feature of `repository`, each with the number
    /// it names, newest first. A view names a feature of the repository its
    /// pane is in; nothing about the link is kept anywhere else.
    pub fn linked(&self, tree: &[ProjectInfo], repository: RepositoryId) -> Vec<(i64, FeatureViewLink)> {
        self.views
            .iter()
            .filter_map(|v| {
                let number = v.view.feature?;
                let (repo, _) = view_refs::locate(tree, v.pane)?;
                (repo.id == repository).then(|| (number, FeatureViewLink { pane: v.pane.0, id: v.view.id.clone(), title: v.view.title.clone() }))
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// How many views a pane's agent has up, for its row on the rail.
    pub fn count(&self, pane: PaneId) -> usize {
        self.views.iter().filter(|v| v.pane == pane).count()
    }

    pub fn refresh(&mut self, pane: PaneId, id: String, out: &mut Vec<ClientMsg>) {
        if self.find(pane, &id).is_some() {
            out.push(ClientMsg::RefreshView { pane, id: id.clone() });
            self.refreshing.insert((pane, id));
        }
    }

    pub fn close(&mut self, pane: PaneId, id: String, out: &mut Vec<ClientMsg>) {
        if self.find(pane, &id).is_some() {
            out.push(ClientMsg::CloseView { pane, id });
        }
    }

    fn find(&self, pane: PaneId, id: &str) -> Option<&ShownView> {
        self.views.iter().find(|v| v.pane == pane && v.view.id == id)
    }

    /// What to send the page, if it has not heard it. `title` names a
    /// pane by its id, for "read by …".
    pub fn take(&mut self, title: impl Fn(PaneId) -> Option<String>) -> Option<ViewsPage> {
        // An empty page is the default, which `Sent` says only once views
        // have been and gone, so a window no agent showed one in hears none.
        let page = self.page(title);
        self.sent.take((page != ViewsPage::default()).then_some(page))
    }

    pub fn forget(&mut self) {
        self.sent.forget();
    }

    fn page(&self, title: impl Fn(PaneId) -> Option<String>) -> ViewsPage {
        ViewsPage {
            views: self
                .views
                .iter()
                .map(|v| PageView {
                    pane: v.pane.0,
                    from: title(v.pane).unwrap_or_default(),
                    at: v.at,
                    refreshing: self.refreshing.contains(&(v.pane, v.view.id.clone())),
                    subject: v.subject.as_ref().map(Link::of),
                    view: v.view.clone(),
                    diagrams: diagrams(&v.view.blocks),
                })
                .collect(),
            open: self
                .open
                .clone()
                .or_else(|| self.views.first().map(|v| (v.pane, v.view.id.clone())))
                .map(|(pane, id)| ViewRef { pane: pane.0, id }),
            popped: self.popped.clone().map(|(pane, id)| ViewRef { pane: pane.0, id }),
        }
    }
}

/// Every diagram block a view holds, however deep, read once each.
fn diagrams(blocks: &[Block]) -> Vec<PageDiagram> {
    let mut found = Vec::new();
    collect(blocks, &mut found);
    found
}

fn collect(blocks: &[Block], found: &mut Vec<PageDiagram>) {
    for block in blocks {
        if let Block::Diagram { source } = block {
            if !found.iter().any(|d| &d.source == source) {
                found.push(read(source));
            }
        }
        block.nested().into_iter().for_each(|inner| collect(inner, found));
    }
}

fn read(source: &str) -> PageDiagram {
    PageDiagram {
        source: source.to_string(),
        participants: flow::participants(source)
            .into_iter()
            .map(|p| PageParticipant { id: p.id, label: p.label, actor: p.actor })
            .collect(),
        rows: flow::rows(source)
            .into_iter()
            .map(|row| match row {
                FlowRow::Step(s) => FlowLine::Step { number: s.number, from: s.from, to: s.to, reply: s.reply, text: s.text },
                FlowRow::Branch { kind, when } => FlowLine::Branch { word: kind, when },
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::AgentView;

    fn shown(pane: u64, id: &str) -> ShownView {
        ShownView {
            pane: PaneId(pane),
            at: 1,
            subject: None,
            view: AgentView { id: id.into(), title: id.into(), ..Default::default() },
        }
    }

    fn ids(views: &mut AgentViews) -> Vec<String> {
        views.page(|_| None).views.into_iter().map(|v| v.view.id).collect()
    }

    #[test]
    fn the_page_hears_of_views_only_once_there_have_been_some() {
        let mut views = AgentViews::default();
        assert_eq!(views.take(|_| None), None);
        views.shown(shown(1, "ci"), None, false);
        assert_eq!(views.take(|_| None).unwrap().views.len(), 1);
        views.closed(PaneId(1), "ci");
        assert_eq!(views.take(|_| None), Some(ViewsPage::default()));
        assert_eq!(views.take(|_| None), None);
    }

    #[test]
    fn a_view_from_the_pane_on_screen_comes_out_and_one_from_elsewhere_waits() {
        let mut views = AgentViews::default();
        assert!(!views.shown(shown(2, "elsewhere"), Some(PaneId(1)), false));
        assert!(views.shown(shown(1, "ci"), Some(PaneId(1)), false));
        assert_eq!(views.page(|_| None).open, Some(ViewRef { pane: 1, id: "ci".into() }));
        // A second, the panel out, does not take it from the first.
        assert!(!views.shown(shown(1, "queries"), Some(PaneId(1)), true));
        assert_eq!(views.page(|_| None).open, Some(ViewRef { pane: 1, id: "ci".into() }));
    }

    #[test]
    fn a_diagram_anywhere_in_a_view_reaches_the_page_read() {
        let mut view = shown(1, "flow");
        let source = "sequenceDiagram\n    participant hook as argus-hook\n    hook->>argusd: view show\n    argusd-->>hook: shown".to_string();
        view.view.blocks = vec![Block::Tabs {
            tabs: vec![argus_protocol::Tab { title: "Flow".into(), blocks: vec![Block::Diagram { source: source.clone() }] }],
        }];
        let mut views = AgentViews::default();
        views.shown(view, None, false);
        let page = views.page(|_| None);
        let drawn = &page.views[0].diagrams[0];
        assert_eq!(drawn.source, source);
        assert_eq!(drawn.participants.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(), ["argus-hook", "argusd"]);
        assert!(matches!(&drawn.rows[1], FlowLine::Step { reply: true, number: 2, .. }));
    }

    #[test]
    fn a_view_popped_out_is_a_page_until_it_goes_back_to_the_dock_or_closes() {
        let mut views = AgentViews::default();
        views.shown(shown(1, "a"), None, false);
        views.shown(shown(1, "b"), None, false);
        assert!(!views.pop_out(PaneId(1), "gone".into()), "only a view the window holds");
        assert!(views.pop_out(PaneId(1), "a".into()));
        let page = views.page(|_| None);
        assert_eq!(page.popped, Some(ViewRef { pane: 1, id: "a".into() }));
        assert_eq!(page.open, page.popped, "the dock's tab follows it");
        assert!(views.open(PaneId(1), "a".into()));
        assert_eq!(views.page(|_| None).popped, None, "opening it in the dock ends the page");
        views.pop_out(PaneId(1), "b".into());
        assert!(views.closed(PaneId(1), "b"), "closing says it was the page");
        assert!(!views.popped());
    }

    #[test]
    fn the_views_naming_a_feature_are_those_of_panes_in_its_repository() {
        use crate::fixtures::{checkout, pane, project, repository};
        use argus_protocol::{PaneKind, PaneStatus};
        let agent = |id| pane(id, PaneKind::Agent, "claude", PaneStatus::Idle);
        let tree = vec![project(1, "src", vec![
            repository(10, "argus", None, vec![checkout(100, "main", true, vec![agent(1)]), checkout(101, "work", false, vec![agent(2)])]),
            repository(11, "other", None, vec![checkout(110, "main", true, vec![agent(3)])]),
        ])];
        let mut views = AgentViews::default();
        let mut name = |pane, id: &str, feature| {
            let mut view = shown(pane, id);
            view.view.feature = feature;
            views.shown(view, None, false);
        };
        name(1, "plan", Some(7));
        name(2, "from-a-worktree", Some(7));
        name(3, "other-repository", Some(7));
        name(1, "unlinked", None);
        let linked = views.linked(&tree, RepositoryId(10));
        let ids: Vec<_> = linked.iter().map(|(n, v)| (*n, v.id.as_str())).collect();
        assert_eq!(ids, [(7, "from-a-worktree"), (7, "plan")], "newest first, every checkout of the repository");
    }

    #[test]
    fn the_newest_is_shown_until_one_is_chosen_and_after_the_one_chosen_closes() {
        let mut views = AgentViews::default();
        assert_eq!(views.page(|_| None).open, None);
        views.shown(shown(1, "old"), None, false);
        views.shown(shown(2, "new"), None, false);
        assert_eq!(views.page(|_| None).open, Some(ViewRef { pane: 2, id: "new".into() }));
        assert!(views.open(PaneId(1), "old".into()));
        assert_eq!(views.page(|_| None).open, Some(ViewRef { pane: 1, id: "old".into() }));
        assert_eq!(views.count(PaneId(1)), 1);
        views.closed(PaneId(1), "old");
        assert_eq!(views.page(|_| None).open, Some(ViewRef { pane: 2, id: "new".into() }));
    }

    #[test]
    fn a_view_shown_again_replaces_its_old_self_at_the_top() {
        let mut views = AgentViews::default();
        views.shown(shown(1, "a"), None, false);
        views.shown(shown(1, "b"), None, false);
        views.shown(shown(1, "a"), None, false);
        assert_eq!(ids(&mut views), ["a", "b"]);
    }

    #[test]
    fn closing_asks_the_daemon_and_waits_for_it() {
        let mut views = AgentViews::default();
        views.shown(shown(1, "ci"), Some(PaneId(1)), false);
        let mut out = Vec::new();
        views.close(PaneId(1), "ci".into(), &mut out);
        assert!(matches!(&out[..], [ClientMsg::CloseView { pane: PaneId(1), id }] if id == "ci"));
        assert_eq!(ids(&mut views), ["ci"]);
        views.closed(PaneId(1), "ci");
        let page = views.page(|_| None);
        assert!(page.views.is_empty());
        assert_eq!(page.open, None);
    }

    #[test]
    fn a_refresh_is_pending_until_the_view_comes_again() {
        let mut views = AgentViews::default();
        views.shown(shown(1, "ci"), None, false);
        let mut out = Vec::new();
        views.refresh(PaneId(1), "ci".into(), &mut out);
        assert!(matches!(&out[..], [ClientMsg::RefreshView { .. }]));
        assert!(views.page(|_| None).views[0].refreshing);
        views.shown(shown(1, "ci"), None, false);
        assert!(!views.page(|_| None).views[0].refreshing);
    }

    #[test]
    fn a_reconnect_watches_again_and_drops_what_the_daemon_will_replay() {
        let mut views = AgentViews::default();
        views.shown(shown(1, "ci"), None, false);
        let mut out = Vec::new();
        views.connected(&mut out);
        assert!(matches!(&out[..], [ClientMsg::WatchViews]));
        assert!(ids(&mut views).is_empty());
    }

    #[test]
    fn nothing_is_asked_about_a_view_this_window_does_not_hold() {
        let mut views = AgentViews::default();
        let mut out = Vec::new();
        views.refresh(PaneId(1), "gone".into(), &mut out);
        views.close(PaneId(1), "gone".into(), &mut out);
        assert!(!views.open(PaneId(1), "gone".into()));
        assert!(out.is_empty());
        assert_eq!(views.page(|_| None).open, None);
    }
}
