//! How the rows a screen holds become what is painted: runs of same-styled
//! text, and rectangles of background colour.
//!
//! Backgrounds are quads, never part of a run, so a coloured cell does not
//! split the text over it; they merge along a row where they touch and down
//! the rows where a rectangle repeats exactly, so a diff's coloured block is
//! one quad. Only the rows and columns that fit the room are laid out: the
//! daemon's grid may be larger than the element until the pane is resized.

use argus_desktop_core::connection::page::{Segment, BOLD, ITALIC, REVERSE, UNDERLINE, WIDE};

use crate::panes::palette::{readable, Palette, Rgb};

/// What makes two stretches of text one shaped line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStyle {
    pub fg: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// Contiguous cells of one style, shaped and painted as one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBatch {
    pub row: u16,
    pub col: u16,
    pub cols: u16,
    pub text: String,
    pub style: TextStyle,
    /// A wide cell is its own batch, since its glyph is forced two cells wide
    /// and so sits centred over both.
    pub wide: bool,
}

/// A rectangle of cells in one background colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fill {
    pub row: u16,
    pub col: u16,
    pub cols: u16,
    pub rows: u16,
    pub color: Rgb,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub batches: Vec<TextBatch>,
    pub fills: Vec<Fill>,
}

/// A segment's text and background after the flags and the contrast floor.
pub fn colours(segment: &Segment, palette: &Palette) -> (Rgb, Option<Rgb>) {
    let mut fore = palette.resolve(segment.2).unwrap_or(palette.fg);
    let mut back = palette.resolve(segment.3);
    if segment.4 & REVERSE != 0 {
        (fore, back) = (back.unwrap_or(palette.bg), Some(fore));
    }
    if let Some(back) = back {
        fore = readable(fore, back);
    }
    (fore, back)
}

/// The cells of `segment` that fit `cols` columns, or none: a wide cell
/// with only one column left is not drawn half.
fn fitted(segment: &Segment, cols: u16) -> Option<(u16, Vec<&str>)> {
    if segment.0 >= cols {
        return None;
    }
    let room = usize::from(cols - segment.0);
    if segment.4 & WIDE != 0 {
        return (room >= 2).then(|| (2, segment.1.iter().take(1).map(String::as_str).collect()));
    }
    let cells: Vec<&str> = segment.1.iter().take(room).map(String::as_str).collect();
    let count = cells.len() as u16;
    (count > 0).then_some((count, cells))
}

/// Runs and background rectangles for the first `fit.0` rows and `fit.1`
/// columns of `lines`.
pub fn layout(lines: &[Vec<Segment>], palette: &Palette, fit: (u16, u16)) -> Layout {
    let mut out = Layout::default();
    // The fills still open: those of the row above, which the next row may extend.
    let mut open: Vec<usize> = Vec::new();
    for (row, segments) in lines.iter().enumerate().take(usize::from(fit.0)) {
        let row = row as u16;
        let mut current: Option<TextBatch> = None;
        let mut row_fills: Vec<Fill> = Vec::new();
        for segment in segments {
            let Some((cols, cells)) = fitted(segment, fit.1) else {
                continue;
            };
            let (fore, back) = colours(segment, palette);
            if let Some(color) = back {
                match row_fills.last_mut() {
                    Some(last) if last.col + last.cols == segment.0 && last.color == color => last.cols += cols,
                    _ => row_fills.push(Fill { row, col: segment.0, cols, rows: 1, color }),
                }
            }
            let style = TextStyle {
                fg: fore,
                bold: segment.4 & BOLD != 0,
                italic: segment.4 & ITALIC != 0,
                underline: segment.4 & UNDERLINE != 0,
            };
            let wide = segment.4 & WIDE != 0;
            let mut text = String::new();
            for cell in cells {
                text.push_str(if cell.is_empty() { " " } else { cell });
            }
            match current.as_mut() {
                Some(batch)
                    if !wide && !batch.wide && batch.style == style && batch.col + batch.cols == segment.0 =>
                {
                    batch.text.push_str(&text);
                    batch.cols += cols;
                }
                _ => {
                    out.batches.extend(current.take().filter(paints));
                    current = Some(TextBatch { row, col: segment.0, cols, text, style, wide });
                }
            }
        }
        out.batches.extend(current.take().filter(paints));

        let mut next_open = Vec::new();
        for fill in row_fills {
            let above = open.iter().copied().find(|&i| {
                let f = &out.fills[i];
                f.col == fill.col && f.cols == fill.cols && f.color == fill.color && f.row + f.rows == row
            });
            match above {
                Some(i) => {
                    out.fills[i].rows += 1;
                    next_open.push(i);
                }
                None => {
                    out.fills.push(fill);
                    next_open.push(out.fills.len() - 1);
                }
            }
        }
        open = next_open;
    }
    out
}

/// A run of blanks with no underline paints nothing; shaping it is waste.
fn paints(batch: &TextBatch) -> bool {
    batch.style.underline || batch.text.chars().any(|c| c != ' ')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ledger;
    use crate::panes::palette::contrast;
    use crate::panes::palette::MINIMUM_CONTRAST;
    use crate::panes::screen::width;

    const FIT: (u16, u16) = (100, 100);

    const fn ansi(i: u32) -> u32 {
        1 << 24 | i
    }

    fn seg(col: u16, text: &str, fg: u32, bg: u32, flags: u8) -> Segment {
        Segment(col, text.chars().map(|c| c.to_string()).collect(), fg, bg, flags)
    }

    fn wide(col: u16, glyph: &str, bg: u32) -> Segment {
        Segment(col, vec![glyph.into()], 0, bg, WIDE)
    }

    fn laid(rows: Vec<Vec<Segment>>) -> Layout {
        layout(&rows, &Palette::of(&ledger::tokens()), FIT)
    }

    #[test]
    fn touching_segments_of_one_style_are_one_run() {
        let out = laid(vec![vec![seg(0, "ab", 0, 0, 0), seg(2, "cd", 0, 0, 0)]]);
        assert_eq!(out.batches.len(), 1);
        assert_eq!(out.batches[0].text, "abcd");
        assert_eq!(out.batches[0].cols, 4);
    }

    #[test]
    fn a_change_of_colour_or_weight_starts_a_new_run() {
        let out = laid(vec![vec![
            seg(0, "ab", 0, 0, 0),
            seg(2, "cd", ansi(1), 0, 0),
            seg(4, "ef", ansi(1), 0, BOLD),
        ]]);
        let texts: Vec<_> = out.batches.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts, ["ab", "cd", "ef"]);
        assert!(out.batches[2].style.bold);
    }

    #[test]
    fn a_gap_between_segments_splits_a_run() {
        let out = laid(vec![vec![seg(0, "ab", 0, 0, 0), seg(5, "cd", 0, 0, 0)]]);
        assert_eq!(out.batches.len(), 2);
        assert_eq!(out.batches[1].col, 5);
    }

    #[test]
    fn a_background_does_not_split_a_run_of_one_text_colour() {
        let out = laid(vec![vec![seg(0, "ab", ansi(0), 0, 0), seg(2, "cd", ansi(0), ansi(4), 0)]]);
        assert_eq!(out.batches.len(), 1);
        assert_eq!(out.fills.len(), 1);
    }

    #[test]
    fn blank_runs_paint_nothing_unless_underlined() {
        let out = laid(vec![vec![seg(0, "   ", 0, 0, 0), seg(5, "  ", 0, 0, UNDERLINE)]]);
        assert_eq!(out.batches.len(), 1);
        assert_eq!(out.batches[0].col, 5);
    }

    #[test]
    fn a_wide_cell_takes_two_columns_and_the_next_segment_follows_it() {
        let out = laid(vec![vec![seg(0, "a", 0, 0, 0), wide(1, "日", 0), seg(3, "b", 0, 0, 0)]]);
        let cols: Vec<_> = out.batches.iter().map(|b| (b.col, b.cols, b.wide)).collect();
        assert_eq!(cols, [(0, 1, false), (1, 2, true), (3, 1, false)]);
    }

    #[test]
    fn a_wide_cell_is_never_merged_into_its_neighbours() {
        let out = laid(vec![vec![seg(0, "ab", 0, 0, 0), wide(2, "日", 0), wide(4, "本", 0), seg(6, "cd", 0, 0, 0)]]);
        let cols: Vec<_> = out.batches.iter().map(|b| (b.col, b.cols, b.wide)).collect();
        assert_eq!(cols, [(0, 2, false), (2, 2, true), (4, 2, true), (6, 2, false)]);
    }

    #[test]
    fn a_wide_cell_background_covers_both_columns() {
        let out = laid(vec![vec![wide(0, "日", ansi(1))]]);
        assert_eq!(out.fills, [Fill { row: 0, col: 0, cols: 2, rows: 1, color: Palette::of(&ledger::tokens()).ansi[1] }]);
    }

    #[test]
    fn touching_backgrounds_of_one_colour_merge_along_a_row() {
        let out = laid(vec![vec![
            seg(0, "ab", 0, ansi(4), 0),
            seg(2, "cd", ansi(2), ansi(4), BOLD),
            seg(4, "ef", 0, ansi(1), 0),
            seg(8, "gh", 0, ansi(1), 0),
        ]]);
        let spans: Vec<_> = out.fills.iter().map(|f| (f.col, f.cols)).collect();
        assert_eq!(spans, [(0, 4), (4, 2), (8, 2)]);
    }

    #[test]
    fn identical_rectangles_on_consecutive_rows_merge_down() {
        let block = |text| vec![seg(2, text, 0, ansi(1), 0)];
        let out = laid(vec![block("xxxx"), block("yyyy"), block("zzzz"), block("xxx")]);
        let rects: Vec<_> = out.fills.iter().map(|f| (f.row, f.cols, f.rows)).collect();
        assert_eq!(rects, [(0, 4, 3), (3, 3, 1)]);
    }

    #[test]
    fn default_background_cells_make_no_quad() {
        assert!(laid(vec![vec![seg(0, "abc", ansi(2), 0, 0)]]).fills.is_empty());
    }

    #[test]
    fn reverse_swaps_text_and_background_and_defaults_to_the_theme() {
        let palette = Palette::of(&ledger::tokens());
        let out = laid(vec![vec![seg(0, "ab", 0, 0, REVERSE)]]);
        assert_eq!(out.fills[0].color, palette.fg);
        assert_eq!(out.batches[0].style.fg, palette.bg);
    }

    #[test]
    fn text_the_background_would_swallow_is_moved_to_the_contrast_floor() {
        let palette = Palette::of(&ledger::tokens());
        let (fore, back) = colours(&seg(0, "x", ansi(0), ansi(8), 0), &palette);
        assert!(contrast(fore, back.expect("a background was set")) >= MINIMUM_CONTRAST);
        assert_ne!(fore, palette.ansi[0]);
        let (fore, _) = colours(&seg(0, "x", ansi(15), ansi(0), 0), &palette);
        assert_eq!(fore, palette.ansi[15], "readable text is left alone");
    }

    #[test]
    fn only_the_rows_that_fit_are_laid_out() {
        let rows = vec![
            vec![seg(0, "a", 0, ansi(1), 0)],
            vec![seg(0, "b", 0, ansi(1), 0)],
            vec![seg(0, "c", 0, ansi(1), 0)],
        ];
        let out = layout(&rows, &Palette::of(&ledger::tokens()), (2, 10));
        let drawn: Vec<_> = out.batches.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(drawn, ["a", "b"]);
        assert_eq!(out.fills.iter().map(|f| f.rows).sum::<u16>(), 2);
    }

    #[test]
    fn only_the_columns_that_fit_are_laid_out() {
        let rows = vec![vec![seg(0, "abcdef", 0, ansi(1), 0), seg(8, "zz", 0, 0, 0)]];
        let out = layout(&rows, &Palette::of(&ledger::tokens()), (1, 4));
        assert_eq!(out.batches.len(), 1);
        assert_eq!(out.batches[0].text, "abcd");
        assert_eq!(out.fills[0].cols, 4);
    }

    #[test]
    fn a_wide_cell_that_would_overhang_the_edge_is_not_drawn_half() {
        let rows = vec![vec![seg(0, "ab", 0, 0, 0), wide(2, "日", 0)]];
        let out = layout(&rows, &Palette::of(&ledger::tokens()), (1, 3));
        assert_eq!(out.batches.len(), 1);
        assert_eq!(out.batches[0].text, "ab");
        let out = layout(&rows, &Palette::of(&ledger::tokens()), (1, 4));
        assert_eq!(out.batches.len(), 2);
    }

    #[test]
    fn merging_loses_no_cell_of_background() {
        let rows: Vec<Vec<Segment>> = (0..6)
            .map(|r| vec![seg(0, "xxxx", 0, ansi(1), 0), seg(6, "yy", 0, ansi(2 + r % 2), 0)])
            .collect();
        let out = laid(rows.clone());
        let held: u32 = rows.iter().flatten().filter(|s| s.3 != 0).map(|s| u32::from(width(s))).sum();
        let merged: u32 = out.fills.iter().map(|f| u32::from(f.cols * f.rows)).sum();
        assert_eq!(merged, held);
    }
}
