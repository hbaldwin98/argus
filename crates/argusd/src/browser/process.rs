//! A browser pane's process: what a terminal's keys, sizes and scrollback
//! come to for a pane whose content is a browser — nothing.
//!
//! The pane is in the tree so that it tiles, closes and is listed as a
//! shell is, and the tree holds every pane's process as one. The browser
//! itself is `Browser`, kept by `state::browsers`; the person's hand on it
//! comes as `ClientMsg::BrowserInput`, never as bytes for a pty.

use std::sync::Arc;

use argus_protocol::{Cell, Cursor, KeyboardMode, MouseTracking, PaneId, ServerMsg};
use tokio::sync::broadcast;

use crate::pty::{PaneSubscription, Scrolled};
use crate::state::processes::{Keyboard, Process};

pub(crate) struct BrowserProcess {
    /// Held so a subscriber's receiver stays open rather than reading as a
    /// pane that ended; nothing is ever sent on it.
    quiet: broadcast::Sender<ServerMsg>,
}

impl BrowserProcess {
    pub(crate) fn new() -> BrowserProcess {
        BrowserProcess { quiet: broadcast::channel(1).0 }
    }
}

impl Process for BrowserProcess {
    fn input(&self) -> Arc<dyn Keyboard> {
        Arc::new(Discard)
    }

    fn resize(&self, _: u16, _: u16) -> anyhow::Result<()> {
        Ok(())
    }

    /// The browser ends with the pane, through `drop_browser_of`, which
    /// every removal passes through: there is no process here to kill.
    fn kill(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn scrollback(&self, _: usize, _: Option<u64>) -> Scrolled {
        Scrolled { cells: Vec::new(), offset: 0, depth: 0, top: 0 }
    }

    fn snapshot_and_subscribe(&self) -> PaneSubscription {
        let blank = vec![vec![Cell::default()]];
        (1, 1, blank, Cursor::default(), MouseTracking::default(), false, KeyboardMode::default(), self.quiet.subscribe())
    }

    fn broadcast_snapshot(&self, _: PaneId) {}
}

/// Bytes meant for a terminal, which a picture has no use for.
struct Discard;

impl Keyboard for Discard {
    fn write(&self, _: &[u8]) -> anyhow::Result<()> {
        Ok(())
    }

    fn paste(&self, _: &[u8]) -> anyhow::Result<()> {
        Ok(())
    }

    fn focus(&self, _: bool) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_browser_pane_takes_a_terminals_calls_and_does_nothing_with_them() {
        let process = BrowserProcess::new();
        let keys = process.input();
        assert!(keys.write(b"ls\r").is_ok() && keys.paste(b"x").is_ok() && keys.focus(true).is_ok());
        assert!(process.resize(40, 120).is_ok() && process.kill().is_ok());
        let scrolled = process.scrollback(5, None);
        assert_eq!((scrolled.cells.len(), scrolled.depth), (0, 0));
        let (rows, cols, grid, _, _, alternate, _, mut quiet) = process.snapshot_and_subscribe();
        assert_eq!((rows, cols, grid.len(), alternate), (1, 1, 1, false));
        assert!(matches!(quiet.try_recv(), Err(broadcast::error::TryRecvError::Empty)), "open and silent");
    }
}
