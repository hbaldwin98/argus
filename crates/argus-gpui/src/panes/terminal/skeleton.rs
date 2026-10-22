//! Bars that stand in for a pane a screen is waiting on, after a delay (SHELL-SCREEN-12).
//!
//! A screen waits while the pane its side should show is not the pane its rows are of: the rail
//! picked another, or the console chose a tab. A live pane's first screen comes well inside
//! [`DELAY`], so the bars would only flash; they show when it does not. The first frame of the
//! pane, or the side showing nothing, takes them away.

use std::time::Duration;

use gpui_kit::{Context, Task};

use super::Terminal;

/// How long a pane being switched to may take before the bars show.
pub const DELAY: Duration = Duration::from_millis(250);

/// The bars' widths as shares of the row, from the page's `waiting` list.
pub const BARS: &[f32] = &[0.55, 0.8, 0.7, 0.85, 0.4, 0.75, 0.6];

#[derive(Default)]
pub struct Skeleton {
    /// The pane the screen is waiting for.
    waiting: Option<u64>,
    shown: bool,
    timer: Option<Task<()>>,
}

impl Skeleton {
    /// Whether the bars are up.
    pub fn shown(&self) -> bool {
        self.shown
    }
}

impl Terminal {
    /// Works out whether this screen is waiting, from what `Heard` says now, and starts the
    /// delay when it newly is. Called wherever that may have changed: a frame, the rail, the console.
    pub(super) fn expect(&mut self, cx: &mut Context<Self>) {
        let heard = self.heard.read(cx);
        let waiting = heard.pane_of(self.side).filter(|pane| heard.screen_of(self.side).pane() != Some(*pane));
        if waiting == self.skeleton.waiting {
            return;
        }
        // Another pane to wait for starts the delay again; none ends it and takes the bars down.
        self.skeleton.waiting = waiting;
        self.skeleton.shown = false;
        self.skeleton.timer = waiting.map(|pane| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(DELAY).await;
                this.update(cx, |this, cx| {
                    if this.skeleton.waiting == Some(pane) {
                        this.skeleton.shown = true;
                        cx.notify();
                    }
                })
                .ok();
            })
        });
    }
}
