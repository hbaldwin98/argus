//! Turning `vt100`'s view of a screen into the protocol's, and back the
//! other way for a paste.
//!
//! Everything here is a pure translation between the two representations,
//! kept apart from the pane runtime so that what a cell, a cursor, or a
//! mouse mode *is* on the wire can be read in one place.

use base64::Engine;

use super::*;

/// A pane's terminal emulator, with the hooks for what a child asks of it.
pub(super) type Vt = vt100::Parser<ChildRequests>;

pub(super) fn new_vt(rows: u16, cols: u16, scrollback: usize) -> Vt {
    vt100::Parser::new_with_callbacks(rows, cols, scrollback, ChildRequests::default())
}

/// What a child asked of its terminal beyond drawing, held until the pump
/// carries it out: text to copy, and the answers owed to its queries.
///
/// The pane's terminal is not the user's, and `vt100` parses both and drops
/// them. Without the copies an agent's own copy command reports success
/// while nothing reaches the clipboard. Clipboard reads (`OSC 52 ; ?`) stay
/// unanswered, since answering would hand the user's clipboard to whatever
/// runs in a pane.
#[derive(Default)]
pub(super) struct ChildRequests {
    copied: Vec<String>,
    replies: Vec<u8>,
}

impl ChildRequests {
    pub(super) fn take_copies(&mut self) -> Vec<String> {
        std::mem::take(&mut self.copied)
    }

    /// Bytes to write back to the child, in the order it asked.
    pub(super) fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies)
    }
}

impl vt100::Callbacks for ChildRequests {
    fn copy_to_clipboard(&mut self, _: &mut vt100::Screen, _selection: &[u8], data: &[u8]) {
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
            return;
        };
        self.copied.push(String::from_utf8_lossy(&bytes).into_owned());
    }

    /// Answers a cursor position request (`CSI 6 n`) with where the cursor
    /// is as it is asked — the parser calls this mid-stream, before the
    /// rest of the read that carried it moves the cursor on.
    ///
    /// A child that asks waits for the answer. ConPTY opened to inherit the
    /// cursor, as portable-pty 0.9 opens it, starts no child until it has
    /// one, and a line editor that asks at every prompt otherwise stalls
    /// there until its own timeout.
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        if c != 'n' || i1.is_some() || i2.is_some() || params != [&[6][..]] {
            return;
        }
        let (row, col) = screen.cursor_position();
        let reply = format!("\x1b[{};{}R", row + 1, col + 1);
        self.replies.extend_from_slice(reply.as_bytes());
    }
}

/// Split out from [`PaneRuntime::scrollback`] so the offset arithmetic can
/// be driven by a parser a test fed directly, with no child to spawn.
pub(super) fn read_scrollback(parser: &mut Vt, offset: usize) -> (Vec<Vec<Cell>>, usize, usize) {
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

pub(super) fn snapshot_grid(parser: &Vt) -> Vec<Vec<Cell>> {
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

pub(super) fn snapshot_cursor(parser: &Vt, shape: CursorShape) -> Cursor {
    let screen = parser.screen();
    let (row, col) = screen.cursor_position();
    Cursor {
        row,
        col,
        visible: !screen.hide_cursor(),
        shape,
    }
}

pub(super) fn snapshot_mouse(parser: &Vt) -> MouseTracking {
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
