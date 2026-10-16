//! What each attached client is showing: the one pty size reconciled out
//! of what they all ask for, and the view of the client the person was
//! last at, which is where they are working.
//!
//! A pty has one size and clients do not have to agree on it, so a request
//! is recorded against the viewer that made it and applied only once every
//! viewer's request has been folded in.

use super::*;

/// Identifies one attached client for as long as its connection lasts.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ViewerId(pub(super) u64);

/// The requested pane sizes of every attached client, what was last
/// actually applied to each pty, and the view each client reported.
#[derive(Default)]
pub(super) struct Viewers {
    wanted: HashMap<PaneId, HashMap<ViewerId, (u16, u16)>>,
    applied: HashMap<PaneId, (u16, u16)>,
    /// Ordered by when each client was last active, the latest last: the
    /// focus is the last view, and falls back to the one before it when
    /// that client goes.
    views: Vec<(ViewerId, View)>,
    /// The pane the child was last told it has the person's focus.
    focused_pane: Option<PaneId>,
}

impl Viewers {
    /// The size a pane should be given the clients currently showing it:
    /// the request of the one the person was last at, as tmux's
    /// `window-size latest` does, so the window being looked at is filled
    /// and the others pad or crop until the person goes back to them. The
    /// smallest-of-all rule this replaced left a desktop window padding a
    /// terminal-sized grid whenever a terminal client showed the same pane.
    /// Only when none of them has reported a view is it the smallest
    /// request in each dimension, which crops nobody.
    fn effective(&self, pane: PaneId) -> Option<(u16, u16)> {
        let wanted = self.wanted.get(&pane)?;
        let latest = self.views.iter().rev().find_map(|(viewer, _)| wanted.get(viewer));
        latest.copied().or_else(|| {
            wanted
                .values()
                .copied()
                .reduce(|(ar, ac), (br, bc)| (ar.min(br), ac.min(bc)))
        })
    }

    /// Records what `viewer` is showing. An active view moves its client
    /// to the front; an inactive one keeps its place, and a client first
    /// heard of inactive goes behind every other.
    fn record_view(&mut self, viewer: ViewerId, view: View) {
        let held = self.views.iter().position(|(v, _)| *v == viewer);
        match held {
            Some(i) if view.active => {
                self.views.remove(i);
                self.views.push((viewer, view));
            }
            Some(i) => self.views[i].1 = view,
            None if view.active => self.views.push((viewer, view)),
            None => self.views.insert(0, (viewer, view)),
        }
    }

    /// Whether `viewer` had reported a view, which it no longer has.
    fn forget_view(&mut self, viewer: ViewerId) -> bool {
        let before = self.views.len();
        self.views.retain(|(v, _)| *v != viewer);
        self.views.len() != before
    }

    /// Moves the focused pane to wherever the person is now, and returns
    /// what its children are to be told: the pane left, then the pane
    /// entered. A pane has focus only while the client showing it is the
    /// active one, so a window losing focus tells its pane too.
    fn refocus(&mut self) -> Vec<(PaneId, bool)> {
        let now = self.focused_view().filter(|view| view.active).and_then(|view| view.pane);
        if now == self.focused_pane {
            return Vec::new();
        }
        let left = std::mem::replace(&mut self.focused_pane, now);
        left.map(|pane| (pane, false)).into_iter().chain(now.map(|pane| (pane, true))).collect()
    }

    fn focused_view(&self) -> Option<&View> {
        self.views.last().map(|(_, view)| view)
    }

    /// Every client that reported a view, the one the person was last at
    /// first.
    fn by_recency(&self) -> Vec<ViewerId> {
        self.views.iter().rev().map(|(viewer, _)| *viewer).collect()
    }

    /// The size to apply for `pane`, or `None` when nothing would change.
    /// A pane no client is showing keeps the size it has: reflowing a
    /// running program's output for an audience of nobody only destroys it.
    fn pending(&self, pane: PaneId) -> Option<(u16, u16)> {
        let size = self.effective(pane)?;
        (self.applied.get(&pane) != Some(&size)).then_some(size)
    }
}

impl Daemon {
    /// Drops every record of a pane's size. Called when the pane itself
    /// goes: nothing is left to reconcile, and a later pane reusing the id
    /// must not inherit what this one's viewers had asked for.
    pub(super) fn forget_pane_sizes(&self, pane: PaneId) {
        let mut viewers = self.viewers.lock().unwrap();
        viewers.wanted.remove(&pane);
        viewers.applied.remove(&pane);
    }

    /// Whether any record of a pane's size is held, for a test to check
    /// that a gone pane left none.
    #[cfg(test)]
    pub(super) fn holds_pane_size(&self, pane: PaneId) -> bool {
        let viewers = self.viewers.lock().unwrap();
        viewers.wanted.contains_key(&pane) || viewers.applied.contains_key(&pane)
    }

    /// Hands out the identity a connection uses to claim pane sizes.
    pub fn new_viewer(&self) -> ViewerId {
        ViewerId(
            self.next_viewer
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        )
    }

    /// Records one client's size for a pane and reconciles the pty against
    /// every client showing it.
    pub fn resize_pane(
        &self,
        viewer: ViewerId,
        pane: PaneId,
        rows: u16,
        cols: u16,
    ) -> anyhow::Result<()> {
        {
            let inner = self.inner.lock().unwrap();
            if find_any_pane_ref(&inner, pane).is_none() {
                anyhow::bail!("no such pane");
            }
        }
        self.viewers
            .lock()
            .unwrap()
            .wanted
            .entry(pane)
            .or_default()
            .insert(viewer, (rows, cols));
        self.reconcile_size(pane)
    }

    /// Drops one client's claim on a pane it has stopped showing, letting
    /// the pane grow back to what the remaining clients can display.
    pub fn release_pane_size(&self, viewer: ViewerId, pane: PaneId) {
        {
            let mut viewers = self.viewers.lock().unwrap();
            let Some(wanted) = viewers.wanted.get_mut(&pane) else {
                return;
            };
            if wanted.remove(&viewer).is_none() {
                return;
            }
            if wanted.is_empty() {
                viewers.wanted.remove(&pane);
            }
        }
        let _ = self.reconcile_size(pane);
    }

    /// Records what a client is showing, and tells whoever watches the
    /// focus that it may have moved. A client becoming the latest takes
    /// the size of every pane it shows.
    pub fn report_view(&self, viewer: ViewerId, view: View) {
        let (shown, moved) = {
            let mut viewers = self.viewers.lock().unwrap();
            viewers.record_view(viewer, view);
            let shown: Vec<PaneId> = viewers.wanted.keys().copied().collect();
            (shown, viewers.refocus())
        };
        self.tell_focus(moved);
        self.pushes.focus_moved();
        for pane in shown {
            let _ = self.reconcile_size(pane);
        }
    }

    /// Tells each pane's child it gained or lost focus, if it asked. Run
    /// without the viewers lock: a write to a full pty can block.
    fn tell_focus(&self, moved: Vec<(PaneId, bool)>) {
        for (pane, focused) in moved {
            if let Some(input) = self.pane_input(pane) {
                let _ = input.focus(focused);
            }
        }
    }

    /// A tick each time a client's view changes or goes; the focus is read
    /// again with [`Daemon::focus_in`].
    pub fn subscribe_views(&self) -> broadcast::Receiver<()> {
        self.pushes.subscribe_focus()
    }

    /// Where the person is working, read against `tree`: the view of the
    /// client they were last at. `None` while no client reports one.
    pub fn focus_in(&self, tree: &[ProjectInfo]) -> Option<Focus> {
        let view = self.viewers.lock().unwrap().focused_view().cloned()?;
        let workspace = {
            let inner = self.inner.lock().unwrap();
            inner
                .workspaces
                .iter()
                .find(|w| w.id == inner.open)
                .map(|w| w.name.clone())
                .unwrap_or_default()
        };
        Some(Focus::of(&view, &workspace, tree))
    }

    /// The clients that reported a view, the one the person was last at
    /// first: whom a question is put to.
    pub(super) fn viewers_by_recency(&self) -> Vec<ViewerId> {
        self.viewers.lock().unwrap().by_recency()
    }

    /// What `view` names, read against the daemon's tree as the focus is.
    pub(super) fn read_view(&self, view: &View) -> Focus {
        let tree = self.snapshot();
        let workspace = {
            let inner = self.inner.lock().unwrap();
            inner
                .workspaces
                .iter()
                .find(|w| w.id == inner.open)
                .map(|w| w.name.clone())
                .unwrap_or_default()
        };
        Focus::of(view, &workspace, &tree)
    }

    /// Drops every claim a disconnecting client held, and its view.
    pub fn release_viewer(&self, viewer: ViewerId) {
        let (forgot, moved) = {
            let mut viewers = self.viewers.lock().unwrap();
            let forgot = viewers.forget_view(viewer);
            (forgot, viewers.refocus())
        };
        self.tell_focus(moved);
        if forgot {
            self.pushes.focus_moved();
        }
        let touched: Vec<PaneId> = {
            let viewers = self.viewers.lock().unwrap();
            viewers
                .wanted
                .iter()
                .filter(|(_, wanted)| wanted.contains_key(&viewer))
                .map(|(pane, _)| *pane)
                .collect()
        };
        for pane in touched {
            self.release_pane_size(viewer, pane);
        }
    }

    fn reconcile_size(&self, pane: PaneId) -> anyhow::Result<()> {
        let Some((rows, cols)) = self.viewers.lock().unwrap().pending(pane) else {
            return Ok(());
        };
        let inner = self.inner.lock().unwrap();
        let p =
            find_any_pane_ref(&inner, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        p.runtime.resize(rows, cols)?;
        // Only once it took. Recording a size the pty rejected would make
        // every later request agreeing with it a no-op.
        self.viewers
            .lock()
            .unwrap()
            .applied
            .insert(pane, (rows, cols));
        // A subscribed client's cached grid is only ever sized by whatever
        // snapshot it last received; incremental Damage can't grow it.
        // Push a fresh full snapshot at the new size so growing a pane
        // (very common — new panes start at a hardcoded default far
        // smaller than most terminal heights) doesn't leave the newly
        // exposed area permanently blank.
        p.runtime.broadcast_snapshot(pane);
        Ok(())
    }

    /// Reads this pane's scrollback `offset` lines up, or from line `top`.
    pub fn pane_scrollback(
        &self,
        pane: PaneId,
        offset: usize,
        top: Option<u64>,
    ) -> anyhow::Result<pty::Scrolled> {
        let inner = self.inner.lock().unwrap();
        let p =
            find_any_pane_ref(&inner, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        Ok(p.runtime.scrollback(offset, top))
    }

    pub fn subscribe_pane(&self, pane: PaneId) -> anyhow::Result<PaneSubscription> {
        let inner = self.inner.lock().unwrap();
        let p =
            find_any_pane_ref(&inner, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        Ok(p.runtime.snapshot_and_subscribe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(pane: u64, active: bool) -> View {
        View {
            pane: Some(PaneId(pane)),
            active,
            ..View::default()
        }
    }

    fn focused_pane(viewers: &Viewers) -> Option<PaneId> {
        viewers.focused_view().and_then(|view| view.pane)
    }

    #[test]
    fn the_focus_is_the_view_of_the_client_last_active() {
        let mut viewers = Viewers::default();
        let (first, second) = (ViewerId(0), ViewerId(1));
        viewers.record_view(first, view(1, true));
        viewers.record_view(second, view(2, true));
        assert_eq!(focused_pane(&viewers), Some(PaneId(2)));

        viewers.record_view(first, view(3, true));
        assert_eq!(focused_pane(&viewers), Some(PaneId(3)));
    }

    #[test]
    fn a_background_clients_selection_moving_does_not_take_the_focus() {
        // A tree closing the pane a client in another window had selected
        // moves its selection without anybody being at it.
        let mut viewers = Viewers::default();
        let (here, elsewhere) = (ViewerId(0), ViewerId(1));
        viewers.record_view(elsewhere, view(1, true));
        viewers.record_view(here, view(2, true));

        viewers.record_view(elsewhere, view(5, false));
        assert_eq!(focused_pane(&viewers), Some(PaneId(2)));

        viewers.record_view(here, view(6, false));
        assert_eq!(
            focused_pane(&viewers),
            Some(PaneId(6)),
            "the client the person was last at still speaks for them"
        );
    }

    #[test]
    fn a_client_first_heard_of_inactive_does_not_take_the_focus() {
        let mut viewers = Viewers::default();
        viewers.record_view(ViewerId(0), view(1, true));
        viewers.record_view(ViewerId(1), view(2, false));
        assert_eq!(focused_pane(&viewers), Some(PaneId(1)));
    }

    #[test]
    fn the_focus_falls_back_when_its_client_goes() {
        let mut viewers = Viewers::default();
        let (first, second) = (ViewerId(0), ViewerId(1));
        viewers.record_view(first, view(1, true));
        viewers.record_view(second, view(2, true));

        assert!(viewers.forget_view(second));
        assert_eq!(focused_pane(&viewers), Some(PaneId(1)));
        assert!(viewers.forget_view(first));
        assert_eq!(viewers.focused_view(), None);
        assert!(!viewers.forget_view(first), "nothing was left to forget");
    }

    fn want(viewers: &mut Viewers, viewer: ViewerId, pane: u64, size: (u16, u16)) {
        viewers.wanted.entry(PaneId(pane)).or_default().insert(viewer, size);
    }

    #[test]
    fn a_pane_takes_the_size_of_the_client_last_active() {
        let mut viewers = Viewers::default();
        let (terminal, window) = (ViewerId(0), ViewerId(1));
        want(&mut viewers, terminal, 1, (30, 100));
        want(&mut viewers, window, 1, (50, 180));

        viewers.record_view(terminal, view(1, true));
        viewers.record_view(window, view(1, true));
        assert_eq!(viewers.effective(PaneId(1)), Some((50, 180)));

        viewers.record_view(terminal, view(1, true));
        assert_eq!(viewers.effective(PaneId(1)), Some((30, 100)));
    }

    #[test]
    fn a_latest_client_not_showing_the_pane_leaves_it_to_the_next_one() {
        let mut viewers = Viewers::default();
        let (small, big, elsewhere) = (ViewerId(0), ViewerId(1), ViewerId(2));
        want(&mut viewers, small, 1, (20, 80));
        want(&mut viewers, big, 1, (50, 180));
        viewers.record_view(small, view(1, true));
        viewers.record_view(big, view(1, true));
        viewers.record_view(elsewhere, view(2, true));

        assert_eq!(viewers.effective(PaneId(1)), Some((50, 180)));
    }

    #[test]
    fn clients_that_report_no_view_get_a_size_that_fits_them_all() {
        let mut viewers = Viewers::default();
        want(&mut viewers, ViewerId(0), 1, (60, 80));
        want(&mut viewers, ViewerId(1), 1, (30, 200));

        assert_eq!(viewers.effective(PaneId(1)), Some((30, 80)));
    }

    #[test]
    fn a_size_already_applied_is_not_applied_again() {
        // Every applied size costs every subscriber a full-grid snapshot,
        // so a second client agreeing with the first must be free.
        let mut viewers = Viewers::default();
        let pane = PaneId(1);
        let (first, second) = (ViewerId(0), ViewerId(1));
        viewers
            .wanted
            .entry(pane)
            .or_default()
            .insert(first, (30, 80));
        assert_eq!(viewers.pending(pane), Some((30, 80)));
        viewers.applied.insert(pane, (30, 80));

        viewers
            .wanted
            .entry(pane)
            .or_default()
            .insert(second, (30, 80));

        assert_eq!(viewers.pending(pane), None);
    }
}
