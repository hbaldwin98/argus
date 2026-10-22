//! What every part of the index sends through and reads from: the window's model and the way out.
//!
//! The index is the session's: a press sends one call and the rows change when the next snapshot
//! says so, so nothing here predicts a result. A held window (the daemon gone) sends nothing, as
//! the page's buttons would not reach anyone.

use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, FeatureRow, Features};
use gpui_kit::{App, Entity};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;

#[derive(Clone)]
pub struct Ctx {
    pub heard: Entity<Heard>,
    pub caller: Caller,
}

impl Ctx {
    /// The index as last heard.
    pub fn features(&self, cx: &App) -> Option<Rc<Features>> {
        self.heard.read(cx).features.clone()
    }

    /// The row of `slug` the index lists now; `rowBySlug` of the page.
    pub fn row(&self, slug: &str, cx: &App) -> Option<FeatureRow> {
        self.features(cx)?.rows.iter().find(|row| row.slug == slug).cloned()
    }

    /// Sends `call` unless the window is held.
    pub fn send(&self, call: Call, cx: &App) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }
}
