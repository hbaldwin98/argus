//! Turning `vt100`'s view of a screen into the protocol's, and back the
//! other way for a paste.
//!
//! Everything here is a pure translation between the two representations,
//! kept apart from the pane runtime so that what a cell, a cursor, or a
//! mouse mode *is* on the wire can be read in one place.

use super::*;

/// Split out from [`PaneRuntime::scrollback`] so the offset arithmetic can
/// be driven by a parser a test fed directly, with no child to spawn.
pub(super) fn read_scrollback(parser: &mut vt100::Parser, offset: usize) -> (Vec<Vec<Cell>>, usize, usize) {
    // vt100 clamps to what it actually retained and exposes no count of its
    // own — `scrollback_len` is the configured cap, not the fill level.
    // Asking for more than exists and reading back what stuck is the only
    // honest way to measure the depth.
    parser.screen_mut().set_scrollback(usize::MAX);
    let depth = parser.screen().scrollback();
    parser.screen_mut().set_scrollback(offset);
    let offset = parser.screen().scrollback();
    let cells = snapshot_grid(parser);
    parser.screen_mut().set_scrollback(0);
    (cells, offset, depth)
}

pub(super) fn snapshot_grid(parser: &vt100::Parser) -> Vec<Vec<Cell>> {
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    let mut grid = Vec::with_capacity(rows as usize);
    for r in 0..rows {
        let mut row = Vec::with_capacity(cols as usize);
        for c in 0..cols {
            row.push(cell_from_vt100(screen.cell(r, c)));
        }
        grid.push(row);
    }
    grid
}

pub(super) fn snapshot_cursor(parser: &vt100::Parser, shape: CursorShape) -> Cursor {
    let screen = parser.screen();
    let (row, col) = screen.cursor_position();
    Cursor {
        row,
        col,
        visible: !screen.hide_cursor(),
        shape,
    }
}

pub(super) fn snapshot_mouse(parser: &vt100::Parser) -> MouseTracking {
    let screen = parser.screen();
    MouseTracking {
        mode: match screen.mouse_protocol_mode() {
            vt100::MouseProtocolMode::None => MouseMode::None,
            vt100::MouseProtocolMode::Press => MouseMode::Press,
            vt100::MouseProtocolMode::PressRelease => MouseMode::PressRelease,
            vt100::MouseProtocolMode::ButtonMotion => MouseMode::ButtonMotion,
            vt100::MouseProtocolMode::AnyMotion => MouseMode::AnyMotion,
        },
        encoding: match screen.mouse_protocol_encoding() {
            vt100::MouseProtocolEncoding::Default => MouseEncoding::Default,
            vt100::MouseProtocolEncoding::Utf8 => MouseEncoding::Utf8,
            vt100::MouseProtocolEncoding::Sgr => MouseEncoding::Sgr,
        },
    }
}

/// Picks DECSCUSR (`CSI Ps SP q`) out of the child's output stream.
///
/// `vt100` does not model the cursor's shape at all, so it is not in the
/// screen state the rest of the pipeline is built from — but a child that
/// asks for a bar means it, and dropping the request leaves every pane
/// wearing the host terminal's block. The sequence is left in the stream
/// for the parser to ignore as it already does; this only watches it go by.
///
/// Resumable across reads: a `read` boundary lands wherever the kernel put
/// it, so a sequence is as likely to be split as not.
#[derive(Default)]
pub(super) struct CursorShapeScanner {
    shape: CursorShape,
    state: ScanState,
}

#[derive(Default, Clone, Copy)]
pub(super) enum ScanState {
    #[default]
    Ground,
    /// Saw ESC.
    Escape,
    /// Inside `CSI`, collecting the numeric parameter.
    Params(Option<u16>),
    /// Saw the intermediate space that makes this DECSCUSR and not some
    /// other CSI ending in a letter.
    Intermediate(Option<u16>),
}

impl CursorShapeScanner {
    pub(super) fn shape(&self) -> CursorShape {
        self.shape
    }

    pub(super) fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state = match (self.state, b) {
                // ESC restarts the machine from anywhere: a truncated
                // sequence must not swallow the one that follows it.
                (_, 0x1b) => ScanState::Escape,
                (ScanState::Escape, b'[') => ScanState::Params(None),
                (ScanState::Params(n), b'0'..=b'9') => ScanState::Params(Some(
                    n.unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(u16::from(b - b'0')),
                )),
                (ScanState::Params(n), b' ') => ScanState::Intermediate(n),
                (ScanState::Intermediate(n), b'q') => {
                    self.shape = CursorShape::from_decscusr(n);
                    ScanState::Ground
                }
                _ => ScanState::Ground,
            };
        }
    }
}

pub(super) fn cell_from_vt100(cell: Option<&vt100::Cell>) -> Cell {
    match cell {
        None => Cell::default(),
        Some(c) => Cell {
            ch: contents_of(c),
            fg: convert_color(c.fgcolor()),
            bg: convert_color(c.bgcolor()),
            bold: c.bold(),
            italic: c.italic(),
            underline: c.underline(),
            reverse: c.inverse(),
        },
    }
}

/// The cell's grapheme, or a blank — which still carries the cell's own
/// colours, so it is a styled space rather than a default cell.
///
/// `vt100::Cell::contents` heap-allocates a `String` on every call,
/// including for a cell that has nothing in it. Most of a screen is blank
/// and every cell is rebuilt on every frame, so asking `has_contents`
/// first is the difference between two allocations per blank cell and
/// none.
pub(super) fn contents_of(c: &vt100::Cell) -> CompactString {
    if !c.has_contents() {
        return BLANK;
    }
    let contents = c.contents();
    if contents.is_empty() {
        // A wide character's continuation cell reports contents it does not
        // have; the screen still needs a blank drawn there.
        return BLANK;
    }
    contents.into()
}

pub(super) fn convert_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Default,
        vt100::Color::Idx(i) => Color::Idx(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

pub(super) fn paste_bytes(bytes: &[u8], bracketed: bool) -> Vec<u8> {
    if !bracketed {
        return bytes.to_vec();
    }
    let mut pasted = Vec::with_capacity(PASTE_START.len() + bytes.len() + PASTE_END.len());
    pasted.extend_from_slice(PASTE_START);
    pasted.extend_from_slice(bytes);
    pasted.extend_from_slice(PASTE_END);
    pasted
}
