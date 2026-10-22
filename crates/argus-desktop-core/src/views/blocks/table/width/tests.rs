use super::*;

fn text(t: &str) -> Value {
    Value::Text { text: t.into() }
}

#[test]
fn a_text_cell_is_as_wide_as_its_characters() {
    assert_eq!(estimate(&text("abcd")), 4.0 * CHAR_W);
}

#[test]
fn a_long_text_cell_is_cut_at_280_and_a_short_one_never_is() {
    assert_eq!(estimate(&text(&"x".repeat(100))), LONG_MAX);
    assert!(estimate(&text(&"x".repeat(LONG))) < LONG_MAX, "a cell of exactly the long length is not yet cut");
}

#[test]
fn a_column_is_as_wide_as_its_widest_cell_or_its_header_and_has_its_padding() {
    let rows: Vec<Vec<Value>> = vec![vec![text("a"), text("a long enough cell")], vec![text("abcdef"), text("b")]];
    let refs: Vec<&[Value]> = rows.iter().map(Vec::as_slice).collect();
    let w = columns(&["T", "Title"], &refs);
    assert_eq!(w[0], header("T").max(6.0 * CHAR_W) + CELL_PAD);
    assert_eq!(w[1], (18.0 * CHAR_W).max(header("Title")) + CELL_PAD);
}

#[test]
fn a_row_with_fewer_cells_than_columns_leaves_the_rest_to_the_others() {
    let rows: Vec<Vec<Value>> = vec![vec![text("a")], vec![text("b"), text("longer")]];
    let refs: Vec<&[Value]> = rows.iter().map(Vec::as_slice).collect();
    assert_eq!(columns(&["x", "y"], &refs)[1], (6.0 * CHAR_W).max(header("y")) + CELL_PAD);
}

#[test]
fn a_header_is_wide_enough_for_its_capitals_and_the_arrow() {
    assert!(columns(&["A very long header"], &[])[0] >= 18.0 * CAP_W);
}

#[test]
fn a_meter_a_chip_and_a_person_are_wider_than_nothing() {
    for value in [
        Value::Progress { value: 1.0, max: 2.0, tone: Default::default() },
        Value::Chip { chip: argus_protocol::Chip { label: "ok".into(), tone: Default::default() } },
        Value::Person { name: "Ada".into() },
    ] {
        assert!(estimate(&value) > 20.0, "{value:?}");
    }
    assert_eq!(estimate(&Value::Empty), 0.0);
}
