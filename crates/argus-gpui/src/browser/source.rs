//! Where the decoded bytes of a browser's latest picture come from: core's `Browsers::picture`,
//! which undoes the base64 once and shares the bytes, so this window never decodes base64 itself.
//!
//! In the running window the source is the session's (`Owner::picture`). A model that is handed
//! events with no session behind it (a test, the scenes photographed) keeps a `Browsers` of its own
//! and feeds it each shot it hears, so the same core code answers.

use std::sync::{Arc, Mutex, MutexGuard};

use argus_desktop_core::browser::picture::Picture;
use argus_desktop_core::browser::{BrowserShot, Browsers};
use argus_protocol::{BrowserFrame, PaneId};

/// A pane's latest picture as bytes, asked for from any thread: it may take a lock.
pub type Source = Arc<dyn Fn(u64) -> Option<Arc<Picture>> + Send + Sync>;

/// A `Browsers` held for a model with no session.
#[derive(Clone, Default)]
pub struct Standalone(Arc<Mutex<Browsers>>);

impl Standalone {
    fn held(&self) -> MutexGuard<'_, Browsers> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Keeps `shot` as `pane`'s latest picture.
    pub fn feed(&self, shot: &BrowserShot) {
        let BrowserShot { pane, jpeg, width, height } = shot.clone();
        self.held().shot(BrowserFrame { pane: PaneId(pane), jpeg, width, height });
    }

    /// A browser that closed has no picture to keep.
    pub fn closed(&self, pane: u64) {
        self.held().closed(PaneId(pane));
    }

    pub fn source(&self) -> Source {
        let held = self.clone();
        Arc::new(move |pane| held.held().picture(PaneId(pane)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shot_fed_is_answered_as_bytes_and_a_closed_pane_has_none() {
        let held = Standalone::default();
        held.feed(&BrowserShot { pane: 3, jpeg: "aGk=".into(), width: 1280, height: 800 });
        let source = held.source();
        let picture = source(3).expect("the picture is there");
        assert_eq!((&*picture.bytes, picture.width, picture.height), (&b"hi"[..], 1280, 800));
        held.closed(3);
        assert!(source(3).is_none());
    }
}
