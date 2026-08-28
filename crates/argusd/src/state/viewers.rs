//! What each attached client is showing, and the one pty size reconciled
//! out of what they all ask for.
//!
//! A pty has one size and clients do not have to agree on it, so a request
//! is recorded against the viewer that made it and applied only once every
//! viewer's request has been folded in.

use super::*;

/// Identifies one attached client for as long as its connection lasts.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ViewerId(pub(super) u64);

/// The requested pane sizes of every attached client, and what was last
/// actually applied to each pty.
#[derive(Default)]
pub(super) struct Viewers {
    wanted: HashMap<PaneId, HashMap<ViewerId, (u16, u16)>>,
    applied: HashMap<PaneId, (u16, u16)>,
}

impl Viewers {
    /// The size a pane should be given the clients currently showing it:
    /// the smallest request in each dimension, so no client is ever sent a
    /// grid with more rows or columns than it has room to draw. A client
    /// with a bigger window pads; the alternative — sizing to the largest —
    /// truncates content out of the smaller one entirely.
    fn effective(&self, pane: PaneId) -> Option<(u16, u16)> {
        let wanted = self.wanted.get(&pane)?;
        wanted
            .values()
            .copied()
            .reduce(|(ar, ac), (br, bc)| (ar.min(br), ac.min(bc)))
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
            if find_pane_ref(&inner.projects, pane).is_none() {
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

    /// Drops every claim a disconnecting client held.
    pub fn release_viewer(&self, viewer: ViewerId) {
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
            find_pane_ref(&inner.projects, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
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

    /// Reads `offset` lines up this pane's scrollback. Returns the rows,
    /// the offset actually reached, and how far back the buffer goes.
    pub fn pane_scrollback(
        &self,
        pane: PaneId,
        offset: usize,
    ) -> anyhow::Result<(Vec<Vec<Cell>>, usize, usize)> {
        let inner = self.inner.lock().unwrap();
        let p =
            find_pane_ref(&inner.projects, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        Ok(p.runtime.scrollback(offset))
    }

    pub fn subscribe_pane(&self, pane: PaneId) -> anyhow::Result<PaneSubscription> {
        let inner = self.inner.lock().unwrap();
        let p =
            find_pane_ref(&inner.projects, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        Ok(p.runtime.snapshot_and_subscribe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
