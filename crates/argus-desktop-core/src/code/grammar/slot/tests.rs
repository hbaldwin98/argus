use super::*;
use crate::code::grammar::fetch::Remote;

fn offer(name: &str, pinned: bool) -> Offer {
    let remote = |size| Remote { url: format!("https://cdn.jsdelivr.net/npm/{name}"), size, sha256: None };
    Offer {
        name: name.into(),
        label: name.into(),
        source: format!("tree-sitter-{name}@1.0.0"),
        symbol: name.into(),
        wasm: remote(2048),
        queries: vec![remote(1024)],
        pinned,
        publisher: None,
    }
}

#[test]
fn an_offer_waits_for_a_yes_and_an_install_shows_what_it_is_doing() {
    // EDITOR-GRAM-6, -7, -8.
    let mut slot = Slot::found(&Found::Offer(offer("kotlin", false)));
    assert!(matches!(slot.shown(false), Some(Shown::Ask(ask)) if ask.question == "No colours for kotlin yet. Install its tree-sitter grammar? "));
    let asked = slot.install().expect("an offer is installed on a yes");
    assert_eq!(asked.name, "kotlin");
    assert_eq!(slot.shown(false), Some(Shown::Text("Installing the kotlin grammar\u{2026}".into())));
    assert!(slot.install().is_none(), "there is nothing left to say yes to");
}

#[test]
fn a_pinned_grammar_settings_installs_is_installing_from_the_first_draw() {
    let slot = Slot::found(&Found::Install(offer("rust", true)));
    assert_eq!(slot.shown(false), Some(Shown::Text("Installing the rust grammar\u{2026}".into())));
}

#[test]
fn nothing_found_and_a_grammar_already_there_leave_nothing_to_say() {
    assert_eq!(Slot::found(&Found::Nothing), Slot::None);
    assert_eq!(Slot::None.shown(false), None);
}

#[test]
fn a_grammar_that_will_not_load_says_why() {
    let slot = Slot::found(&Found::Failed("Could not load the grammar: bad".into()));
    assert_eq!(slot.shown(false), Some(Shown::Text("Could not load the grammar: bad".into())));
}

#[test]
fn an_install_that_failed_says_what_it_was_and_why_and_one_that_worked_says_nothing() {
    let o = offer("kotlin", false);
    assert_eq!(Slot::installed(&o, &Ok(())), Slot::None);
    assert_eq!(Slot::installed(&o, &Err("it answered 404".into())), Slot::Said("Could not install tree-sitter-kotlin@1.0.0: it answered 404".into()));
}

#[test]
fn not_now_empties_the_slot_and_remembers_the_name_for_the_window() {
    let mut declined = HashSet::new();
    let mut slot = Slot::Offer(offer("kotlin", false));
    slot.not_now(&mut declined);
    assert_eq!(slot, Slot::None);
    assert!(declined.contains("kotlin"));
    let mut said = Slot::Said("failed".into());
    said.not_now(&mut declined);
    assert_eq!(declined.len(), 1, "only an offer is declined");
}

#[test]
fn a_held_clash_has_the_bars_room() {
    // EDITOR-CLASH-4: while a clash is held the bar shows no grammar offer.
    let slot = Slot::Offer(offer("kotlin", false));
    assert_eq!(slot.shown(true), None);
    assert!(slot.shown(false).is_some());
}

#[test]
fn files_are_of_one_kind_by_extension_and_extensionless_ones_by_whole_path() {
    // EDITOR-GRAM-6: `extOf` and `sameKind`.
    assert!(same_kind("src/a.rs", "lib/b.RS"));
    assert!(!same_kind("a.rs", "a.py"));
    assert!(same_kind("a.test.ts", "x/b.ts"));
    assert!(!same_kind("a/Makefile", "b/Makefile"), "no dot: the whole path");
    assert!(same_kind("a/Makefile", "a/Makefile"));
    assert!(same_kind(".bashrc", "x/y.bashrc"), "a leading dot is not an extension: `.bashrc` is `bashrc`");
}
