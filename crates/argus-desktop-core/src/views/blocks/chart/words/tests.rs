use argus_protocol::Tone;

use super::*;

fn series(name: &str, values: &[Option<f64>]) -> Series {
    Series { name: name.into(), values: values.to_vec(), tone: Tone::Neutral }
}

#[test]
fn numbers_are_said_in_compact_notation_to_two_decimals() {
    assert_eq!(compact(0.0), "0");
    assert_eq!(compact(999.0), "999");
    assert_eq!(compact(1_234.0), "1.23K");
    assert_eq!(compact(15_000.0), "15K");
    assert_eq!(compact(2_500_000.0), "2.5M");
    assert_eq!(compact(3e9), "3B");
    assert_eq!(compact(0.5), "0.5");
    assert_eq!(compact(-1_500.0), "-1.5K");
}

#[test]
fn a_number_that_rounds_up_to_the_next_unit_is_said_in_it() {
    assert_eq!(compact(999_999.0), "1M");
}

#[test]
fn a_tiny_negative_is_not_said_as_minus_zero() {
    assert_eq!(compact(-0.001), "0");
}

#[test]
fn the_summary_names_the_shape_the_series_the_points_and_the_range() {
    let s = [series("injected", &[Some(0.0), Some(11_400.0)]), series("brief", &[Some(3_000.0), None])];
    assert_eq!(summary(ChartShape::Bar, &s, 2, Some("tokens")), "bar chart of injected, brief over 2 points, from 0 to 11.4K tokens");
}

#[test]
fn a_chart_with_no_series_says_so_and_no_range() {
    assert_eq!(summary(ChartShape::Line, &[], 0, None), "line chart of no series over 0 points");
}

#[test]
fn a_hover_reads_the_label_the_grouped_value_and_the_unit() {
    assert_eq!(hover("start", 10_600.0, Some("tokens")), "start: 10,600 tokens");
    assert_eq!(hover("start", 4.0, None), "start: 4");
    assert_eq!(hover("start", 4.0, Some("")), "start: 4");
}
