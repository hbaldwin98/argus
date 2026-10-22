use super::*;

#[test]
fn the_last_four_columns_after_the_first_two_may_be_dropped_the_last_being_rank_one() {
    let ranks: Vec<usize> = (0..7).map(|c| drop_rank(c, 7)).collect();
    assert_eq!(ranks, [0, 0, 0, 4, 3, 2, 1]);
}

#[test]
fn the_first_two_columns_stay_however_few_there_are() {
    let ranks: Vec<usize> = (0..3).map(|c| drop_rank(c, 3)).collect();
    assert_eq!(ranks, [0, 0, 1]);
    assert_eq!(drop_rank(1, 2), 0);
}

#[test]
fn each_rank_goes_at_its_own_width_the_last_first() {
    assert!(shown(false, 1, 701.0));
    assert!(!shown(false, 1, 700.0));
    assert!(shown(false, 2, 621.0));
    assert!(!shown(false, 2, 620.0));
    assert!(!shown(false, 3, 540.0));
    assert!(!shown(false, 4, 460.0));
    assert!(shown(false, 4, 461.0));
}

#[test]
fn an_optional_column_goes_at_560_and_a_plain_one_never() {
    assert!(!shown(true, 0, 560.0));
    assert!(shown(true, 0, 561.0));
    assert!(shown(false, 0, 100.0));
}

#[test]
fn nothing_is_dropped_before_the_box_is_measured() {
    assert!(shown(true, 4, 0.0));
}
