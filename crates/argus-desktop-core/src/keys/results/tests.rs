use super::*;
use crate::connection::page::{PaletteCheckout, PaletteFeature, PalettePane};
use crate::keys::keymap::commands;
use crate::layout::docks::Panel;

fn palette() -> Palette {
    Palette {
        open: true,
        panes: vec![PalettePane { id: 3, title: "claude".into(), agent: true, status: "waiting", place: "argus / main".into() }],
        checkouts: vec![
            PaletteCheckout { id: 10, name: "main".into(), repository: "argus".into(), branch: "main".into(), git: Some("!4".into()), clean: false },
            PaletteCheckout { id: 11, name: "spike".into(), repository: "argus".into(), branch: String::new(), git: Some("clean".into()), clean: true },
        ],
        features: vec![PaletteFeature { slug: "trim".into(), number: 41, title: "Trim context".into(), standing: "active" }],
        features_loaded: true,
    }
}

fn tab() -> ExtTab {
    ExtTab { panel: Panel::Extension("x".into()), extension: "git-extras".into(), view: "log".into(), title: "Log".into(), icon: None, page: false, on: false }
}

fn command_of(extension: &str, name: &str) -> ExtCommandPage {
    ExtCommandPage { extension: extension.into(), command: name.into(), title: "Fetch all".into(), chords: Vec::new(), scope: "anywhere", view: None }
}

/// SHELL-PALETTE-4: commands, then the extensions', panes, features and checkouts.
#[test]
fn the_rows_are_the_commands_then_the_extensions_panes_features_and_checkouts() {
    let rows = results(&commands(), &[tab()], &[command_of("git-extras", "fetch")], Some(&palette()));
    let groups: Vec<Group> = rows.iter().map(|row| row.group).collect();
    let first_other = groups.iter().position(|group| *group != Group::Commands).unwrap();
    assert!(groups[..first_other].iter().all(|group| *group == Group::Commands));
    assert_eq!(&groups[first_other..], [Group::Panes, Group::Features, Group::Checkouts, Group::Checkouts]);
    let texts: Vec<_> = rows[first_other - 2..first_other].iter().map(|row| row.text.as_str()).collect();
    assert_eq!(texts, ["Go to Log", "Fetch all"], "the extensions' rows close the commands");
}

#[test]
fn before_the_palette_is_heard_only_the_commands_are_listed() {
    let rows = results(&commands(), &[], &[], None);
    assert_eq!(rows.len(), commands().len());
    assert!(rows.iter().all(|row| row.group == Group::Commands));
}

/// SHELL-PALETTE-7: a command is sentence-cased, and its place's title comes before its keys only
/// when its place is not Anywhere.
#[test]
fn a_command_says_its_place_only_when_it_is_not_anywhere_and_its_label_is_sentence_cased() {
    let rows = results(&commands(), &[], &[], None);
    let workspace = rows.iter().find(|row| row.does == Does::Command(Action::Workspace)).unwrap();
    assert!(workspace.text.chars().next().unwrap().is_uppercase());
    assert!(matches!(&workspace.aside, Aside::Keys { place: None, chords } if !chords.is_empty()), "{:?}", workspace.aside);
    let in_pane = rows.iter().find(|row| matches!(&row.aside, Aside::Keys { place: Some(_), .. }));
    assert!(in_pane.is_some(), "a command bound in the pane names the pane's place");
}

/// SHELL-PALETTE-7: a pane has a terminal mark, its place as its detail, its status as its word.
#[test]
fn a_pane_row_shows_where_it_is_and_how_it_stands_and_takes_the_keys_when_chosen() {
    let rows = results(&[], &[], &[], Some(&palette()));
    let pane = &rows[0];
    assert_eq!((pane.mark, pane.detail.as_deref(), pane.aside.clone()), (Mark::Terminal, Some("argus / main"), Aside::Word("waiting")));
    assert_eq!(pane.does, Does::Select(3));
    assert!(pane.does.gives_the_pane_the_keys());
    assert!(!rows[1].does.gives_the_pane_the_keys());
}

/// SHELL-PALETTE-7, 13: a feature is `#<number> <title>`, a checkout `<repository> / <name>` with
/// its branch and its git summary only when it is not clean.
#[test]
fn a_feature_and_a_checkout_are_written_as_the_search_matches_them() {
    let rows = results(&[], &[], &[], Some(&palette()));
    let feature = &rows[1];
    assert_eq!((feature.text.as_str(), feature.aside.clone()), ("#41 Trim context", Aside::Word("active")));
    assert_eq!(feature.does, Does::GoToFeature("trim".into()));
    let (dirty, clean) = (&rows[2], &rows[3]);
    assert_eq!((dirty.text.as_str(), dirty.detail.as_deref(), dirty.aside.clone()), ("argus / main", Some("main"), Aside::Git("!4".into())));
    assert_eq!((clean.detail.clone(), clean.aside.clone()), (None, Aside::None), "no branch is no detail; a clean one says nothing");
    assert_eq!(dirty.does, Does::GoToCheckout(10));
}

#[test]
fn an_extensions_view_and_command_carry_the_extension_as_their_detail() {
    let rows = results(&[], &[tab()], &[command_of("git-extras", "fetch")], None);
    assert_eq!(rows[0].does, Does::ExtensionView { extension: "git-extras".into(), view: "log".into() });
    assert_eq!((rows[0].mark, rows[0].detail.as_deref()), (Mark::Puzzle, Some("git-extras")));
    assert_eq!(rows[1].does, Does::ExtensionCommand { extension: "git-extras".into(), command: "fetch".into() });
}

#[test]
fn a_rows_key_is_the_same_whenever_the_lists_are_rebuilt() {
    let again = results(&commands(), &[], &[], Some(&palette()));
    let keys: Vec<_> = again.iter().map(|row| row.key.clone()).collect();
    assert_eq!(keys, results(&commands(), &[], &[], Some(&palette())).iter().map(|row| row.key.clone()).collect::<Vec<_>>());
    let unique: std::collections::HashSet<_> = keys.iter().collect();
    assert_eq!(unique.len(), keys.len(), "no two rows share a key");
    assert!(keys.contains(&"pane 3".to_owned()) && keys.contains(&"feature trim".to_owned()) && keys.contains(&"checkout 10".to_owned()));
}

/// SHELL-PALETTE-11.
#[test]
fn nothing_says_the_features_are_still_coming_only_under_the_hash_before_they_arrive() {
    let mut waiting = palette();
    waiting.features_loaded = false;
    assert_eq!(nothing(true, Some(&waiting)), "Reading the features\u{2026}");
    assert_eq!(nothing(true, None), "Reading the features\u{2026}");
    assert_eq!(nothing(true, Some(&palette())), "No match.");
    assert_eq!(nothing(false, Some(&waiting)), "No match.");
}

/// SHELL-PALETTE-8.
#[test]
fn the_highlight_stays_on_its_row_by_key_and_clamps_when_the_row_is_gone() {
    let rows = ["a", "b", "c"];
    assert_eq!(highlight(&rows, Some("c"), 0), 2, "the row moved, the highlight followed");
    assert_eq!(highlight(&rows, Some("gone"), 7), 2, "else the old place, clamped to the last row");
    assert_eq!(highlight(&rows, None, 1), 1);
    assert_eq!(highlight(&[], Some("a"), 4), 0);
}

/// SHELL-KEYS-21: an extension's command is run on the row the sidebar holds, as its key is.
#[test]
fn an_extension_command_is_run_where_the_sidebar_is_picked() {
    let run = Does::ExtensionCommand { extension: "x".into(), command: "go".into() };
    assert!(matches!(run.call(Some((4, Some(9)))), Call::ExtensionCommand { checkout: Some(4), pane: Some(9), .. }));
    assert!(matches!(run.call(Some((4, None))), Call::ExtensionCommand { checkout: Some(4), pane: None, .. }));
    assert!(matches!(run.call(None), Call::ExtensionCommand { checkout: None, pane: None, .. }));
    assert!(matches!(Does::Command(Action::Workspace).call(Some((4, None))), Call::Command { action: Action::Workspace }));
    assert!(matches!(Does::Select(3).call(None), Call::Select { pane: 3 }));
}
