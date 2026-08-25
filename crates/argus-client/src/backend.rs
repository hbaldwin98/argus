//! The crossterm backend, wrapped to stop the cursor thrashing.
//!
//! Two things sit between ratatui and the terminal here.
//!
//! **Synchronized output.** A frame is a diff plus a cursor move, written
//! as several syscalls. Without a synchronized-update wrapper the terminal
//! is free to present a half-written frame with the cursor already moved,
//! which is what a heavy repaint looks like as flicker. `BeginSynchronized-
//! Update` / `EndSynchronizedUpdate` (DEC mode 2026) makes the whole frame
//! land at once; terminals that don't know the mode ignore it.
//!
//! **Cursor de-churn.** Ratatui's `Terminal::draw` ends every frame with an
//! unconditional `Hide` or `Show`, and `CrosstermBackend` writes those with
//! `execute!`, so each one flushes on its own. An agent redrawing a spinner
//! toggles cursor visibility continuously, so that is a visible blink plus
//! two extra flushes per frame at 60Hz. Both are dropped here when nothing
//! actually changed.

use std::io::{self, Write};

use argus_protocol::CursorShape;
use crossterm::cursor::{Hide, SetCursorStyle, Show};
use crossterm::queue;
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use ratatui::backend::{Backend, CrosstermBackend, ClearType, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};

pub struct TermBackend<W: Write> {
    inner: CrosstermBackend<W>,
    /// `None` until the first frame has said one way or the other, so the
    /// opening frame always writes what it wants rather than assuming the
    /// terminal agrees with us.
    visible: Option<bool>,
    shape: Option<CursorShape>,
}

impl<W: Write> TermBackend<W> {
    pub fn new(writer: W) -> Self {
        TermBackend {
            inner: CrosstermBackend::new(writer),
            visible: None,
            shape: None,
        }
    }

    /// Opens a synchronized update. Queued rather than executed: it has to
    /// reach the terminal ahead of the frame's own bytes, and they share
    /// this writer, so queuing puts it in front of them without a flush of
    /// its own.
    pub fn begin_frame(&mut self) -> io::Result<()> {
        queue!(self.inner, BeginSynchronizedUpdate)
    }

    /// Closes it, and presents everything written since `begin_frame`.
    pub fn end_frame(&mut self) -> io::Result<()> {
        queue!(self.inner, EndSynchronizedUpdate)?;
        Write::flush(&mut self.inner)
    }

    /// Closes an update that may or may not be open, on the way out.
    ///
    /// A frame that fails partway through — a broken pipe, a draw that
    /// errors — returns without its `end_frame`, and a terminal left inside
    /// a synchronized update shows nothing at all. Cheap to send when there
    /// was nothing open: an unpaired end is a no-op.
    pub fn abandon_frame(&mut self) {
        let _ = queue!(self.inner, EndSynchronizedUpdate);
        let _ = Write::flush(&mut self.inner);
    }

    /// Asks the terminal for the shape the focused pane's child wants.
    /// Written inside the frame, so it presents with everything else.
    pub fn set_cursor_shape(&mut self, shape: CursorShape) -> io::Result<()> {
        if self.shape == Some(shape) {
            return Ok(());
        }
        self.shape = Some(shape);
        queue!(self.inner, style_of(shape))
    }
}

fn style_of(shape: CursorShape) -> SetCursorStyle {
    match shape {
        CursorShape::Default => SetCursorStyle::DefaultUserShape,
        CursorShape::BlinkingBlock => SetCursorStyle::BlinkingBlock,
        CursorShape::SteadyBlock => SetCursorStyle::SteadyBlock,
        CursorShape::BlinkingUnderline => SetCursorStyle::BlinkingUnderScore,
        CursorShape::SteadyUnderline => SetCursorStyle::SteadyUnderScore,
        CursorShape::BlinkingBar => SetCursorStyle::BlinkingBar,
        CursorShape::SteadyBar => SetCursorStyle::SteadyBar,
    }
}

impl<W: Write> Write for TermBackend<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Write::flush(&mut self.inner)
    }
}

impl<W: Write> Backend for TermBackend<W> {
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.inner.draw(content)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        if self.visible == Some(false) {
            return Ok(());
        }
        self.visible = Some(false);
        queue!(self.inner, Hide)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        if self.visible == Some(true) {
            return Ok(());
        }
        self.visible = Some(true);
        queue!(self.inner, Show)
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        self.inner.append_lines(n)
    }

    fn size(&self) -> io::Result<Size> {
        self.inner.size()
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A writer that keeps everything, so a test can ask what actually
    /// reached the terminal. The buffer is shared rather than owned
    /// because the backend takes the writer and does not hand it back.
    #[derive(Clone, Default)]
    struct Recorder(Rc<RefCell<Vec<u8>>>);

    impl Write for Recorder {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A backend and the tape of what it wrote.
    fn recording() -> (TermBackend<Recorder>, Recorder) {
        let tape = Recorder::default();
        (TermBackend::new(tape.clone()), tape)
    }

    fn written(backend: &mut TermBackend<Recorder>, tape: &Recorder) -> String {
        Write::flush(backend).unwrap();
        String::from_utf8_lossy(&tape.0.borrow()).replace('\u{1b}', "ESC")
    }

    #[test]
    fn a_repeated_hide_is_written_once() {
        // The reason this exists: ratatui re-decides cursor visibility on
        // every frame, and an agent's spinner means every frame. Writing
        // the same Hide sixty times a second is a visible blink.
        let (mut backend, tape) = recording();
        backend.hide_cursor().unwrap();
        backend.hide_cursor().unwrap();
        backend.hide_cursor().unwrap();

        assert_eq!(written(&mut backend, &tape), "ESC[?25l");
    }

    #[test]
    fn a_change_of_mind_is_written() {
        let (mut backend, tape) = recording();
        backend.hide_cursor().unwrap();
        backend.show_cursor().unwrap();
        backend.hide_cursor().unwrap();

        assert_eq!(written(&mut backend, &tape), "ESC[?25lESC[?25hESC[?25l");
    }

    #[test]
    fn the_first_frame_states_what_it_wants_rather_than_assuming() {
        // The terminal's cursor was left wherever the last program put it,
        // so "already visible" is not something the opening frame knows.
        let (mut backend, tape) = recording();
        backend.show_cursor().unwrap();

        assert_eq!(written(&mut backend, &tape), "ESC[?25h");
    }

    #[test]
    fn a_frame_is_wrapped_in_a_synchronized_update() {
        let (mut backend, tape) = recording();
        backend.begin_frame().unwrap();
        backend.hide_cursor().unwrap();
        backend.end_frame().unwrap();

        assert_eq!(written(&mut backend, &tape), "ESC[?2026hESC[?25lESC[?2026l");
    }

    #[test]
    fn a_shape_is_written_once_and_only_when_it_changes() {
        let (mut backend, tape) = recording();
        backend.set_cursor_shape(CursorShape::SteadyBar).unwrap();
        backend.set_cursor_shape(CursorShape::SteadyBar).unwrap();
        backend.set_cursor_shape(CursorShape::Default).unwrap();

        assert_eq!(written(&mut backend, &tape), "ESC[6 qESC[0 q");
    }
}
