use super::*;
use crate::extensions::keys::{ExtBinding, ExtChord};
use crate::keys::keymap::groups;

fn the_sections() -> Vec<Section> {
    all(&groups(), &[])
}

fn by_id<'a>(sections: &'a [Section], id: &str) -> &'a Section {
    sections.iter().find(|section| section.id == id).unwrap_or_else(|| panic!("no section {id}"))
}

fn rows(section: &Section) -> impl Iterator<Item = &Row> {
    section.rows.iter().chain(&section.without)
}

/// SHELL-SHEET-11: the sheet is generated from the keymap, so every binding of a group lands on a
/// row of its section, the merged pairs standing for their members.
#[test]
fn every_binding_of_the_keymap_is_on_a_row_of_its_group_merged_pairs_apart() {
    let keymap = groups();
    let sections = the_sections();
    assert_eq!(sections.len(), keymap.len());
    for group in &keymap {
        let section = by_id(&sections, &place_id(group.place));
        // The pane's pairs: next and previous (two into one), and the arrows and the moves (four
        // into one, twice).
        let merged = if group.place == Place::Pane { 1 + 3 + 3 } else { 0 };
        assert_eq!(rows(section).count(), group.bindings.len() - merged, "{}", section.id);
        let labels: Vec<&str> = rows(section).map(|row| row.label.as_str()).collect();
        for binding in &group.bindings {
            let merged_away = matches!(
                binding.action,
                Action::OtherSide
                    | Action::PreviousSide
                    | Action::FocusLeft
                    | Action::FocusRight
                    | Action::FocusUp
                    | Action::FocusDown
                    | Action::MovePaneLeft
                    | Action::MovePaneRight
                    | Action::MovePaneUp
                    | Action::MovePaneDown
            );
            assert!(merged_away || labels.contains(&binding.label), "{}: {}", section.id, binding.label);
        }
    }
}

/// SHELL-SHEET-5.
#[test]
fn the_pane_section_says_the_leader_once_in_no_row_that_follows_it() {
    let sections = the_sections();
    let pane = by_id(&sections, "pane");
    assert!(pane.chorded);
    assert!(pane.rows.iter().all(|row| row.keys.iter().all(|keys| !keys.contains(&"Ctrl Space".to_owned()))));
    let has = |wanted: &str| pane.without.iter().any(|row| row.keys.iter().any(|keys| keys.join(" ") == wanted));
    assert!(has("Ctrl Space Ctrl Space"), "the leader sent on keeps both its presses");
    assert!(has("Shift PgUp"));
    assert!(pane.without.iter().any(|row| row.label == "paste"));
}

/// SHELL-SHEET-6.
#[test]
fn next_and_previous_and_the_arrows_are_a_row_each_in_the_pane() {
    let sections = the_sections();
    let pane = by_id(&sections, "pane");
    let row = |label: &str| pane.rows.iter().find(|row| row.label.contains(label)).unwrap_or_else(|| panic!("no row {label}"));
    assert_eq!(row("next / previous").keys, [vec!["o".to_owned()], vec!["O".to_owned()]]);
    assert_eq!(row("pane that way").keys, [vec!["\u{2190} \u{2192} \u{2191} \u{2193}".to_owned()]]);
    assert_eq!(row("trading places").keys, [vec!["Shift + arrow".to_owned()]]);
    assert!(!pane.rows.iter().any(|row| row.label.contains("left") || row.label.contains("right")));
}

fn chord(key: &str, shift: bool) -> KeyChord {
    KeyChord { key: key.into(), ctrl: false, alt: false, shift, meta: false, leader: false }
}

fn binding(action: Action, key: &str, shift: bool) -> SourceBinding {
    SourceBinding { action: Some(action), chords: vec![chord(key, shift)], label: format!("{action:?}") }
}

/// SHELL-SHEET-6: the four of a set that share no modifier prefix are not merged.
#[test]
fn arrows_that_share_no_modifier_stay_separate_rows() {
    let source = Source {
        place: "pane".into(),
        title: "In a pane".into(),
        bindings: vec![
            binding(Action::FocusLeft, "ArrowLeft", false),
            binding(Action::FocusRight, "ArrowRight", true),
            binding(Action::FocusUp, "ArrowUp", false),
            binding(Action::FocusDown, "ArrowDown", false),
        ],
    };
    let sections = sections(&[source], "Ctrl Space");
    assert_eq!(sections[0].rows.len(), 4, "the merge is skipped");
    assert!(sections[0].rows.iter().all(|row| row.label != "the keys to the pane that way"));
}

/// SHELL-SHEET-6: a set that shares a modifier is one row, its cap the modifier and `arrow`.
#[test]
fn arrows_that_share_a_modifier_are_one_row_with_one_cap() {
    let with = |action, key| binding(action, key, true);
    let source = Source {
        place: "pane".into(),
        title: "In a pane".into(),
        bindings: vec![
            with(Action::MovePaneLeft, "ArrowLeft"),
            with(Action::MovePaneRight, "ArrowRight"),
            with(Action::MovePaneUp, "ArrowUp"),
            with(Action::MovePaneDown, "ArrowDown"),
        ],
    };
    let sections = sections(&[source], "Ctrl Space");
    assert_eq!(sections[0].rows, [Row { label: "move the pane that way, trading places".into(), keys: vec![vec!["Shift + arrow".into()]] }]);
}

#[test]
fn a_pair_of_keys_on_one_binding_stays_one_row() {
    let sections = the_sections();
    let next = by_id(&sections, "feature").rows.iter().find(|row| row.label == "next feature").unwrap();
    assert_eq!(next.keys, [vec!["j".to_owned()], vec!["\u{2193}".to_owned()]]);
}

/// SHELL-SHEET-7.
#[test]
fn the_filter_finds_a_row_by_its_action_or_its_key_grouped_by_section() {
    let sections = the_sections();
    let found = matching(&sections, "paste");
    assert_eq!(found.iter().map(|section| section.id.as_str()).collect::<Vec<_>>(), ["pane"]);
    assert!(found.iter().all(|section| rows(section).all(|row| row.label.to_lowercase().contains("paste"))));
    let by_key = matching(&sections, "pgup");
    assert!(!by_key.is_empty() && by_key.iter().all(|section| rows(section).count() > 0));
    assert_eq!(matching(&sections, "  ").len(), sections.len());
    assert!(matching(&sections, "zzzzzz").is_empty());
}

#[test]
fn every_word_of_the_query_has_to_be_in_the_row() {
    let sections = the_sections();
    // The chord's own first word: Ctrl here, Cmd on macOS.
    let paste = sections.iter().flat_map(rows).find(|row| row.label.to_lowercase().contains("paste")).expect("a paste row");
    let modifier = paste.keys[0][0].split_whitespace().next().expect("a chord").to_lowercase();
    let both = matching(&sections, &format!("paste {modifier}"));
    let neither = matching(&sections, "paste zzzz");
    assert!(!both.is_empty());
    assert!(neither.is_empty());
}

fn ids(sections: &[Section]) -> Vec<String> {
    sections.iter().map(|section| section.id.clone()).collect()
}

/// SHELL-SHEET-3.
#[test]
fn the_sheet_opens_on_the_section_for_where_the_person_is() {
    let ids = ids(&the_sections());
    let at = Where { stage: "workspace".into(), ..Where::default() };
    assert_eq!(section_for(&Where { pane: true, ..at.clone() }, &ids), "pane");
    assert_eq!(section_for(&Where { stage: "feature".into(), ..at.clone() }, &ids), "feature");
    assert_eq!(section_for(&Where { stage: "feature".into(), outline: true, ..at.clone() }, &ids), "outline");
    assert_eq!(section_for(&Where { stage: "feature".into(), outline: true, diagram: true, ..at.clone() }, &ids), "diagram");
    assert_eq!(section_for(&Where { stage: "code".into(), ..at.clone() }, &ids), "code");
    assert_eq!(section_for(&Where { dialog: Some(Dialog::Review), pane: true, ..at.clone() }, &ids), "review");
    assert_eq!(section_for(&Where { rail: true, ..at.clone() }, &ids), "rail");
    assert_eq!(section_for(&at, &ids), "anywhere");
    assert_eq!(section_for(&Where { pane: true, ..at }, &["anywhere".to_owned()]), "anywhere", "the keymap has no such section");
}

/// SHELL-SHEET-9: Down and Up change section and stop at the ends.
#[test]
fn down_and_up_change_section_and_stop_at_the_ends() {
    let ids: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
    assert_eq!(step(&ids, "a", true).as_deref(), Some("b"));
    assert_eq!(step(&ids, "c", true).as_deref(), Some("c"), "no wrap");
    assert_eq!(step(&ids, "a", false).as_deref(), Some("a"), "no wrap");
    assert_eq!(step(&ids, "c", false).as_deref(), Some("b"));
    assert_eq!(step(&[], "a", true), None);
}

/// SHELL-SHEET-2: one section per extension group after the keymap's.
#[test]
fn an_extensions_heading_is_a_section_after_the_keymaps() {
    let group = ExtGroup {
        title: "Git extras".into(),
        bindings: vec![ExtBinding {
            label: "fetch all".into(),
            chords: vec![ExtChord { key: "f".into(), ctrl: true, alt: false, shift: false, meta: false, leader: false }],
        }],
    };
    let sections = all(&groups(), &[group]);
    let last = sections.last().unwrap();
    assert_eq!((last.id.as_str(), last.title.as_str()), ("ext/Git extras", "Git extras"));
    assert_eq!(last.rows, [Row { label: "fetch all".into(), keys: vec![vec!["Ctrl F".into()]] }]);
    assert_eq!(sections.len(), groups().len() + 1);
}

/// The stages that have a section of their own are named as the keymap's places are.
#[test]
fn a_stage_is_named_as_the_pages_stage_is() {
    let ids = ids(&the_sections());
    for (stage, section) in [(Stage::Feature, "feature"), (Stage::Checkouts, "checkouts"), (Stage::Code, "code")] {
        assert_eq!(stage_id(stage), section);
        assert_eq!(section_for(&Where { stage: stage_id(stage), ..Where::default() }, &ids), section);
    }
    assert_eq!(stage_id(Stage::Composer), "new", "the composer is `new` on the page");
    assert_eq!(section_for(&Where { stage: stage_id(Stage::Workspace), ..Where::default() }, &ids), "anywhere");
}
