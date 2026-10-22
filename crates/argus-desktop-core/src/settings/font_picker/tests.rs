use super::*;

fn picker() -> FontPicker {
    let mut picker = FontPicker::new("JetBrains Mono");
    picker.update(["Cascadia Mono", "Consolas", "Fira Code"].map(String::from).to_vec(), "");
    picker
}

fn family(row: &Row) -> &str {
    match row {
        Row::Family { family, .. } => family,
        Row::Note(note) => note,
    }
}

/// The default's row leads the list while nothing is typed (SHELL-FONT-2).
#[test]
fn an_untyped_list_leads_with_the_default_then_every_family() {
    let rows = picker().rows();
    assert_eq!(rows[0], Row::Family { family: String::new(), label: "JetBrains Mono".into() });
    assert_eq!(rows.iter().skip(1).map(family).collect::<Vec<_>>(), ["Cascadia Mono", "Consolas", "Fira Code"]);
}

#[test]
fn typing_narrows_by_substring_without_regard_to_case_and_drops_the_default_row() {
    let mut picker = picker();
    picker.typed("CON");
    assert_eq!(picker.rows().iter().map(family).collect::<Vec<_>>(), ["Consolas"]);
    assert!(picker.is_open(), "typing opens the list");
    assert_eq!(picker.active(), 0);
}

#[test]
fn a_name_nothing_matches_says_so_and_enter_uses_it_as_typed() {
    let mut picker = picker();
    picker.typed("Zed Mono");
    assert_eq!(picker.rows(), [Row::Note("None installed matches; Enter uses it as typed".into())]);
    assert_eq!(picker.enter(), Some("Zed Mono".into()));
    assert!(!picker.is_open());
}

#[test]
fn no_families_yet_is_a_note_about_reading_them() {
    let mut picker = FontPicker::new("Geist");
    assert_eq!(picker.rows().last(), Some(&Row::Note("Reading the installed fonts\u{2026}".into())));
    picker.open();
    assert_eq!(picker.active(), 0, "the note is never the row Enter picks, but the default's is");
    assert_eq!(picker.enter(), None, "the default is what is chosen already");
}

#[test]
fn opening_starts_on_the_chosen_family() {
    let mut picker = picker();
    picker.update(["Cascadia Mono", "Consolas", "Fira Code"].map(String::from).to_vec(), "Consolas");
    picker.open();
    assert_eq!(picker.active(), 2, "default, Cascadia, Consolas");
}

#[test]
fn down_and_up_stop_at_the_ends_and_the_first_opens_a_closed_list() {
    let mut picker = picker();
    picker.step(true);
    assert!(picker.is_open());
    assert_eq!(picker.active(), 0, "opening is not a step");
    for _ in 0..9 {
        picker.step(true);
    }
    assert_eq!(picker.active(), 3, "clamped at the last family");
    for _ in 0..9 {
        picker.step(false);
    }
    assert_eq!(picker.active(), 0);
}

#[test]
fn enter_picks_the_active_row_and_a_row_already_chosen_chooses_nothing() {
    let mut picker = picker();
    picker.open();
    picker.step(true);
    assert_eq!(picker.enter(), Some("Cascadia Mono".into()));
    picker.update(vec!["Cascadia Mono".into()], "Cascadia Mono");
    picker.open();
    picker.step(true);
    assert_eq!(picker.pick("Cascadia Mono"), None);
}

#[test]
fn choosing_the_default_row_chooses_the_empty_family() {
    let mut picker = picker();
    picker.update(vec!["Consolas".into()], "Consolas");
    picker.open();
    picker.step(false);
    assert_eq!(picker.enter(), Some(String::new()));
}

#[test]
fn leaving_the_field_commits_what_was_typed_once_and_a_picked_row_is_not_typed() {
    let mut picker = picker();
    picker.typed("Fira");
    assert_eq!(picker.blur(), Some("Fira".into()));
    assert_eq!(picker.blur(), None, "committed once");
    picker.typed("Fira");
    assert_eq!(picker.pick("Fira Code"), Some("Fira Code".into()));
    assert_eq!(picker.blur(), None, "picking cleared what was typed");
}

#[test]
fn leaving_a_field_typed_as_the_chosen_name_chooses_nothing() {
    let mut picker = picker();
    picker.update(vec!["Consolas".into()], "Consolas");
    picker.typed("Consolas");
    assert_eq!(picker.blur(), None);
}

#[test]
fn escape_closes_only_the_list_and_gives_the_typed_name_up() {
    let mut picker = picker();
    assert!(!picker.escape(), "a closed list leaves Escape to the dialog");
    picker.typed("Fi");
    assert!(picker.escape());
    assert!(!picker.is_open());
    assert_eq!(picker.blur(), None, "what was typed is not chosen");
}

#[test]
fn the_default_row_is_named_for_the_theme() {
    let mut picker = picker();
    picker.set_default_name("Geist");
    assert_eq!(picker.rows()[0], Row::Family { family: String::new(), label: "Geist".into() });
}
