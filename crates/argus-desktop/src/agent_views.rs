//! Which views agents showed, as this window holds them, and the one out in
//! the side panel.
//!
//! The daemon owns the views: a person closing one or asking for a refresh
//! sends the request and changes nothing here until the daemon says it
//! changed. Only which one is open, and that a refresh is pending, are the
//! window's own.

use std::collections::HashSet;

use argus_protocol::{ClientMsg, PaneId, ShownView};

use crate::page::{Link, PageView, ViewRef, ViewsPage};
use crate::sent::Sent;

#[derive(Default)]
pub struct AgentViews {
    /// Newest first.
    views: Vec<ShownView>,
    open: Option<(PaneId, String)>,
    refreshing: HashSet<(PaneId, String)>,
    sent: Sent<ViewsPage>,
}

impl AgentViews {
    /// A new connection replays every view the daemon holds, so what this
    /// window had is dropped rather than doubled.
    pub fn connected(&mut self, out: &mut Vec<ClientMsg>) {
        self.views.clear();
        self.refreshing.clear();
        out.push(ClientMsg::WatchViews);
    }

    /// A view came. It opens by itself only when its pane is the one on
    /// screen and nothing else is open: the person is watching that agent,
    /// and anywhere else it would take the panel from under them.
    pub fn shown(&mut self, view: ShownView, on_screen: Option<PaneId>) {
        let key = (view.pane, view.view.id.clone());
        self.refreshing.remove(&key);
        self.views.retain(|v| (v.pane, v.view.id.as_str()) != (key.0, key.1.as_str()));
        if self.open.is_none() && on_screen == Some(view.pane) {
            self.open = Some(key);
        }
        self.views.insert(0, view);
    }

    pub fn closed(&mut self, pane: PaneId, id: &str) {
        self.views.retain(|v| (v.pane, v.view.id.as_str()) != (pane, id));
        self.refreshing.remove(&(pane, id.to_string()));
        if self.open.as_ref().is_some_and(|(p, i)| (*p, i.as_str()) == (pane, id)) {
            self.open = None;
        }
    }

    pub fn open(&mut self, pane: PaneId, id: String) {
        if self.find(pane, &id).is_some() {
            self.open = Some((pane, id));
        }
    }

    pub fn hide(&mut self) {
        self.open = None;
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
                    subject: v.subject.as_ref().map(|l| Link { tracker: l.tracker.clone(), key: l.key.clone(), url: l.url.clone() }),
                    view: v.view.clone(),
                })
                .collect(),
            open: self.open.as_ref().map(|(pane, id)| ViewRef { pane: pane.0, id: id.clone() }),
        }
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
            view: AgentView { id: id.into(), title: id.into(), status: None, subject: None, blocks: Vec::new() },
        }
    }

    fn ids(views: &mut AgentViews) -> Vec<String> {
        views.page(|_| None).views.into_iter().map(|v| v.view.id).collect()
    }

    #[test]
    fn the_page_hears_of_views_only_once_there_have_been_some() {
        let mut views = AgentViews::default();
        assert_eq!(views.take(|_| None), None);
        views.shown(shown(1, "ci"), None);
        assert_eq!(views.take(|_| None).unwrap().views.len(), 1);
        views.closed(PaneId(1), "ci");
        assert_eq!(views.take(|_| None), Some(ViewsPage::default()));
        assert_eq!(views.take(|_| None), None);
    }

    #[test]
    fn a_view_from_the_pane_on_screen_opens_and_one_from_elsewhere_waits() {
        let mut views = AgentViews::default();
        views.shown(shown(2, "elsewhere"), Some(PaneId(1)));
        assert_eq!(views.page(|_| None).open, None);
        views.shown(shown(1, "ci"), Some(PaneId(1)));
        assert_eq!(views.page(|_| None).open, Some(ViewRef { pane: 1, id: "ci".into() }));
        // A second from the same pane does not take the panel from the first.
        views.shown(shown(1, "queries"), Some(PaneId(1)));
        assert_eq!(views.page(|_| None).open, Some(ViewRef { pane: 1, id: "ci".into() }));
    }

    #[test]
    fn a_view_shown_again_replaces_its_old_self_at_the_top() {
        let mut views = AgentViews::default();
        views.shown(shown(1, "a"), None);
        views.shown(shown(1, "b"), None);
        views.shown(shown(1, "a"), None);
        assert_eq!(ids(&mut views), ["a", "b"]);
    }

    #[test]
    fn closing_asks_the_daemon_and_waits_for_it() {
        let mut views = AgentViews::default();
        views.shown(shown(1, "ci"), Some(PaneId(1)));
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
        views.shown(shown(1, "ci"), None);
        let mut out = Vec::new();
        views.refresh(PaneId(1), "ci".into(), &mut out);
        assert!(matches!(&out[..], [ClientMsg::RefreshView { .. }]));
        assert!(views.page(|_| None).views[0].refreshing);
        views.shown(shown(1, "ci"), None);
        assert!(!views.page(|_| None).views[0].refreshing);
    }

    #[test]
    fn a_reconnect_watches_again_and_drops_what_the_daemon_will_replay() {
        let mut views = AgentViews::default();
        views.shown(shown(1, "ci"), None);
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
        views.open(PaneId(1), "gone".into());
        assert!(out.is_empty());
        assert_eq!(views.page(|_| None).open, None);
    }
}
