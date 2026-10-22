//! The room a cell tells the daemon (SHELL-BROWSER-11): the screen's box, as it is laid out, and
//! the display's pixel density.
//!
//! Core decides what is worth sending (nothing under 50 px, nothing already told, one report in
//! 100 ms a browser, a reconnect telling it again); this measures the box and hands core each
//! change, and asks core again when it says a held size may go. Both sides read one clock, the
//! process's own, since core's throttle is a function of it.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use argus_desktop_core::connection::page::Call;
use gpui_kit::{canvas, prelude::*, App, Bounds, EntityId, IntoElement, Pixels, Window};

use crate::connection::caller::Caller;

/// The window's clock: the time since the first read, which is when the first cell reported.
pub fn now() -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

/// What the box was last reported as, whole pixels and the density, so a frame that lays it out
/// the same says nothing.
type Told = Option<(u32, u32, u32)>;

/// One cell's reports. A handle: the paint and the timer it arms share one.
#[derive(Clone)]
pub struct Reporter {
    pane: u64,
    caller: Caller,
    told: Rc<Cell<Told>>,
    /// When the timer in flight fires, so a report that is held back arms one only if none is.
    armed: Rc<Cell<Option<Duration>>>,
}

impl Reporter {
    pub fn new(pane: u64, caller: Caller) -> Self {
        Self { pane, caller, told: Rc::default(), armed: Rc::default() }
    }

    /// Records the area its parent was laid out in, and reports it when it is not the one last
    /// reported. It fills its parent and takes no room. `owner` is repainted when the box changes
    /// so a click maps against the box that is drawn.
    pub fn measure(&self, into: Rc<Cell<Bounds<Pixels>>>, owner: EntityId) -> impl IntoElement {
        let reporter = self.clone();
        canvas(
            move |bounds, window, cx| {
                let moved = into.get().size != bounds.size;
                into.set(bounds);
                reporter.report(bounds, window, cx);
                if moved {
                    cx.notify(owner);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }

    fn report(&self, bounds: Bounds<Pixels>, window: &Window, cx: &mut App) {
        let (width, height) = (f64::from(bounds.size.width), f64::from(bounds.size.height));
        let scale = f64::from(window.scale_factor());
        let key = (width.floor() as u32, height.floor() as u32, (scale * 1000.0) as u32);
        if self.told.get() == Some(key) {
            return;
        }
        self.told.set(Some(key));
        self.caller.call(Call::BrowserViewport { pane: self.pane, width, height, scale, at: now() });
        self.arm(cx);
    }

    /// Asks core again when it says a held size may go: one timer at a time, the soonest. Called on
    /// every change of what is heard too, since a reconnect parks the last size again with no box
    /// changing to report it.
    pub fn arm(&self, cx: &mut App) {
        let Some(due) = self.caller.next_viewport() else { return };
        if self.armed.get().is_some_and(|armed| armed <= due) {
            return;
        }
        self.armed.set(Some(due));
        let wait = due.saturating_sub(now());
        let this = self.clone();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(wait).await;
            this.armed.set(None);
            this.caller.call(Call::BrowserViewportsDue { at: now() });
            cx.update(|cx| this.arm(cx));
        })
        .detach();
    }
}

