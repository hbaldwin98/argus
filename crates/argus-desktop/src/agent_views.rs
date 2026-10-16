//! Which views agents showed, as this window holds them, and the one their
//! panel shows.
//!
//! The daemon owns the views: a person closing one or asking for a refresh
//! sends the request and changes nothing here until the daemon says it
//! changed. Only which one is chosen, and that a refresh is pending, are
//! the window's own; whether the panel is on screen is `docks`'s, and
//! whether a view is open as a page is `stage`'s.

use std::collections::{HashMap, HashSet};

use argus_protocol::{ClientMsg, PaneId, ProjectInfo, RepositoryId, ShownView, ViewPress};

use argus_protocol::flow::{self, FlowRow};
use argus_protocol::Block;

use crate::page::{FeatureViewLink, FlowLine, Link, PageDiagram, PageParticipant, PageView, ViewRef, ViewsPage};
use crate::sent::Sent;
use crate::view_changes::ViewChanges;
use crate::view_refs;

#[derive(Default)]
pub struct AgentViews {
    /// Newest first.
    views: Vec<ShownView>,
    /// The view the panel shows; the newest while none is chosen.
    open: Option<(PaneId, String)>,
    refreshing: HashSet<(PaneId, String)>,
    /// Action ids pressed in a view and not answered by its being shown again.
    pressed: HashMap<(PaneId, String), Vec<String>>,
    changes: ViewChanges,
    sent: Sent<ViewsPage>,
}

impl AgentViews {
    /// A new connection replays every view the daemon holds, so what this
    /// window had is dropped rather than doubled.
    pub fn connected(&mut self, out: &mut Vec<ClientMsg>) {
        self.views.clear();
        self.refreshing.clear();
        self.pressed.clear();
        self.changes.reset();
        out.push(ClientMsg::WatchViews);
    }

    /// A view came. It comes out by itself, saying so, only when its pane
    /// is the one on screen and the panel is not in front of the person
    /// already (out in its dock, or its view open as the page): the person
    /// is watching that agent, and anywhere else it would take the panel
    /// from under them.
    pub fn shown(&mut self, view: ShownView, on_screen: Option<PaneId>, out: bool) -> bool {
        let key = (view.pane, view.view.id.clone());
        self.refreshing.remove(&key);
        self.pressed.remove(&key);
        self.changes.end_flashes();
        let before = self.find(key.0, &key.1).map(|v| v.view.blocks.clone());
        self.views.retain(|v| (v.pane, v.view.id.as_str()) != (key.0, key.1.as_str()));
        let comes_out = !out && on_screen == Some(view.pane);
        if comes_out {
            self.open = Some(key.clone());
        }
        self.views.insert(0, view);
        // Seen as it changes when the panel is in front of the person, or
        // comes out for them, on this very view; anywhere else it waits
        // to be opened.
        let seen = (out || comes_out) && self.displayed().as_ref() == Some(&key);
        let after = &self.views[0].view.blocks;
        self.changes.record(key, before.as_deref(), after, seen);
        comes_out
    }

    /// The view the panel shows.
    fn displayed(&self) -> Option<(PaneId, String)> {
        self.open.clone().or_else(|| self.views.first().map(|v| (v.pane, v.view.id.clone())))
    }

    /// A view closed.
    pub fn closed(&mut self, pane: PaneId, id: &str) {
        self.views.retain(|v| (v.pane, v.view.id.as_str()) != (pane, id));
        let key = (pane, id.to_string());
        self.refreshing.remove(&key);
        self.pressed.remove(&key);
        self.changes.forget(&key);
        if self.open.as_ref().is_some_and(|(p, i)| (*p, i.as_str()) == (pane, id)) {
            self.open = None;
        }
    }

    /// Chooses a view, saying whether the window holds it.
    pub fn open(&mut self, pane: PaneId, id: String) -> bool {
        let held = self.find(pane, &id).is_some();
        if held {
            self.changes.end_flashes();
            self.open = Some((pane, id));
        }
        held
    }

    /// Chooses a view to open as a page, saying whether the window holds
    /// it; the panel's tab follows it, as the dock it goes back to.
    pub fn pop_out(&mut self, pane: PaneId, id: String) -> bool {
        let held = self.find(pane, &id).is_some();
        if held {
            self.open = Some((pane, id));
        }
        held
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

    /// Sends a press and holds its action as pending until the agent shows
    /// the view again: the page disables the button meanwhile, and the view
    /// itself is left as it was.
    pub fn press(&mut self, pane: PaneId, press: ViewPress, out: &mut Vec<ClientMsg>) {
        if self.find(pane, &press.view).is_some() {
            let pending = self.pressed.entry((pane, press.view.clone())).or_default();
            if !pending.contains(&press.action) {
                pending.push(press.action.clone());
            }
            out.push(ClientMsg::PressView { pane, press });
        }
    }

    /// The daemon could not take a press: the action is pending no longer,
    /// since no agent will answer it by showing the view again. Gives the
    /// sentence to say, led by the view's title when this window holds it.
    pub fn press_failed(&mut self, pane: PaneId, view: &str, action: &str, reason: &str) -> String {
        let key = (pane, view.to_string());
        if let Some(pending) = self.pressed.get_mut(&key) {
            pending.retain(|a| a != action);
            if pending.is_empty() {
                self.pressed.remove(&key);
            }
        }
        match self.find(pane, view) {
            Some(held) => format!("{}: {reason}", held.view.title),
            None => reason.to_string(),
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

    /// The title a view the window holds goes by.
    pub fn title(&self, pane: PaneId, id: &str) -> Option<&str> {
        self.find(pane, id).map(|v| v.view.title.as_str())
    }

    /// What to send the page, if it has not heard it. `title` names a
    /// pane by its id, for "read by …". `out` is whether the panel is in
    /// front of the person, out in its dock or its view open as the page:
    /// the view it shows is seen then, and what changed in it
    /// while it was not is flashed once instead of waiting to be opened.
    /// `popped` is the view `stage` has open as a page.
    pub fn take(
        &mut self,
        out: bool,
        can_press: bool,
        popped: Option<(PaneId, &str)>,
        title: impl Fn(PaneId) -> Option<String>,
    ) -> Option<ViewsPage> {
        if out {
            if let Some(key) = self.displayed() {
                self.changes.reveal(&key);
            }
        }
        // An empty page is the default, which `Sent` says only once views
        // have been and gone, so a window no agent showed one in hears none;
        // what the daemon can do alone does not make a page worth sending.
        let popped = popped.map(|(pane, id)| ViewRef { pane: pane.0, id: id.to_string() });
        let page = ViewsPage { can_press, popped, ..self.page(title) };
        let empty = ViewsPage { can_press: false, ..page.clone() } == ViewsPage::default();
        self.sent.take((!empty).then_some(page))
    }

    pub fn forget(&mut self) {
        self.sent.forget();
    }

    fn page(&self, title: impl Fn(PaneId) -> Option<String>) -> ViewsPage {
        ViewsPage {
            views: self
                .views
                .iter()
                .map(|v| {
                    let key = (v.pane, v.view.id.clone());
                    let (changed, unseen) = self.changes.marks(&key);
                    PageView {
                        pane: v.pane.0,
                        from: title(v.pane).unwrap_or_default(),
                        at: v.at,
                        refreshing: self.refreshing.contains(&key),
                        pressed: self.pressed.get(&key).cloned().unwrap_or_default(),
                        subject: v.subject.as_ref().map(Link::of),
                        view: v.view.clone(),
                        diagrams: diagrams(&v.view.blocks),
                        changed,
                        unseen,
                    }
                })
                .collect(),
            open: self
                .displayed()
                .map(|(pane, id)| ViewRef { pane: pane.0, id }),
            // Which is a page is `stage`'s, which `take` is told.
            popped: None,
            can_press: false,
        }
    }
}

/// Every diagram block a view holds, however deep, read once each.
pub(crate) fn diagrams(blocks: &[Block]) -> Vec<PageDiagram> {
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
        assert_eq!(views.take(false, false, None, |_| None), None);
        views.shown(shown(1, "ci"), None, false);
        assert_eq!(views.take(false, false, None, |_| None).unwrap().views.len(), 1);
        views.closed(PaneId(1), "ci");
        assert_eq!(views.take(false, false, None, |_| None), Some(ViewsPage::default()));
        assert_eq!(views.take(false, false, None, |_| None), None);
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
    fn a_view_popped_out_is_chosen_for_the_panel_only_if_the_window_holds_it() {
        let mut views = AgentViews::default();
        views.shown(shown(1, "a"), None, false);
        views.shown(shown(1, "b"), None, false);
        assert!(!views.pop_out(PaneId(1), "gone".into()), "only a view the window holds");
        assert!(views.pop_out(PaneId(1), "a".into()));
        let a = Some(ViewRef { pane: 1, id: "a".into() });
        assert_eq!(views.page(|_| None).open, a, "the dock's tab follows it");
        assert_eq!(views.take(false, false, Some((PaneId(1), "a")), |_| None).unwrap().popped, a);
        assert_eq!(views.take(false, false, None, |_| None).unwrap().popped, None);
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

    fn press(view: &str, action: &str) -> ViewPress {
        ViewPress { view: view.into(), action: action.into(), rows: vec![], form: None, answers: vec![] }
    }

    #[test]
    fn a_press_the_daemon_could_not_take_is_no_longer_pending_and_names_its_view() {
        let mut views = AgentViews::default();
        views.shown(shown(1, "ci"), None, false);
        let mut out = Vec::new();
        views.press(PaneId(1), press("ci", "retry"), &mut out);
        views.press(PaneId(1), press("ci", "stop"), &mut out);
        let said = views.press_failed(PaneId(1), "ci", "retry", "the agent has gone");
        assert_eq!(said, "ci: the agent has gone");
        assert_eq!(views.page(|_| None).views[0].pressed, ["stop"], "only that action is released");
        views.press_failed(PaneId(1), "ci", "stop", "x");
        assert!(views.pressed.is_empty());
        assert_eq!(views.press_failed(PaneId(1), "gone", "retry", "no such view"), "no such view");
    }

    #[test]
    fn the_title_a_view_goes_by_is_the_one_its_agent_last_gave_it() {
        let mut views = AgentViews::default();
        views.shown(shown(1, "ci"), None, false);
        assert_eq!(views.title(PaneId(1), "ci"), Some("ci"));
        let mut view = shown(1, "ci");
        view.view.title = "Build status".into();
        views.shown(view, None, false);
        assert_eq!(views.title(PaneId(1), "ci"), Some("Build status"));
        views.closed(PaneId(1), "ci");
        assert_eq!(views.title(PaneId(1), "ci"), None);
    }

    #[test]
    fn the_page_says_whether_buttons_can_be_pressed_without_that_alone_making_it_worth_sending() {
        let mut views = AgentViews::default();
        assert!(views.take(false, true, None, |_| None).is_none(), "no views, nothing to hear");
        views.shown(shown(1, "ci"), None, false);
        assert!(views.take(false, false, None, |_| None).is_some_and(|p| !p.can_press));
        assert!(views.take(false, true, None, |_| None).is_some_and(|p| p.can_press), "a daemon upgrade is heard");
    }

    #[test]
    fn a_pressed_action_is_pending_until_its_view_is_shown_again_or_closed_or_reconnected() {
        let mut views = AgentViews::default();
        views.shown(shown(1, "ci"), None, false);
        let mut out = Vec::new();
        views.press(PaneId(1), press("ci", "retry"), &mut out);
        views.press(PaneId(1), press("ci", "retry"), &mut out);
        views.press(PaneId(1), press("ci", "stop"), &mut out);
        assert_eq!(out.len(), 3, "each press is sent");
        assert!(matches!(&out[0], ClientMsg::PressView { pane: PaneId(1), press } if press.action == "retry"));
        assert_eq!(views.page(|_| None).views[0].pressed, ["retry", "stop"]);
        views.shown(shown(1, "ci"), None, false);
        assert!(views.page(|_| None).views[0].pressed.is_empty());

        views.press(PaneId(1), press("ci", "retry"), &mut out);
        views.closed(PaneId(1), "ci");
        views.shown(shown(1, "ci"), None, false);
        assert!(views.page(|_| None).views[0].pressed.is_empty());

        views.press(PaneId(1), press("ci", "retry"), &mut out);
        views.connected(&mut out);
        views.shown(shown(1, "ci"), None, false);
        assert!(views.pressed.is_empty());
    }

    #[test]
    fn nothing_is_sent_for_a_press_in_a_view_this_window_does_not_hold() {
        let mut views = AgentViews::default();
        let mut out = Vec::new();
        views.press(PaneId(1), press("gone", "retry"), &mut out);
        assert!(out.is_empty());
        assert!(views.pressed.is_empty());
    }

    fn md(text: &str) -> Block {
        Block::Markdown { text: text.into() }
    }

    fn with(pane: u64, id: &str, blocks: Vec<Block>) -> ShownView {
        let mut view = shown(pane, id);
        view.view.blocks = blocks;
        view
    }

    #[test]
    fn a_view_changed_while_it_is_in_front_of_the_person_flashes_and_is_not_unseen() {
        let mut views = AgentViews::default();
        views.shown(with(1, "ci", vec![md("a"), md("b")]), Some(PaneId(1)), true);
        views.shown(with(1, "ci", vec![md("a"), md("B")]), Some(PaneId(1)), true);
        let page = views.page(|_| None);
        assert_eq!(page.views[0].changed, ["1"]);
        assert!(!page.views[0].unseen);
        // The next thing that happens to a view ends the flash.
        views.shown(with(1, "ci", vec![md("a"), md("B")]), Some(PaneId(1)), true);
        assert!(views.page(|_| None).views[0].changed.is_empty());
    }

    #[test]
    fn a_view_changed_out_of_sight_stays_marked_until_it_is_opened() {
        let mut views = AgentViews::default();
        views.shown(with(1, "ci", vec![md("a"), md("b")]), None, false);
        views.shown(with(1, "other", vec![md("z")]), None, false);
        views.shown(with(1, "ci", vec![md("A"), md("b")]), None, false);
        views.shown(with(1, "ci", vec![md("A"), md("B")]), None, false);
        let page = views.page(|_| None);
        let ci = page.views.iter().find(|v| v.view.id == "ci").unwrap();
        assert_eq!(ci.changed, ["0", "1"], "marks from successive changes are kept");
        assert!(ci.unseen);
        // The panel coming out onto another view does not clear it.
        views.open(PaneId(1), "other".into());
        views.take(true, true, None, |_| None);
        let page = views.page(|_| None);
        assert!(page.views.iter().find(|v| v.view.id == "ci").unwrap().unseen);
        // Opening it is seeing it: the marks flash once, and it is no longer unseen.
        assert!(views.open(PaneId(1), "ci".into()));
        let page = views.take(true, true, None, |_| None).unwrap();
        let ci = page.views.iter().find(|v| v.view.id == "ci").unwrap();
        assert_eq!(ci.changed, ["0", "1"]);
        assert!(!ci.unseen);
        views.open(PaneId(1), "other".into());
        let page = views.take(true, true, None, |_| None).unwrap();
        assert!(page.views.iter().find(|v| v.view.id == "ci").unwrap().changed.is_empty());
    }

    #[test]
    fn a_panel_that_comes_out_by_itself_onto_a_changed_view_flashes_it() {
        let mut views = AgentViews::default();
        views.shown(with(1, "ci", vec![md("a")]), None, false);
        assert!(views.shown(with(1, "ci", vec![md("b")]), Some(PaneId(1)), false));
        let page = views.page(|_| None);
        assert_eq!(page.views[0].changed, ["0"]);
        assert!(!page.views[0].unseen);
    }

    #[test]
    fn a_view_first_shown_or_shown_unchanged_is_not_marked() {
        let mut views = AgentViews::default();
        views.shown(with(1, "ci", vec![md("a")]), None, false);
        views.shown(with(1, "ci", vec![md("a")]), None, false);
        let page = views.page(|_| None);
        assert!(page.views[0].changed.is_empty() && !page.views[0].unseen);
    }

    #[test]
    fn closing_a_view_forgets_what_was_unseen_in_it() {
        let mut views = AgentViews::default();
        views.shown(with(1, "ci", vec![md("a")]), None, false);
        views.shown(with(1, "ci", vec![md("b")]), None, false);
        views.closed(PaneId(1), "ci");
        views.shown(with(1, "ci", vec![md("b")]), None, false);
        assert!(!views.page(|_| None).views[0].unseen);
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
