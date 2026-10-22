//! A table written as tab-separated text, which a spreadsheet pastes as a grid.

/// A tab or a newline inside a cell would break the grid, so each run of them becomes one space.
fn line(cells: &[String]) -> String {
    cells.iter().map(|cell| squeeze(cell)).collect::<Vec<_>>().join("\t")
}

fn squeeze(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    let mut breaking = false;
    for c in cell.chars() {
        if matches!(c, '\t' | '\r' | '\n') {
            if !breaking {
                out.push(' ');
            }
            breaking = true;
        } else {
            out.push(c);
            breaking = false;
        }
    }
    out
}

/// The titles on a first line and a line per row, in the order given.
pub fn tsv(titles: &[String], rows: &[Vec<String>]) -> String {
    std::iter::once(line(titles)).chain(rows.iter().map(|row| line(row))).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests;
