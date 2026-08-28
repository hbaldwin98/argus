//! Owning the terminal for the length of a run: entering and leaving the
//! alternate screen, and presenting one frame at a time.

use std::io::{self, Write};

use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use ratatui::Terminal;

use crate::app::App;
use crate::backend::TermBackend;
use crate::ui;

/// One frame is written as hundreds of small writes. `io::Stdout` is a
/// line writer with a 1 KiB buffer, and terminal output carries almost no
/// newlines, so unbuffered each full frame became a long run of tiny
/// console writes — the fixed ~6ms a frame cost that showed up in the
/// profile whether or not anything had changed. Buffered, a frame is one
/// write, at `end_frame`.
const FRAME_BUFFER: usize = 1 << 20;

pub type Term = Terminal<TermBackend<io::BufWriter<io::Stdout>>>;

pub fn enter_terminal() -> anyhow::Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    Ok(Terminal::new(TermBackend::new(
        io::BufWriter::with_capacity(FRAME_BUFFER, stdout),
    ))?)
}

/// One frame, presented all at once.
///
/// The draw itself is a diff, a cursor move, and a visibility change,
/// written as several syscalls; wrapping them in a synchronized update is
/// what stops the terminal presenting a half-drawn frame with the cursor
/// already moved. The shape goes inside the wrapper for the same reason.
pub fn draw_frame(terminal: &mut Term, app: &mut App) -> anyhow::Result<std::time::Duration> {
    let mut ui_took = std::time::Duration::ZERO;
    terminal.backend_mut().begin_frame()?;
    terminal.draw(|f| {
        let began = std::time::Instant::now();
        ui::render(f, app);
        ui_took = began.elapsed();
    })?;
    let shape = app
        .layout
        .cursor
        .map_or(argus_protocol::CursorShape::Default, |c| c.shape);
    terminal.backend_mut().set_cursor_shape(shape)?;
    terminal.backend_mut().end_frame()?;
    Ok(ui_took)
}

pub fn leave_terminal(terminal: &mut Term) -> anyhow::Result<()> {
    // Ahead of everything else: if the last frame died partway through, the
    // terminal is still inside a synchronized update and would show none of
    // what follows.
    terminal.backend_mut().abandon_frame();
    disable_raw_mode()?;
    // The cursor shape belongs to whatever the user runs next, not to the
    // last pane that happened to be focused here.
    let _ = terminal
        .backend_mut()
        .set_cursor_shape(argus_protocol::CursorShape::Default);
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    Ok(())
}

pub fn ring_bell(terminal: &mut Term) -> io::Result<()> {
    terminal.backend_mut().write_all(b"\x07")?;
    terminal.backend_mut().flush()
}
