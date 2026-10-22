use ropey::Rope;

use super::*;

fn go(text: &str, from: usize, m: Motion, count: Option<usize>) -> Option<Reach> {
    let r = Rope::from_str(text);
    let mut last = None;
    let mut aim = Aim { want: 0, last_find: &mut last, operator: false };
    reach(m, Text(&r), from, count, &mut aim)
}

#[test]
fn a_motion_says_how_an_operator_takes_it() {
    let w = go("ab cd", 0, Motion::WordForward { big: false }, None).unwrap();
    assert!(!w.linewise && !w.inclusive);
    let e = go("ab cd", 0, Motion::WordEnd { big: false }, None).unwrap();
    assert!(e.inclusive);
    let j = go("a\nb", 0, Motion::Down, None).unwrap();
    assert!(j.linewise && j.vertical);
    assert!(go("ab", 0, Motion::LineEnd, None).unwrap().inclusive);
    assert!(go("a(b)", 1, Motion::Matching, None).unwrap().far);
}

#[test]
fn a_motion_that_cannot_go_gives_nothing() {
    assert_eq!(go("ab", 0, Motion::Left, None), None);
    assert_eq!(go("ab", 1, Motion::Right, None), None, "a cursor does not rest on the break");
    assert_eq!(go("a", 0, Motion::Down, None), None);
    assert_eq!(go("a", 0, Motion::Up, None), None);
    assert_eq!(go("abc", 0, Motion::Find(Find::To, 'z'), None), None);
    assert_eq!(go("abc", 0, Motion::RepeatFind { reverse: false }, None), None, "no find to repeat");
    assert_eq!(go("abc", 0, Motion::Matching, None), None);
    assert_eq!(go("a\nb", 2, Motion::Plus, None), None);
}

#[test]
fn an_operator_may_take_the_last_character() {
    let r = Rope::from_str("ab");
    let mut last = None;
    let mut aim = Aim { want: 0, last_find: &mut last, operator: true };
    assert_eq!(reach(Motion::Right, Text(&r), 1, None, &mut aim).unwrap().to, 2);
}

#[test]
fn a_find_is_remembered_for_the_repeat_keys() {
    let r = Rope::from_str("a-b-c-d");
    let mut last = None;
    let mut aim = Aim { want: 0, last_find: &mut last, operator: false };
    assert_eq!(reach(Motion::Find(Find::To, '-'), Text(&r), 0, None, &mut aim).unwrap().to, 1);
    assert_eq!(reach(Motion::RepeatFind { reverse: false }, Text(&r), 1, None, &mut aim).unwrap().to, 3);
    assert_eq!(reach(Motion::RepeatFind { reverse: true }, Text(&r), 3, None, &mut aim).unwrap().to, 1);
    assert_eq!(last, Some((Find::To, '-')));
}

#[test]
fn a_count_on_g_is_a_line_number() {
    assert_eq!(go("a\nb\nc", 0, Motion::FileEnd, Some(2)).unwrap().to, 2);
    assert_eq!(go("a\nb\nc", 0, Motion::FileEnd, Some(99)).unwrap().to, 4);
    assert_eq!(go("a\nb\nc", 4, Motion::FileStart, None).unwrap().to, 0);
}

#[test]
fn up_and_down_aim_for_the_wanted_column() {
    let r = Rope::from_str("abcdef\nab\nabcdef");
    let mut last = None;
    let mut aim = Aim { want: 4, last_find: &mut last, operator: false };
    let down = reach(Motion::Down, Text(&r), 4, None, &mut aim).unwrap();
    assert_eq!(down.to, 8, "a short line gives its last character");
    let again = reach(Motion::Down, Text(&r), down.to, None, &mut aim).unwrap();
    assert_eq!(again.to, 14, "and the column comes back on a long one");
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    fn a_motion() -> impl Strategy<Value = Motion> {
        prop_oneof![
            Just(Motion::Left),
            Just(Motion::Right),
            Just(Motion::Up),
            Just(Motion::Down),
            Just(Motion::LineStart),
            Just(Motion::FirstNonBlank),
            Just(Motion::LineEnd),
            Just(Motion::Column),
            any::<bool>().prop_map(|big| Motion::WordForward { big }),
            any::<bool>().prop_map(|big| Motion::WordEnd { big }),
            any::<bool>().prop_map(|big| Motion::WordBack { big }),
            any::<bool>().prop_map(|big| Motion::WordEndBack { big }),
            Just(Motion::FileStart),
            Just(Motion::FileEnd),
            proptest::sample::select(vec!['a', '(', 'é', ' ']).prop_map(|c| Motion::Find(Find::To, c)),
            proptest::sample::select(vec!['a', ')', 'é']).prop_map(|c| Motion::Find(Find::TillBack, c)),
            Just(Motion::RepeatFind { reverse: true }),
            Just(Motion::ParagraphForward),
            Just(Motion::ParagraphBack),
            Just(Motion::Matching),
            Just(Motion::Line),
            Just(Motion::Plus),
            Just(Motion::Minus),
            Just(Motion::Underscore),
        ]
    }

    proptest! {
        // Not in any one source: a motion lands in the text on a character,
        // wherever it starts and whatever the count.
        #[test]
        fn a_motion_lands_in_the_text_on_a_character(
            text in "[ab (){}é😀.\n]{0,30}",
            from in 0usize..40,
            m in a_motion(),
            count in proptest::option::of(0usize..8),
            operator in any::<bool>(),
        ) {
            let r = Rope::from_str(&text);
            let from = r.floor_char_boundary(from.min(text.len()));
            let mut last = Some((Find::To, 'a'));
            let mut aim = Aim { want: 2, last_find: &mut last, operator };
            if let Some(reach) = super::reach(m, Text(&r), from, count, &mut aim) {
                prop_assert!(reach.to <= text.len());
                prop_assert!(r.is_char_boundary(reach.to));
            }
        }

        // The word motions only go their way.
        #[test]
        fn word_motions_go_the_way_they_say(text in "[ab .\n]{0,30}", from in 0usize..30, big in any::<bool>()) {
            let r = Rope::from_str(&text);
            let from = r.floor_char_boundary(from.min(text.len()));
            let t = Text(&r);
            prop_assert!(word::forward(t, from, big) >= from);
            prop_assert!(word::end(t, from, big) >= from.min(t.prev(t.len())));
            prop_assert!(word::back(t, from, big) <= from);
            prop_assert!(word::end_back(t, from, big) <= from);
        }
    }
}
