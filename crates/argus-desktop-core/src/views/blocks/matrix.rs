//! A matrix cell: how strongly it is tinted and what its tooltip says.

/// The share of the cell's tone mixed into the fill, 0.1 to 0.9. It rises from a faint wash to most
/// of the tone, so the lightest cell is still a cell and the darkest keeps its text readable. A
/// weight outside 0 to 1, or not a number, is held to it.
pub fn share(weight: f64) -> f32 {
    let weight = if weight.is_nan() { 0.0 } else { weight.clamp(0.0, 1.0) };
    ((weight * 80.0).round() + 10.0) as f32 / 100.0
}

/// `row · column: label`, or without the label when the cell has none.
pub fn title(row: &str, column: &str, label: Option<&str>) -> String {
    match label.filter(|label| !label.is_empty()) {
        Some(label) => format!("{row} \u{b7} {column}: {label}"),
        None => format!("{row} \u{b7} {column}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tint_runs_from_ten_to_ninety_percent() {
        assert_eq!(share(0.0), 0.10);
        assert_eq!(share(1.0), 0.90);
        assert_eq!(share(0.5), 0.50);
    }

    #[test]
    fn a_weight_outside_the_range_or_not_a_number_is_held_to_it() {
        assert_eq!(share(-3.0), 0.10);
        assert_eq!(share(7.0), 0.90);
        assert_eq!(share(f64::NAN), 0.10);
    }

    #[test]
    fn a_cell_without_a_label_is_titled_by_its_place_alone() {
        assert_eq!(title("Linux", "test", Some("1 flaky")), "Linux \u{b7} test: 1 flaky");
        assert_eq!(title("Linux", "test", None), "Linux \u{b7} test");
        assert_eq!(title("Linux", "test", Some("")), "Linux \u{b7} test");
    }
}
