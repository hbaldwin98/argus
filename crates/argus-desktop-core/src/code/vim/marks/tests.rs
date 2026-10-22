use super::*;
use crate::code::change::Change;

fn set(len: usize, from: usize, to: usize, insert: &str) -> ChangeSet {
    ChangeSet::of(&[Change { from, to, insert: insert.into() }], len).unwrap()
}

#[test]
fn a_place_moves_with_text_put_or_taken_before_it() {
    assert_eq!(follow(&set(10, 2, 2, "abc"), 5), Some(8));
    assert_eq!(follow(&set(10, 2, 4, ""), 5), Some(3));
    assert_eq!(follow(&set(10, 2, 4, "x"), 5), Some(4));
    assert_eq!(follow(&set(10, 7, 9, ""), 5), Some(5), "after it changes nothing");
}

#[test]
fn text_put_at_a_place_leaves_it_before_the_new_text() {
    // The bookmark of the library is made with no `insertLeft`.
    assert_eq!(follow(&set(10, 5, 5, "new"), 5), Some(5));
}

#[test]
fn a_replaced_run_that_holds_the_place_strictly_takes_it_away() {
    assert_eq!(follow(&set(10, 2, 8, ""), 5), None);
    assert_eq!(follow(&set(10, 2, 8, "xyz"), 5), None);
}

#[test]
fn a_run_that_begins_or_ends_at_the_place_leaves_it_at_its_start() {
    assert_eq!(follow(&set(10, 5, 8, ""), 5), Some(5));
    assert_eq!(follow(&set(10, 5, 8, "xy"), 5), Some(5));
    assert_eq!(follow(&set(10, 2, 5, ""), 5), Some(2));
}

#[test]
fn only_a_whole_replaced_line_start_is_lost_by_a_held_line() {
    // A held line is the start of a line: a change that covers its first character takes it.
    let mut marks = Marks::default();
    let h = marks.track(5);
    marks.follow(&set(10, 5, 6, ""));
    assert_eq!(marks.line_at(h), None);
    let mut marks = Marks::default();
    let h = marks.track(5);
    marks.follow(&set(10, 2, 5, ""));
    assert_eq!(marks.line_at(h), Some(2), "the text before it was taken: the line moves up");
    let mut marks = Marks::default();
    let h = marks.track(5);
    marks.follow(&set(10, 5, 5, "x\n"));
    assert_eq!(marks.line_at(h), Some(7), "text put at it is before it now");
    marks.release();
    assert_eq!(marks.line_at(h), None);
}

#[test]
fn only_names_a_person_can_type_are_marks() {
    let mut marks = Marks::default();
    for name in ['a', 'Z', '7', '_', '<', '>'] {
        marks.set(name, 3);
        assert_eq!(marks.get(name), Some(3), "{name}");
    }
    for name in ['!', ' ', '\u{1}', '-'] {
        marks.set(name, 3);
        assert_eq!(marks.get(name), None, "{name:?}");
    }
    marks.set_hidden(ANCHOR, 4);
    assert_eq!(marks.get(ANCHOR), Some(4));
    assert!(marks.all().all(|(n, _)| !n.is_control()), "a hidden place is not listed");
}

#[test]
fn the_dot_mark_is_where_the_last_edit_ended_and_follows_what_comes_after() {
    let mut marks = Marks::default();
    assert_eq!(marks.get('.'), None);
    marks.follow(&set(10, 4, 5, "xyz"));
    assert_eq!(marks.get('.'), Some(7));
    marks.follow_remote(&set(12, 0, 0, "ab"));
    assert_eq!(marks.get('.'), Some(9), "moved by the disk's change");
    marks.follow(&set(14, 12, 12, "q"));
    assert_eq!(marks.get('.'), Some(13));
}

#[test]
fn the_jump_list_walks_between_where_the_cursor_was_and_went() {
    let mut j = Jumps::default();
    j.add(0, 5);
    assert_eq!(j.peek(-1), Some(0));
    assert_eq!(j.walk(-1, 5), Some(0));
    assert_eq!(j.walk(1, 0), Some(5));
    assert_eq!(j.walk(1, 5), Some(5), "no further than the newest");
}

#[test]
fn a_jump_from_where_the_last_one_ended_is_not_recorded_twice() {
    let mut j = Jumps::default();
    j.add(0, 5);
    j.add(5, 9);
    assert_eq!(j.walk(-1, 9), Some(5));
    assert_eq!(j.walk(-1, 5), Some(0));
    assert_eq!(j.walk(-1, 0), Some(0), "the start of the list");
}

#[test]
fn a_walk_with_nothing_recorded_goes_nowhere() {
    let mut j = Jumps::default();
    assert_eq!(j.walk(-1, 3), None);
    assert_eq!(j.walk(2, 3), None);
    assert_eq!(j.peek(-1), None);
}

#[test]
fn a_place_edited_away_is_skipped_by_a_walk() {
    let mut j = Jumps::default();
    j.add(0, 5);
    j.add(5, 9);
    j.add(9, 12);
    // Text 4..10 goes: the places at 5 and 9 are inside it.
    j.follow(&set(14, 4, 10, ""));
    assert_eq!(j.walk(-1, 6), Some(0), "the two edited away are passed over");
}

#[test]
fn the_list_holds_a_hundred_places() {
    let mut j = Jumps::default();
    for i in 0..300 {
        j.add(i, i + 1);
    }
    let mut back = 0;
    let mut at = j.walk(0, 0);
    while let Some(p) = j.walk(-1, usize::MAX) {
        if Some(p) == at {
            break;
        }
        at = Some(p);
        back += 1;
        assert!(back < 1000, "it ends");
    }
    assert!((98..=100).contains(&back), "walked {back} back");
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    /// A run of sorted, apart changes on a text of `len`.
    fn changes(len: usize) -> impl Strategy<Value = Vec<Change>> {
        proptest::collection::vec((0..=len, 0..4usize, "[a-z\n]{0,3}"), 0..4).prop_map(move |picks| {
            let mut at = 0;
            let mut out = Vec::new();
            for (gap, width, insert) in picks {
                let from = (at + gap % (len - at + 1)).min(len);
                let to = (from + width).min(len);
                out.push(Change { from, to, insert });
                at = to;
            }
            out
        })
    }

    proptest! {
        // The library's bookmark: a place that survives is where the change set
        // maps it, and a change set that puts nothing across it moves it by what
        // was put before it.
        #[test]
        fn a_place_that_survives_is_where_the_change_set_maps_it(
            len in 1usize..30, pos in 0usize..30, picks in changes(30)
        ) {
            let len = len.max(picks.iter().map(|c| c.to).max().unwrap_or(0));
            let pos = pos.min(len);
            let set = ChangeSet::of(&picks, len).unwrap();
            if let Some(to) = follow(&set, pos) {
                prop_assert!(to <= set.new_len());
                prop_assert_eq!(to, set.map_pos(pos, -1));
            }
        }

        // A held line is never put past the end, and after a change that touched
        // none of the text up to it, it only moves.
        #[test]
        fn a_held_line_stays_in_the_text(len in 1usize..30, pos in 0usize..30, picks in changes(30)) {
            let len = len.max(picks.iter().map(|c| c.to).max().unwrap_or(0));
            let pos = pos.min(len);
            let set = ChangeSet::of(&picks, len).unwrap();
            let mut marks = Marks::default();
            let h = marks.track(pos);
            marks.follow(&set);
            if let Some(at) = marks.line_at(h) {
                prop_assert!(at <= set.new_len());
            }
        }
    }
}
