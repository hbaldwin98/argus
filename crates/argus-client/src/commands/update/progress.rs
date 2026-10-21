//! How `argus update` shows what it is doing: a bar while the archive
//! downloads, redrawn in place, then one line per step as each is done.
//!
//! On a terminal it draws in colour, unless `NO_COLOR` is set; piped or
//! redirected, it writes the step lines alone, with no bar and no escape
//! sequence, so a log of it reads cleanly.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use crossterm::style::{Attribute, Color, Stylize};

/// The bar is redrawn no oftener than this, so a fast download does not
/// spend its time writing to the terminal.
const REDRAW: Duration = Duration::from_millis(50);
const WIDEST_BAR: usize = 36;

pub struct Progress {
    out: Box<dyn Write + Send>,
    terminal: bool,
    colour: bool,
    /// The terminal's width when it is not to be asked, for a test.
    columns: Option<usize>,
    started: Instant,
    drawn: Option<Instant>,
}

impl Progress {
    pub fn on_stderr() -> Self {
        let terminal = std::io::stderr().is_terminal();
        let colour = terminal && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty());
        Progress {
            out: Box::new(std::io::stderr()),
            terminal,
            colour,
            columns: None,
            started: Instant::now(),
            drawn: None,
        }
    }

    /// A heading, set apart from the steps under it on a terminal.
    pub fn heading(&mut self, text: &str) {
        let text = if self.colour { text.attribute(Attribute::Bold).to_string() } else { text.to_string() };
        let gap = if self.terminal { "\n" } else { "" };
        self.write(&format!("{text}\n{gap}"));
    }

    fn write(&mut self, text: &str) {
        let _ = self.out.write_all(text.as_bytes());
        let _ = self.out.flush();
    }

    /// Starts the clock the bar's speed is measured by.
    pub fn start_download(&mut self) {
        self.started = Instant::now();
        self.drawn = None;
    }

    /// Redraws the bar for `done` bytes of `total`, when it is time to.
    pub fn downloaded(&mut self, done: u64, total: Option<u64>) {
        if !self.terminal {
            return;
        }
        let now = Instant::now();
        let finished = total.is_some_and(|total| done >= total);
        if !finished && self.drawn.is_some_and(|drawn| now - drawn < REDRAW) {
            return;
        }
        self.drawn = Some(now);
        let stats = stats(done, total, now - self.started);
        let columns = self
            .columns
            .unwrap_or_else(|| crossterm::terminal::size().map_or(80, |(columns, _)| columns as usize));
        let room = columns.saturating_sub(stats.chars().count() + 6).min(WIDEST_BAR);
        let line = match total {
            Some(total) if room >= 10 => {
                let (filled, track) = bar(done as f64 / total.max(1) as f64, room);
                if self.colour {
                    format!("{}{}  {}", filled.with(Color::Cyan), track.with(Color::DarkGrey), stats.with(Color::Grey))
                } else {
                    format!("{filled}{track}  {stats}")
                }
            }
            _ => stats,
        };
        self.write(&format!("\r\x1b[2K  {line}"));
    }

    /// How long since [`Self::start_download`].
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// A step done, in place of the bar if one is drawn.
    pub fn done(&mut self, step: &str, detail: &str) {
        self.line("✓", Color::Green, step, detail);
    }

    pub fn failed(&mut self, step: &str, detail: &str) {
        self.line("✗", Color::Red, step, detail);
    }

    /// Something to know that is neither done nor failed.
    pub fn note(&mut self, step: &str, detail: &str) {
        self.line("•", Color::Yellow, step, detail);
    }

    fn line(&mut self, mark: &str, colour: Color, step: &str, detail: &str) {
        let clear = if self.terminal { "\r\x1b[2K" } else { "" };
        self.drawn = None;
        let step = format!("{step:<11}");
        let line = if self.colour {
            format!("{clear}  {} {} {}\n", mark.with(colour), step.attribute(Attribute::Bold), detail.with(Color::Grey))
        } else if self.terminal {
            format!("{clear}  {mark} {step} {detail}\n")
        } else {
            format!("{} {detail}\n", step.trim_end())
        };
        self.write(&line);
    }
}

/// The bar's filled part and its track, `width` cells together, in half
/// cells: a filled cell is `━`, and a half one ends it with `╸`.
fn bar(fraction: f64, width: usize) -> (String, String) {
    let halves = (fraction.clamp(0.0, 1.0) * (width * 2) as f64).round() as usize;
    let (full, half) = (halves / 2, halves % 2);
    let filled = "━".repeat(full) + if half == 1 { "╸" } else { "" };
    let track = "━".repeat(width - full - half);
    (filled, track)
}

/// `11.2 / 17.5 MB · 6.4 MB/s · 1s left`, or without the total, and so
/// without what is left, when the server did not say how much there is.
fn stats(done: u64, total: Option<u64>, elapsed: Duration) -> String {
    let seconds = elapsed.as_secs_f64();
    let rate = if seconds > 0.0 { done as f64 / seconds } else { 0.0 };
    let speed = format!("{}/s", size(rate as u64));
    match total {
        Some(total) => {
            let left = if rate > 0.0 { (total.saturating_sub(done) as f64 / rate).ceil() as u64 } else { 0 };
            let left = if done >= total || rate == 0.0 { String::new() } else { format!(" · {}", time_left(left)) };
            format!("{} / {} · {speed}{left}", megabytes(done), size(total))
        }
        None => format!("{} · {speed}", size(done)),
    }
}

/// `17.5 MB`, or `512 KB` under a megabyte.
pub fn size(bytes: u64) -> String {
    if bytes < 1024 * 1024 {
        format!("{} KB", bytes.div_ceil(1024))
    } else {
        format!("{} MB", megabytes(bytes))
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / (1024.0 * 1024.0))
}

fn time_left(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds}s left"),
        _ => format!("{}m {:02}s left", seconds / 60, seconds % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// What a progress wrote, shared with the test reading it.
    #[derive(Clone, Default)]
    struct Written(Arc<Mutex<Vec<u8>>>);

    impl Write for Written {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Written {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    fn progress(terminal: bool, colour: bool) -> (Progress, Written) {
        let written = Written::default();
        let progress = Progress {
            out: Box::new(written.clone()),
            terminal,
            colour,
            columns: Some(100),
            started: Instant::now() - Duration::from_secs(2),
            drawn: None,
        };
        (progress, written)
    }

    /// Plays a whole update through `progress`: heading, a bar redrawn as
    /// the archive arrives, then the steps.
    fn update(progress: &mut Progress) {
        let mb = 1024 * 1024;
        progress.heading("Updating Argus 0.38.0 → 0.38.1");
        for done in [0, 4 * mb, 9 * mb, 16 * mb] {
            progress.drawn = None;
            progress.downloaded(done, Some(16 * mb));
        }
        progress.done("Downloaded", "16.0 MB in 2.0s");
        progress.done("Verified", "SHA-256 matches the release's");
        progress.done("Installed", "4 executables in /opt/argus/bin");
        progress.note("Restart", "argusd still runs the old version");
    }

    #[test]
    fn piped_it_writes_the_steps_alone_and_no_escape() {
        let (mut progress, written) = progress(false, false);
        update(&mut progress);
        let text = written.text();
        assert!(!text.contains('\x1b') && !text.contains('\r') && !text.contains('━'), "{text:?}");
        assert!(text.starts_with("Updating Argus 0.38.0 → 0.38.1\nDownloaded 16.0 MB in 2.0s\n"), "{text:?}");
    }

    #[test]
    fn on_a_terminal_without_colour_the_bar_is_drawn_and_replaced_in_place() {
        let (mut progress, written) = progress(true, false);
        update(&mut progress);
        let text = written.text();
        assert!(text.contains("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  16.0 / 16.0 MB"), "{text:?}");
        assert!(text.contains("\r\x1b[2K  ✓ Downloaded  16.0 MB in 2.0s\n"), "{text:?}");
        assert!(!text.contains("\x1b[3"), "no colour: {text:?}");
    }

    /// The frames an update draws on a terminal, each redraw of the bar on
    /// a line of its own: `cargo test -p argus dump_update_progress --
    /// --ignored --nocapture`, which also shows the colours.
    #[test]
    #[ignore]
    fn dump_update_progress() {
        let (mut progress, written) = progress(true, true);
        update(&mut progress);
        for frame in written.text().split("\r\x1b[2K") {
            println!("{}", frame.strip_suffix('\n').unwrap_or(frame));
        }
    }

    #[test]
    fn the_bar_fills_by_half_cells_and_keeps_its_width() {
        assert_eq!(bar(0.0, 4), (String::new(), "━━━━".into()));
        assert_eq!(bar(0.5, 4), ("━━".into(), "━━".into()));
        assert_eq!(bar(0.375, 4), ("━╸".into(), "━━".into()));
        assert_eq!(bar(1.0, 4), ("━━━━".into(), String::new()));
        for step in 0..=100 {
            let (filled, track) = bar(step as f64 / 100.0, 13);
            assert_eq!(filled.chars().count() + track.chars().count(), 13, "{step}%");
        }
    }

    #[test]
    fn a_fraction_past_either_end_is_held_to_it() {
        assert_eq!(bar(1.7, 3), ("━━━".into(), String::new()));
        assert_eq!(bar(-1.0, 3), (String::new(), "━━━".into()));
    }

    #[test]
    fn the_stats_say_how_far_how_fast_and_how_long_is_left() {
        let mb = 1024 * 1024;
        assert_eq!(stats(4 * mb, Some(16 * mb), Duration::from_secs(2)), "4.0 / 16.0 MB · 2.0 MB/s · 6s left");
        assert_eq!(stats(16 * mb, Some(16 * mb), Duration::from_secs(8)), "16.0 / 16.0 MB · 2.0 MB/s");
        assert_eq!(stats(3 * mb, None, Duration::from_secs(1)), "3.0 MB · 3.0 MB/s");
        assert_eq!(stats(0, Some(16 * mb), Duration::ZERO), "0.0 / 16.0 MB · 0 KB/s");
    }

    #[test]
    fn a_long_wait_is_said_in_minutes() {
        assert_eq!(time_left(59), "59s left");
        assert_eq!(time_left(125), "2m 05s left");
    }
}
