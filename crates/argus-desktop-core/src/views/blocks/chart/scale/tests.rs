use super::*;

#[test]
fn the_axis_runs_from_zero_to_a_round_number_above_the_top() {
    let a = axis(0.0, 11_400.0);
    assert_eq!((a.lo, a.hi), (0.0, 15_000.0));
    assert_eq!(a.ticks, [0.0, 5_000.0, 10_000.0, 15_000.0]);
}

#[test]
fn a_negative_value_pulls_the_floor_below_zero() {
    let a = axis(-30.0, 80.0);
    assert!(a.lo <= -30.0 && a.hi >= 80.0);
    assert!(a.ticks.contains(&0.0));
}

#[test]
fn a_flat_series_gets_a_span_of_one() {
    let a = axis(0.0, 0.0);
    assert_eq!((a.lo, a.hi), (0.0, 1.0));
    assert!(a.ticks.len() >= 2);
}

#[test]
fn a_range_that_is_not_a_number_falls_back_to_zero_to_one() {
    let a = axis(0.0, f64::INFINITY);
    assert_eq!((a.lo, a.hi, a.ticks), (0.0, 1.0, vec![0.0, 1.0]));
}

#[test]
fn ticks_are_tidied_so_a_sum_of_steps_is_not_shown_with_float_dust() {
    assert!(axis(0.0, 0.3).ticks.contains(&0.3));
}

#[test]
fn a_huge_value_ends_at_the_tick_cap() {
    let a = axis(0.0, 1e15);
    assert!(a.ticks.len() <= MAX_TICKS);
}

#[test]
fn the_extent_holds_the_seed_and_both_ends() {
    assert_eq!(extent(&[3.0, 9.0, 5.0], 0.0), (0.0, 9.0));
    assert_eq!(extent(&[-3.0, 2.0], 0.0), (-3.0, 2.0));
    assert_eq!(extent(&[], 7.0), (7.0, 7.0));
}

#[test]
fn a_long_array_does_not_overflow_anything() {
    let big = vec![1.0; 2_000_000];
    assert_eq!(extent(&big, 0.0), (0.0, 1.0));
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        // CHART-2: the axis holds the data and has between two ticks and the cap, always ascending.
        #[test]
        fn the_axis_holds_the_data_with_ascending_ticks(min in -1e9f64..0.0, span in 0.0f64..1e9) {
            let a = axis(min, min + span);
            prop_assert!(a.lo <= min.min(0.0) + 1e-6 && a.hi >= (min + span).max(a.lo) - 1e-6);
            prop_assert!(a.ticks.len() >= 2 && a.ticks.len() <= MAX_TICKS);
            prop_assert!(a.ticks.windows(2).all(|w| w[0] < w[1]));
        }
    }
}
