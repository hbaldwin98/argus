use super::*;

#[test]
fn the_points_run_edge_to_edge_with_the_highest_value_at_the_top() {
    let p = points(&[1.0, 5.0, 3.0]);
    assert_eq!(p.len(), 3);
    assert_eq!((p[0].0, p[1].0, p[2].0), (0.0, 50.0, 100.0));
    assert_eq!(p[0].1, 28.0);
    assert_eq!(p[1].1, 4.0);
    assert!(p[2].1 > p[1].1 && p[2].1 < p[0].1);
}

#[test]
fn a_flat_series_is_drawn_at_mid_height() {
    assert!(points(&[4.0, 4.0, 4.0]).iter().all(|p| p.1 == 16.0));
}

#[test]
fn values_that_are_not_numbers_are_dropped() {
    assert_eq!(points(&[1.0, f64::NAN, 3.0, f64::INFINITY]).len(), 2);
}

#[test]
fn one_point_sits_in_the_middle_and_none_has_no_points() {
    assert_eq!(points(&[7.0]), [(50.0, 16.0)]);
    assert!(points(&[]).is_empty());
}

#[test]
fn the_summary_says_how_many_and_how_far() {
    assert_eq!(summary(&[2.0, 9.0, f64::NAN]), "trend over 2 points, from 2 to 9");
    assert_eq!(summary(&[]), "no trend");
}
