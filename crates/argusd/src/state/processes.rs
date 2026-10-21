//! Starting a pane's process, and what the daemon can do to one once it
//! runs: the port every pane is started through. The pty is the adapter a
//! daemon runs on; tests drive a scripted one that exits when they say.

use std::path::Path;
use std::sync::Arc;

use argus_protocol::PaneId;

use crate::pty::{self, PaneRuntime};

use super::PaneSubscription;

/// Called once, when the process ends, with its exit code if it had one.
pub(super) type OnExit = Box<dyn FnOnce(Option<i32>) + Send>;

pub(super) trait Processes: Send + Sync {
    fn start(
        &self,
        id: PaneId,
        cwd: &Path,
        spec: pty::Spawn,
        on_exit: OnExit,
    ) -> anyhow::Result<Box<dyn Process>>;
}

/// A running pane process, as the tree holds it.
pub(crate) trait Process: Send {
    /// A handle to its keyboard that outlives the tree lock it was found
    /// under.
    fn input(&self) -> Arc<dyn Keyboard>;
    fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()>;
    /// Ends it and whatever it started. Its exit is still announced
    /// through `on_exit`, to a tree every caller has already taken it out
    /// of.
    fn kill(&self) -> anyhow::Result<()>;
    fn scrollback(&self, offset: usize, top: Option<u64>) -> pty::Scrolled;
    fn snapshot_and_subscribe(&self) -> PaneSubscription;
    fn broadcast_snapshot(&self, pane: PaneId);
}

pub(crate) trait Keyboard: Send + Sync {
    fn write(&self, bytes: &[u8]) -> anyhow::Result<()>;
    /// Pasted text, bracketed when the program asked for that.
    fn paste(&self, bytes: &[u8]) -> anyhow::Result<()>;
    /// The pane gained or lost the person's focus; said to the program
    /// only if it asked to hear.
    fn focus(&self, focused: bool) -> anyhow::Result<()>;
}

/// Every pane in a pty of its own.
pub(super) struct Pty;

impl Processes for Pty {
    fn start(
        &self,
        id: PaneId,
        cwd: &Path,
        spec: pty::Spawn,
        on_exit: OnExit,
    ) -> anyhow::Result<Box<dyn Process>> {
        Ok(Box::new(PaneRuntime::spawn(id, cwd, spec, on_exit)?))
    }
}

impl Process for PaneRuntime {
    fn input(&self) -> Arc<dyn Keyboard> {
        Arc::new(PaneRuntime::input(self))
    }

    fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()> {
        PaneRuntime::resize(self, rows, cols)
    }

    fn kill(&self) -> anyhow::Result<()> {
        PaneRuntime::kill(self)
    }

    fn scrollback(&self, offset: usize, top: Option<u64>) -> pty::Scrolled {
        PaneRuntime::scrollback(self, offset, top)
    }

    fn snapshot_and_subscribe(&self) -> PaneSubscription {
        PaneRuntime::snapshot_and_subscribe(self)
    }

    fn broadcast_snapshot(&self, pane: PaneId) {
        PaneRuntime::broadcast_snapshot(self, pane)
    }
}

impl Keyboard for pty::PaneInput {
    fn write(&self, bytes: &[u8]) -> anyhow::Result<()> {
        pty::PaneInput::write(self, bytes)
    }

    fn paste(&self, bytes: &[u8]) -> anyhow::Result<()> {
        pty::PaneInput::paste(self, bytes)
    }

    fn focus(&self, focused: bool) -> anyhow::Result<()> {
        pty::PaneInput::focus(self, focused)
    }
}
