use super::*;

fn text(t: &str) -> Value {
    Value::Text { text: t.into() }
}

fn number(n: f64) -> Value {
    Value::Number { number: n, unit: None }
}

fn date(d: &str) -> Value {
    Value::Date { date: d.into() }
}

fn run(values: &[Value], direction: Direction) -> Vec<usize> {
    let rows: Vec<Vec<Value>> = values.iter().cloned().map(|v| vec![v]).collect();
    let refs: Vec<&[Value]> = rows.iter().map(Vec::as_slice).collect();
    order(&refs, 0, direction)
}

#[test]
fn numbers_sort_by_magnitude_not_by_their_digits() {
    let cells = [number(10.0), number(9.0), number(100.0)];
    assert_eq!(run(&cells, Direction::Ascending), [1, 0, 2]);
    assert_eq!(run(&cells, Direction::Descending), [2, 0, 1]);
}

#[test]
fn text_sorts_without_regard_to_case() {
    assert_eq!(run(&[text("b"), text("A"), text("c")], Direction::Ascending), [1, 0, 2]);
}

#[test]
fn rows_that_compare_equal_keep_the_order_they_were_drawn_in_either_way() {
    let same = [text("x"), text("X"), text("x")];
    assert_eq!(run(&same, Direction::Ascending), [0, 1, 2]);
    assert_eq!(run(&same, Direction::Descending), [0, 1, 2]);
}

#[test]
fn a_date_that_cannot_be_read_goes_last_going_up_and_first_coming_down_as_the_pages_does() {
    let dates = [date("2025-03-01"), date("not a date"), date("2024-01-01")];
    assert_eq!(run(&dates, Direction::Ascending), [2, 0, 1]);
    assert_eq!(run(&dates, Direction::Descending), [1, 0, 2]);
}

#[test]
fn times_sort_by_when_they_were_not_by_how_they_are_written() {
    let times = [Value::DateTime { at: "2025-01-01T10:00:00+02:00".into() }, Value::DateTime { at: "2025-01-01T09:00:00Z".into() }];
    // 10:00 at +02:00 is 08:00 UTC, before 09:00Z though it reads later.
    assert_eq!(run(&times, Direction::Ascending), [0, 1]);
}

#[test]
fn a_meter_sorts_by_how_far_along_it_is() {
    let meters = [Value::Progress { value: 3.0, max: 4.0, tone: Default::default() }, Value::Progress { value: 1.0, max: 100.0, tone: Default::default() }];
    assert_eq!(run(&meters, Direction::Ascending), [1, 0]);
}

#[test]
fn a_row_with_no_cell_in_the_column_sorts_as_empty_text() {
    let rows = [vec![text("b")], vec![]];
    let refs: Vec<&[Value]> = rows.iter().map(Vec::as_slice).collect();
    assert_eq!(order(&refs, 0, Direction::Ascending), [1, 0]);
}

#[test]
fn the_first_click_sorts_up_a_second_turns_it_down_and_another_column_starts_up() {
    assert_eq!(click(None, false, 2), (2, Direction::Ascending));
    assert_eq!(click(Some(2), true, 2), (2, Direction::Descending));
    assert_eq!(click(Some(2), false, 2), (2, Direction::Ascending));
    assert_eq!(click(Some(2), true, 1), (1, Direction::Ascending));
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        // A copy writes the order the person sees, so an order must be a permutation of the rows.
        #[test]
        fn an_order_is_a_permutation_of_the_rows(values in proptest::collection::vec(any::<i32>(), 0..40), down in any::<bool>()) {
            let cells: Vec<Value> = values.iter().map(|v| number(f64::from(*v))).collect();
            let direction = if down { Direction::Descending } else { Direction::Ascending };
            let mut at = run(&cells, direction);
            at.sort_unstable();
            prop_assert_eq!(at, (0..values.len()).collect::<Vec<_>>());
        }

        #[test]
        fn each_direction_leaves_the_keys_in_order(values in proptest::collection::vec(0u8..5, 0..30)) {
            let cells: Vec<Value> = values.iter().map(|v| number(f64::from(*v))).collect();
            let up: Vec<u8> = run(&cells, Direction::Ascending).iter().map(|&i| values[i]).collect();
            prop_assert!(up.windows(2).all(|w| w[0] <= w[1]));
            let down: Vec<u8> = run(&cells, Direction::Descending).iter().map(|&i| values[i]).collect();
            prop_assert!(down.windows(2).all(|w| w[0] >= w[1]));
        }
    }
}
