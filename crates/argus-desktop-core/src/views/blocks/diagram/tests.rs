use super::*;
use crate::connection::page::PageParticipant;

fn who(id: &str, actor: bool) -> PageParticipant {
    PageParticipant { id: id.into(), label: id.into(), actor }
}

fn step(number: usize, from: &str, to: Option<&str>, reply: bool, text: &str) -> FlowLine {
    FlowLine::Step { number, from: from.into(), to: to.map(str::to_owned), reply, text: text.into() }
}

fn diagram(participants: Vec<PageParticipant>, rows: Vec<FlowLine>) -> PageDiagram {
    PageDiagram { source: String::new(), participants, rows }
}

fn flow() -> Drawn {
    layout(&diagram(
        vec![who("hook", true), who("argusd", false), who("pane", false)],
        vec![
            step(1, "hook", Some("argusd"), false, "view show"),
            FlowLine::Branch { word: "alt".into(), when: "daemon is up".into() },
            step(2, "argusd", None, false, "check the view"),
            step(3, "argusd", Some("pane"), false, "tell the pane"),
            step(4, "argusd", Some("hook"), true, "shown"),
        ],
    ))
    .unwrap()
}

#[test]
fn nothing_to_draw_without_a_participant_or_a_row() {
    assert!(layout(&diagram(vec![], vec![step(1, "a", Some("b"), false, "x")])).is_none());
    assert!(layout(&diagram(vec![who("a", false)], vec![])).is_none());
}

#[test]
fn lifelines_run_left_to_right_in_the_order_declared_and_never_overlap_their_boxes() {
    let d = flow();
    assert!(d.cols.windows(2).all(|pair| pair[1].x - pair[0].x >= (pair[0].box_w + pair[1].box_w) / 2.0 + 16.0 - 1e-3));
    assert_eq!(d.cols[0].x, PAD + d.cols[0].box_w / 2.0);
}

#[test]
fn a_gap_widens_until_the_text_between_the_lifelines_fits() {
    let long = "a message that is a good deal longer than the boxes";
    let d = layout(&diagram(vec![who("a", false), who("b", false)], vec![step(1, "a", Some("b"), false, long)])).unwrap();
    let need = long.chars().count() as f32 * 7.0 + 44.0;
    assert!(d.cols[1].x - d.cols[0].x >= need - 1e-3);
}

#[test]
fn a_message_across_two_gaps_shares_its_room_between_them() {
    let long = "x".repeat(60);
    let d = layout(&diagram(vec![who("a", false), who("b", false), who("c", false)], vec![step(1, "a", Some("c"), false, &long)])).unwrap();
    let need = 60.0 * 7.0 + 44.0;
    assert!(d.cols[2].x - d.cols[0].x >= need - 1e-3);
}

#[test]
fn a_participant_only_a_message_names_gets_a_lifeline_labelled_by_its_id() {
    let d = layout(&diagram(vec![who("a", false)], vec![step(1, "a", Some("stranger"), false, "hi")])).unwrap();
    assert_eq!(d.cols.len(), 2);
    assert_eq!((d.cols[1].id.as_str(), d.cols[1].label.as_str(), d.cols[1].actor), ("stranger", "stranger", false));
}

#[test]
fn a_box_is_as_wide_as_its_label_and_never_under_ninety() {
    let d = layout(&diagram(vec![who("a", false), who("a-rather-long-participant-name", false)], vec![step(1, "a", Some("a-rather-long-participant-name"), false, "x")])).unwrap();
    assert_eq!(d.cols[0].box_w, 90.0);
    assert_eq!(d.cols[1].box_w, 30.0 * 7.0 + 24.0);
}

#[test]
fn a_long_label_is_cut_at_eighty_characters() {
    let name = "n".repeat(100);
    let d = layout(&diagram(vec![PageParticipant { id: "a".into(), label: name, actor: false }, who("b", false)], vec![step(1, "a", Some("b"), false, "x")])).unwrap();
    assert_eq!(d.cols[0].label.chars().count(), 80);
    assert!(d.cols[0].label.ends_with('\u{2026}'));
}

#[test]
fn rows_are_laid_down_the_page_at_their_own_heights() {
    let d = flow();
    let ys: Vec<f32> = d
        .items
        .iter()
        .map(|item| match item {
            Item::Branch { y, .. } | Item::Arrow { y, .. } | Item::SelfCall { y, .. } => *y,
        })
        .collect();
    // An actor makes room above the boxes: 32 + 30 + 12 is where the first row starts.
    assert_eq!(ys[0], 32.0 + 30.0 + 12.0 + 28.0, "an arrow is 28 under its row's top");
    assert_eq!(ys[1], 32.0 + 30.0 + 12.0 + 44.0, "a branch band starts where the row does");
    assert_eq!(ys[2], 32.0 + 30.0 + 12.0 + 44.0 + 32.0 + 14.0, "a self-call is 14 under its row's top");
    assert_eq!(ys[3], 32.0 + 30.0 + 12.0 + 44.0 + 32.0 + 52.0 + 28.0);
}

#[test]
fn the_foot_boxes_come_after_the_last_row_and_the_height_holds_them() {
    let d = flow();
    let rows_end = 32.0 + 30.0 + 12.0 + 44.0 + 32.0 + 52.0 + 44.0 + 44.0;
    assert_eq!(d.bottom, rows_end + 8.0);
    assert_eq!(d.height, d.bottom + 30.0 + 8.0);
}

#[test]
fn no_actor_means_no_room_for_a_stick_figure() {
    let d = layout(&diagram(vec![who("a", false), who("b", false)], vec![step(1, "a", Some("b"), false, "x")])).unwrap();
    assert_eq!(d.top, 4.0);
}

#[test]
fn an_arrow_leaves_just_off_the_senders_lifeline_toward_the_receiver() {
    let d = flow();
    let (hook, argusd) = (d.cols[0].x, d.cols[1].x);
    match &d.items[0] {
        Item::Arrow { from_x, to_x, mid_x, number, reply, .. } => {
            assert_eq!((*from_x, *to_x, *number, *reply), (hook + 9.0, argusd, 1, false));
            assert_eq!(*mid_x, (hook + argusd) / 2.0, "the text is centred between the lifelines, not the dot and the head");
        }
        other => panic!("{other:?}"),
    }
    // The reply goes back left, and is a reply.
    match d.items.last().unwrap() {
        Item::Arrow { from_x, to_x, reply, .. } => assert_eq!((*from_x, *to_x, *reply), (argusd - 9.0, hook, true)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_step_from_a_participant_to_itself_that_is_not_a_self_call_is_skipped_but_takes_its_room() {
    let d = layout(&diagram(vec![who("a", false), who("b", false)], vec![step(1, "a", Some("a"), false, "odd"), step(2, "a", Some("b"), false, "x")])).unwrap();
    assert_eq!(d.items.len(), 1);
    match &d.items[0] {
        Item::Arrow { y, .. } => assert_eq!(*y, 4.0 + 30.0 + 12.0 + 44.0 + 28.0),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_last_lifelines_self_call_label_widens_the_drawing() {
    let long = "a label that runs well past the last lifeline and its box";
    let d = layout(&diagram(vec![who("a", false), who("b", false)], vec![step(1, "b", None, false, long)])).unwrap();
    let last = d.cols.last().unwrap();
    assert!(d.width >= last.x + 32.0 + 8.0 + long.chars().count() as f32 * 7.0 + 20.0 - 1e-3);
}

#[test]
fn a_branch_label_widens_a_narrow_drawing() {
    let when = "w".repeat(70);
    let d = layout(&diagram(vec![who("a", false)], vec![FlowLine::Branch { word: "opt".into(), when }, step(1, "a", None, false, "x")])).unwrap();
    assert!(d.width >= (3.0 + 70.0) * 7.0 + 40.0 + 24.0 - 1e-3);
}

#[test]
fn the_summary_counts_steps_and_names_the_lifelines() {
    assert_eq!(flow().summary, "Sequence diagram: 4 steps between hook, argusd, pane");
    let one = layout(&diagram(vec![who("a", false), who("b", false)], vec![step(1, "a", Some("b"), false, "x")])).unwrap();
    assert_eq!(one.summary, "Sequence diagram: 1 step between a, b");
}

/// FIGURE-2: the larger view is titled by what was drawn, counting steps and not branch bands.
#[test]
fn the_larger_view_is_titled_by_the_steps_it_draws() {
    let rows = vec![step(1, "a", Some("b"), false, "x"), FlowLine::Branch { word: "opt".into(), when: "w".into() }, step(2, "b", Some("a"), true, "y")];
    assert_eq!(title(&diagram(vec![who("a", false), who("b", false)], rows)), "Sequence diagram \u{b7} 2 steps");
    assert_eq!(title(&diagram(vec![who("a", false)], vec![step(1, "a", None, false, "x")])), "Sequence diagram \u{b7} 1 step");
}

/// DIAGRAM-3: a label cut at 80 characters keeps its whole text for a tooltip, where it is drawn.
#[test]
fn a_cut_label_carries_its_whole_text_and_where_it_is_drawn() {
    let long = "x".repeat(MAX_LABEL + 20);
    let d = layout(&diagram(
        vec![who("a", false), PageParticipant { id: "b".into(), label: long.clone(), actor: false }],
        vec![step(1, "a", Some("b"), false, &long), step(2, "a", None, false, "short")],
    ))
    .unwrap();
    // The participant's box at the head and the foot, and the one message; the short one is whole.
    assert_eq!(d.clipped.len(), 3);
    assert!(d.clipped.iter().all(|c| c.full == long && c.w > 0.0 && c.h > 0.0));
    let message = d.clipped.iter().find(|c| c.h == TEXT_H).unwrap();
    assert!((message.x + message.w / 2.0 - (d.cols[0].x + d.cols[1].x) / 2.0).abs() < 1e-3, "centred on the message");
    let boxes: Vec<f32> = d.clipped.iter().filter(|c| c.h == HEAD_H).map(|c| c.y).collect();
    assert_eq!(boxes, [d.top, d.bottom]);
}

#[test]
fn a_label_that_fits_has_nothing_to_hover() {
    assert!(flow().clipped.is_empty());
}
