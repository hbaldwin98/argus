//! A pane's terminal emulator — `alacritty_terminal` — and the translation
//! from its screen into the protocol's, and back the other way for a paste.
//!
//! The emulator is wrapped whole so that what a cell, a cursor, or a mouse
//! mode *is* on the wire can be read in one place, and so the pump and the
//! runtime never touch the emulator's own types.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell as TermCell, Flags};
use alacritty_terminal::term::{Config, Osc52, Term, TermDamage, TermMode};
use alacritty_terminal::vte::ansi::{
    self, CursorShape as TermShape, CursorStyle, NamedColor, Processor, StdSyncHandler,
};

use super::*;

/// The shape the emulator reports when no child has asked for one. Hollow,
/// because DECSCUSR cannot ask for it, so it can only mean "the host's own
/// shape" — which is what [`CursorShape::Default`] tells the client.
const UNSET_SHAPE: CursorStyle = CursorStyle {
    shape: TermShape::HollowBlock,
    blinking: false,
};

/// A pane's terminal: the emulator, its parser, and what the child asked
/// of it that the pump has not yet carried out.
pub(super) struct Vt {
    term: Term<Requests>,
    parser: Processor<StdSyncHandler>,
    requests: Requests,
}

/// What a child asked of its terminal beyond drawing, held until the pump
/// carries it out: text to copy, and the answers owed to its queries.
///
/// The emulator hands these over as events, mid-parse and through a shared
/// reference, hence the lock. Clipboard reads (`OSC 52 ; ?`) are never
/// answered — the emulator is set to copy only — since answering would hand
/// the user's clipboard to whatever runs in a pane. Queries (`CSI 6 n` and
/// the rest the emulator answers) are: a child that asks waits for the
/// answer, and ConPTY opened to inherit the cursor, as portable-pty 0.9
/// opens it, starts no child until it has one.
#[derive(Clone, Default)]
struct Requests(Arc<StdMutex<Asked>>);

#[derive(Default)]
struct Asked {
    copied: Vec<String>,
    replies: Vec<u8>,
}

impl EventListener for Requests {
    fn send_event(&self, event: Event) {
        let mut asked = self.0.lock().unwrap();
        match event {
            Event::ClipboardStore(_, text) => asked.copied.push(text),
            Event::PtyWrite(text) => asked.replies.extend_from_slice(text.as_bytes()),
            _ => {}
        }
    }
}

/// The size the emulator is asked to be.
struct Size {
    rows: u16,
    cols: u16,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }

    fn screen_lines(&self) -> usize {
        self.rows as usize
    }

    fn columns(&self) -> usize {
        self.cols as usize
    }
}

impl Vt {
    pub(super) fn new(rows: u16, cols: u16, scrollback: usize) -> Vt {
        let config = Config {
            scrolling_history: scrollback,
            default_cursor_style: UNSET_SHAPE,
            osc52: Osc52::OnlyCopy,
            ..Config::default()
        };
        let requests = Requests::default();
        Vt {
            term: Term::new(config, &Size { rows, cols }, requests.clone()),
            parser: Processor::new(),
            requests,
        }
    }

    pub(super) fn process(&mut self, bytes: &[u8]) {
        self.end_overdue_sync();
        self.parser.advance(&mut self.term, bytes);
    }

    /// When a synchronized update (`CSI ? 2026 h`) the child began and has
    /// not ended must be drawn anyway. The parser holds everything after
    /// the start back until the end arrives, so a child that never sends it
    /// would freeze the pane; the pump wakes at this and calls
    /// [`Vt::end_overdue_sync`].
    pub(super) fn sync_deadline(&self) -> Option<Instant> {
        self.parser.sync_timeout().sync_timeout()
    }

    /// Draws a synchronized update whose deadline has passed. Answers
    /// whether it did, so the pump knows the screen changed.
    pub(super) fn end_overdue_sync(&mut self) -> bool {
        match self.sync_deadline() {
            Some(deadline) if deadline <= Instant::now() => {
                self.parser.stop_sync(&mut self.term);
                true
            }
            _ => false,
        }
    }

    /// Draws whatever a synchronized update is holding back, due or not:
    /// for a child that has exited, whose update will never end.
    pub(super) fn end_sync_now(&mut self) {
        if self.sync_deadline().is_some() {
            self.parser.stop_sync(&mut self.term);
        }
    }

    pub(super) fn take_copies(&mut self) -> Vec<String> {
        std::mem::take(&mut self.requests.0.lock().unwrap().copied)
    }

    /// Bytes to write back to the child, in the order it asked.
    pub(super) fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.requests.0.lock().unwrap().replies)
    }

    pub(super) fn size(&self) -> (u16, u16) {
        (
            self.term.screen_lines() as u16,
            self.term.columns() as u16,
        )
    }

    pub(super) fn resize(&mut self, rows: u16, cols: u16) {
        self.term.resize(Size { rows, cols });
    }

    /// The whole live screen.
    pub(super) fn grid(&self) -> Vec<Vec<Cell>> {
        let (rows, _) = self.size();
        (0..rows as i32).map(|line| self.row(line)).collect()
    }

    /// Brings `live` up to the screen, converting only the lines the
    /// emulator marked changed since the last call — which is the point of
    /// this emulator: a keystroke's echo converts a cell or two, not every
    /// cell on the screen. A `live` of another size is rebuilt whole.
    pub(super) fn refresh(&mut self, live: &mut Vec<Vec<Cell>>) {
        let (rows, cols) = self.size();
        if live.len() != rows as usize || live.iter().any(|row| row.len() != cols as usize) {
            *live = self.grid();
            // Read rather than only reset: reading is also what moves the
            // emulator's note of where the cursor was, which the next
            // frame's damage is measured from.
            let _ = self.term.damage();
            self.term.reset_damage();
            return;
        }
        let lines: Vec<(usize, usize, usize)> = match self.term.damage() {
            TermDamage::Full => (0..rows as usize)
                .map(|line| (line, 0, cols as usize - 1))
                .collect(),
            TermDamage::Partial(damaged) => damaged
                .map(|bounds| (bounds.line, bounds.left, bounds.right))
                .collect(),
        };
        self.term.reset_damage();
        let grid = self.term.grid();
        for (line, left, right) in lines {
            let Some(row) = live.get_mut(line) else {
                continue;
            };
            let right = right.min(row.len().saturating_sub(1));
            for (col, cell) in row.iter_mut().enumerate().take(right + 1).skip(left) {
                *cell = convert_cell(&grid[Line(line as i32)][Column(col)]);
            }
        }
    }

    /// One row by line, where lines below zero are history.
    fn row(&self, line: i32) -> Vec<Cell> {
        let grid = self.term.grid();
        let row = &grid[Line(line)];
        (0..self.term.columns())
            .map(|col| convert_cell(&row[Column(col)]))
            .collect()
    }

    pub(super) fn cursor(&self) -> Cursor {
        let point = self.term.grid().cursor.point;
        let style = self.term.cursor_style();
        Cursor {
            row: point.line.0.max(0) as u16,
            col: point.column.0 as u16,
            visible: self.term.mode().contains(TermMode::SHOW_CURSOR)
                && style.shape != TermShape::Hidden,
            shape: convert_shape(style),
        }
    }

    pub(super) fn mouse(&self) -> MouseTracking {
        let mode = self.term.mode();
        MouseTracking {
            mode: if mode.contains(TermMode::MOUSE_MOTION) {
                MouseMode::AnyMotion
            } else if mode.contains(TermMode::MOUSE_DRAG) {
                MouseMode::ButtonMotion
            } else if mode.contains(TermMode::MOUSE_REPORT_CLICK) {
                MouseMode::PressRelease
            } else {
                MouseMode::None
            },
            encoding: if mode.contains(TermMode::SGR_MOUSE) {
                MouseEncoding::Sgr
            } else if mode.contains(TermMode::UTF8_MOUSE) {
                MouseEncoding::Utf8
            } else {
                MouseEncoding::Default
            },
        }
    }

    pub(super) fn alternate_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    pub(super) fn bracketed_paste(&self) -> bool {
        self.term.mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// The screen's worth of rows sitting `offset` lines above the live
    /// screen, with the offset actually reached and how deep the history
    /// goes. Read by line, so the live screen every other watcher is
    /// diffed against never moves. The alternate screen keeps no history,
    /// so a full-screen child answers with a depth of zero rather than
    /// letting the shell's history show through underneath it.
    pub(super) fn scrollback(&self, offset: usize) -> (Vec<Vec<Cell>>, usize, usize) {
        let depth = self.term.grid().history_size();
        let offset = offset.min(depth);
        let (rows, _) = self.size();
        let top = -(offset as i32);
        let cells = (0..rows as i32).map(|r| self.row(top + r)).collect();
        (cells, offset, depth)
    }
}

pub(super) fn convert_cell(c: &TermCell) -> Cell {
    // The second half of a wide character, and the gap left at the end of
    // a line a wide character did not fit on, hold nothing of their own;
    // the screen still needs a blank drawn there, in the cell's colours.
    let spacer = c
        .flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
    let ch = if spacer || c.c == ' ' && c.zerowidth().is_none() {
        BLANK
    } else {
        let mut text = CompactString::default();
        text.push(c.c);
        for &mark in c.zerowidth().unwrap_or_default() {
            text.push(mark);
        }
        text
    };
    Cell {
        ch,
        fg: convert_color(c.fg),
        bg: convert_color(c.bg),
        bold: c.flags.contains(Flags::BOLD),
        italic: c.flags.contains(Flags::ITALIC),
        underline: c.flags.intersects(Flags::ALL_UNDERLINES),
        reverse: c.flags.contains(Flags::INVERSE),
    }
}

/// The protocol has the sixteen named colours as indices and no names for
/// the terminal's own foreground and background, which are its default.
pub(super) fn convert_color(c: ansi::Color) -> Color {
    match c {
        ansi::Color::Spec(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
        ansi::Color::Indexed(i) => Color::Idx(i),
        ansi::Color::Named(named) => {
            let index = named as usize;
            if index < 16 {
                Color::Idx(index as u8)
            } else if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize)
                .contains(&index)
            {
                Color::Idx((index - NamedColor::DimBlack as usize) as u8)
            } else {
                Color::Default
            }
        }
    }
}

fn convert_shape(style: CursorStyle) -> CursorShape {
    match (style.shape, style.blinking) {
        (TermShape::Block, true) => CursorShape::BlinkingBlock,
        (TermShape::Block, false) => CursorShape::SteadyBlock,
        (TermShape::Underline, true) => CursorShape::BlinkingUnderline,
        (TermShape::Underline, false) => CursorShape::SteadyUnderline,
        (TermShape::Beam, true) => CursorShape::BlinkingBar,
        (TermShape::Beam, false) => CursorShape::SteadyBar,
        (TermShape::HollowBlock | TermShape::Hidden, _) => CursorShape::Default,
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
