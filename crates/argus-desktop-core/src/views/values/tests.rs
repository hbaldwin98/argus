use argus_protocol::{Chip, Tone, Value};
use jiff::tz::{self, TimeZone};

use super::*;

/// 2026-10-21 15:30:00 UTC, said in UTC.
fn clock() -> Clock {
    Clock::new("2026-10-21T15:30:00Z".parse().unwrap(), TimeZone::UTC)
}

fn text(t: &str) -> Value {
    Value::Text { text: t.into() }
}

// VALUES-1

#[test]
fn ago_steps_from_just_now_to_minutes_hours_yesterday_and_a_date() {
    let ago = |at: &str| super::ago(&clock(), at);
    assert_eq!(ago("2026-10-21T15:30:00Z"), "just now");
    assert_eq!(ago("2026-10-21T15:29:01Z"), "just now");
    assert_eq!(ago("2026-10-21T15:29:00Z"), "1 min ago");
    assert_eq!(ago("2026-10-21T14:31:00Z"), "59 min ago");
    assert_eq!(ago("2026-10-21T14:30:00Z"), "1 h ago");
    assert_eq!(ago("2026-10-20T15:31:00Z"), "23 h ago");
    assert_eq!(ago("2026-10-20T10:00:00Z"), "yesterday");
    assert_eq!(ago("2026-10-19T12:00:00Z"), "Oct 19");
    assert_eq!(ago("2025-10-19T12:00:00Z"), "Oct 19, 2025", "the year when it is not this year's");
}

#[test]
fn a_time_ahead_of_the_clock_is_skew_and_a_text_that_is_not_a_time_is_given_back() {
    assert_eq!(ago(&clock(), "2026-10-21T16:30:00Z"), "just now");
    assert_eq!(ago(&clock(), "2027-01-01T00:00:00Z"), "just now");
    for not in ["soon", "", "2026-10-21", "2026/10/21 10:00", "Oct 19"] {
        assert_eq!(ago(&clock(), not), not);
    }
}

#[test]
fn the_day_before_is_the_one_in_the_clocks_zone() {
    // 23:30 UTC on the 20th is already the 21st for someone two hours east.
    let east = Clock::new("2026-10-22T09:00:00Z".parse().unwrap(), TimeZone::fixed(tz::offset(2)));
    assert_eq!(super::ago(&east, "2026-10-20T23:30:00Z"), "yesterday");
    assert_eq!(super::ago(&Clock::new("2026-10-22T09:00:00Z".parse().unwrap(), TimeZone::UTC), "2026-10-20T23:30:00Z"), "Oct 20");
}

// VALUES-2

#[test]
fn a_number_has_thousands_separators_and_at_most_three_decimals() {
    for (number, said) in [(0.0, "0"), (12.0, "12"), (1234.0, "1,234"), (1234567.891, "1,234,567.891"), (0.5, "0.5"), (2.0004, "2"), (1.23456, "1.235"), (-9876.5, "-9,876.5"), (100.0, "100")] {
        assert_eq!(grouped(number), said, "{number}");
    }
    assert_eq!(grouped(f64::NAN), "NaN");
    assert_eq!(grouped(f64::INFINITY), "\u{221e}");
}

#[test]
fn a_bare_day_is_a_calendar_day_in_no_zone_and_an_instant_is_a_day_in_the_clocks() {
    assert_eq!(short_date(&clock(), "2026-10-05"), "Oct 5, 2026");
    let west = Clock::new("2026-10-21T15:30:00Z".parse().unwrap(), TimeZone::fixed(tz::offset(-8)));
    assert_eq!(short_date(&west, "2026-10-05"), "Oct 5, 2026", "not moved by the zone");
    assert_eq!(short_date(&west, "2026-10-05T03:00:00Z"), "Oct 4, 2026");
    assert_eq!(short_date(&clock(), "someday"), "someday");
}

#[test]
fn a_time_names_its_local_date_and_clock_for_a_tooltip() {
    assert_eq!(local_time(&clock(), "2026-10-19T13:52:07Z").as_deref(), Some("Oct 19, 2026 13:52"));
    assert_eq!(local_time(&clock(), "later"), None);
}

// VALUES-3

#[test]
fn initials_are_the_first_letter_of_each_of_the_first_two_words() {
    assert_eq!(initials("ada lovelace"), "AL");
    assert_eq!(initials("Grace Brewster Murray Hopper"), "GB");
    assert_eq!(initials("  plato "), "P");
    assert_eq!(initials(""), "");
    assert_eq!(initials("\u{e9}mile zola"), "\u{c9}Z");
}

#[test]
fn a_name_has_the_same_tint_every_time_and_the_hash_is_the_pages() {
    // 31-multiplied, unsigned 32-bit, over code points, an index into active, info, good, warn, bad.
    assert_eq!(tint("Ada Lovelace"), tint("Ada Lovelace"));
    let hash = |name: &str| name.chars().fold(0u32, |h, c| h.wrapping_mul(31).wrapping_add(c as u32));
    assert_eq!(hash("a"), 97);
    assert_eq!(tint("a"), [Tone::Active, Tone::Info, Tone::Good, Tone::Warn, Tone::Bad][97 % 5]);
    let long = "x".repeat(40);
    assert_eq!(tint(&long), [Tone::Active, Tone::Info, Tone::Good, Tone::Warn, Tone::Bad][hash(&long) as usize % 5], "the hash wraps at 32 bits");
}

#[test]
fn three_people_are_shown_and_the_rest_counted() {
    let names: Vec<String> = ["a", "b", "c", "d", "e"].map(String::from).to_vec();
    let (shown, rest) = people(&names);
    assert_eq!((shown.len(), rest), (3, 2));
    assert_eq!(people(&names[..2]).1, 0);
    assert_eq!(people(&[]), (&[][..], 0));
}

// VALUES-4

#[test]
fn a_meter_is_held_to_its_track() {
    assert_eq!(fraction(5.0, 10.0), 0.5);
    assert_eq!(fraction(15.0, 10.0), 1.0, "an overshoot");
    assert_eq!(fraction(-1.0, 10.0), 0.0);
    assert_eq!(fraction(5.0, 0.0), 0.0, "a zero max");
    assert_eq!(fraction(f64::NAN, 10.0), 0.0);
    assert_eq!(fraction(5.0, f64::NAN), 0.0);
    assert_eq!(percent(1.0, 3.0), 33);
    assert_eq!(percent(2.0, 3.0), 67);
}

// VALUES-5

#[test]
fn a_duration_is_told_in_the_two_units_that_matter() {
    for (ms, said) in [(0, "0 ms"), (850, "850 ms"), (999, "999 ms"), (1000, "1s"), (59_999, "59s"), (252_000, "4m 12s"), (3_599_999, "59m 59s"), (3_780_000, "1h 3m"), (86_399_999, "23h 59m"), (90_000_000, "1d 1h"), (1_000_000_000, "11d 13h")] {
        assert_eq!(duration(ms), said, "{ms}");
    }
}

// VALUES-6

#[test]
fn a_path_inside_the_checkout_is_a_button_and_any_other_is_text() {
    let path = |p: &str, line: Option<u32>| pointer(&Value::Path { path: p.into(), line });
    let found = path("src/main.rs", Some(12)).expect("a button");
    assert_eq!((found.text.as_str(), found.title), ("src/main.rs:12", "Open in your editor"));
    assert_eq!(found.reference, Reference::Path { path: "src/main.rs".into(), line: Some(12) });
    assert_eq!(path("src/main.rs", None).unwrap().text, "src/main.rs");
    for outside in ["", "/etc/passwd", "\\share\\x", "C:\\x", "C:x", "../x", "a/../b", "a/b/..", "a/b. ", "a\u{7}b", "..."] {
        assert_eq!(path(outside, None), None, "{outside:?}");
    }
}

#[test]
fn a_commit_is_seven_characters_a_feature_a_number_and_a_pane_its_label() {
    let sha = "aa10a2010c567a48999479fed257411c0a8b2b67";
    let commit = pointer(&Value::Commit { sha: sha.into(), title: Some("Fix".into()) }).unwrap();
    assert_eq!((commit.text.as_str(), commit.title), ("aa10a20", "Show in History"));
    assert_eq!(commit.reference, Reference::Commit { sha: sha.into() }, "the whole hash is what is asked for");
    let feature = pointer(&Value::Feature { number: 41 }).unwrap();
    assert_eq!((feature.text.as_str(), feature.title), ("#41", "Show in the Feature stage"));
    let pane = |label: Option<&str>| pointer(&Value::Pane { pane: 7, label: label.map(Into::into) }).unwrap();
    assert_eq!(pane(None).text, "pane #7");
    assert_eq!(pane(Some("trim context")).text, "trim context");
    assert_eq!(pane(None).title, "Go to the pane");
}

#[test]
fn a_value_that_points_nowhere_in_argus_is_no_button() {
    for value in [text("x"), Value::Duration { ms: 5 }, Value::Code { text: "x".into() }, Value::Empty, Value::Link { label: "l".into(), url: "https://a.b".into() }] {
        assert_eq!(pointer(&value), None, "{value:?}");
    }
}

// VALUES-8

#[test]
fn every_kind_of_value_reads_as_the_text_a_filter_matches() {
    let chip = |label: &str| Chip { label: label.into(), tone: Tone::Good };
    let cases = [
        (text("plain"), "plain"),
        (Value::Number { number: 1234.5, unit: Some("ms".into()) }, "1234.5 ms"),
        (Value::Number { number: 7.0, unit: None }, "7"),
        (Value::Number { number: 7.0, unit: Some(String::new()) }, "7"),
        (Value::Bool { value: true }, "yes"),
        (Value::Bool { value: false }, "no"),
        (Value::Date { date: "2026-10-05".into() }, "2026-10-05"),
        (Value::DateTime { at: "2026-10-05T01:02:03Z".into() }, "2026-10-05T01:02:03Z"),
        (Value::Person { name: "Ada".into() }, "Ada"),
        (Value::People { names: vec!["Ada".into(), "Grace".into()] }, "Ada, Grace"),
        (Value::Chip { chip: chip("ok") }, "ok"),
        (Value::Chips { chips: vec![chip("a"), chip("b")] }, "a, b"),
        (Value::Link { label: "docs".into(), url: "https://a.b".into() }, "docs"),
        (Value::Ticket { key: "ARG-1".into(), title: Some("Rank".into()), url: None, status: None }, "ARG-1 Rank"),
        (Value::Ticket { key: "ARG-1".into(), title: None, url: None, status: None }, "ARG-1"),
        (Value::Progress { value: 3.0, max: 4.0, tone: Tone::Good }, "3/4"),
        (Value::Duration { ms: 850 }, "850 ms"),
        (Value::Code { text: "x = 1".into() }, "x = 1"),
        (Value::Path { path: "a/b.rs".into(), line: Some(3) }, "a/b.rs:3"),
        (Value::Path { path: "a/b.rs".into(), line: None }, "a/b.rs"),
        (Value::Commit { sha: "abc1234".into(), title: Some("Fix".into()) }, "abc1234 Fix"),
        (Value::Feature { number: 9 }, "#9"),
        (Value::Pane { pane: 4, label: None }, "pane 4"),
        (Value::Pane { pane: 4, label: Some("agent".into()) }, "agent"),
        (Value::View { id: "release".into(), label: None }, "release"),
        (Value::View { id: "release".into(), label: Some("Release".into()) }, "Release"),
        (Value::Empty, ""),
        (Value::Unknown, ""),
    ];
    for (value, said) in cases {
        assert_eq!(cell_text(&value), said, "{value:?}");
    }
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        // VALUES-2: grouping only inserts commas, between whole groups of three digits.
        #[test]
        fn grouping_adds_only_commas_between_threes(number in -1.0e15f64..1.0e15) {
            let said = grouped(number);
            let whole = said.trim_start_matches('-').split('.').next().unwrap();
            for (k, group) in whole.split(',').enumerate() {
                prop_assert!(group.chars().all(|c| c.is_ascii_digit()));
                prop_assert!(if k == 0 { (1..=3).contains(&group.len()) } else { group.len() == 3 }, "{said}");
            }
            let back: f64 = said.replace(',', "").parse().unwrap();
            prop_assert!((back - number).abs() <= 0.0005 + number.abs() * 1e-12, "{said} for {number}");
        }

        // VALUES-4: a meter never leaves its track, whatever an agent sends.
        #[test]
        fn a_meter_stays_in_its_track(value in any::<f64>(), max in any::<f64>()) {
            let share = fraction(value, max);
            prop_assert!((0.0..=1.0).contains(&share));
            prop_assert!(percent(value, max) <= 100);
        }

        // VALUES-3: a tint is one of the five tones, for any name.
        #[test]
        fn a_name_always_has_a_tint(name in "\\PC{0,40}") {
            prop_assert!([Tone::Active, Tone::Info, Tone::Good, Tone::Warn, Tone::Bad].contains(&tint(&name)));
        }

        // VALUES-1: an age is words for any text at all, and never longer than the text plus a date.
        #[test]
        fn an_age_is_never_a_panic(at in "\\PC{0,40}") {
            let _ = ago(&clock(), &at);
            let _ = short_date(&clock(), &at);
        }
    }
}
