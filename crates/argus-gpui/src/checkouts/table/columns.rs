//! How wide the table's three content columns are, as the page's table draws them.
//!
//! The page lays its table out with the browser's automatic algorithm, in which the Last commit
//! column is `width: 100%` and the others take what their nowrap content needs: the Checkout column
//! is as wide as its longest name with its tags, the State column as its longest line of words, the
//! actions column as `.row-actions` makes it. GPUI lays rows out one at a time, so a column is given
//! one width for every row, and the text is measured with the window's own text system, shaped in
//! the faces the cells are set in, so a column is as wide as the page's and Last commit gets as
//! much room. The Last commit column takes the rest, and gives way (`ck_last_hides_below`) when
//! there is no room for it.

use argus_desktop_core::checkouts::says::{branch_beside, branch_cell, panes, state_of, Say};
use argus_desktop_core::checkouts::table::CheckoutsRow;
use gpui_kit::{font, px, FontWeight, SharedString, TextRun, Window};

use crate::theme::Tokens;

/// How wide `text` is in the page's pixels, set in the mono face or the interface face at `size`
/// and `weight`.
pub type Measure<'a> = &'a dyn Fn(&str, bool, f32, f32) -> f32;

/// The columns' widths in the page's pixels, cell padding included.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Widths {
    pub name: f32,
    pub state: f32,
    pub actions: f32,
}

/// A measure that shapes the text with `window`'s text system. A size is the page's pixels, so the
/// width is too whatever the zoom is.
pub fn shaped<'a>(window: &'a Window, tokens: &'a Tokens) -> impl Fn(&str, bool, f32, f32) -> f32 + 'a {
    move |text, mono, size, weight| {
        if text.is_empty() {
            return 0.0;
        }
        let mut face = font(SharedString::from(if mono { tokens.font_mono } else { tokens.font_ui }));
        face.weight = FontWeight(weight);
        let run = TextRun { len: text.len(), font: face, color: gpui_kit::transparent_black(), background_color: None, underline: None, strikethrough: None };
        let line = window.text_system().shape_line(SharedString::from(text.to_owned()), px(size), &[run], None);
        f32::from(line.width())
    }
}

/// What a Checkout cell takes: the name in semibold mono, the `primary` tag, the branch beside it.
fn name_of(row: &CheckoutsRow, tokens: &Tokens, m: Measure) -> f32 {
    let Some(name) = &row.name else {
        // A branch row: the icon, then the name in mono.
        return tokens.icon_sm + tokens.sp_6 + m(&row.branch, true, tokens.fs_sm, 400.0);
    };
    let small = tokens.type_small;
    let tag = if row.primary { tokens.sp_6 + m("primary", false, small.size, small.weight) + 2.0 * tokens.sp_8 } else { 0.0 };
    let beside = branch_beside(row).map_or(0.0, |branch| tokens.sp_8 + m(branch, true, tokens.fs_xs, 400.0));
    m(name, true, tokens.fs_sm, tokens.weight_semibold) + tag + beside
}

/// What a State cell takes: each thing it says, with 8px between them.
fn state_width(row: &CheckoutsRow, tokens: &Tokens, m: Measure) -> f32 {
    let words = |text: &str| m(text, false, tokens.fs_sm, 400.0);
    let mono = |text: &str| m(text, true, tokens.fs_sm, 400.0);
    let says: Vec<f32> = if row.name.is_some() {
        state_of(row)
            .iter()
            .map(|say| match say {
                Say::Detached => mono("detached"),
                Say::Changed(text) => words(text),
                Say::Distance(text) => mono(text),
                Say::Panes(n) => *n as f32 * (tokens.status_box + tokens.sp_6) + words(&panes(*n)),
                Say::Feature { number, .. } => mono(&format!("#{number}")),
            })
            .collect()
    } else {
        let cell = branch_cell(row);
        [
            cell.git.map(|git| mono(&git)),
            cell.note.map(|note| m(note, false, tokens.type_small.size, tokens.type_small.weight)),
            cell.feature.map(|(number, _)| mono(&format!("#{number}"))),
        ]
        .into_iter()
        .flatten()
        .collect()
    };
    let gaps = says.len().saturating_sub(1) as f32 * tokens.sp_8;
    says.iter().sum::<f32>() + gaps
}

/// The widths for `rows`, each at least what its header says.
pub fn widths(rows: &[&CheckoutsRow], tokens: &Tokens, m: Measure) -> Widths {
    let pad = 2.0 * tokens.sp_12;
    let label = tokens.type_label;
    let most = |cell: &dyn Fn(&CheckoutsRow) -> f32, head: &str| {
        // The header's letters are spaced 0.07em apart.
        let head = m(head, false, label.size, label.weight) + head.chars().count() as f32 * 0.07 * label.size;
        rows.iter().map(|row| cell(row)).fold(head, f32::max) + pad
    };
    Widths {
        name: most(&|row| name_of(row, tokens, m), "CHECKOUT").min(tokens.ck_name_w),
        state: most(&|row| state_width(row, tokens, m), "STATE"),
        // `.row-actions` asks for 184px, but the auto layout gives the column to Last commit down to
        // what the buttons need: the wider of the two words in semibold, their box and edge, and
        // the overflow beside them.
        actions: {
            let words = rows.iter().map(|row| if row.checkout.is_some() { "Open" } else { "Check out" }).map(|words| m(words, false, tokens.fs_sm, tokens.weight_semibold)).fold(0.0, f32::max);
            words + 2.0 * (tokens.sp_8 + tokens.hairline) + tokens.sp_4 + tokens.icon_btn + pad
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkouts::stage::tests::{branch as branch_row, checkout};
    use crate::theme::themes::ledger;

    fn row() -> CheckoutsRow {
        checkout(1, "desktop", "desktop")
    }

    /// A stand-in for the text system: a character is a fixed share of its size.
    fn average(text: &str, mono: bool, size: f32, _: f32) -> f32 {
        text.chars().count() as f32 * size * if mono { 0.6 } else { 0.55 }
    }

    #[test]
    fn a_column_is_as_wide_as_its_longest_cell_and_the_name_is_capped_at_the_pages_width() {
        let tokens = ledger::tokens();
        let short = CheckoutsRow { name: Some("a".into()), branch: "a".into(), ..row() };
        let long = CheckoutsRow { name: Some("desktop-design".into()), branch: "desktop-design".into(), ..row() };
        let (a, b) = (widths(&[&short], &tokens, &average), widths(&[&short, &long], &tokens, &average));
        assert!(b.name > a.name, "the longer name widens the column");
        let huge = CheckoutsRow { name: Some("x".repeat(200)), branch: "x".repeat(200), ..row() };
        assert_eq!(widths(&[&huge], &tokens, &average).name, tokens.ck_name_w);
    }

    #[test]
    fn a_state_that_says_more_takes_more() {
        let tokens = ledger::tokens();
        let quiet = widths(&[&row()], &tokens, &average).state;
        let busy = CheckoutsRow { changed: 6, ahead: 3, feature: Some(42), ..row() };
        assert!(widths(&[&busy], &tokens, &average).state > quiet);
        let branch = CheckoutsRow { note: Some("remote only"), ..branch_row("old") };
        assert!(widths(&[&branch], &tokens, &average).state > quiet, "a branch's note is said");
    }

    #[test]
    fn the_columns_are_what_the_measure_says_so_a_wider_face_widens_them() {
        let tokens = ledger::tokens();
        let wide = |text: &str, mono: bool, size: f32, weight: f32| average(text, mono, size, weight) * 1.5;
        let busy = CheckoutsRow { changed: 6, ahead: 3, feature: Some(42), ..row() };
        let (a, b) = (widths(&[&busy], &tokens, &average), widths(&[&busy], &tokens, &wide));
        assert!(b.state > a.state && b.name > a.name);
    }

    #[test]
    fn the_actions_column_is_its_buttons_wide_and_a_branch_row_widens_it() {
        let tokens = ledger::tokens();
        let (open, both) = (widths(&[&row()], &tokens, &average).actions, widths(&[&row(), &branch_row("old")], &tokens, &average).actions);
        assert!(open < 184.0 && both > open, "Check out is the longer word");
    }
}
