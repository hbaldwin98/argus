//! Times the daemon sends, said in the person's own clock: `13:52 today`,
//! `13:52 yesterday`, `Oct 12 13:52`, `15m ago`.
//!
//! The page's `kit/clock.ts` does this with the webview's time zone; a window
//! with no webview needs the zone too, so both the moment it is and the zone
//! it is said in are given to a [`Clock`], and a test pins both. This is not
//! `argus_protocol::ago` (`16m`, `2h`, `—`), which the rail's `used` column
//! says: the two texts differ on purpose (SHELL-TIME-4) and neither stands in
//! for the other. Month names are English whatever the system's language is;
//! the page's come from the webview's locale, which this does not read.

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The moment it is and the zone it is said in.
#[derive(Debug, Clone)]
pub struct Clock {
    now: Timestamp,
    zone: TimeZone,
}

impl Clock {
    pub fn new(now: Timestamp, zone: TimeZone) -> Self {
        Self { now, zone }
    }

    /// The machine's moment and zone, read now.
    pub fn local() -> Self {
        Self::new(Timestamp::now(), TimeZone::system())
    }

    /// `at` as it falls in this clock's zone, or `None` when it says none:
    /// unset, empty, or not RFC 3339.
    fn parsed(&self, at: Option<&str>) -> Option<Zoned> {
        let stamp: Timestamp = at?.trim().parse().ok()?;
        Some(stamp.to_zoned(self.zone.clone()))
    }

    /// `at` as it falls in this clock's zone, for a vocabulary of times other than this module's
    /// (`views::values`); none when it is not RFC 3339.
    pub fn zoned(&self, at: &str) -> Option<Zoned> {
        self.parsed(Some(at))
    }

    /// The moment this clock says it is, in its zone.
    pub fn now_zoned(&self) -> Zoned {
        self.now.to_zoned(self.zone.clone())
    }

    fn time(at: &Zoned) -> String {
        format!("{:02}:{:02}", at.hour(), at.minute())
    }

    /// The day `at` falls on, from where the person is looking.
    fn day(&self, at: &Zoned) -> String {
        let today = self.now.to_zoned(self.zone.clone()).date();
        let date = at.date();
        if date == today {
            "today".to_owned()
        } else if today.yesterday().is_ok_and(|yesterday| date == yesterday) {
            "yesterday".to_owned()
        } else {
            short(date)
        }
    }

    /// `at` as a time and its day (SHELL-TIME-1), or `None` when it says none.
    pub fn clock(&self, at: Option<&str>) -> Option<String> {
        let at = self.parsed(at)?;
        Some(format!("{} {}", Self::time(&at), self.day(&at)))
    }

    /// How long ago, for a recent time: `15m ago`; the day and time past a
    /// week (SHELL-TIME-2).
    pub fn ago(&self, at: Option<&str>) -> Option<String> {
        let stamp = self.parsed(at)?;
        // Rounded to the second as the page's `Math.round` does, and a time
        // ahead of now is "just now" rather than negative.
        let millis = self.now.as_millisecond() - stamp.timestamp().as_millisecond();
        let seconds = (millis + 500).div_euclid(1000).max(0);
        Some(match seconds {
            0..=59 => "just now".to_owned(),
            60..=3599 => format!("{}m ago", seconds / 60),
            3600..=86_399 => format!("{}h ago", seconds / 3600),
            86_400..=604_799 => format!("{}d ago", seconds / 86_400),
            _ => format!("{} {}", Self::time(&stamp), self.day(&stamp)),
        })
    }

    /// A time alone, for a line of something said: its day is the session's
    /// (SHELL-TIME-3).
    pub fn hhmm(&self, at: Option<&str>) -> Option<String> {
        self.parsed(at).map(|at| Self::time(&at))
    }

    /// From `start` to `end`, the day said once when both fall on it
    /// (SHELL-TIME-3). With either unreadable it says the other's clock.
    pub fn span(&self, start: Option<&str>, end: Option<&str>) -> Option<String> {
        let (Some(from), Some(to)) = (self.parsed(start), self.parsed(end)) else {
            return self.clock(start.or(end));
        };
        if from.date() == to.date() {
            Some(format!("{}\u{2013}{} {}", Self::time(&from), Self::time(&to), self.day(&to)))
        } else {
            Some(format!("{} \u{2013} {}", self.clock(start)?, self.clock(end)?))
        }
    }
}

/// `Oct 12`.
pub fn short(date: Date) -> String {
    format!("{} {}", MONTHS[usize::from(date.month().unsigned_abs()) - 1], date.day())
}

#[cfg(test)]
mod tests;
