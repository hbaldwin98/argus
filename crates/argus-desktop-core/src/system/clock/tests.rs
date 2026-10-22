use super::*;
use jiff::tz::{self, TimeZone};

/// 2026-10-21 15:30:00 UTC, in a zone `hours` from it.
fn at_offset(hours: i8) -> Clock {
    Clock::new("2026-10-21T15:30:00Z".parse().unwrap(), TimeZone::fixed(tz::offset(hours)))
}

fn utc() -> Clock {
    at_offset(0)
}

#[test]
fn a_time_today_says_its_clock_and_today() {
    assert_eq!(utc().clock(Some("2026-10-21T09:05:00Z")).as_deref(), Some("09:05 today"));
}

#[test]
fn a_time_the_day_before_says_yesterday_and_an_older_one_its_date() {
    assert_eq!(utc().clock(Some("2026-10-20T23:59:00Z")).as_deref(), Some("23:59 yesterday"));
    assert_eq!(utc().clock(Some("2026-10-12T13:52:00Z")).as_deref(), Some("13:52 Oct 12"));
    assert_eq!(utc().clock(Some("2026-01-02T00:00:00Z")).as_deref(), Some("00:00 Jan 2"));
}

#[test]
fn the_day_is_the_one_in_the_clocks_zone_not_the_times_own() {
    // The same instant, 20:00 UTC the day before: yesterday in London,
    // already today for someone five and a half hours east.
    let stamp = Some("2026-10-20T20:00:00Z");
    assert_eq!(utc().clock(stamp).as_deref(), Some("20:00 yesterday"));
    let east = Clock::new("2026-10-21T15:30:00Z".parse().unwrap(), TimeZone::fixed(tz::Offset::from_seconds(19_800).unwrap()));
    assert_eq!(east.clock(stamp).as_deref(), Some("01:30 today"));
}

#[test]
fn an_offset_in_the_text_is_read_in_the_clocks_zone() {
    assert_eq!(at_offset(-5).clock(Some("2026-10-21T09:05:00+02:00")).as_deref(), Some("02:05 today"));
}

#[test]
fn a_time_that_is_not_one_says_nothing() {
    for bad in [None, Some(""), Some("   "), Some("yesterday"), Some("2026-13-45T00:00:00Z")] {
        assert_eq!(utc().clock(bad), None, "{bad:?}");
        assert_eq!(utc().ago(bad), None, "{bad:?}");
        assert_eq!(utc().hhmm(bad), None, "{bad:?}");
    }
}

#[test]
fn ago_steps_through_the_largest_unit_that_fits() {
    // Each pair is how many seconds before now, and what it says.
    let table = [
        (0, "just now"),
        (59, "just now"),
        (60, "1m ago"),
        (3_599, "59m ago"),
        (3_600, "1h ago"),
        (86_399, "23h ago"),
        (86_400, "1d ago"),
        (604_799, "6d ago"),
    ];
    for (seconds, says) in table {
        let then = "2026-10-21T15:30:00Z".parse::<Timestamp>().unwrap() - jiff::SignedDuration::from_secs(seconds);
        assert_eq!(utc().ago(Some(&then.to_string())).as_deref(), Some(says), "{seconds} s");
    }
}

#[test]
fn ago_past_a_week_says_the_clock() {
    assert_eq!(utc().ago(Some("2026-10-14T15:30:00Z")).as_deref(), Some("15:30 Oct 14"));
}

#[test]
fn ago_rounds_to_the_second_and_a_time_ahead_of_now_is_just_now() {
    // 59.5 s rounds up to the minute, as the page's Math.round does.
    assert_eq!(utc().ago(Some("2026-10-21T15:29:00.500Z")).as_deref(), Some("1m ago"));
    assert_eq!(utc().ago(Some("2026-10-21T15:29:00.501Z")).as_deref(), Some("just now"));
    assert_eq!(utc().ago(Some("2026-10-21T18:00:00Z")).as_deref(), Some("just now"));
}

#[test]
fn hhmm_is_the_time_alone_in_the_clocks_zone() {
    assert_eq!(utc().hhmm(Some("2026-10-12T13:52:30Z")).as_deref(), Some("13:52"));
    assert_eq!(at_offset(2).hhmm(Some("2026-10-12T13:52:30Z")).as_deref(), Some("15:52"));
}

#[test]
fn a_span_within_one_day_says_the_day_once() {
    let span = utc().span(Some("2026-10-21T09:00:00Z"), Some("2026-10-21T10:15:00Z"));
    assert_eq!(span.as_deref(), Some("09:00\u{2013}10:15 today"));
}

#[test]
fn a_span_across_days_says_each_end_in_full() {
    let span = utc().span(Some("2026-10-20T23:00:00Z"), Some("2026-10-21T01:00:00Z"));
    assert_eq!(span.as_deref(), Some("23:00 yesterday \u{2013} 01:00 today"));
}

#[test]
fn a_span_is_cut_by_the_zone_not_by_the_texts_own_date() {
    // 23:00 and 01:00 UTC are one night in a zone two hours west.
    let west = at_offset(-2);
    let span = west.span(Some("2026-10-20T23:00:00Z"), Some("2026-10-21T01:00:00Z"));
    assert_eq!(span.as_deref(), Some("21:00\u{2013}23:00 yesterday"));
}

#[test]
fn a_span_with_an_end_it_cannot_read_falls_back_to_the_other() {
    assert_eq!(utc().span(Some("2026-10-21T09:00:00Z"), None).as_deref(), Some("09:00 today"));
    assert_eq!(utc().span(None, Some("2026-10-21T10:15:00Z")).as_deref(), Some("10:15 today"));
    assert_eq!(utc().span(Some("2026-10-21T09:00:00Z"), Some("nonsense")).as_deref(), Some("09:00 today"));
    assert_eq!(utc().span(None, None), None);
}

mod properties {
    use super::*;
    use proptest::prelude::*;

    // SHELL-TIME-2: whatever the time, a reading is always one of the four
    // phrasings and never panics, for any zone a person can be in.
    proptest! {
        #[test]
        fn ago_is_always_one_of_its_phrasings(back in -400_000_000i64..400_000_000, zone in -12i8..=14) {
            let now: Timestamp = "2026-10-21T15:30:00Z".parse().unwrap();
            let then = now - jiff::SignedDuration::from_secs(back);
            let clock = Clock::new(now, TimeZone::fixed(tz::offset(zone)));
            let says = clock.ago(Some(&then.to_string())).expect("a real time is read");
            let phrased = says == "just now"
                || ["m ago", "h ago", "d ago"].iter().any(|unit| says.ends_with(unit))
                || says.contains(" today") || says.contains(" yesterday")
                || MONTHS.iter().any(|month| says.contains(month));
            prop_assert!(phrased, "{says}");
        }

        // SHELL-TIME-3: a span inside one day is the two clocks around a dash, the
        // day once after them.
        #[test]
        fn a_span_inside_one_day_is_its_two_hhmms_and_the_day(
            minute_a in 0u32..1440, minute_b in 0u32..1440, zone in -12i8..=14,
        ) {
            let midnight: Timestamp = "2026-10-12T00:00:00Z".parse().unwrap();
            let clock = Clock::new("2026-10-21T15:30:00Z".parse().unwrap(), TimeZone::fixed(tz::offset(zone)));
            let at = |minute: u32| (midnight + jiff::SignedDuration::from_secs(i64::from(minute) * 60)).to_string();
            let (a, b) = (at(minute_a), at(minute_b));
            let span = clock.span(Some(&a), Some(&b)).expect("a span");
            let (from, to) = (clock.hhmm(Some(&a)).unwrap(), clock.hhmm(Some(&b)).unwrap());
            if span.contains(" \u{2013} ") {
                // Across a day boundary in this zone: each end carries its own day.
                prop_assert!(span.starts_with(&from) && span.contains(&to), "{span}");
            } else {
                prop_assert!(span.starts_with(&format!("{from}\u{2013}{to} ")), "{span}");
            }
        }
    }
}
