//! Which views agents have shown, per pane, and which go when.
//!
//! Held in memory only, and never handed to the store: a view is what an
//! agent read from a ticket a moment ago, and nothing read from a ticket
//! outlives the agent's next look at it. A restart of the daemon, or the
//! pane's removal, is the end of its views.
//!
//! Every change to what is held — a show, a patch, a close, a pane going —
//! is made, and told to the watchers, under the one lock, so a watcher
//! sees each change in the order it was made.

use argus_protocol::{
    find_action, press_request, AgentView, PaneId, ServerMsg, ShownView, ViewChange, ViewPress,
    MAX_VIEWS_PER_PANE, MAX_VIEW_BYTES,
};
use std::sync::Arc;

use tokio::sync::broadcast;

use super::view_timers::{spawn_timer, Timers};
use super::*;

/// The views, oldest shown first across every pane, which is the order a
/// client that starts watching is replayed them in.
#[derive(Default)]
pub(super) struct Views {
    shown: Vec<ShownView>,
    /// The task asking each view's agent to refresh it, for the views shown
    /// with a `refresh_every`.
    pub(super) timers: Timers,
}

impl Views {
    /// Keeps `view`, in the place of the pane's view with its id when it has
    /// one. Past the pane's limit its oldest goes; the id that did is
    /// returned.
    fn show(&mut self, view: ShownView) -> Option<String> {
        let held = self
            .shown
            .iter()
            .position(|v| v.pane == view.pane && v.view.id == view.view.id);
        if let Some(at) = held {
            self.shown[at] = view;
            return None;
        }
        let pane = view.pane;
        self.shown.push(view);
        let on_pane = self.shown.iter().filter(|v| v.pane == pane).count();
        if on_pane <= MAX_VIEWS_PER_PANE {
            return None;
        }
        let oldest = self.shown.iter().position(|v| v.pane == pane)?;
        let id = self.shown.remove(oldest).view.id;
        self.timers.stop(pane, &id);
        Some(id)
    }

    fn close(&mut self, pane: PaneId, id: &str) -> bool {
        let before = self.shown.len();
        self.shown.retain(|v| !(v.pane == pane && v.view.id == id));
        self.timers.stop(pane, id);
        self.shown.len() != before
    }

    /// The ids the pane had, oldest first.
    fn drop_pane(&mut self, pane: PaneId) -> Vec<String> {
        let ids = self.shown.iter().filter(|v| v.pane == pane).map(|v| v.view.id.clone());
        let ids: Vec<String> = ids.collect();
        self.shown.retain(|v| v.pane != pane);
        for id in &ids {
            self.timers.stop(pane, id);
        }
        ids
    }

    fn get(&self, pane: PaneId, id: &str) -> Option<&ShownView> {
        self.shown.iter().find(|v| v.pane == pane && v.view.id == id)
    }
}

/// What the daemon tells the connections watching views: a [`ServerMsg`]
/// already, since each watcher is sent it as it is.
pub(super) type ViewEvent = ServerMsg;

impl Daemon {
    /// An agent's `view` request: shows the view, patches one, or closes one.
    ///
    /// The refusal is the limit's own words, with the path to the offending
    /// block, for the agent to fix and send again. Closing a view the pane
    /// does not have is not an error: the person may have closed it first,
    /// and the agent's aim, that it be gone, is met.
    pub fn change_view_for_agent(
        self: &Arc<Self>,
        pane: PaneId,
        change: ViewChange,
    ) -> anyhow::Result<()> {
        self.agent_scope(pane)?;
        match change {
            ViewChange::Show { view } => {
                let view = checked_within_size(view)?;
                self.hold_view(pane, |_| Ok(view))
            }
            ViewChange::Close { id } => {
                self.close_view(pane, id.trim());
                Ok(())
            }
            ViewChange::Patch { id, path, block } => {
                let id = id.trim().to_string();
                self.hold_view(pane, |views| {
                    let held = views
                        .get(pane, &id)
                        .ok_or_else(|| anyhow::anyhow!("the pane shows no view {id:?}"))?;
                    let patched = held.view.clone().patched(&path, block).map_err(|e| anyhow::anyhow!("{e}"))?;
                    checked_within_size(patched)
                })
            }
        }
    }

    /// Holds the view `make` builds, from what is held now, and tells the
    /// watchers; all under the views lock. The pane is looked for in the
    /// tree under it as well: a pane being removed takes its views next, by
    /// the same lock, so a view shown as its pane goes is either refused
    /// here or dropped there, and never left with a timer and no pane.
    ///
    /// A view shown again keeps the refresh timer it has while its interval
    /// is unchanged, and a patch never changes the interval.
    fn hold_view(
        self: &Arc<Self>,
        pane: PaneId,
        make: impl FnOnce(&Views) -> anyhow::Result<AgentView>,
    ) -> anyhow::Result<()> {
        let mut views = self.agent_views.lock().unwrap();
        self.agent_scope(pane)?;
        let view = make(&views)?;
        let (id, every) = (view.id.clone(), view.refresh_every);
        let shown = ShownView {
            pane,
            at: super::decisions::now(),
            subject: view.subject_link(),
            view,
        };
        // Sent under the lock, so a watcher that subscribes in
        // `watch_views` sees each change either in its replay or after it,
        // and never in the wrong order.
        if let Some(id) = views.show(shown.clone()) {
            let _ = self.agent_views_tx.send(ServerMsg::ViewClosed { pane, id });
        }
        let _ = self.agent_views_tx.send(ServerMsg::ViewShown(Box::new(shown)));
        views.timers.keep(pane, &id, every, |every| spawn_timer(self, pane, &id, every));
        Ok(())
    }

    /// The person's close from a client, or the agent's.
    pub fn close_view(&self, pane: PaneId, id: &str) {
        let mut views = self.agent_views.lock().unwrap();
        if views.close(pane, id) {
            let _ = self.agent_views_tx.send(ServerMsg::ViewClosed { pane, id: id.to_string() });
        }
    }

    /// Everything a watcher is replayed, oldest first, and the receiver of
    /// what follows it.
    pub fn watch_views(&self) -> (Vec<ShownView>, broadcast::Receiver<ViewEvent>) {
        let views = self.agent_views.lock().unwrap();
        (views.shown.clone(), self.agent_views_tx.subscribe())
    }

    /// Asks the agent that showed a view to look again. The view stays as
    /// it is until the agent shows it anew, so an agent that does not
    /// answer leaves the person looking at what was true when it was read.
    /// A view that is not held is ignored, and `false` says so.
    pub fn refresh_view(&self, pane: PaneId, id: &str) -> bool {
        let Some(shown) = self.agent_views.lock().unwrap().get(pane, id).cloned() else {
            return false;
        };
        self.notify(pane, &refresh_request(&shown.view));
        true
    }

    /// Tells the agent that showed a view which of its actions the person
    /// pressed.
    ///
    /// An action the view does not offer is not relayed: what is typed to
    /// the agent must be something its own view said, never text a client
    /// made up. Nor is one that cannot be told to an agent that is not
    /// running. Each is the person's to hear of, so the reason comes back
    /// as the `Err`, in words for them.
    pub fn press_view(&self, pane: PaneId, press: ViewPress) -> Result<(), String> {
        let shown = self.agent_views.lock().unwrap().get(pane, &press.view).cloned();
        let shown = shown.ok_or("That view is no longer shown.")?;
        let not_offered = "That view no longer offers that action.";
        let action = find_action(&shown.view, &press.action).ok_or(not_offered)?;
        let press = press.offered_by(&shown.view).ok_or(not_offered)?;
        if !self.notify(pane, &press_request(&shown.view, action, &press)) {
            return Err("The agent is not running, so it could not be told.".into());
        }
        Ok(())
    }

    /// Called as a pane leaves the tree: its views go with it.
    pub(super) fn drop_views_of(&self, pane: PaneId) {
        let mut views = self.agent_views.lock().unwrap();
        for id in views.drop_pane(pane) {
            let _ = self.agent_views_tx.send(ServerMsg::ViewClosed { pane, id });
        }
    }
}

/// The view if it keeps every limit and fits the size one may be held at.
fn checked_within_size(view: AgentView) -> anyhow::Result<AgentView> {
    let view = view.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
    // The body limit bounds what was sent; a view that serialises
    // larger than it, from defaults filled in that the sender never
    // wrote, is as much a dump as one that was sent so.
    let size = serde_json::to_vec(&view).map_or(0, |bytes| bytes.len());
    anyhow::ensure!(size <= MAX_VIEW_BYTES, "a view is at most {MAX_VIEW_BYTES} bytes");
    Ok(view)
}

/// What is typed to the agent when it has no inbox, so it must stay one
/// line: a newline in a title would send the request early.
fn refresh_request(view: &AgentView) -> String {
    let title = argus_protocol::view_press::plain(&view.title);
    let source = match &view.subject {
        Some(url) => argus_protocol::view_press::plain(url),
        None => "its source".to_string(),
    };
    format!(
        "Refresh the view `{id}` (\"{title}\"): read {source} again with your own tools and show \
         it anew with `argus-hook view show --stdin`, same id.",
        id = argus_protocol::view_press::plain(&view.id),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::task::AbortHandle;

    fn shown(pane: u64, id: &str) -> ShownView {
        ShownView {
            pane: PaneId(pane),
            at: 0,
            subject: None,
            view: AgentView {
                id: id.into(),
                title: format!("title of {id}"),
                ..Default::default()
            },
        }
    }

    fn ids(views: &Views, pane: u64) -> Vec<&str> {
        let on = views.shown.iter().filter(|v| v.pane == PaneId(pane));
        on.map(|v| v.view.id.as_str()).collect()
    }

    #[test]
    fn showing_an_id_the_pane_has_replaces_it_in_place() {
        let mut views = Views::default();
        views.show(shown(1, "a"));
        views.show(shown(1, "b"));
        let mut again = shown(1, "a");
        again.view.title = "newer".into();
        assert_eq!(views.show(again), None);
        assert_eq!(ids(&views, 1), ["a", "b"]);
        assert_eq!(views.get(PaneId(1), "a").unwrap().view.title, "newer");
    }

    #[test]
    fn the_same_id_on_two_panes_is_two_views() {
        let mut views = Views::default();
        views.show(shown(1, "a"));
        views.show(shown(2, "a"));
        assert!(views.close(PaneId(1), "a"));
        assert_eq!(ids(&views, 2), ["a"]);
    }

    #[test]
    fn a_ninth_view_drops_the_panes_oldest_and_says_which() {
        let mut views = Views::default();
        views.show(shown(2, "other"));
        for n in 0..MAX_VIEWS_PER_PANE {
            assert_eq!(views.show(shown(1, &format!("v{n}"))), None);
        }
        assert_eq!(views.show(shown(1, "v8")), Some("v0".to_string()));
        assert_eq!(ids(&views, 1).len(), MAX_VIEWS_PER_PANE);
        assert_eq!(ids(&views, 2), ["other"], "another pane's views are not counted");
    }

    #[test]
    fn closing_what_is_not_held_changes_nothing() {
        let mut views = Views::default();
        views.show(shown(1, "a"));
        assert!(!views.close(PaneId(1), "zzz"));
        assert!(!views.close(PaneId(2), "a"));
        assert!(views.close(PaneId(1), "a"));
    }

    #[test]
    fn a_pane_going_takes_its_views_and_names_them() {
        let mut views = Views::default();
        views.show(shown(1, "a"));
        views.show(shown(2, "x"));
        views.show(shown(1, "b"));
        assert_eq!(views.drop_pane(PaneId(1)), ["a", "b"]);
        assert_eq!(ids(&views, 2), ["x"]);
        assert!(views.drop_pane(PaneId(1)).is_empty());
    }

    /// A task that never ends on its own, standing for a refresh timer.
    fn pending_timer() -> AbortHandle {
        tokio::spawn(std::future::pending::<()>()).abort_handle()
    }

    fn timed(views: &mut Views, pane: u64, id: &str) -> AbortHandle {
        let timer = pending_timer();
        views.timers.keep(PaneId(pane), id, Some(60), |_| Some(timer.clone()));
        timer
    }

    async fn stopped(timer: &AbortHandle) -> bool {
        for _ in 0..50 {
            if timer.is_finished() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        false
    }

    #[tokio::test]
    async fn closing_a_view_stops_its_timer_and_no_other() {
        let mut views = Views::default();
        views.show(shown(1, "a"));
        views.show(shown(1, "b"));
        let a = timed(&mut views, 1, "a");
        let b = timed(&mut views, 1, "b");
        views.close(PaneId(1), "a");
        assert!(stopped(&a).await);
        assert!(!b.is_finished() && views.timers.is_refreshing(PaneId(1), "b"));
    }

    #[tokio::test]
    async fn a_ninth_view_stops_the_timer_of_the_one_it_displaces() {
        let mut views = Views::default();
        views.show(shown(1, "v0"));
        let oldest = timed(&mut views, 1, "v0");
        for n in 1..=MAX_VIEWS_PER_PANE {
            views.show(shown(1, &format!("v{n}")));
        }
        assert!(stopped(&oldest).await);
        assert!(!views.timers.is_refreshing(PaneId(1), "v0"));
    }

    #[tokio::test]
    async fn a_pane_going_stops_the_timers_of_its_views() {
        let mut views = Views::default();
        views.show(shown(1, "a"));
        views.show(shown(2, "a"));
        let mine = timed(&mut views, 1, "a");
        let other = timed(&mut views, 2, "a");
        views.drop_pane(PaneId(1));
        assert!(stopped(&mine).await);
        assert!(!other.is_finished());
    }

    #[test]
    fn a_refresh_request_stays_on_one_line_and_names_the_source() {
        let mut view = shown(1, "t").view;
        view.title = "line\none".into();
        view.subject = Some("https://example.com/browse/ENG-1".into());
        let text = refresh_request(&view);
        assert!(!text.contains('\n'), "{text}");
        assert!(text.contains("`t`") && text.contains("https://example.com/browse/ENG-1"), "{text}");
        view.subject = None;
        assert!(refresh_request(&view).contains("its source"));
    }

    #[test]
    fn a_refresh_request_has_no_control_character_in_any_part() {
        let mut view = shown(1, "t\u{1b}x").view;
        view.title = "a\u{7}b".into();
        view.subject = Some("https://example.com/a\nb".into());
        let text = refresh_request(&view);
        assert!(!text.chars().any(char::is_control), "{text:?}");
    }
}
