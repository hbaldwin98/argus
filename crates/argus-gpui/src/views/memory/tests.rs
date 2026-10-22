use super::*;

fn keys(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

// CROSS-13 of the audit's item 45: a republish must not reset what the person was doing.

#[test]
fn a_tab_is_kept_where_it_was_chosen_and_the_first_until_one_is() {
    let mut memory = Memory::default();
    assert_eq!(memory.tab("1/ci.2", 3), 0);
    memory.choose_tab("1/ci.2", 2);
    assert_eq!(memory.tab("1/ci.2", 3), 2);
    assert_eq!(memory.tab("1/ci.3", 3), 0, "another block's tabs are its own");
    assert_eq!(memory.tab("2/ci.2", 3), 0, "and another view's");
}

#[test]
fn a_view_that_comes_again_with_fewer_tabs_shows_its_last_not_nothing() {
    let mut memory = Memory::default();
    memory.choose_tab("1/ci.0", 4);
    assert_eq!(memory.tab("1/ci.0", 2), 1);
    assert_eq!(memory.tab("1/ci.0", 0), 0, "no tabs at all is the first of none");
}

#[test]
fn a_fold_the_person_set_beats_how_the_agent_says_it_starts() {
    let mut memory = Memory::default();
    assert!(memory.open("1/ci.0", true) && !memory.open("1/ci.0", false), "the agent's word until the person chooses");
    memory.set_open("1/ci.0", false);
    assert!(!memory.open("1/ci.0", true), "a reader's choice is not undone by a republish");
}

#[test]
fn ticked_and_opened_rows_are_trimmed_to_the_rows_that_still_exist() {
    let mut memory = Memory::default();
    memory.toggle_selected("1/ci.0", "a");
    memory.toggle_selected("1/ci.0", "b");
    memory.toggle_row("1/ci.0", "b");
    assert_eq!(memory.selection("1/ci.0", &keys(&["a", "b", "c"])).len(), 2);
    assert_eq!(memory.selection("1/ci.0", &keys(&["b", "c"])).into_iter().collect::<Vec<_>>(), ["b"], "a gone row is let go");
    assert!(memory.selection("1/ci.0", &keys(&["a", "b"])).len() == 1, "and does not come back when the row does");
    assert_eq!(memory.open_rows("1/ci.0", &keys(&["b"])).len(), 1);
    memory.toggle_selected("1/ci.0", "b");
    assert!(memory.selection("1/ci.0", &keys(&["b"])).is_empty(), "a second toggle unticks");
}

#[test]
fn select_all_ticks_exactly_the_keys_it_is_given() {
    let mut memory = Memory::default();
    memory.toggle_selected("1/ci.0", "z");
    memory.set_selection("1/ci.0", keys(&["a", "b"]));
    assert_eq!(memory.selection("1/ci.0", &keys(&["a", "b", "z"])).into_iter().collect::<Vec<_>>(), ["a", "b"]);
}

#[test]
fn a_filter_typed_is_kept_and_emptied_is_forgotten() {
    let mut memory = Memory::default();
    assert_eq!(memory.filter("1/ci.0"), "");
    memory.set_filter("1/ci.0", "fail");
    assert_eq!(memory.filter("1/ci.0"), "fail");
    memory.set_filter("1/ci.0", "");
    assert_eq!(memory.filter("1/ci.0"), "");
    assert!(memory.filters.is_empty());
}

#[test]
fn a_draft_is_kept_until_the_form_is_sent_and_only_that_forms() {
    let mut memory = Memory::default();
    memory.set_draft("1/ci.0|title", Draft::Text("half".into()));
    memory.set_draft("1/ci.0|urgent", Draft::Flag(true));
    memory.set_draft("1/ci.1|title", Draft::Text("other form".into()));
    assert_eq!(memory.draft("1/ci.0|title"), Some(&Draft::Text("half".into())));
    assert_eq!(memory.sent("1/ci.0|"), 0);
    memory.clear_drafts("1/ci.0|");
    assert_eq!((memory.sent("1/ci.0|"), memory.sent("1/ci.1|")), (1, 0), "a sending is counted for the form sent");
    assert_eq!(memory.draft("1/ci.0|title"), None);
    assert_eq!(memory.draft("1/ci.0|urgent"), None);
    assert!(memory.draft("1/ci.1|title").is_some());
}

#[test]
fn a_log_remembers_where_it_was_left() {
    let mut memory = Memory::default();
    assert_eq!(memory.log("1/ci.0"), None);
    memory.set_log("1/ci.0", LogPlace { top: 120.0, tail: false });
    assert_eq!(memory.log("1/ci.0"), Some(LogPlace { top: 120.0, tail: false }));
}


#[test]
fn an_open_confirmation_is_kept_by_its_button_until_answered() {
    let mut memory = Memory::default();
    assert!(!memory.asking("1/ci.2|deploy"));
    memory.set_asking("1/ci.2|deploy", true);
    assert!(memory.asking("1/ci.2|deploy"));
    assert!(!memory.asking("1/ci.2|abort"), "another button's question is its own");
    memory.set_asking("1/ci.2|deploy", false);
    assert!(!memory.asking("1/ci.2|deploy"));
}
