use super::*;

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(|cell| (*cell).to_owned()).collect()
}

#[test]
fn the_titles_come_first_then_a_line_per_row_tab_separated() {
    assert_eq!(tsv(&row(&["File", "Lines"]), &[row(&["a.rs", "3"]), row(&["b.rs", "9"])]), "File\tLines\na.rs\t3\nb.rs\t9");
}

#[test]
fn a_tab_or_newline_inside_a_cell_becomes_one_space() {
    assert_eq!(tsv(&row(&["x"]), &[row(&["a\tb\r\nc"])]), "x\na b c");
}

#[test]
fn a_table_with_no_rows_is_its_titles() {
    assert_eq!(tsv(&row(&["a", "b"]), &[]), "a\tb");
}
