//! What the menus list and where the highlight goes (BOARD-MENU-2, 3, 5, 6).

use argus_desktop_core::connection::page::{AfterChoice, Agent, FeatureRow, Move};

use super::items::*;

fn row(agent: bool) -> FeatureRow {
    let place = |to: &'static str, label: &str, disabled: Option<&str>| Move { to, label: label.into(), disabled: disabled.map(str::to_owned) };
    FeatureRow {
        slug: "trim".into(),
        number: 41,
        title: "Trim".into(),
        standing: "active",
        group: "active",
        tasks_done: 0,
        tasks_total: 0,
        decided: 0,
        open: 0,
        branch: None,
        agent: agent.then(|| Agent { pane: 7, title: "claude".into(), status: "working", needs_you: false }),
        after: Vec::new(),
        held: None,
        peeked: false,
        moves: vec![
            place("queued", "Queued", Some("follows the work")),
            place("active", "Active", Some("here now")),
            place("held", "Held\u{2026}", None),
            place("archived", "Archived", None),
        ],
        footer: String::new(),
        next: None,
    }
}

fn choices() -> Vec<AfterChoice> {
    vec![AfterChoice { slug: "a".into(), number: 3, title: "Three".into(), waits: false }, AfterChoice { slug: "b".into(), number: 5, title: "Five".into(), waits: true }]
}

fn texts(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::Label(words) => format!("# {words}"),
            Entry::Rule => "---".into(),
            Entry::Item(item) => item.text.clone(),
        })
        .collect()
}

#[test]
fn the_menu_names_the_feature_offers_its_pane_then_the_moves() {
    let entries = moves(&row(true), &[]);
    assert_eq!(texts(&entries), ["# #41 Trim", "Go to its pane", "---", "# Move to", "Queued", "Active", "Held\u{2026}", "Archived"]);
    assert_eq!(texts(&moves(&row(false), &[]))[..2], ["# #41 Trim", "# Move to"], "no agent, no pane and no rule");
}

#[test]
fn a_move_that_follows_the_work_is_off_with_its_reason_and_the_one_here_is_ticked() {
    let entries = moves(&row(false), &[]);
    let Entry::Item(queued) = &entries[2] else { panic!("an item") };
    assert_eq!((queued.disabled, queued.note.as_deref(), queued.key, &queued.lead), (true, Some("follows the work"), None, &Lead::None));
    let Entry::Item(active) = &entries[3] else { panic!("an item") };
    assert_eq!((&active.lead, active.note.as_deref()), (&Lead::Here, Some("here now")));
    let Entry::Item(held) = &entries[4] else { panic!("an item") };
    assert_eq!((held.disabled, held.key, &held.does), (false, Some("H"), &Does::Move("held")));
    let Entry::Item(archived) = &entries[5] else { panic!("an item") };
    assert_eq!(archived.key, Some("Shift A"));
}

#[test]
fn go_to_its_pane_carries_the_pane_and_the_key_g() {
    let entries = moves(&row(true), &[]);
    let Entry::Item(go) = &entries[1] else { panic!("an item") };
    assert_eq!((&go.does, go.key), (&Does::GoToPane(7), Some("G")));
}

#[test]
fn comes_after_is_offered_only_when_the_peek_has_choices() {
    assert!(!texts(&moves(&row(false), &[])).contains(&"Comes after\u{2026}".to_owned()));
    let entries = moves(&row(false), &choices());
    assert_eq!(texts(&entries)[entries.len() - 2..], ["---", "Comes after\u{2026}"]);
}

#[test]
fn the_comes_after_menu_ticks_what_the_feature_waits_on_and_says_take_back() {
    let entries = comes_after(&row(false), &choices());
    assert_eq!(texts(&entries), ["# #41 comes after\u{2026}", "Three", "Five"]);
    let Entry::Item(waiting) = &entries[2] else { panic!("an item") };
    assert_eq!((&waiting.lead, waiting.number, waiting.note.as_deref(), &waiting.does), (&Lead::Tick(true), Some(5), Some("take back"), &Does::Wait { on: "b".into(), waits: true }));
    let Entry::Item(free) = &entries[1] else { panic!("an item") };
    assert_eq!((&free.lead, free.note.as_deref(), &free.does), (&Lead::Tick(false), None, &Does::Wait { on: "a".into(), waits: false }));
}

#[test]
fn only_the_choices_that_can_be_made_take_the_highlight() {
    let entries = moves(&row(true), &[]);
    assert_eq!(enabled(&entries), [1, 6, 7], "Queued and Active are off");
}

#[test]
fn the_highlight_wraps_at_either_end_and_starts_on_the_first_or_last() {
    let list = [1, 6, 7];
    assert_eq!(stepped(&list, None, 1), Some(1));
    assert_eq!(stepped(&list, None, -1), Some(7));
    assert_eq!(stepped(&list, Some(1), 1), Some(6));
    assert_eq!(stepped(&list, Some(7), 1), Some(1));
    assert_eq!(stepped(&list, Some(1), -1), Some(7));
    assert_eq!(stepped(&[], Some(1), 1), None);
}
