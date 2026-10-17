//! An opt-in measurement of where a keystroke's time goes.
//!
//! Typing lag has two very different causes that feel identical: the client
//! spending too long painting a frame, or the round trip out to the pty and
//! back taking too long to deliver the echo. Guessing between them is how
//! an afternoon disappears, so `ARGUS_PROFILE=1` writes one line a second
//! to `argus-profile.log` beside the daemon's log, with both numbers in it.

use std::io::Write;
use std::time::{Duration, Instant};

use argus_protocol::PaneId;

const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

/// One second of counters, in the shape they are reported.
#[derive(Default, PartialEq, Debug)]
pub struct Window {
    pub frames: u32,
    pub draw_total: Duration,
    pub draw_max: Duration,
    pub ui_total: Duration,
    pub ui_max: Duration,
    pub msgs: u32,
    pub keys: u32,
    pub echoes: u32,
    pub echo_total: Duration,
    pub echo_max: Duration,
}

impl Window {
    pub fn line(&self) -> String {
        format!(
            "frames={} draw_avg={:.1}ms draw_max={:.1}ms ui_avg={:.1}ms ui_max={:.1}ms msgs={} keys={} echo_avg={:.1}ms echo_max={:.1}ms",
            self.frames,
            avg_ms(self.draw_total, self.frames),
            ms(self.draw_max),
            avg_ms(self.ui_total, self.frames),
            ms(self.ui_max),
            self.msgs,
            self.keys,
            avg_ms(self.echo_total, self.echoes),
            ms(self.echo_max),
        )
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn avg_ms(total: Duration, n: u32) -> f64 {
    if n == 0 {
        0.0
    } else {
        ms(total) / f64::from(n)
    }
}

/// The counters, plus the one keystroke whose echo is still outstanding.
#[derive(Default)]
pub struct Counters {
    window: Window,
    awaiting: Option<(PaneId, Instant)>,
}

impl Counters {
    /// `took` is the whole frame; `ui` is the widget pass inside it, so a
    /// slow frame says whether the time went on deciding what to draw or on
    /// getting it to the terminal.
    pub fn draw(&mut self, took: Duration, ui: Duration) {
        self.window.frames += 1;
        self.window.draw_total += took;
        self.window.draw_max = self.window.draw_max.max(took);
        self.window.ui_total += ui;
        self.window.ui_max = self.window.ui_max.max(ui);
    }

    pub fn server_msg(&mut self) {
        self.window.msgs += 1;
    }

    /// A key went to `pane`. Only the first key of a run is timed: the ones
    /// behind it are waiting on the same round trip and would report the
    /// queue, not the latency.
    pub fn key(&mut self, pane: Option<PaneId>, now: Instant) {
        self.window.keys += 1;
        if let (Some(pane), None) = (pane, self.awaiting) {
            self.awaiting = Some((pane, now));
        }
    }

    /// Damage came back for `pane`. If a key to that pane is outstanding,
    /// this is its echo.
    pub fn damage(&mut self, pane: PaneId, now: Instant) {
        let Some((awaited, sent)) = self.awaiting else {
            return;
        };
        if awaited != pane {
            return;
        }
        self.awaiting = None;
        let took = now.duration_since(sent);
        self.window.echoes += 1;
        self.window.echo_total += took;
        self.window.echo_max = self.window.echo_max.max(took);
    }

    /// The window if it is due, resetting the counters. `None` while the
    /// second is still running, or when nothing happened in it.
    pub fn due(&mut self, now: Instant, since: &mut Instant) -> Option<Window> {
        if now.duration_since(*since) < FLUSH_INTERVAL {
            return None;
        }
        *since = now;
        let window = std::mem::take(&mut self.window);
        (window != Window::default()).then_some(window)
    }
}

pub struct Profile {
    counters: Counters,
    since: Instant,
    file: std::fs::File,
}

impl Profile {
    /// A profile only when asked for one, and only if the file opens —
    /// measurement must never be the reason the client fails to start.
    pub fn from_env() -> Option<Profile> {
        if std::env::var("ARGUS_PROFILE").ok().as_deref() != Some("1") {
            return None;
        }
        let path = argus_protocol::config_dir().join("argus-profile.log");
        let _ = std::fs::create_dir_all(path.parent()?);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        Some(Profile {
            counters: Counters::default(),
            since: Instant::now(),
            file,
        })
    }

    pub fn counters(&mut self) -> &mut Counters {
        &mut self.counters
    }

    pub fn flush(&mut self) {
        let now = Instant::now();
        let Some(window) = self.counters.due(now, &mut self.since) else {
            return;
        };
        let _ = writeln!(self.file, "{}", window.line());
    }
}

/// `Option<Profile>` is what the loop actually holds; these keep the call
/// sites from spelling out the same match every time.
pub trait Record {
    fn record(&mut self, f: impl FnOnce(&mut Counters));
    fn flush_due(&mut self);
}

impl Record for Option<Profile> {
    fn record(&mut self, f: impl FnOnce(&mut Counters)) {
        if let Some(profile) = self {
            f(profile.counters());
        }
    }

    fn flush_due(&mut self) {
        if let Some(profile) = self {
            profile.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_echo_timed_is_the_round_trip_of_the_first_key_of_a_run() {
        let mut c = Counters::default();
        let start = Instant::now();
        let pane = PaneId(1);

        c.key(Some(pane), start);
        c.key(Some(pane), start + Duration::from_millis(5));
        c.damage(pane, start + Duration::from_millis(20));

        let mut since = start;
        let window = c
            .due(start + FLUSH_INTERVAL, &mut since)
            .expect("a window with keys in it is worth reporting");
        assert_eq!(window.keys, 2);
        assert_eq!(window.echoes, 1, "the queued key must not be timed too");
        assert_eq!(window.echo_max, Duration::from_millis(20));
    }

    #[test]
    fn damage_from_another_pane_is_not_mistaken_for_the_echo() {
        let mut c = Counters::default();
        let start = Instant::now();

        c.key(Some(PaneId(1)), start);
        c.damage(PaneId(2), start + Duration::from_millis(3));

        let mut since = start;
        let window = c.due(start + FLUSH_INTERVAL, &mut since).unwrap();
        assert_eq!(window.echoes, 0);
    }

    #[test]
    fn a_quiet_second_writes_nothing() {
        let mut c = Counters::default();
        let start = Instant::now();
        let mut since = start;

        assert!(c.due(start + FLUSH_INTERVAL, &mut since).is_none());
    }

    #[test]
    fn the_window_is_not_reported_before_its_second_is_up() {
        let mut c = Counters::default();
        let start = Instant::now();
        let mut since = start;
        c.draw(Duration::from_millis(4), Duration::from_millis(1));

        assert!(c
            .due(start + Duration::from_millis(999), &mut since)
            .is_none());
        assert!(c.due(start + FLUSH_INTERVAL, &mut since).is_some());
    }

    #[test]
    fn the_line_carries_both_halves_of_the_latency() {
        let mut c = Counters::default();
        let start = Instant::now();
        c.draw(Duration::from_millis(2), Duration::from_millis(1));
        c.draw(Duration::from_millis(8), Duration::from_millis(3));
        c.server_msg();
        c.key(Some(PaneId(1)), start);
        c.damage(PaneId(1), start + Duration::from_millis(30));
        let mut since = start;

        let line = c.due(start + FLUSH_INTERVAL, &mut since).unwrap().line();

        assert!(line.contains("frames=2"), "{line}");
        assert!(line.contains("draw_avg=5.0ms"), "{line}");
        assert!(line.contains("draw_max=8.0ms"), "{line}");
        assert!(line.contains("ui_avg=2.0ms"), "{line}");
        assert!(line.contains("echo_max=30.0ms"), "{line}");
    }
}
