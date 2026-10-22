//! What a cell tells the daemon of the room it has (SHELL-BROWSER-11): the
//! box floored to whole pixels and the display's scale, so the page is laid
//! out at that size and pictured one to one.
//!
//! The window reports the box whenever it changes; this decides which of
//! those reports are sent. Nothing is sent under 50 px (a box not on
//! screen), nor what was last sent for that browser, and a report coming
//! faster than `EVERY` after the last is held and only the newest of the
//! held is sent when its time comes. The clock is a `Duration` the caller
//! reads however it likes, so this is a pure function of the reports and
//! of that clock. A reconnect empties what was told, not what the cell
//! last reported, so the new daemon is told again without the box changing.

use std::collections::BTreeMap;
use std::time::Duration;

use argus_protocol::PaneId;

/// The least a box may be, a side, to be told to the daemon.
pub const LEAST: u32 = 50;
/// The soonest one report follows another for the same browser: a window
/// dragged by its edge changes the box every frame, and the daemon re-lays
/// the page out for each it is told.
pub const EVERY: Duration = Duration::from_millis(100);

/// A size and density to lay a page out at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

#[derive(Default)]
pub struct Viewports {
    /// What each browser was last told, and when.
    told: BTreeMap<PaneId, (Viewport, Duration)>,
    /// What each cell last reported that is worth telling, sent or not.
    last: BTreeMap<PaneId, Viewport>,
    /// Reports that came too soon, or for a browser not open yet, or that
    /// a reconnect has made untold.
    parked: BTreeMap<PaneId, Viewport>,
}

impl Viewports {
    /// A cell's box changed to `width` x `height` at `scale`. What is to be
    /// sent now, if anything; `open` is whether the daemon has this
    /// browser, which is the only kind it can be told of.
    pub fn report(&mut self, pane: PaneId, width: f64, height: f64, scale: f64, open: bool, now: Duration) -> Option<Viewport> {
        // `as u32` saturates, and sends NaN to 0, which is under `LEAST`.
        let (width, height) = (width.floor() as u32, height.floor() as u32);
        if width < LEAST || height < LEAST {
            self.parked.remove(&pane);
            return None;
        }
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        let viewport = Viewport { width, height, scale };
        self.last.insert(pane, viewport);
        self.parked.insert(pane, viewport);
        self.release(pane, open, now)
    }

    /// The parked report of `pane`, if it may go now.
    fn release(&mut self, pane: PaneId, open: bool, now: Duration) -> Option<Viewport> {
        let viewport = *self.parked.get(&pane)?;
        match self.told.get(&pane) {
            Some((told, _)) if *told == viewport => {
                self.parked.remove(&pane);
                return None;
            }
            Some((_, at)) if now.saturating_sub(*at) < EVERY => return None,
            _ => {}
        }
        if !open {
            return None;
        }
        self.parked.remove(&pane);
        self.told.insert(pane, (viewport, now));
        Some(viewport)
    }

    /// What is due at `now`: the held reports whose time has come.
    pub fn due(&mut self, now: Duration, open: impl Fn(PaneId) -> bool) -> Vec<(PaneId, Viewport)> {
        let panes: Vec<PaneId> = self.parked.keys().copied().collect();
        panes.into_iter().filter_map(|pane| self.release(pane, open(pane), now).map(|viewport| (pane, viewport))).collect()
    }

    /// When the soonest report held for an open browser may go, so the
    /// window knows when to ask again; `None` with nothing held.
    pub fn next(&self, open: impl Fn(PaneId) -> bool) -> Option<Duration> {
        self.parked
            .keys()
            .filter(|pane| open(**pane))
            .map(|pane| self.told.get(pane).map_or(Duration::ZERO, |(_, at)| *at + EVERY))
            .min()
    }

    /// A new connection has told the daemon nothing: every browser is
    /// told again what its cell last reported.
    pub fn reconnected(&mut self) {
        self.told.clear();
        self.parked = self.last.clone();
    }

    /// The browser is gone, and its cell with it.
    pub fn closed(&mut self, pane: PaneId) {
        self.told.remove(&pane);
        self.last.remove(&pane);
        self.parked.remove(&pane);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: PaneId = PaneId(1);

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_box_under_fifty_pixels_a_side_is_not_told() {
        let mut views = Viewports::default();
        assert_eq!(views.report(P, 49.9, 400.0, 1.0, true, ms(0)), None);
        assert_eq!(views.report(P, 400.0, 49.0, 1.0, true, ms(0)), None);
        assert_eq!(views.report(P, f64::NAN, 400.0, 1.0, true, ms(0)), None);
        assert_eq!(views.report(P, 50.0, 50.0, 2.0, true, ms(0)), Some(Viewport { width: 50, height: 50, scale: 2.0 }));
    }

    #[test]
    fn a_size_is_floored_and_an_unknown_scale_is_one() {
        let mut views = Viewports::default();
        assert_eq!(views.report(P, 640.9, 480.2, 0.0, true, ms(0)), Some(Viewport { width: 640, height: 480, scale: 1.0 }));
    }

    #[test]
    fn what_was_told_is_not_told_again() {
        let mut views = Viewports::default();
        assert!(views.report(P, 640.0, 480.0, 1.0, true, ms(0)).is_some());
        assert_eq!(views.report(P, 640.4, 480.9, 1.0, true, ms(500)), None, "the same whole pixels");
        assert!(views.report(P, 641.0, 480.0, 1.0, true, ms(600)).is_some());
        assert!(views.report(P, 641.0, 480.0, 1.5, true, ms(900)).is_some(), "a new density is a new viewport");
    }

    #[test]
    fn a_report_that_comes_too_soon_is_held_and_the_newest_goes_when_its_time_comes() {
        let mut views = Viewports::default();
        let open = |_| true;
        assert!(views.report(P, 600.0, 400.0, 1.0, true, ms(0)).is_some());
        assert_eq!(views.report(P, 610.0, 400.0, 1.0, true, ms(30)), None);
        assert_eq!(views.report(P, 620.0, 400.0, 1.0, true, ms(60)), None);
        assert_eq!(views.next(open), Some(EVERY));
        assert!(views.due(ms(99), open).is_empty());
        assert_eq!(views.due(ms(100), open), [(P, Viewport { width: 620, height: 400, scale: 1.0 })]);
        assert_eq!(views.next(open), None);
        assert!(views.due(ms(500), open).is_empty(), "told once");
    }

    #[test]
    fn a_held_report_that_comes_back_to_what_was_told_is_dropped() {
        let mut views = Viewports::default();
        assert!(views.report(P, 600.0, 400.0, 1.0, true, ms(0)).is_some());
        assert_eq!(views.report(P, 700.0, 400.0, 1.0, true, ms(10)), None);
        assert_eq!(views.report(P, 600.0, 400.0, 1.0, true, ms(20)), None);
        assert!(views.due(ms(200), |_| true).is_empty());
    }

    #[test]
    fn a_reconnect_tells_the_new_daemon_the_last_size_again() {
        let mut views = Viewports::default();
        assert!(views.report(P, 600.0, 400.0, 1.0, true, ms(0)).is_some());
        views.reconnected();
        assert_eq!(views.due(ms(1), |_| false), [], "not before the daemon replays the browser");
        assert_eq!(views.due(ms(2), |_| true), [(P, Viewport { width: 600, height: 400, scale: 1.0 })]);
    }

    #[test]
    fn a_cell_that_reports_before_its_browser_is_open_is_told_when_it_opens() {
        let mut views = Viewports::default();
        assert_eq!(views.report(P, 600.0, 400.0, 1.0, false, ms(0)), None);
        assert_eq!(views.next(|_| false), None);
        assert_eq!(views.due(ms(1), |_| true).len(), 1);
    }

    #[test]
    fn a_closed_browser_leaves_nothing_to_tell() {
        let mut views = Viewports::default();
        views.report(P, 600.0, 400.0, 1.0, true, ms(0));
        views.report(P, 700.0, 400.0, 1.0, true, ms(1));
        views.closed(P);
        views.reconnected();
        assert!(views.due(ms(500), |_| true).is_empty());
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // SHELL-BROWSER-11: nothing is sent under 50 px, and nothing twice in a row; reports
            // and the clock only ever move forward.
            #[test]
            fn what_is_sent_is_big_enough_and_never_the_same_twice(
                steps in prop::collection::vec((0.0f64..900.0, 0.0f64..900.0, 1u64..80), 1..40),
            ) {
                let mut views = Viewports::default();
                let (mut now, mut last) = (0u64, None);
                let mut sent = Vec::new();
                for (w, h, dt) in steps {
                    now += dt;
                    sent.extend(views.report(P, w, h, 1.0, true, ms(now)));
                    sent.extend(views.due(ms(now), |_| true).into_iter().map(|(_, v)| v));
                }
                sent.extend(views.due(ms(now + 1000), |_| true).into_iter().map(|(_, v)| v));
                for v in sent {
                    prop_assert!(v.width >= LEAST && v.height >= LEAST);
                    prop_assert_ne!(Some(v), last);
                    last = Some(v);
                }
            }
        }
    }
}
