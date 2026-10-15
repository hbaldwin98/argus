//! Which views agents showed, as this window holds them, and the one their
//! panel shows.
//!
//! The daemon owns the views: a person closing one or asking for a refresh
//! sends the request and changes nothing here until the daemon says it
//! changed. Only which one is chosen, and that a refresh is pending, are
//! the window's own; whether the panel is on screen is `docks`'s.

use std::collections::{BTreeSet, HashMap, HashSet};

use argus_protocol::{ClientMsg, PaneId, ShownView, ViewPress};

use argus_protocol::flow::{self, FlowRow};
use argus_protocol::Block;

use crate::page::{FlowLine, Link, PageDiagram, PageParticipant, PageView, ViewRef, ViewsPage};
use crate::sent::Sent;

#[derive(Default)]
pub struct AgentViews {
    /// Newest first.
    views: Vec<ShownView>,
    /// The view the panel shows; the newest while none is chosen.
    open: Option<(PaneId, String)>,
    refreshing: HashSet<(PaneId, String)>,
    /// Action ids pressed in a view and not answered by its being shown again.
    pressed: HashMap<(PaneId, String), Vec<String>>,
    /// The blocks, by path, that changed in a view since the person last
    /// had it in front of them. Kept until they open the view.
    unseen: HashMap<(PaneId, String), BTreeSet<String>>,
    /// What changed in a view that was on screen as it did, or that has
    /// just been opened onto: sent so the page can flash it once. Kept
    /// until the next thing happens to a view, not until it is sent, so a
    /// page redrawn a moment later does not lose the flash before it ran.
    flashing: HashMap<(PaneId, String), BTreeSet<String>>,
    sent: Sent<ViewsPage>,
}

impl AgentViews {
    /// A new connection replays every view the daemon holds, so what this
    /// window had is dropped rather than doubled.
    pub fn connected(&mut self, out: &mut Vec<ClientMsg>) {
        self.views.clear();
        self.refreshing.clear();
        self.pressed.clear();
        self.unseen.clear();
        self.flashing.clear();
        out.push(ClientMsg::WatchViews);
    }

    /// A view came. It comes out by itself, saying so, only when its pane
    /// is the one on screen and the panel is not out already: the person is
    /// watching that agent, and anywhere else it would take the panel from
    /// under them.
    pub fn shown(&mut self, view: ShownView, on_screen: Option<PaneId>, out: bool) -> bool {
        let key = (view.pane, view.view.id.clone());
        self.refreshing.remove(&key);
        self.pressed.remove(&key);
        self.flashing.clear();
        let before = self.find(key.0, &key.1).map(|v| v.view.blocks.clone());
        self.views.retain(|v| (v.pane, v.view.id.as_str()) != (key.0, key.1.as_str()));
        let comes_out = !out && on_screen == Some(view.pane);
        if comes_out {
            self.open = Some(key.clone());
        }
        let changed = before.map(|before| changed_blocks(&before, &view.view.blocks)).unwrap_or_default();
        self.views.insert(0, view);
        // Seen as it changes when the panel is in front of the person, or
        // comes out for them, on this very view; anywhere else it waits
        // to be opened.
        if !changed.is_empty() {
            let seen = (out || comes_out) && self.displayed().as_ref() == Some(&key);
            let marks = if seen { &mut self.flashing } else { &mut self.unseen };
            marks.entry(key).or_default().extend(changed);
        }
        comes_out
    }

    /// The view the panel shows.
    fn displayed(&self) -> Option<(PaneId, String)> {
        self.open.clone().or_else(|| self.views.first().map(|v| (v.pane, v.view.id.clone())))
    }

    pub fn closed(&mut self, pane: PaneId, id: &str) {
        self.views.retain(|v| (v.pane, v.view.id.as_str()) != (pane, id));
        self.refreshing.remove(&(pane, id.to_string()));
        self.pressed.remove(&(pane, id.to_string()));
        self.unseen.remove(&(pane, id.to_string()));
        self.flashing.clear();
        if self.open.as_ref().is_some_and(|(p, i)| (*p, i.as_str()) == (pane, id)) {
            self.open = None;
        }
    }

    /// Chooses a view, saying whether the window holds it.
    pub fn open(&mut self, pane: PaneId, id: String) -> bool {
        let held = self.find(pane, &id).is_some();
        if held {
            self.flashing.clear();
            self.open = Some((pane, id));
        }
        held
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

    pub fn close(&mut self, pane: PaneId, id: String, out: &mut Vec<ClientMsg>) {
        if self.find(pane, &id).is_some() {
            out.push(ClientMsg::CloseView { pane, id });
        }
    }

    fn find(&self, pane: PaneId, id: &str) -> Option<&ShownView> {
        self.views.iter().find(|v| v.pane == pane && v.view.id == id)
    }

    /// What to send the page, if it has not heard it. `title` names a
    /// pane by its id, for "read by …". `out` is whether the panel is on
    /// screen: the view it shows is seen then, and what changed in it
    /// while it was not is flashed once instead of waiting to be opened.
    pub fn take(&mut self, out: bool, title: impl Fn(PaneId) -> Option<String>) -> Option<ViewsPage> {
        if out {
            if let Some(key) = self.displayed() {
                if let Some(marks) = self.unseen.remove(&key) {
                    self.flashing.entry(key).or_default().extend(marks);
                }
            }
        }
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
                .map(|v| {
                    let key = (v.pane, v.view.id.clone());
                    let unseen = self.unseen.get(&key);
                    let marked = unseen.into_iter().chain(self.flashing.get(&key)).flatten();
                    PageView {
                        pane: v.pane.0,
                        from: title(v.pane).unwrap_or_default(),
                        at: v.at,
                        refreshing: self.refreshing.contains(&key),
                        pressed: self.pressed.get(&key).cloned().unwrap_or_default(),
                        subject: v.subject.as_ref().map(Link::of),
                        view: v.view.clone(),
                        diagrams: diagrams(&v.view.blocks),
                        changed: marked.cloned().collect::<BTreeSet<_>>().into_iter().collect(),
                        unseen: unseen.is_some_and(|marks| !marks.is_empty()),
                    }
                })
                .collect(),
            open: self
                .displayed()
                .map(|(pane, id)| ViewRef { pane: pane.0, id }),
        }
    }
}

/// The paths, as the page names a block under its view (`2`, `1.0.3`), of
/// the blocks in `after` that are not as they were in `before`.
///
/// Only the innermost block that differs is named, so a section with one
/// changed line in it marks the line and not everything beside it. A
/// block whose kind changed, or whose container has a different shape (a
/// tab or column more or fewer), is named whole; so is one that was added.
/// A path is built as the page builds it: a section's, card's, details'
/// and grid's blocks follow its own path, and a column, a tab or an open
/// table row adds its index first.
fn changed_blocks(before: &[Block], after: &[Block]) -> Vec<String> {
    let mut found = Vec::new();
    diff_list(before, after, "", &mut found);
    found
}

fn diff_list(before: &[Block], after: &[Block], prefix: &str, found: &mut Vec<String>) {
    for (i, new) in after.iter().enumerate() {
        let path = if prefix.is_empty() { i.to_string() } else { format!("{prefix}.{i}") };
        match before.get(i) {
            Some(old) => diff_block(old, new, &path, found),
            None => found.push(path),
        }
    }
}

fn diff_block(old: &Block, new: &Block, path: &str, found: &mut Vec<String>) {
    if old == new {
        return;
    }
    let (inside_old, inside_new) = (inner_lists(old), inner_lists(new));
    let same_shape = inside_old.len() == inside_new.len()
        && inside_old.iter().zip(&inside_new).all(|(a, b)| a.0 == b.0)
        && shell(old) == shell(new);
    if !same_shape {
        found.push(path.to_string());
        return;
    }
    for ((_, a), (key, b)) in inside_old.iter().zip(&inside_new) {
        let at = if key.is_empty() { path.to_string() } else { format!("{path}.{key}") };
        diff_list(a, b, &at, found);
    }
}

/// The lists of blocks directly inside a block, each with the index the
/// page adds to the block's path before its own (none for a section's).
fn inner_lists(block: &Block) -> Vec<(String, &[Block])> {
    match block {
        Block::Section { blocks, .. } | Block::Card { blocks, .. } | Block::Details { blocks, .. } => {
            vec![(String::new(), blocks)]
        }
        Block::Grid { items, .. } => vec![(String::new(), items)],
        Block::Columns { columns } => columns.iter().enumerate().map(|(i, c)| (i.to_string(), c.as_slice())).collect(),
        Block::Tabs { tabs } => tabs.iter().enumerate().map(|(i, t)| (i.to_string(), t.blocks.as_slice())).collect(),
        Block::Table { details, .. } => details
            .iter()
            .enumerate()
            .filter_map(|(i, d)| d.as_ref().map(|d| (i.to_string(), d.as_slice())))
            .collect(),
        _ => Vec::new(),
    }
}

/// The block with the blocks inside it emptied, so that what is left is
/// the container's own content.
fn shell(block: &Block) -> Block {
    let mut bare = block.clone();
    match &mut bare {
        Block::Section { blocks, .. } | Block::Card { blocks, .. } | Block::Details { blocks, .. } => blocks.clear(),
        Block::Grid { items, .. } => items.clear(),
        Block::Columns { columns } => columns.iter_mut().for_each(Vec::clear),
        Block::Tabs { tabs } => tabs.iter_mut().for_each(|t| t.blocks.clear()),
        Block::Table { details, .. } => details.iter_mut().flatten().for_each(Vec::clear),
        _ => {}
    }
    bare
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
        assert_eq!(views.take(false, |_| None), None);
        views.shown(shown(1, "ci"), None, false);
        assert_eq!(views.take(false, |_| None).unwrap().views.len(), 1);
        views.closed(PaneId(1), "ci");
        assert_eq!(views.take(false, |_| None), Some(ViewsPage::default()));
        assert_eq!(views.take(false, |_| None), None);
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

    fn section(blocks: Vec<Block>) -> Block {
        Block::Section { heading: "s".into(), blocks }
    }

    fn changed(before: Vec<Block>, after: Vec<Block>) -> Vec<String> {
        changed_blocks(&before, &after)
    }

    #[test]
    fn only_the_blocks_whose_content_differs_are_named() {
        assert_eq!(changed(vec![md("a"), md("b")], vec![md("a"), md("B")]), ["1"]);
        assert!(changed(vec![md("a")], vec![md("a")]).is_empty());
    }

    #[test]
    fn a_nested_change_names_the_inner_block_and_not_its_containers() {
        let before = vec![md("x"), section(vec![md("a"), Block::Divider, md("c")])];
        let after = vec![md("x"), section(vec![md("a"), Block::Divider, md("C")])];
        assert_eq!(changed(before, after), ["1.2"]);
    }

    #[test]
    fn a_columns_and_a_tabs_paths_add_the_column_or_tab_first() {
        let columns = |b: &str| Block::Columns { columns: vec![vec![md("a")], vec![md("a"), md(b)]] };
        assert_eq!(changed(vec![columns("b")], vec![columns("B")]), ["0.1.1"]);
        let tabs = |b: &str| Block::Tabs {
            tabs: vec![
                argus_protocol::Tab { title: "one".into(), blocks: vec![md("a")] },
                argus_protocol::Tab { title: "two".into(), blocks: vec![md(b)] },
            ],
        };
        assert_eq!(changed(vec![md("x"), tabs("b")], vec![md("x"), tabs("B")]), ["1.1.0"]);
    }

    #[test]
    fn a_containers_own_change_names_the_container() {
        let with_heading = |h: &str| Block::Section { heading: h.into(), blocks: vec![md("a")] };
        assert_eq!(changed(vec![with_heading("old")], vec![with_heading("new")]), ["0"]);
    }

    #[test]
    fn a_block_added_or_of_another_kind_is_named_whole() {
        assert_eq!(changed(vec![md("a")], vec![md("a"), md("b")]), ["1"]);
        assert_eq!(changed(vec![md("a")], vec![Block::Divider]), ["0"]);
        assert_eq!(changed(vec![section(vec![md("a")])], vec![section(vec![md("a"), md("b")])]), ["0.1"]);
        let columns = |n: usize| Block::Columns { columns: vec![vec![md("a")]; n] };
        assert_eq!(changed(vec![columns(1)], vec![columns(2)]), ["0"]);
    }

    #[test]
    fn a_blocks_removed_are_not_named() {
        assert!(changed(vec![md("a"), md("b")], vec![md("a")]).is_empty());
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
        views.take(true, |_| None);
        let page = views.page(|_| None);
        assert!(page.views.iter().find(|v| v.view.id == "ci").unwrap().unseen);
        // Opening it is seeing it: the marks flash once, and it is no longer unseen.
        assert!(views.open(PaneId(1), "ci".into()));
        let page = views.take(true, |_| None).unwrap();
        let ci = page.views.iter().find(|v| v.view.id == "ci").unwrap();
        assert_eq!(ci.changed, ["0", "1"]);
        assert!(!ci.unseen);
        views.open(PaneId(1), "other".into());
        let page = views.take(true, |_| None).unwrap();
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
