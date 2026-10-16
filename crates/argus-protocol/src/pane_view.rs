//! A pane's view as a client holds it: the live grid the daemon's damage
//! lands on, the view parked above it in the pane's history, how each of
//! the daemon's pane messages changes them, and what a scroll gesture over
//! the pane sends — the child's bytes, a request for history, or nothing.
//!
//! Every client keeps one per pane it draws and does only the drawing
//! itself, so the wheel, the parked view and its requests behave the same
//! in each.
//!
//! The daemon owns the history; a view owns only where it is parked in it.
//! An offset is a request and the rows arrive later, so nothing here waits
//! for an answer and a burst of notches costs one request each. While
//! parked, damage keeps landing on the live grid, so going back to the
//! bottom is immediate and needs no fresh subscription; the parked rows are
//! deliberately not re-read as it lands, since they are what the person
//! scrolled up to read and would otherwise move out from under them.
//! Scrolling further is asked by line — the number the daemon gave the
//! view's first row — so it moves from the text on screen however far
//! output has pushed that text back meanwhile.

use crate::keys::{self, Key, KeyCode, KeyboardMode, Mods};
use crate::mouse::{self, MouseEvent, MouseKind};
use crate::{grid_from_runs, Cell, ClientMsg, Cursor, MouseTracking, PaneId, ServerMsg};

/// Lines one wheel notch moves a parked view, as terminals scroll for one
/// detent of a physical wheel.
const WHEEL_LINES: i32 = 3;

/// What a client says when the daemon had no history behind the screen.
pub const NOTHING_FURTHER_BACK: &str = "nothing further back";

pub struct PaneView {
    pane: PaneId,
    rows: u16,
    cols: u16,
    live: Vec<Vec<Cell>>,
    cursor: Cursor,
    mouse: MouseTracking,
    alternate_screen: bool,
    keyboard: KeyboardMode,
    /// Whether the daemon has sent the grid yet; damage before it has
    /// nothing to land on.
    received: bool,
    parked: Option<History>,
}

/// A view above the live screen.
struct History {
    offset: u32,
    /// How far back the daemon last said the buffer goes; `None` until an
    /// answer arrives, and until then a move up is not bounded here, so a
    /// second notch before the first answer does not clamp the view back
    /// to live.
    depth: Option<u32>,
    /// The daemon's number for the first row, which that line keeps as
    /// output pushes it back. `None` from a daemon that numbers no lines.
    top: Option<u64>,
    /// The rows last read, seeded from the live grid so the first move
    /// draws text for the frame before the answer lands.
    rows: Vec<Vec<Cell>>,
}

/// Where a parked view is: lines above the live screen, how deep the
/// history goes as far as is known (never less than the offset), and the
/// daemon's number for its first line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parked {
    pub offset: u32,
    pub depth: u32,
    pub top: Option<u64>,
}

/// Which of the drawn rows (`PaneView::rows`) a change touched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Redraw {
    #[default]
    Nothing,
    Rows(Vec<u16>),
    /// Every row: a new grid, or the view moved between live and history.
    All,
}

/// What a message or gesture did: what to redraw, what to send the daemon,
/// and what to tell the person.
#[derive(Debug, Default)]
#[must_use]
pub struct Effect {
    pub redraw: Redraw,
    pub send: Option<ClientMsg>,
    pub note: Option<&'static str>,
}

impl Effect {
    fn redraw(redraw: Redraw) -> Effect {
        Effect { redraw, ..Effect::default() }
    }
}

impl PaneView {
    pub fn new(pane: PaneId) -> PaneView {
        PaneView {
            pane,
            rows: 0,
            cols: 0,
            live: Vec::new(),
            cursor: Cursor::default(),
            mouse: MouseTracking::default(),
            alternate_screen: false,
            keyboard: KeyboardMode::default(),
            received: false,
            parked: None,
        }
    }

    /// The pane a message is about, when it is one a view takes.
    pub fn about(msg: &ServerMsg) -> Option<PaneId> {
        match msg {
            ServerMsg::PaneRows { pane, .. }
            | ServerMsg::PaneSnapshot { pane, .. }
            | ServerMsg::RowDamage { pane, .. }
            | ServerMsg::Damage { pane, .. }
            | ServerMsg::ScrollbackRows { pane, .. }
            | ServerMsg::ScrollbackRuns { pane, .. } => Some(*pane),
            _ => None,
        }
    }

    /// Applies a message about this pane's view; `None` for any other,
    /// which is left alone.
    pub fn receive(&mut self, msg: &ServerMsg) -> Option<Effect> {
        if PaneView::about(msg) != Some(self.pane) {
            return None;
        }
        Some(match msg {
            ServerMsg::PaneRows { rows, cols, runs, cursor, mouse, alternate_screen, keyboard, .. } => {
                self.modes(*cursor, *mouse, *alternate_screen, *keyboard);
                self.replace(*rows, *cols, grid_from_runs(*rows, *cols, runs))
            }
            ServerMsg::PaneSnapshot { rows, cols, cells, cursor, mouse, alternate_screen, keyboard, .. } => {
                self.modes(*cursor, *mouse, *alternate_screen, *keyboard);
                self.replace(*rows, *cols, cells.clone())
            }
            ServerMsg::RowDamage { .. } | ServerMsg::Damage { .. } if !self.received => Effect::default(),
            ServerMsg::RowDamage { scroll, runs, cursor, mouse, alternate_screen, keyboard, .. } => {
                self.modes(*cursor, *mouse, *alternate_screen, *keyboard);
                let mut touched = Vec::new();
                // The scroll moves what the runs were diffed against, so it
                // goes first, and every row it touched is redrawn.
                if let Some(scroll) = scroll {
                    scroll.apply(&mut self.live);
                    let bottom = scroll.bottom.min(self.rows);
                    touched.extend(scroll.top.min(bottom)..bottom);
                }
                for run in runs {
                    run.apply(&mut self.live);
                    touched.push(run.row);
                }
                self.touched(touched)
            }
            ServerMsg::Damage { spans, cursor, mouse, alternate_screen, keyboard, .. } => {
                self.modes(*cursor, *mouse, *alternate_screen, *keyboard);
                let mut touched = Vec::new();
                for span in spans {
                    let Some(row) = self.live.get_mut(span.row as usize) else {
                        continue;
                    };
                    for (slot, cell) in row.iter_mut().skip(span.col as usize).zip(&span.cells) {
                        *slot = cell.clone();
                    }
                    touched.push(span.row);
                }
                self.touched(touched)
            }
            ServerMsg::ScrollbackRows { offset, depth, cells, .. } => {
                self.history(*offset, *depth, None, cells.clone())
            }
            ServerMsg::ScrollbackRuns { offset, depth, rows, cols, runs, top, .. } => {
                self.history(*offset, *depth, *top, grid_from_runs(*rows, *cols, runs))
            }
            _ => unreachable!("`about` names only the messages matched above"),
        })
    }

    /// The wheel over the pane, `notches` up when negative, at the cell
    /// `at` (row, column) under the pointer. A child that asked for the
    /// mouse gets its wheel reports; a full-screen child that asked for
    /// nothing gets a cursor key a notch, xterm's alternate scroll, which
    /// is what Claude Code, Codex and Cursor Agent listen for; and the
    /// normal screen, which has history behind it, moves the view. A
    /// parked view keeps moving whatever the child asked for, so the way
    /// back down is the way up.
    pub fn wheel(&mut self, notches: i32, at: (u16, u16)) -> Effect {
        if notches == 0 {
            return Effect::default();
        }
        if self.parked.is_none() {
            let (up, count) = (notches < 0, notches.unsigned_abs() as usize);
            let kind = if up { MouseKind::ScrollUp } else { MouseKind::ScrollDown };
            if let Some(report) = mouse::encode(&MouseEvent { kind, row: at.0, col: at.1 }, self.mouse) {
                return self.input(report.repeat(count));
            }
            if self.alternate_screen {
                let code = if up { KeyCode::Up } else { KeyCode::Down };
                return self.input(keys::encode(&Key::new(code, Mods::NONE)).repeat(count));
            }
        }
        self.scroll(notches.saturating_mul(WHEEL_LINES))
    }

    /// Moves the view by whole screens less a line, so the line at the
    /// edge stays in view: what Shift-PageUp/PageDown mean.
    pub fn page(&mut self, pages: i32) -> Effect {
        let height = i32::from(self.rows).max(2) - 1;
        self.scroll(pages.saturating_mul(height))
    }

    /// Moves the view by `lines`: up into the history when negative, back
    /// down toward the live screen when positive.
    pub fn scroll(&mut self, lines: i32) -> Effect {
        let (offset, ceiling, top) = match &self.parked {
            Some(history) => (history.offset, history.depth.map_or(i64::MAX, i64::from), history.top),
            None => (0, i64::MAX, None),
        };
        let want = (i64::from(offset) - i64::from(lines)).clamp(0, ceiling);
        // Clamped at either end, a run of notches asks for the same place
        // over and over; only a move is worth a round trip.
        if want == i64::from(offset) {
            return Effect::default();
        }
        // The live grid has been kept current the whole time, so the bottom
        // needs no request.
        if want == 0 {
            return self.live();
        }
        let top = top.map(|top| top.saturating_add_signed(i64::from(offset) - want));
        self.park(want as u32, top)
    }

    /// Back on the live screen, wherever the person's attention returns to
    /// the present: typing, pasting, or leaving the pane.
    pub fn live(&mut self) -> Effect {
        match self.parked.take() {
            Some(_) => Effect::redraw(Redraw::All),
            None => Effect::default(),
        }
    }

    pub fn pane(&self) -> PaneId {
        self.pane
    }

    /// Whether the daemon has sent the grid yet.
    pub fn received(&self) -> bool {
        self.received
    }

    pub fn size(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    /// The rows to draw: the parked view's when there is one, else live.
    pub fn rows(&self) -> &[Vec<Cell>] {
        self.parked.as_ref().map_or(&self.live, |history| &history.rows)
    }

    /// The live screen, parked or not.
    pub fn live_rows(&self) -> &[Vec<Cell>] {
        &self.live
    }

    /// The child's cursor, which is on the live screen and not a parked one.
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    /// What mouse reports the child asked for.
    pub fn mouse(&self) -> MouseTracking {
        self.mouse
    }

    /// The kitty keyboard flags the child has pushed, which decide how
    /// [`keys::encode_with`] spells a key for it.
    pub fn keyboard(&self) -> KeyboardMode {
        self.keyboard
    }

    pub fn alternate_screen(&self) -> bool {
        self.alternate_screen
    }

    /// Where the view is parked; `None` while live.
    pub fn parked(&self) -> Option<Parked> {
        self.parked.as_ref().map(|history| Parked {
            offset: history.offset,
            depth: history.depth.unwrap_or(0).max(history.offset),
            top: history.top,
        })
    }

    /// A whole grid. A snapshot is how a resize reaches a client, so a
    /// parked view is asked for again where it stands: its rows are the old
    /// size and nothing else will replace them. Until the answer they are
    /// cut or padded to the new size, so drawing never reads past them.
    fn replace(&mut self, rows: u16, cols: u16, mut grid: Vec<Vec<Cell>>) -> Effect {
        fit(&mut grid, rows, cols);
        (self.rows, self.cols, self.live, self.received) = (rows, cols, grid, true);
        let send = self.parked.as_mut().map(|history| {
            fit(&mut history.rows, rows, cols);
            ClientMsg::Scrollback { pane: self.pane, offset: history.offset, top: history.top }
        });
        Effect { redraw: Redraw::All, send, note: None }
    }

    fn modes(&mut self, cursor: Cursor, mouse: MouseTracking, alternate_screen: bool, keyboard: KeyboardMode) {
        (self.cursor, self.mouse, self.alternate_screen, self.keyboard) = (cursor, mouse, alternate_screen, keyboard);
    }

    /// Live rows that changed, which are drawn only while live.
    fn touched(&self, mut rows: Vec<u16>) -> Effect {
        rows.retain(|row| *row < self.rows);
        if self.parked.is_some() || rows.is_empty() {
            return Effect::default();
        }
        rows.sort_unstable();
        rows.dedup();
        Effect::redraw(Redraw::Rows(rows))
    }

    /// The rows the daemon read from the history. Taken only while parked:
    /// an answer arriving after the view went back to live is stale and
    /// must not pull it up again. Answers come in request order on one
    /// connection, and the daemon clamps, so the offset it gives is the
    /// truth about where these rows came from.
    fn history(&mut self, offset: u32, depth: u32, top: Option<u64>, mut rows: Vec<Vec<Cell>>) -> Effect {
        if self.parked.is_none() {
            return Effect::default();
        }
        // Nothing behind the live screen — an empty buffer, or a
        // full-screen child that keeps no history — leaves no view to hold.
        if offset == 0 {
            self.parked = None;
            return Effect { redraw: Redraw::All, send: None, note: Some(NOTHING_FURTHER_BACK) };
        }
        fit(&mut rows, self.rows, self.cols);
        self.parked = Some(History { offset, depth: Some(depth), top, rows });
        Effect::redraw(Redraw::All)
    }

    fn park(&mut self, offset: u32, top: Option<u64>) -> Effect {
        let redraw = match &mut self.parked {
            Some(history) => {
                history.offset = offset;
                history.top = top;
                Redraw::Nothing
            }
            None => {
                self.parked = Some(History { offset, depth: None, top, rows: self.live.clone() });
                Redraw::All
            }
        };
        Effect { redraw, send: Some(ClientMsg::Scrollback { pane: self.pane, offset, top }), note: None }
    }

    fn input(&self, bytes: Vec<u8>) -> Effect {
        let send = (!bytes.is_empty()).then_some(ClientMsg::Input { pane: self.pane, bytes });
        Effect { send, ..Effect::default() }
    }
}

/// `rows` by `cols`, cut or padded with blanks.
fn fit(grid: &mut Vec<Vec<Cell>>, rows: u16, cols: u16) {
    grid.resize(rows as usize, Vec::new());
    for row in grid {
        row.resize(cols as usize, Cell::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CellRun, CellSpan, MouseEncoding, MouseMode, Scroll};

    const PANE: PaneId = PaneId(2);

    fn row(text: &str, cols: usize) -> Vec<Cell> {
        let mut cells: Vec<Cell> = text.chars().map(|c| Cell { ch: c.to_string().into(), ..Cell::default() }).collect();
        cells.resize(cols, Cell::default());
        cells
    }

    fn text(rows: &[Vec<Cell>]) -> Vec<String> {
        rows.iter().map(|r| r.iter().map(|c| c.ch.as_str()).collect::<String>().trim_end().to_string()).collect()
    }

    fn snapshot(lines: &[&str], cols: u16, mouse: MouseTracking, alternate_screen: bool) -> ServerMsg {
        ServerMsg::PaneSnapshot {
            pane: PANE,
            rows: lines.len() as u16,
            cols,
            cells: lines.iter().map(|l| row(l, cols as usize)).collect(),
            cursor: Cursor::default(),
            mouse,
            alternate_screen,
            keyboard: Default::default(),
        }
    }

    fn row_damage(scroll: Option<Scroll>, runs: Vec<CellRun>, mouse: MouseTracking, alternate_screen: bool) -> ServerMsg {
        ServerMsg::RowDamage { pane: PANE, scroll, runs, cursor: Cursor::default(), mouse, alternate_screen, keyboard: Default::default() }
    }

    /// A view of five live rows on the normal screen.
    fn showing() -> PaneView {
        let mut view = PaneView::new(PANE);
        let _ = view.receive(&snapshot(&["live"; 5], 10, MouseTracking::default(), false));
        view
    }

    fn history(offset: u32, depth: u32, top: Option<u64>, mark: &str) -> ServerMsg {
        let grid = vec![row(mark, 10); 5];
        ServerMsg::ScrollbackRuns { pane: PANE, offset, depth, rows: 5, cols: 10, runs: crate::grid_runs(&grid), top }
    }

    fn asked(effect: &Effect) -> Option<(u32, Option<u64>)> {
        match &effect.send {
            Some(ClientMsg::Scrollback { pane: PANE, offset, top }) => Some((*offset, *top)),
            _ => None,
        }
    }

    fn typed(effect: &Effect) -> Option<Vec<u8>> {
        match &effect.send {
            Some(ClientMsg::Input { pane: PANE, bytes }) => Some(bytes.clone()),
            _ => None,
        }
    }

    fn sgr() -> MouseTracking {
        MouseTracking { mode: MouseMode::PressRelease, encoding: MouseEncoding::Sgr }
    }

    #[test]
    fn the_wheel_parks_the_view_and_asks_further_up_by_line() {
        let mut view = showing();
        let first = view.wheel(-1, (0, 0));
        assert_eq!(asked(&first), Some((3, None)));
        assert_eq!(first.redraw, Redraw::All, "the view left the live screen");
        assert_eq!(text(view.rows())[0], "live", "drawn from the live rows until the answer");

        let _ = view.receive(&history(3, 50, Some(100), "old"));
        assert_eq!(text(view.rows())[0], "old");
        assert_eq!(asked(&view.wheel(-1, (0, 0))), Some((6, Some(97))));
        assert_eq!(asked(&view.wheel(1, (0, 0))), Some((3, Some(100))));
    }

    #[test]
    fn a_second_notch_before_the_answer_keeps_climbing() {
        // Without a depth yet there is nothing to bound it by; bounding by
        // zero sent a quick spin of the wheel straight back to live.
        let mut view = showing();
        let _ = view.wheel(-1, (0, 0));
        assert_eq!(asked(&view.wheel(-1, (0, 0))), Some((6, None)));
        assert_eq!(view.parked().map(|p| p.offset), Some(6));
    }

    #[test]
    fn scrolling_stops_at_the_depth_the_daemon_gave() {
        let mut view = showing();
        let _ = view.wheel(-1, (0, 0));
        let _ = view.receive(&history(3, 4, Some(10), "h"));
        assert_eq!(asked(&view.wheel(-1, (0, 0))), Some((4, Some(9))));
        let _ = view.receive(&history(4, 4, Some(9), "t"));
        assert!(view.wheel(-1, (0, 0)).send.is_none(), "already at the top");
    }

    #[test]
    fn scrolling_down_to_the_bottom_is_live_without_asking() {
        let mut view = showing();
        let _ = view.wheel(-1, (0, 0));
        let _ = view.receive(&history(3, 50, Some(100), "old"));
        let down = view.wheel(4, (0, 0));
        assert!(down.send.is_none(), "the live screen is already streaming");
        assert_eq!(down.redraw, Redraw::All);
        assert!(view.parked().is_none());
        assert_eq!(text(view.rows())[0], "live");
    }

    #[test]
    fn a_page_is_a_screen_less_a_line() {
        let mut view = showing();
        assert_eq!(asked(&view.page(-1)), Some((4, None)));
        assert!(view.page(1).send.is_none());
        assert!(view.parked().is_none());
    }

    #[test]
    fn a_late_answer_does_not_pull_a_live_view_up() {
        let mut view = showing();
        let _ = view.wheel(-1, (0, 0));
        let _ = view.live();
        let late = view.receive(&history(3, 50, Some(100), "old")).unwrap();
        assert_eq!(late.redraw, Redraw::Nothing);
        assert!(view.parked().is_none());
    }

    #[test]
    fn a_history_with_nothing_behind_the_screen_leaves_it_live_and_says_so() {
        let mut view = showing();
        let _ = view.wheel(-1, (0, 0));
        let answer = view.receive(&history(0, 0, None, "")).unwrap();
        assert_eq!(answer.note, Some(NOTHING_FURTHER_BACK));
        assert!(view.parked().is_none());
    }

    #[test]
    fn a_parked_view_says_how_deep_the_history_goes() {
        let mut view = showing();
        let _ = view.wheel(-1, (0, 0));
        assert_eq!(view.parked(), Some(Parked { offset: 3, depth: 3, top: None }), "no deeper than known");
        let _ = view.receive(&history(3, 500, Some(7), "old"));
        assert_eq!(view.parked(), Some(Parked { offset: 3, depth: 500, top: Some(7) }));
    }

    #[test]
    fn the_wheel_over_a_full_screen_child_is_cursor_keys() {
        let mut view = PaneView::new(PANE);
        let _ = view.receive(&snapshot(&["x"; 5], 10, MouseTracking::default(), true));
        assert_eq!(typed(&view.wheel(-2, (0, 0))), Some(b"\x1b[A\x1b[A".to_vec()));
        assert_eq!(typed(&view.wheel(1, (0, 0))), Some(b"\x1b[B".to_vec()));
        assert!(view.parked().is_none());
    }

    #[test]
    fn the_wheel_over_a_child_that_asked_for_the_mouse_is_its_wheel_report() {
        // Claude Code asks for the mouse; a cursor key there walks its
        // prompt history instead of scrolling.
        let mut view = PaneView::new(PANE);
        let _ = view.receive(&snapshot(&["x"; 5], 10, sgr(), true));
        assert_eq!(typed(&view.wheel(-2, (1, 3))), Some("\x1b[<64;4;2M".repeat(2).into_bytes()));
    }

    #[test]
    fn a_parked_view_keeps_scrolling_whatever_the_child_asks_for_meanwhile() {
        let mut view = showing();
        let _ = view.wheel(-2, (0, 0));
        let _ = view.receive(&history(6, 50, None, "old"));
        let _ = view.receive(&row_damage(None, Vec::new(), sgr(), true));
        assert_eq!(asked(&view.wheel(1, (0, 0))), Some((3, None)), "the way down is the way it came up");
    }

    #[test]
    fn a_snapshot_re_reads_a_parked_view_at_the_new_size() {
        // A snapshot is how a resize reaches a client; the parked rows are
        // the old size and only a fresh read replaces them.
        let mut view = showing();
        let _ = view.wheel(-1, (0, 0));
        let _ = view.receive(&history(3, 50, Some(100), "old"));
        let resized = view.receive(&snapshot(&["wide"; 8], 20, MouseTracking::default(), false)).unwrap();
        assert_eq!(asked(&resized), Some((3, Some(100))));
        assert_eq!(view.parked().map(|p| p.offset), Some(3), "and stays where it was");
        assert_eq!(view.rows().len(), 8, "drawn at the new size until the answer");
        assert!(view.rows().iter().all(|r| r.len() == 20));
        assert_eq!(text(view.rows())[0], "old");
    }

    #[test]
    fn a_live_snapshot_asks_for_nothing() {
        let mut view = showing();
        let effect = view.receive(&snapshot(&["a"], 4, MouseTracking::default(), false)).unwrap();
        assert!(effect.send.is_none());
        assert_eq!(effect.redraw, Redraw::All);
        assert_eq!(view.size(), (1, 4));
    }

    #[test]
    fn history_is_drawn_at_the_screens_size() {
        let mut view = showing();
        let _ = view.wheel(-1, (0, 0));
        let _ = view.receive(&ServerMsg::ScrollbackRows { pane: PANE, offset: 3, depth: 9, cells: vec![row("old", 4)] });
        assert_eq!(view.rows().len(), 5);
        assert!(view.rows().iter().all(|r| r.len() == 10));
    }

    #[test]
    fn damage_lands_on_the_live_grid_while_the_parked_rows_hold_still() {
        let mut view = showing();
        let _ = view.wheel(-1, (0, 0));
        let _ = view.receive(&history(3, 50, None, "old"));
        let damage = row_damage(None, vec![CellRun::encode(0, 0, &row("new", 3))], MouseTracking::default(), false);
        assert_eq!(view.receive(&damage).unwrap().redraw, Redraw::Nothing, "nothing drawn moved");
        assert_eq!(text(view.rows())[0], "old");
        assert_eq!(text(view.live_rows())[0], "newe");
        let _ = view.live();
        assert_eq!(text(view.rows())[0], "newe");
    }

    #[test]
    fn a_scroll_lands_before_the_runs_and_redraws_its_region() {
        let mut view = PaneView::new(PANE);
        let _ = view.receive(&snapshot(&["a", "b", "c", "d"], 4, MouseTracking::default(), false));
        let scroll = Scroll { top: 0, bottom: 3, up: 1 };
        let damage = row_damage(Some(scroll), vec![CellRun::encode(2, 0, &row("new", 3))], MouseTracking::default(), false);
        assert_eq!(view.receive(&damage).unwrap().redraw, Redraw::Rows(vec![0, 1, 2]));
        assert_eq!(text(view.rows()), ["b", "c", "new", "d"], "the run lands on the row the scroll opened");
    }

    #[test]
    fn a_span_writes_what_fits_and_a_row_past_the_grid_is_dropped() {
        // Damage can arrive for a grid since replaced by a smaller one.
        let mut view = PaneView::new(PANE);
        let _ = view.receive(&snapshot(&["ab"], 2, MouseTracking::default(), false));
        let damage = ServerMsg::Damage {
            pane: PANE,
            spans: vec![CellSpan { row: 0, col: 1, cells: row("XYZ", 3) }, CellSpan { row: 9, col: 0, cells: row("Q", 1) }],
            cursor: Cursor::default(),
            mouse: MouseTracking::default(),
            alternate_screen: false,
            keyboard: Default::default(),
        };
        assert_eq!(view.receive(&damage).unwrap().redraw, Redraw::Rows(vec![0]));
        assert_eq!(text(view.rows()), ["aX"]);
    }

    #[test]
    fn damage_before_the_grid_has_nothing_to_land_on() {
        let mut view = PaneView::new(PANE);
        let damage = row_damage(None, Vec::new(), MouseTracking::default(), true);
        assert_eq!(view.receive(&damage).unwrap().redraw, Redraw::Nothing);
        assert!(!view.received() && !view.alternate_screen());
    }

    #[test]
    fn another_panes_messages_are_left_alone() {
        let mut view = PaneView::new(PaneId(99));
        assert!(view.receive(&snapshot(&["x"], 4, MouseTracking::default(), false)).is_none());
        assert!(view.receive(&ServerMsg::Restarting).is_none());
        assert!(!view.received());
    }
}

