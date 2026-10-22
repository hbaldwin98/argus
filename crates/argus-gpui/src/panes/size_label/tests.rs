use super::*;

fn parked(offset: u32, depth: u32) -> Option<Parked> {
    Some(Parked { offset, depth })
}

#[test]
fn the_size_is_silent_at_rest_and_shows_just_after_a_resize() {
    let t0 = Instant::now();
    let mut label = SizeLabel::default();
    assert!(!label.frame(Some(1), 24, 80, None, t0), "the first grid is nobody's resize");
    assert_eq!(label.text(t0), "");
    assert!(label.frame(Some(1), 30, 100, None, t0));
    assert_eq!(label.text(t0), "100 \u{d7} 30");
    assert_eq!(label.text(t0 + SHOWN_FOR - Duration::from_millis(1)), "100 \u{d7} 30");
    assert_eq!(label.text(t0 + SHOWN_FOR), "", "at rest it says nothing");
}

#[test]
fn another_panes_grid_is_not_a_resize() {
    let t0 = Instant::now();
    let mut label = SizeLabel::default();
    label.frame(Some(1), 24, 80, None, t0);
    assert!(!label.frame(Some(2), 30, 100, None, t0));
    assert_eq!(label.text(t0), "");
}

#[test]
fn a_frame_of_the_same_size_does_not_start_the_size_again() {
    let t0 = Instant::now();
    let mut label = SizeLabel::default();
    label.frame(Some(1), 24, 80, None, t0);
    label.frame(Some(1), 30, 100, None, t0);
    let later = t0 + Duration::from_secs(5);
    assert!(!label.frame(Some(1), 30, 100, None, later));
    assert_eq!(label.text(later), "");
}

#[test]
fn a_parked_view_keeps_its_place_at_rest_and_leads_the_size_while_it_shows() {
    let t0 = Instant::now();
    let mut label = SizeLabel::default();
    label.frame(Some(1), 24, 80, parked(3, 40), t0);
    assert_eq!(label.text(t0), "\u{2191} 3/40");
    label.frame(Some(1), 30, 100, parked(3, 40), t0);
    assert_eq!(label.text(t0), "\u{2191} 3/40 \u{b7} 100 \u{d7} 30");
    assert_eq!(label.text(t0 + SHOWN_FOR), "\u{2191} 3/40");
    label.frame(Some(1), 30, 100, None, t0 + SHOWN_FOR);
    assert_eq!(label.text(t0 + SHOWN_FOR), "", "back live, nothing to say");
}

#[test]
fn a_pane_with_no_grid_yet_says_nothing() {
    let label = SizeLabel::default();
    assert_eq!(label.text(Instant::now()), "");
    let mut label = SizeLabel::default();
    label.frame(Some(1), 0, 0, parked(1, 2), Instant::now());
    assert_eq!(label.text(Instant::now()), "");
}
