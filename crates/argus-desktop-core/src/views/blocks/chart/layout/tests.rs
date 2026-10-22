use super::*;

fn s(name: &str, tone: Tone, values: &[Option<f64>]) -> Series {
    Series { name: name.into(), values: values.to_vec(), tone }
}

fn labels(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("p{i}")).collect()
}

fn some(values: &[f64]) -> Vec<Option<f64>> {
    values.iter().map(|v| Some(*v)).collect()
}

#[test]
fn the_grid_spans_the_plot_with_the_top_tick_at_the_top_and_zero_on_the_baseline() {
    let c = layout(ChartShape::Bar, &labels(2), &[s("a", Tone::Neutral, &some(&[0.0, 10.0]))], None);
    assert_eq!((c.left, c.right), (44.0, 470.0));
    let first = c.ticks.first().unwrap();
    let last = c.ticks.last().unwrap();
    assert_eq!(first.label, "0");
    assert!((first.y - c.baseline).abs() < 1e-3);
    assert!((last.y - 10.0).abs() < 1e-3, "top tick at the plot's top, got {}", last.y);
}

#[test]
fn bars_sit_in_bands_and_a_line_runs_edge_to_edge() {
    let data = [s("a", Tone::Neutral, &some(&[1.0, 2.0, 3.0]))];
    let bar = layout(ChartShape::Bar, &labels(3), &data, None);
    let line = layout(ChartShape::Line, &labels(3), &data, None);
    assert_eq!(line.labels[0].x, 44.0);
    assert_eq!(line.labels[2].x, 470.0);
    assert!((bar.labels[0].x - (44.0 + 426.0 / 6.0)).abs() < 1e-3);
}

#[test]
fn a_single_point_is_centred() {
    let c = layout(ChartShape::Line, &labels(1), &[s("a", Tone::Neutral, &some(&[5.0]))], None);
    assert!((c.labels[0].x - 257.0).abs() < 1e-3);
    assert!(c.series[0].runs.is_empty(), "a run of one draws only its dot");
    assert_eq!(c.series[0].dots.len(), 1);
}

#[test]
fn labels_are_thinned_so_the_longest_fits_its_slot() {
    let long: Vec<String> = (0..30).map(|i| format!("a long label {i}")).collect();
    let values = some(&vec![1.0; 30]);
    let c = layout(ChartShape::Line, &long, &[s("a", Tone::Neutral, &values)], None);
    assert!(c.labels.len() < 30 && c.labels.len() >= 2);
    assert_eq!(c.labels[0].text, "a long label 0");
    let step = c.labels[1].x - c.labels[0].x;
    assert!(step >= 14.0 * 6.0, "labels no closer than their text, got {step}");
}

#[test]
fn a_gap_breaks_the_line_into_runs_and_a_lone_point_keeps_only_its_dot() {
    let v = [Some(1.0), Some(2.0), None, Some(3.0), None, Some(4.0), Some(5.0)];
    let c = layout(ChartShape::Line, &labels(7), &[s("a", Tone::Neutral, &v)], None);
    assert_eq!(c.series[0].runs.iter().map(Vec::len).collect::<Vec<_>>(), [2, 2]);
    assert_eq!(c.series[0].dots.len(), 5);
}

#[test]
fn a_value_that_is_not_a_number_is_a_gap() {
    let v = [Some(1.0), Some(f64::NAN), Some(3.0)];
    let c = layout(ChartShape::Line, &labels(3), &[s("a", Tone::Neutral, &v)], None);
    assert_eq!(c.series[0].dots.len(), 2);
    assert!(c.series[0].runs.is_empty());
}

#[test]
fn a_bar_hangs_from_the_baseline_and_a_negative_one_hangs_below_it() {
    let c = layout(ChartShape::Bar, &labels(2), &[s("a", Tone::Neutral, &some(&[10.0, -10.0]))], None);
    let (up, down) = (&c.series[0].bars[0], &c.series[0].bars[1]);
    assert!((up.y + up.h - c.baseline).abs() < 1e-3);
    assert!((down.y - c.baseline).abs() < 1e-3);
}

#[test]
fn a_zero_bar_is_one_pixel_tall_and_no_bar_is_thinner_than_one() {
    let c = layout(ChartShape::Bar, &labels(1), &[s("a", Tone::Neutral, &some(&[0.0]))], None);
    assert_eq!(c.series[0].bars[0].h, 1.0);
    let many: Vec<String> = labels(400);
    let c = layout(ChartShape::Bar, &many, &[s("a", Tone::Neutral, &some(&vec![1.0; 400]))], None);
    assert!(c.series[0].bars.iter().all(|b| b.w >= 1.0));
}

#[test]
fn series_share_a_band_side_by_side() {
    let two = [s("a", Tone::Neutral, &some(&[1.0])), s("b", Tone::Good, &some(&[1.0]))];
    let c = layout(ChartShape::Bar, &labels(1), &two, None);
    let (a, b) = (&c.series[0].bars[0], &c.series[1].bars[0]);
    assert!(b.x > a.x + a.w - 1.0 + 0.4 && b.x < a.x + a.w + 1.5);
}

#[test]
fn series_of_one_tone_differ_by_dash_and_fill_and_others_start_solid() {
    let data = [
        s("a", Tone::Neutral, &some(&[1.0])),
        s("b", Tone::Good, &some(&[1.0])),
        s("c", Tone::Neutral, &some(&[1.0])),
        s("d", Tone::Neutral, &some(&[1.0])),
        s("e", Tone::Neutral, &some(&[1.0])),
        s("f", Tone::Neutral, &some(&[1.0])),
    ];
    let c = layout(ChartShape::Line, &labels(1), &data, None);
    assert!(c.series[0].dash.is_empty() && c.series[1].dash.is_empty());
    assert_eq!(c.series[2].dash, [6.0, 3.0]);
    assert_eq!(c.series[3].dash, [2.0, 3.0]);
    assert_eq!(c.series[4].dash, [8.0, 3.0, 2.0, 3.0]);
    assert!(c.series[5].dash.is_empty(), "the fifth twin cycles back to solid");
    assert_eq!((c.series[0].fill, c.series[2].fill, c.series[3].fill, c.series[4].fill), (1.0, 0.6, 0.35, 0.8));
}

#[test]
fn an_area_is_filled_at_eighteen_percent_of_the_series_fill() {
    let data = [s("a", Tone::Neutral, &some(&[1.0, 2.0])), s("b", Tone::Neutral, &some(&[1.0, 2.0]))];
    let c = layout(ChartShape::Area, &labels(2), &data, None);
    assert!(c.series[0].area);
    assert!((c.series[0].area_fill() - 0.18).abs() < 1e-6);
    assert!((c.series[1].area_fill() - 0.108).abs() < 1e-6);
}

#[test]
fn a_mark_reads_its_label_value_and_unit() {
    let c = layout(ChartShape::Bar, &labels(2), &[s("a", Tone::Neutral, &some(&[10_600.0, 2.0]))], Some("tokens"));
    assert_eq!(c.series[0].bars[0].tip, "p0: 10,600 tokens");
    let c = layout(ChartShape::Line, &labels(2), &[s("a", Tone::Neutral, &some(&[10_600.0, 2.0]))], None);
    assert_eq!(c.series[0].dots[1].tip, "p1: 2");
}

#[test]
fn values_past_the_labels_are_not_drawn() {
    let c = layout(ChartShape::Line, &labels(2), &[s("a", Tone::Neutral, &some(&[1.0, 2.0, 3.0, 4.0]))], None);
    assert_eq!(c.series[0].dots.len(), 2);
}

#[test]
fn a_chart_with_no_series_still_has_its_grid() {
    let c = layout(ChartShape::Line, &labels(3), &[], None);
    assert!(c.series.is_empty());
    assert!(c.ticks.len() >= 2);
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        // CHART-5: every mark is inside the box, whatever the data.
        #[test]
        fn every_mark_is_inside_the_box(values in proptest::collection::vec(proptest::option::of(-1e6f64..1e6), 0..40), shape in 0u8..3) {
            let shape = [ChartShape::Line, ChartShape::Bar, ChartShape::Area][shape as usize];
            let c = layout(shape, &labels(values.len()), &[s("a", Tone::Neutral, &values)], None);
            for d in &c.series {
                for m in d.dots.iter().chain(&d.bars) {
                    prop_assert!(m.x >= 0.0 && m.x + m.w <= W + 1e-3, "x {}", m.x);
                    prop_assert!(m.y >= 0.0 - 1e-3 && m.y + m.h <= H + 1e-3, "y {}", m.y);
                }
            }
        }
    }
}
