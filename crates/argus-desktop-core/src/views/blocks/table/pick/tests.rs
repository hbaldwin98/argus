use super::*;

fn set(keys: &[&str]) -> BTreeSet<String> {
    keys.iter().map(|key| (*key).to_owned()).collect()
}

#[test]
fn a_table_is_picked_from_only_when_every_row_has_a_key() {
    assert_eq!(mode(RowSelect::Many, 3, 3), RowSelect::Many);
    assert_eq!(mode(RowSelect::Many, 2, 3), RowSelect::None);
    assert_eq!(mode(RowSelect::Many, 0, 3), RowSelect::None);
}

#[test]
fn a_row_is_remembered_by_the_agents_key_else_by_its_place() {
    assert_eq!(row_ids(&["a".into(), "b".into()], 2), ["a", "b"]);
    assert_eq!(row_ids(&[], 2), ["#0", "#1"]);
    assert_eq!(row_ids(&["a".into()], 2), ["#0", "#1"]);
}

#[test]
fn select_all_is_checked_when_every_shown_row_is_picked_and_mixed_when_some_are() {
    assert_eq!(all_state(&["a", "b"], &set(&["a", "b", "z"])), Tri::All);
    assert_eq!(all_state(&["a", "b"], &set(&["a"])), Tri::Mixed);
    assert_eq!(all_state(&["a", "b"], &set(&["z"])), Tri::Clear);
}

#[test]
fn select_all_over_nothing_shown_is_clear() {
    assert_eq!(all_state(&[], &set(&["a"])), Tri::Clear);
}

#[test]
fn select_all_leaves_the_rows_a_filter_hides_as_they_were() {
    let chosen = set(&["hidden"]);
    assert_eq!(toggle_all(&chosen, &["a", "b"], true), set(&["a", "b", "hidden"]));
    assert_eq!(toggle_all(&set(&["a", "b", "hidden"]), &["a", "b"], false), set(&["hidden"]));
}

#[test]
fn a_radio_picks_its_row_alone() {
    assert_eq!(only("b"), set(&["b"]));
}

#[test]
fn the_count_and_the_actions_follow_the_pick() {
    assert_eq!(count_line(RowSelect::Many, 2), "2 selected");
    assert_eq!(count_line(RowSelect::None, 2), "");
    assert!(actions_off(RowSelect::One, 0));
    assert!(!actions_off(RowSelect::One, 1));
    assert!(!actions_off(RowSelect::None, 0));
}
