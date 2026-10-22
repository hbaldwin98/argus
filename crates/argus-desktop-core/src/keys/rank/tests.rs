use super::*;

fn row(group: Group, text: &str) -> Candidate {
    Candidate { group, text: text.to_owned() }
}

fn commands(texts: &[&str]) -> Vec<Candidate> {
    texts.iter().map(|text| row(Group::Commands, text)).collect()
}

fn at(ranked: &Ranked) -> Vec<usize> {
    ranked.flat().iter().map(|hit| hit.at).collect()
}

/// SHELL-PALETTE-6: a subsequence of the needle, whitespace removed, in the text, ignoring case.
#[test]
fn the_needle_is_a_case_blind_subsequence_with_its_whitespace_removed() {
    let (_, marks) = fuzzy("GW ", "Go to the Workspace").unwrap();
    assert_eq!(marks, [0, 10]);
    assert!(fuzzy("zz", "Go to the Workspace").is_none());
    assert!(fuzzy("ow", "wo").is_none(), "left to right: the o has to come before the w");
}

#[test]
fn an_empty_needle_matches_everything_with_nothing_marked() {
    assert_eq!(fuzzy("  ", "anything"), Some((0.0, Vec::new())));
}

/// SHELL-PALETTE-6: +5 for a hit right after the last, +3 for one at a word start, minus 0.1 per
/// index of the first hit and 0.01 per character of the text.
#[test]
fn a_run_and_a_word_start_score_and_a_late_start_and_a_long_text_cost() {
    let (score, _) = fuzzy("ab", "ab").unwrap();
    assert!((score - (5.0 + 3.0 - 0.0 - 0.02)).abs() < 1e-5, "{score}");
    let (late, _) = fuzzy("b", "xxb").unwrap();
    assert!((late - (-0.2 - 0.03)).abs() < 1e-5, "{late}");
    let (word, _) = fuzzy("b", "a/b").unwrap();
    assert!(word > late, "after a slash is a word start");
    for separator in ["/", "#", "-", "_", ".", " "] {
        let (score, _) = fuzzy("b", &format!("a{separator}b")).unwrap();
        assert!(score > 2.0, "{separator:?} starts a word");
    }
}

/// SHELL-PALETTE-5: no needle keeps each group's own order and its first six.
#[test]
fn with_no_needle_each_group_keeps_its_order_and_shows_six() {
    let mut rows = commands(&["a", "b", "c", "d", "e", "f", "g", "h"]);
    rows.push(row(Group::Panes, "p"));
    let ranked = rank("  ", &rows);
    assert_eq!(at(&ranked), [0, 1, 2, 3, 4, 5, 8]);
    assert_eq!(ranked.scope, None);
}

/// SHELL-PALETTE-5: a scope with no needle shows its whole group, and the others go.
#[test]
fn a_scope_with_no_needle_shows_all_of_its_group_and_none_of_the_others() {
    let mut rows = commands(&["a", "b", "c", "d", "e", "f", "g", "h"]);
    rows.push(row(Group::Features, "#1 one"));
    let commands_only = rank(">", &rows);
    assert_eq!((commands_only.scope, at(&commands_only)), (Some(Group::Commands), vec![0, 1, 2, 3, 4, 5, 6, 7]));
    let features_only = rank(" # ", &rows);
    assert_eq!((features_only.scope, at(&features_only)), (Some(Group::Features), vec![8]));
}

/// SHELL-PALETTE-5, 6: with a needle every match shows, best first, ties in their order.
#[test]
fn a_needle_shows_every_match_best_first_and_ties_keep_their_order() {
    let rows = commands(&["xxx check", "check", "chat check", "c-h-e-c-k", "check two", "check three", "check four", "check five"]);
    let ranked = rank("check", &rows);
    let shown = at(&ranked);
    assert_eq!(shown.len(), 8, "the cap of six is for no needle");
    assert_eq!(shown[0], 1, "a word start and a run beat the rest");
    let tie = commands(&["abc", "abc"]);
    assert_eq!(at(&rank("abc", &tie)), [0, 1]);
}

#[test]
fn a_row_with_no_match_is_hidden_and_a_group_with_no_rows_has_no_heading() {
    let rows = vec![row(Group::Commands, "open"), row(Group::Panes, "shell")];
    let ranked = rank("sh", &rows);
    assert_eq!(ranked.groups.len(), 1);
    assert_eq!(ranked.groups[0].0, Group::Panes);
}

#[test]
fn the_groups_come_in_the_palettes_order_whatever_the_order_given() {
    let rows = vec![row(Group::Checkouts, "a"), row(Group::Panes, "a"), row(Group::Commands, "a"), row(Group::Features, "#1 a")];
    let order: Vec<_> = rank("", &rows).groups.iter().map(|(group, _)| *group).collect();
    assert_eq!(order, GROUPS);
}

/// SHELL-PALETTE-6: a feature's text is matched with its `#` unscoped, and without it under `#`,
/// its marks counted back.
#[test]
fn a_features_hash_is_matched_unscoped_and_skipped_under_the_scope() {
    let rows = vec![row(Group::Features, "#41 Trim context")];
    let unscoped = rank("#4", &rows);
    assert_eq!(unscoped.scope, Some(Group::Features), "a leading # is the scope, so the 4 is the needle");
    assert_eq!(unscoped.groups[0].1[0].marks, [1], "the 4 sits after the # in the whole text");
    let plain = rank("4", &rows);
    assert_eq!(plain.groups[0].1[0].marks, [1]);
    let with_hash_in_text = rank("t#", &[row(Group::Features, "#41 t#x")]);
    assert_eq!(with_hash_in_text.groups[0].1[0].marks, [4, 5], "unscoped it is matched with its # included");
}

#[test]
fn only_a_leading_scope_character_scopes() {
    let rows = vec![row(Group::Commands, "a > b"), row(Group::Features, "#1 a")];
    assert_eq!(rank("a > b", &rows).scope, None);
}

#[test]
fn marks_count_characters_not_bytes() {
    let ranked = rank("e", &[row(Group::Commands, "\u{e9}\u{e9}e")]);
    assert_eq!(ranked.groups[0].1[0].marks, [2]);
}

mod properties {
    use super::*;
    use proptest::prelude::*;

    fn texts() -> impl Strategy<Value = Vec<String>> {
        proptest::collection::vec("[a-c /#_.-]{0,12}", 0..14)
    }

    proptest! {
        // SHELL-PALETTE-6: a row that stays holds the needle's characters, in order, at its marks.
        #[test]
        fn every_mark_is_a_needle_character_in_order(rows in texts(), needle in "[a-c]{0,4}") {
            let candidates: Vec<_> = rows.iter().map(|text| row(Group::Commands, text)).collect();
            for (_, hits) in rank(&needle, &candidates).groups {
                for hit in hits {
                    let text: Vec<char> = candidates[hit.at].text.chars().collect();
                    let marked: String = hit.marks.iter().map(|&mark| text[mark]).collect();
                    prop_assert_eq!(marked, needle.clone());
                    prop_assert!(hit.marks.windows(2).all(|pair| pair[0] < pair[1]));
                }
            }
        }

        // SHELL-PALETTE-5: no needle and no scope never shows a group more than six rows.
        #[test]
        fn no_group_shows_more_than_six_without_a_needle(rows in texts()) {
            let candidates: Vec<_> = rows.iter().map(|text| row(Group::Commands, text)).collect();
            for (_, hits) in rank("", &candidates).groups {
                prop_assert!(hits.len() <= SHOWN);
            }
        }

        // SHELL-PALETTE-6: a row is shown exactly when the needle falls in it.
        #[test]
        fn a_row_is_shown_exactly_when_its_text_holds_the_needle(rows in texts(), needle in "[a-c]{1,3}") {
            let candidates: Vec<_> = rows.iter().map(|text| row(Group::Commands, text)).collect();
            let shown: Vec<usize> = { let mut shown = at(&rank(&needle, &candidates)); shown.sort(); shown };
            let expected: Vec<usize> = (0..rows.len()).filter(|&at| fuzzy(&needle, &rows[at]).is_some()).collect();
            prop_assert_eq!(shown, expected);
        }
    }
}
