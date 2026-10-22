use super::*;

fn text(t: &str) -> Value {
    Value::Text { text: t.into() }
}

#[test]
fn a_row_is_matched_by_the_text_of_any_of_its_cells_without_regard_to_case() {
    let row = row_text(&[text("Build"), Value::Number { number: 12.0, unit: Some("s".into()) }]);
    assert!(matches(&row, "BUI"));
    assert!(matches(&row, "12 s"));
    assert!(!matches(&row, "test"));
}

#[test]
fn what_is_typed_is_trimmed_and_nothing_typed_lets_every_row_through() {
    let row = row_text(&[text("alpha")]);
    assert!(matches(&row, "  alpha "));
    assert!(matches(&row, ""));
    assert!(matches(&row, "   "));
}

#[test]
fn cells_are_not_run_together_so_a_match_cannot_span_two_of_them() {
    let row = row_text(&[text("ab"), text("cd")]);
    assert!(!matches(&row, "bc"));
}

#[test]
fn the_count_is_said_only_while_filtering() {
    assert_eq!(count_line(3, 40, "x"), "3 of 40 rows");
    assert_eq!(count_line(40, 40, ""), "");
    assert_eq!(count_line(40, 40, "  "), "");
}

#[test]
fn a_cut_table_says_how_many_it_was() {
    assert_eq!(showing(40, Some(212)).as_deref(), Some("Showing 40 of 212 rows"));
    assert_eq!(showing(40, Some(40)), None);
    assert_eq!(showing(40, None), None);
}
