//! One screen cell on the wire, and the diff that turns a new grid into
//! the changed spans a client can apply to the one it already has.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// What an empty cell holds. A `const` rather than a literal at each use so
/// it costs nothing to make: `CompactString` stores anything this short
/// inline, so no blank cell anywhere in the pipeline touches the allocator.
pub const BLANK: CompactString = CompactString::const_new(" ");

/// What the child asked its cursor to look like, via DECSCUSR
/// (`CSI Ps SP q`). Carried separately from position because the shape is
/// a deliberate signal — a bar for insert mode, a block for normal mode —
/// and rendering every child's cursor as the host terminal's default block
/// throws that signal away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CursorShape {
    /// The child has not asked for anything; the host's own shape stands.
    #[default]
    Default,
    BlinkingBlock,
    SteadyBlock,
    BlinkingUnderline,
    SteadyUnderline,
    BlinkingBar,
    SteadyBar,
}

impl CursorShape {
    /// The DECSCUSR parameter, as written by the child. An absent parameter
    /// means the same as `0`: go back to the host's shape.
    pub fn from_decscusr(param: Option<u16>) -> Self {
        match param.unwrap_or(0) {
            1 => CursorShape::BlinkingBlock,
            2 => CursorShape::SteadyBlock,
            3 => CursorShape::BlinkingUnderline,
            4 => CursorShape::SteadyUnderline,
            5 => CursorShape::BlinkingBar,
            6 => CursorShape::SteadyBar,
            // 0 and anything unrecognised: leave the host's shape alone
            // rather than guessing at one the child did not ask for.
            _ => CursorShape::Default,
        }
    }
}

/// The child terminal's cursor, in grid-relative coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub row: u16,
    pub col: u16,
    pub visible: bool,
    /// Defaulted on the wire so a client and daemon of different vintages
    /// still agree about position and visibility.
    #[serde(default)]
    pub shape: CursorShape,
}

impl Default for Cursor {
    fn default() -> Self {
        Cursor {
            row: 0,
            col: 0,
            visible: true,
            shape: CursorShape::Default,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Color {
    #[default]
    Default,
    Idx(u8),
    Rgb(u8, u8, u8),
}

/// One character cell of terminal screen state, wire-sized to stay cheap to
/// diff and to ship: a damage span is a contiguous run of these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    /// The cell's grapheme — one character, plus any combining marks.
    ///
    /// `CompactString` rather than `String` because a grid is rebuilt,
    /// diffed, shipped and applied sixty times a second per pane, and a
    /// heap allocation per cell at each of those steps is most of what that
    /// costs. Anything up to 24 bytes lives inline, which every grapheme a
    /// terminal cell can hold does. It serializes as a plain string, so the
    /// wire format is unchanged.
    pub ch: CompactString,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: BLANK,
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            italic: false,
            underline: false,
            reverse: false,
        }
    }
}

/// A contiguous horizontal run of changed cells starting at (row, col).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellSpan {
    pub row: u16,
    pub col: u16,
    pub cells: Vec<Cell>,
}

/// Diff two equally-sized grids into the minimal set of changed spans.
pub fn diff_grid(prev: Option<&Vec<Vec<Cell>>>, cur: &[Vec<Cell>]) -> Vec<CellSpan> {
    let mut spans = Vec::new();
    for (row_idx, row) in cur.iter().enumerate() {
        let prev_row = prev.and_then(|p| p.get(row_idx));
        let mut col = 0usize;
        while col < row.len() {
            let changed = match prev_row {
                Some(pr) => pr.get(col) != Some(&row[col]),
                None => true,
            };
            if !changed {
                col += 1;
                continue;
            }
            let start = col;
            let mut cells = Vec::new();
            while col < row.len() {
                let still_changed = match prev_row {
                    Some(pr) => pr.get(col) != Some(&row[col]),
                    None => true,
                };
                if !still_changed {
                    break;
                }
                cells.push(row[col].clone());
                col += 1;
            }
            spans.push(CellSpan {
                row: row_idx as u16,
                col: start as u16,
                cells,
            });
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use compact_str::ToCompactString;

    fn row(s: &str) -> Vec<Cell> {
        s.chars()
            .map(|c| Cell {
                ch: c.to_compact_string(),
                ..Default::default()
            })
            .collect()
    }

    fn spans_of(prev: &[&str], cur: &[&str]) -> Vec<CellSpan> {
        let prev: Vec<Vec<Cell>> = prev.iter().map(|r| row(r)).collect();
        let cur: Vec<Vec<Cell>> = cur.iter().map(|r| row(r)).collect();
        diff_grid(Some(&prev), &cur)
    }

    fn text_of(span: &CellSpan) -> String {
        span.cells.iter().map(|c| c.ch.as_str()).collect()
    }

    #[test]
    fn no_change_produces_no_spans() {
        assert!(spans_of(&["abc", "def"], &["abc", "def"]).is_empty());
    }

    #[test]
    fn a_single_changed_cell_ships_only_that_cell() {
        let spans = spans_of(&["abc"], &["aXc"]);
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].row, spans[0].col), (0, 1));
        assert_eq!(text_of(&spans[0]), "X");
    }

    #[test]
    fn adjacent_changes_coalesce_into_one_span() {
        let spans = spans_of(&["abcde"], &["aXYZe"]);
        assert_eq!(spans.len(), 1, "one run, not three: {spans:?}");
        assert_eq!((spans[0].row, spans[0].col), (0, 1));
        assert_eq!(text_of(&spans[0]), "XYZ");
    }

    #[test]
    fn separated_changes_stay_separate_spans() {
        let spans = spans_of(&["abcde"], &["XbcdY"]);
        assert_eq!(spans.len(), 2);
        assert_eq!((spans[0].col, text_of(&spans[0])), (0, "X".to_string()));
        assert_eq!((spans[1].col, text_of(&spans[1])), (4, "Y".to_string()));
    }

    #[test]
    fn each_changed_row_gets_its_own_span_tagged_with_the_row() {
        let spans = spans_of(&["aa", "bb", "cc"], &["aa", "bX", "Yc"]);
        assert_eq!(spans.len(), 2);
        assert_eq!((spans[0].row, spans[0].col), (1, 1));
        assert_eq!((spans[1].row, spans[1].col), (2, 0));
    }

    #[test]
    fn no_previous_grid_ships_everything() {
        let cur: Vec<Vec<Cell>> = ["ab", "cd"].iter().map(|r| row(r)).collect();
        let spans = diff_grid(None, &cur);
        assert_eq!(spans.len(), 2, "one full-width span per row");
        assert_eq!(text_of(&spans[0]), "ab");
        assert_eq!(text_of(&spans[1]), "cd");
    }

    #[test]
    fn a_grown_grid_ships_the_newly_exposed_rows_whole() {
        // Resize grows the pane: rows the previous grid never had are
        // entirely new, so the whole row must ship.
        let prev: Vec<Vec<Cell>> = vec![row("ab")];
        let cur: Vec<Vec<Cell>> = vec![row("ab"), row("cd")];
        let spans = diff_grid(Some(&prev), &cur);
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].row, text_of(&spans[0])), (1, "cd".to_string()));
    }

    #[test]
    fn a_widened_row_ships_only_the_new_columns() {
        let prev: Vec<Vec<Cell>> = vec![row("ab")];
        let cur: Vec<Vec<Cell>> = vec![row("abcd")];
        let spans = diff_grid(Some(&prev), &cur);
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].col, text_of(&spans[0])), (2, "cd".to_string()));
    }

    #[test]
    fn attribute_only_changes_count_as_damage() {
        // Same character, different styling — the client would render the
        // wrong colour if this were treated as unchanged.
        let prev = vec![row("a")];
        let cur = vec![vec![Cell {
            ch: "a".into(),
            bold: true,
            ..Default::default()
        }]];
        let spans = diff_grid(Some(&prev), &cur);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].cells[0].bold);
    }

    #[test]
    fn a_cell_is_still_a_plain_string_on_the_wire() {
        // `ch` stores itself inline rather than on the heap, which is the
        // whole point of the type — but that has to stay an implementation
        // detail. A client and a daemon built from different commits have
        // to agree, so the encoding must be byte-for-byte what a `String`
        // produced.
        #[derive(Serialize)]
        struct AsString {
            ch: String,
            fg: Color,
            bg: Color,
            bold: bool,
            italic: bool,
            underline: bool,
            reverse: bool,
        }

        let compact = Cell {
            ch: "e\u{301}".into(),
            fg: Color::Idx(3),
            bold: true,
            ..Default::default()
        };
        let string = AsString {
            ch: "e\u{301}".to_string(),
            fg: Color::Idx(3),
            bg: Color::Default,
            bold: true,
            italic: false,
            underline: false,
            reverse: false,
        };

        assert_eq!(
            rmp_serde::to_vec_named(&compact).unwrap(),
            rmp_serde::to_vec_named(&string).unwrap()
        );
    }

    #[test]
    fn default_cell_is_a_blank_with_default_colors() {
        let c = Cell::default();
        assert_eq!(c.ch, " ");
        assert_eq!(c.fg, Color::Default);
        assert_eq!(c.bg, Color::Default);
    }
}

/// What the child asked for in the way of mouse reporting, from the xterm
/// private modes (`DECSET 9/1000/1002/1003` and the `1006` encoding).
///
/// It has to travel with the screen because forwarding mouse sequences to a
/// child that never asked for them is not a no-op: the child reads them as
/// typed text, and `[<65;40;12M` lands in whatever it was prompting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MouseMode {
    /// The child wants no mouse reports at all.
    #[default]
    None,
    /// X10: presses only.
    Press,
    /// VT200: presses and releases.
    PressRelease,
    /// Presses, releases, and motion while a button is held.
    ButtonMotion,
    /// All of the above plus motion with no button held.
    AnyMotion,
}

/// How the child wants those reports encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MouseEncoding {
    /// The original `CSI M Cb Cx Cy`, one byte per field, offset by 32.
    #[default]
    Default,
    /// The same, with the coordinate bytes written as UTF-8.
    Utf8,
    /// `CSI < Cb ; Px ; Py M/m`, which is the only one that survives past
    /// column 223 and can say which button was released.
    Sgr,
}

/// The child's mouse reporting as a whole: what to report, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MouseTracking {
    pub mode: MouseMode,
    pub encoding: MouseEncoding,
}

impl MouseTracking {
    /// Whether the child asked for mouse reports at all.
    pub fn enabled(&self) -> bool {
        self.mode != MouseMode::None
    }

    /// Whether a motion event with no button held should be reported.
    pub fn wants_bare_motion(&self) -> bool {
        self.mode == MouseMode::AnyMotion
    }

    /// Whether motion with a button held should be reported.
    pub fn wants_drag(&self) -> bool {
        matches!(self.mode, MouseMode::ButtonMotion | MouseMode::AnyMotion)
    }

    /// Whether a button release should be reported. X10 mode reports the
    /// press alone, and a stray release there is the same garbage as a
    /// report to a child that wanted none.
    pub fn wants_release(&self) -> bool {
        !matches!(self.mode, MouseMode::None | MouseMode::Press)
    }
}
