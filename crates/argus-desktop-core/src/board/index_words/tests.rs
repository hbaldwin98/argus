use super::*;
use crate::board::features::{self, Index, Listing};
use crate::board::lenses::flows::FlowCursor;
use crate::connection::page::Lens;
use crate::fixtures;

static SHOWN: Listing = Listing { layout: Layout::List, show_folded: true, filter: String::new(), opened: None };

/// The fixture board as the index lists it: #7 active with its agent, #8 after #7, #10 new, #6 done,
/// #5 held, #4 archived.
fn index() -> Features {
    let board = fixtures::board();
    let tree = fixtures::feature_tree();
    let tasks = fixtures::tasks("trim");
    features::build(&Index {
        repository: "argus",
        board: Some(&board),
        tree: &tree,
        listing: &SHOWN,
        peek: Some("trim"),
        peek_tasks: Some(&tasks),
        peek_diagrams: None,
        opened: false,
        lens: Lens::Work,
        work: None,
        record: None,
        flow: FlowCursor::default(),
        views: &[],
        now: 3 * 86_400 + 60,
    })
}

fn row(number: i64) -> FeatureRow {
    index().rows.into_iter().find(|row| row.number == number).unwrap()
}

#[test]
fn a_row_says_what_holds_it_back_before_what_comes_next() {
    assert_eq!(note(&row(5)), [Bit::Faint("held: ".into()), Bit::Text("\u{201c}until the bump lands\u{201d}".into())]);
    assert_eq!(note(&row(8)), [Bit::Faint("after ".into()), Bit::Mono("#7".into())]);
}

#[test]
fn the_peeked_row_says_what_is_next_then_its_agent() {
    let bits = note(&row(7));
    assert_eq!(bits[0], Bit::Faint("next ".into()));
    assert_eq!(bits[1], Bit::MonoFaint("1.2".into()));
    assert_eq!(bits[3], Bit::Text(" \u{b7} ".into()));
    assert!(matches!(&bits[4], Bit::Agent(agent) if agent.title == "claude"));
}

#[test]
fn a_row_with_nothing_to_say_by_its_work_says_by_its_standing() {
    assert_eq!(note(&row(10)), [Bit::Faint("nothing written yet".into())]);
    assert_eq!(note(&row(6)), [Bit::Faint("every task done \u{b7} archive it to accept it".into())]);
    assert_eq!(note(&row(4)), [Bit::Faint("archived".into())]);
}

#[test]
fn the_tasks_cell_says_new_none_or_the_count() {
    assert_eq!(tasks(&row(10)), Tasks::New);
    assert_eq!(tasks(&row(4)), Tasks::Progress { done: 2, total: 2 });
    assert_eq!(tasks(&FeatureRow { tasks_total: 0, standing: "queued", ..row(8) }), Tasks::None);
}

#[test]
fn the_questions_cell_prefers_what_is_open_to_what_was_decided() {
    assert_eq!(questions(&row(7)), Some(("1 open".into(), Tone::Attention)));
    assert_eq!(questions(&FeatureRow { open: 0, decided: 2, ..row(7) }), Some(("2 decided".into(), Tone::Ok)));
    assert_eq!(questions(&row(8)), None);
}

#[test]
fn a_card_corner_is_the_agent_then_what_it_follows_then_new() {
    assert_eq!(corner(&row(7)), Some(Corner::Agent("working")));
    assert_eq!(corner(&row(8)), Some(Corner::After("#7".into())));
    assert_eq!(corner(&row(10)), Some(Corner::New));
    assert_eq!(corner(&row(5)), None);
}

#[test]
fn a_card_work_line_is_words_until_there_are_tasks() {
    assert_eq!(work(&row(10)), Work::Words("nothing written yet"));
    assert_eq!(work(&FeatureRow { tasks_total: 0, standing: "queued", ..row(8) }), Work::Words("no tasks yet"));
    assert!(matches!(work(&row(7)), Work::Progress { done: 3, total: 7, .. }));
}

#[test]
fn a_footer_keeps_its_time_apart_only_when_it_ends_with_ago() {
    assert_eq!(footer("claude is on it \u{b7} 3d ago"), ("claude is on it", Some("3d ago")));
    assert_eq!(footer("held"), ("held", None));
    assert_eq!(footer("written down \u{b7} soon"), ("written down \u{b7} soon", None));
    assert_eq!(footer("a \u{b7} b \u{b7} 1w ago"), ("a \u{b7} b", Some("1w ago")));
}

#[test]
fn the_strip_counts_what_is_nonzero_with_attention_and_ok_tones() {
    let features = index();
    assert_eq!(
        counts(&features),
        [Part::Attention("1 active".into()), Part::Text(" \u{b7} ".into()), Part::Text("2 queued".into()), Part::Text(" \u{b7} ".into()), Part::Ok("1 done".into())]
    );
    let none = Features { active: 0, queued: 3, done: 0, ..features };
    assert_eq!(counts(&none), [Part::Text("3 queued".into())]);
}

#[test]
fn an_empty_list_says_why_by_the_filter_and_the_fold() {
    let base = Features { filter: String::new(), folded: 0, show_folded: false, ..Features::default() };
    assert_eq!(nothing_listed(&base), "Every feature here is held or archived.");
    assert_eq!(nothing_listed(&Features { filter: "zz".into(), ..base.clone() }), "Nothing matches \u{201c}zz\u{201d}.");
    assert_eq!(nothing_listed(&Features { filter: "zz".into(), folded: 2, ..base.clone() }), "Only held or archived features match \u{201c}zz\u{201d}.");
    assert_eq!(nothing_listed(&Features { filter: "zz".into(), folded: 2, show_folded: true, ..base }), "Every feature here is held or archived.");
}

#[test]
fn the_fold_toggle_is_the_lists_and_only_while_something_is_folded() {
    let on_list = Features { layout: "list", folded: 4, ..Features::default() };
    assert_eq!(fold_toggle(&on_list), Some(("Show held and archived ", 4)));
    assert_eq!(fold_toggle(&Features { show_folded: true, ..on_list.clone() }), Some(("Fold held and archived ", 4)));
    assert_eq!(fold_toggle(&Features { folded: 0, ..on_list.clone() }), None);
    assert_eq!(fold_toggle(&Features { layout: "board", ..on_list }), None);
}

#[test]
fn the_key_bar_names_the_layout_v_would_switch_to() {
    assert_eq!(key_bar("list")[2], ("V", "board"));
    assert_eq!(key_bar("board")[2], ("V", "list"));
}

#[test]
fn only_held_and_archived_take_a_card_and_the_rest_say_why_they_follow_the_work() {
    let active = row(7);
    assert_eq!(drop_verdict(&active, FeatureColumn::Held), Landing::Hold);
    assert_eq!(drop_verdict(&active, FeatureColumn::Archived), Landing::Archive);
    assert_eq!(drop_verdict(&active, FeatureColumn::Active), Landing::Nothing, "its own column");
    assert_eq!(drop_verdict(&active, FeatureColumn::Queued), Landing::Refuse("Queued follows the work: follows the work.".into()));
    assert_eq!(drop_verdict(&active, FeatureColumn::Done), Landing::Refuse("Done follows the work: when its 4 open tasks are done.".into()));
    assert_eq!(drop_verdict(&row(10), FeatureColumn::Active), Landing::Refuse("Active follows the work: when a task starts or an agent arrives.".into()));
}

#[test]
fn a_held_card_dropped_on_held_or_an_archived_one_on_archived_does_nothing() {
    assert_eq!(drop_verdict(&row(5), FeatureColumn::Held), Landing::Nothing);
    assert_eq!(drop_verdict(&row(4), FeatureColumn::Archived), Landing::Nothing);
    assert_eq!(drop_verdict(&row(5), FeatureColumn::Archived), Landing::Archive);
    assert_eq!(drop_verdict(&row(4), FeatureColumn::Held), Landing::Hold);
}

#[test]
fn a_column_lights_only_where_the_card_would_be_taken() {
    let active = row(7);
    assert_eq!(target(&active, Some(FeatureColumn::Held)), Some(FeatureColumn::Held));
    assert_eq!(target(&active, Some(FeatureColumn::Done)), None);
    assert_eq!(target(&active, None), None);
    assert_eq!(dragged(&active, Some(FeatureColumn::Archived)), "#7 is being dragged to Archived");
    assert_eq!(dragged(&active, Some(FeatureColumn::Queued)), "#7 is being dragged");
    assert_eq!(drop_banner(&active, FeatureColumn::Archived), "Drop to archive: you accept #7 as finished");
    assert_eq!(drop_banner(&active, FeatureColumn::Held), "Drop to hold: you say why #7 waits");
}

#[test]
fn a_moves_key_is_held_s_h_and_archive_s_shift_a() {
    assert_eq!(move_key("held"), Some("H"));
    assert_eq!(move_key("unhold"), Some("H"));
    assert_eq!(move_key("archived"), Some("Shift A"));
    assert_eq!(move_key("reopen"), Some("Shift A"));
    assert_eq!(move_key("queued"), None);
}

#[test]
fn a_group_word_is_its_column() {
    for column in [FeatureColumn::Queued, FeatureColumn::Active, FeatureColumn::Done, FeatureColumn::Held, FeatureColumn::Archived] {
        assert_eq!(FeatureColumn::of(column.word()), Some(column));
    }
    assert_eq!(FeatureColumn::of("new"), None);
    assert_eq!(group_tone(FeatureColumn::Active), Tone::Attention);
    assert_eq!(group_tone(FeatureColumn::Done), Tone::Ok);
    assert_eq!(group_tone(FeatureColumn::Held), Tone::Plain);
}
