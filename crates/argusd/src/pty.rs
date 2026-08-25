//! One PTY-backed pane: spawns a child process attached to a pty, mirrors its
//! output into a `vt100` grid on a coalesced ~60Hz tick, and broadcasts the
//! changed cell spans. See DESIGN.md §2 and §8.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use argus_protocol::{diff_grid, Cell, Color, Cursor, PaneId, ServerMsg};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tokio::sync::broadcast;

const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;
const SCROLLBACK_LINES: usize = 4000;
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const OUTPUT_QUEUE_CHUNKS: usize = 256;
const MAX_CHUNKS_PER_FRAME: usize = 64;
/// How long the output pump keeps draining a dead child's remaining output
/// before announcing the exit. Short-lived commands routinely exit before
/// any of their output has been drained.
const EXIT_FLUSH_GRACE: Duration = Duration::from_millis(500);
const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

/// What to run in a newly-opened pty: the user's shell, or a named program
/// (an agent CLI) with its own args and extra environment variables.
pub enum Spawn {
    DefaultShell,
    Program {
        program: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
}

/// Builds the command to run a named program with args. On Windows this
/// routes through `cmd.exe /C` so PATHEXT resolution finds `.cmd`/`.bat`
/// shims (e.g. npm-installed CLIs) the same way a typed command would;
/// `CreateProcess` alone only resolves bare `.exe` targets.
#[cfg(windows)]
fn program_command(program: &str, args: &[String]) -> CommandBuilder {
    let mut c = CommandBuilder::new("cmd.exe");
    c.arg("/C");
    c.arg(program);
    c.args(args);
    c
}

fn strip_herdr_context(
    command: &mut CommandBuilder,
    keys: impl IntoIterator<Item = std::ffi::OsString>,
) {
    for key in keys {
        if key.to_string_lossy().starts_with("HERDR_") {
            command.env_remove(key);
        }
    }
}

#[cfg(unix)]
fn program_command(program: &str, args: &[String]) -> CommandBuilder {
    let mut c = CommandBuilder::new(program);
    c.args(args);
    c
}

pub struct PaneRuntime {
    master: Box<dyn MasterPty + Send>,
    input: PaneInput,
    parser: Arc<StdMutex<vt100::Parser>>,
    child: Arc<StdMutex<Box<dyn Child + Send + Sync>>>,
    damage_tx: broadcast::Sender<ServerMsg>,
}

#[derive(Clone)]
pub struct PaneInput {
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
    parser: Arc<StdMutex<vt100::Parser>>,
}

impl PaneInput {
    pub fn write(&self, bytes: &[u8]) -> anyhow::Result<()> {
        self.writer.lock().unwrap().write_all(bytes)?;
        Ok(())
    }

    pub fn paste(&self, bytes: &[u8]) -> anyhow::Result<()> {
        let bracketed = self.parser.lock().unwrap().screen().bracketed_paste();
        self.writer
            .lock()
            .unwrap()
            .write_all(&paste_bytes(bytes, bracketed))?;
        Ok(())
    }
}

impl PaneRuntime {
    /// Spawns the pane's process and its background reader/pump tasks.
    /// `on_exit` fires exactly once, off the pump task, when the child dies.
    pub fn spawn(
        id: PaneId,
        cwd: &Path,
        spec: Spawn,
        on_exit: impl FnOnce(Option<i32>) + Send + 'static,
    ) -> anyhow::Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = match spec {
            Spawn::DefaultShell => CommandBuilder::new_default_prog(),
            Spawn::Program { program, args, env } => {
                let mut c = program_command(&program, &args);
                for (k, v) in env {
                    c.env(k, v);
                }
                c
            }
        };
        // Argus owns the outer Herdr pane. Processes nested in its PTYs must
        // not compete with the client's aggregate lifecycle report for it.
        strip_herdr_context(&mut cmd, std::env::vars_os().map(|(key, _)| key));
        cmd.cwd(cwd);

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let parser = Arc::new(StdMutex::new(vt100::Parser::new(
            DEFAULT_ROWS,
            DEFAULT_COLS,
            SCROLLBACK_LINES,
        )));
        let mut reader = pair.master.try_clone_reader()?;
        let input = PaneInput {
            writer: Arc::new(StdMutex::new(pair.master.take_writer()?)),
            parser: parser.clone(),
        };
        let child = Arc::new(StdMutex::new(child));
        let (damage_tx, _) = broadcast::channel::<ServerMsg>(64);

        let (byte_tx, mut byte_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(OUTPUT_QUEUE_CHUNKS);
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if byte_tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        {
            let parser = parser.clone();
            let child = child.clone();
            let damage_tx = damage_tx.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(FRAME_INTERVAL);
                let mut prev: Option<Vec<Vec<Cell>>> = None;
                let mut prev_cursor = None;
                let mut on_exit = Some(on_exit);
                loop {
                    interval.tick().await;

                    let mut dirty = false;
                    for _ in 0..MAX_CHUNKS_PER_FRAME {
                        let Ok(chunk) = byte_rx.try_recv() else { break };
                        parser.lock().unwrap().process(&chunk);
                        dirty = true;
                    }
                    if dirty {
                        let parser = parser.lock().unwrap();
                        let cur = snapshot_grid(&parser);
                        let cursor = snapshot_cursor(&parser);
                        let spans = diff_grid(prev.as_ref(), &cur);
                        if !spans.is_empty() || prev_cursor != Some(cursor) {
                            let _ = damage_tx.send(ServerMsg::Damage { pane: id, spans, cursor });
                        }
                        prev = Some(cur);
                        prev_cursor = Some(cursor);
                    }

                    let exited = child.lock().unwrap().try_wait().ok().flatten();
                    if let Some(status) = exited {
                        // The child is gone, but its final output may still
                        // be in flight between the reader thread and this
                        // pump — a short-lived command can exit before a
                        // single byte has been drained. Breaking out here
                        // would lose its entire output, so flush what's
                        // still coming before announcing the exit. Bounded
                        // by a grace deadline: the reader can't be relied on
                        // to hit EOF, since `PaneRuntime` still holds the
                        // pty master open.
                        let grace = tokio::time::Instant::now() + EXIT_FLUSH_GRACE;
                        let mut flushed = false;
                        while tokio::time::Instant::now() < grace {
                            match tokio::time::timeout(FRAME_INTERVAL, byte_rx.recv()).await {
                                Ok(Some(chunk)) => {
                                    parser.lock().unwrap().process(&chunk);
                                    flushed = true;
                                }
                                // EOF: the reader is done, nothing more can arrive.
                                Ok(None) => break,
                                // A quiet frame after we already flushed means
                                // the output has stopped coming.
                                Err(_) if flushed => break,
                                Err(_) => continue,
                            }
                        }
                        if flushed {
                            let parser = parser.lock().unwrap();
                            let cur = snapshot_grid(&parser);
                            let cursor = snapshot_cursor(&parser);
                            let spans = diff_grid(prev.as_ref(), &cur);
                            if !spans.is_empty() || prev_cursor != Some(cursor) {
                                let _ = damage_tx.send(ServerMsg::Damage { pane: id, spans, cursor });
                            }
                        }

                        let code = Some(status.exit_code() as i32);
                        let _ = damage_tx.send(ServerMsg::PaneClosed { pane: id, code });
                        if let Some(cb) = on_exit.take() {
                            cb(code);
                        }
                        break;
                    }
                }
            });
        }

        Ok(PaneRuntime {
            master: pair.master,
            input,
            parser,
            child,
            damage_tx,
        })
    }

    pub fn input(&self) -> PaneInput {
        self.input.clone()
    }

    pub fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        self.parser.lock().unwrap().set_size(rows, cols);
        Ok(())
    }

    pub fn kill(&self) -> anyhow::Result<()> {
        self.child.lock().unwrap().kill()?;
        Ok(())
    }

    pub fn full_snapshot(&self) -> (u16, u16, Vec<Vec<Cell>>, Cursor) {
        let parser = self.parser.lock().unwrap();
        let (rows, cols) = parser.screen().size();
        (rows, cols, snapshot_grid(&parser), snapshot_cursor(&parser))
    }

    /// The damage stream on its own. Only tests want this: a real
    /// subscriber needs the grid the stream continues from, which is
    /// [`PaneRuntime::snapshot_and_subscribe`].
    #[cfg(test)]
    pub fn subscribe(&self) -> broadcast::Receiver<ServerMsg> {
        self.damage_tx.subscribe()
    }

    /// A full grid and the damage stream that continues it, taken together.
    ///
    /// Atomic on purpose: the pump needs the parser lock to produce a frame,
    /// so holding it across both halves means no frame can be published
    /// between them. Taken separately, a frame landing in that gap belongs
    /// to neither — it is newer than the snapshot and older than the
    /// receiver — and the cells it carried stay wrong on the subscriber's
    /// grid until something else happens to overwrite them.
    pub fn snapshot_and_subscribe(&self) -> (u16, u16, Vec<Vec<Cell>>, Cursor, broadcast::Receiver<ServerMsg>) {
        let parser = self.parser.lock().unwrap();
        let (rows, cols) = parser.screen().size();
        let rx = self.damage_tx.subscribe();
        (rows, cols, snapshot_grid(&parser), snapshot_cursor(&parser), rx)
    }

    /// Pushes a fresh full-grid snapshot to whoever is currently subscribed.
    /// Used after a resize, since a subscriber's cached grid can only be
    /// grown or shrunk by replacing it wholesale — incremental Damage spans
    /// referencing indices outside its current size are meaningless to it.
    pub fn broadcast_snapshot(&self, pane: PaneId) {
        let (rows, cols, cells, cursor) = self.full_snapshot();
        let _ = self.damage_tx.send(ServerMsg::PaneSnapshot {
            pane,
            rows,
            cols,
            cells,
            cursor,
        });
    }
}

fn snapshot_grid(parser: &vt100::Parser) -> Vec<Vec<Cell>> {
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

fn snapshot_cursor(parser: &vt100::Parser) -> Cursor {
    let screen = parser.screen();
    let (row, col) = screen.cursor_position();
    Cursor {
        row,
        col,
        visible: !screen.hide_cursor(),
    }
}

fn cell_from_vt100(cell: Option<&vt100::Cell>) -> Cell {
    match cell {
        None => Cell::default(),
        Some(c) => {
            let ch = c.contents();
            Cell {
                ch: if ch.is_empty() { " ".to_string() } else { ch },
                fg: convert_color(c.fgcolor()),
                bg: convert_color(c.bgcolor()),
                bold: c.bold(),
                italic: c.italic(),
                underline: c.underline(),
                reverse: c.inverse(),
            }
        }
    }
}

fn convert_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Default,
        vt100::Color::Idx(i) => Color::Idx(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn paste_bytes(bytes: &[u8], bracketed: bool) -> Vec<u8> {
    if !bracketed {
        return bytes.to_vec();
    }
    let mut pasted = Vec::with_capacity(PASTE_START.len() + bytes.len() + PASTE_END.len());
    pasted.extend_from_slice(PASTE_START);
    pasted.extend_from_slice(bytes);
    pasted.extend_from_slice(PASTE_END);
    pasted
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::CellSpan;

    #[test]
    fn nested_processes_do_not_inherit_the_outer_herdr_pane() {
        let mut command = CommandBuilder::new("dummy");
        command.env("HERDR_ENV", "1");
        command.env("HERDR_PANE_ID", "w1:p1");
        command.env("ARGUS_PANE", "7");

        strip_herdr_context(
            &mut command,
            ["HERDR_ENV".into(), "HERDR_PANE_ID".into()],
        );

        assert_eq!(command.get_env("HERDR_ENV"), None);
        assert_eq!(command.get_env("HERDR_PANE_ID"), None);
        assert_eq!(command.get_env("ARGUS_PANE"), Some(std::ffi::OsStr::new("7")));
    }

    /// Flattens a grid to one string per row, trailing blanks trimmed.
    fn rows_of(grid: &[Vec<Cell>]) -> Vec<String> {
        grid.iter()
            .map(|r| r.iter().map(|c| c.ch.as_str()).collect::<String>().trim_end().to_string())
            .collect()
    }

    fn grid_contains(grid: &[Vec<Cell>], needle: &str) -> bool {
        rows_of(grid).iter().any(|r| r.contains(needle))
    }

    fn echo(text: &str) -> Spawn {
        Spawn::Program {
            program: "echo".to_string(),
            args: vec![text.to_string()],
            env: Vec::new(),
        }
    }

    /// Polls the pane's own grid until `pred` holds or the deadline passes.
    /// Polling the grid rather than sleeping a fixed time keeps the test both
    /// fast and non-flaky: process startup latency varies wildly.
    async fn wait_for(pane: &PaneRuntime, pred: impl Fn(&[Vec<Cell>]) -> bool) -> Vec<Vec<Cell>> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let (_, _, grid, _) = pane.full_snapshot();
            if pred(&grid) {
                return grid;
            }
            if tokio::time::Instant::now() > deadline {
                panic!("timed out; last screen was:\n{}", rows_of(&grid).join("\n"));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn a_programs_output_lands_on_the_panes_grid() {
        let pane = PaneRuntime::spawn(PaneId(1), &std::env::temp_dir(), echo("argus-marker"), |_| {}).unwrap();
        wait_for(&pane, |g| grid_contains(g, "argus-marker")).await;
    }

    #[tokio::test]
    async fn a_short_lived_commands_output_is_not_lost_to_its_own_exit() {
        // Regression: the pump used to break out of its loop as soon as
        // `try_wait` reported the child gone, discarding output still in
        // flight from the reader thread. A command fast enough to exit
        // within a frame or two would leave a permanently blank pane.
        // Repeated because the race only lost the output sometimes.
        for i in 0..10 {
            let marker = format!("fast-exit-{i}");
            let pane =
                PaneRuntime::spawn(PaneId(20 + i), &std::env::temp_dir(), echo(&marker), |_| {}).unwrap();
            wait_for(&pane, |g| grid_contains(g, &marker)).await;
        }
    }

    #[tokio::test]
    async fn output_is_broadcast_as_damage_to_subscribers() {
        let pane = PaneRuntime::spawn(PaneId(7), &std::env::temp_dir(), echo("damage-marker"), |_| {}).unwrap();
        let mut rx = pane.subscribe();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        let mut seen = String::new();
        loop {
            let left = deadline - tokio::time::Instant::now();
            let msg = tokio::time::timeout(left, rx.recv())
                .await
                .unwrap_or_else(|_| panic!("no damage carrying the marker; saw {seen:?}"))
                .unwrap();
            match msg {
                ServerMsg::Damage { pane, spans, .. } => {
                    assert_eq!(pane, PaneId(7), "damage must be tagged with its own pane");
                    seen.push_str(&span_text(&spans));
                    if seen.contains("damage-marker") {
                        return;
                    }
                }
                ServerMsg::PaneClosed { .. } => panic!("exited before the marker appeared; saw {seen:?}"),
                _ => {}
            }
        }
    }

    fn span_text(spans: &[CellSpan]) -> String {
        spans
            .iter()
            .flat_map(|s| s.cells.iter().map(|c| c.ch.as_str()))
            .collect()
    }

    #[tokio::test]
    async fn typed_input_reaches_the_child_and_its_output_comes_back() {
        // The M1 spine end-to-end, without a terminal: keystrokes in, screen
        // out. This is the loop that otherwise needs a real TUI to check.
        let pane = PaneRuntime::spawn(
            PaneId(2),
            &std::env::temp_dir(),
            Spawn::DefaultShell,
            |_| {},
        )
        .unwrap();
        // Let the shell draw its prompt before typing at it.
        tokio::time::sleep(Duration::from_millis(500)).await;
        pane.input().write(b"echo argus-typed\r").unwrap();
        wait_for(&pane, |g| grid_contains(g, "argus-typed")).await;
        let _ = pane.kill();
    }

    #[tokio::test]
    async fn a_child_exit_fires_on_exit_once_and_announces_pane_closed() {
        let seen: Arc<StdMutex<Vec<Option<i32>>>> = Arc::new(StdMutex::new(Vec::new()));
        let sink = seen.clone();
        let pane = PaneRuntime::spawn(PaneId(3), &std::env::temp_dir(), echo("bye"), move |code| {
            sink.lock().unwrap().push(code);
        })
        .unwrap();
        let mut rx = pane.subscribe();

        let closed = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Ok(ServerMsg::PaneClosed { pane, code }) = rx.recv().await {
                    return (pane, code);
                }
            }
        })
        .await
        .expect("the pane should have announced its exit");

        assert_eq!(closed.0, PaneId(3));
        assert_eq!(closed.1, Some(0), "a successful echo exits clean");
        // The pump breaks out of its loop on exit, so the callback can only
        // ever run once — a second call would double-mark the tree.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(*seen.lock().unwrap(), vec![Some(0)]);
    }

    #[tokio::test]
    async fn killing_a_pane_makes_it_exit() {
        let pane =
            PaneRuntime::spawn(PaneId(4), &std::env::temp_dir(), Spawn::DefaultShell, |_| {}).unwrap();
        let mut rx = pane.subscribe();
        tokio::time::sleep(Duration::from_millis(300)).await;
        pane.kill().unwrap();

        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Ok(ServerMsg::PaneClosed { .. }) = rx.recv().await {
                    return;
                }
            }
        })
        .await
        .expect("kill should end the pane");
    }

    #[tokio::test]
    async fn a_pane_starts_at_the_default_size_and_resize_changes_it() {
        let pane =
            PaneRuntime::spawn(PaneId(5), &std::env::temp_dir(), Spawn::DefaultShell, |_| {}).unwrap();
        let (rows, cols, grid, _) = pane.full_snapshot();
        assert_eq!((rows, cols), (DEFAULT_ROWS, DEFAULT_COLS));
        assert_eq!(grid.len(), DEFAULT_ROWS as usize);
        assert_eq!(grid[0].len(), DEFAULT_COLS as usize);

        pane.resize(40, 120).unwrap();
        let (rows, cols, grid, _) = pane.full_snapshot();
        assert_eq!((rows, cols), (40, 120));
        assert_eq!(grid.len(), 40, "the grid must actually grow, not just the pty");
        assert_eq!(grid[0].len(), 120);
        let _ = pane.kill();
    }

    #[tokio::test]
    async fn resize_pushes_a_full_snapshot_so_new_area_is_not_left_blank() {
        // Incremental Damage can't grow a subscriber's cached grid, so a
        // resize has to re-send the whole screen at the new size.
        let pane =
            PaneRuntime::spawn(PaneId(6), &std::env::temp_dir(), Spawn::DefaultShell, |_| {}).unwrap();
        let mut rx = pane.subscribe();
        pane.resize(40, 120).unwrap();
        pane.broadcast_snapshot(PaneId(6));

        let msg = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match rx.recv().await.unwrap() {
                    m @ ServerMsg::PaneSnapshot { .. } => return m,
                    _ => continue,
                }
            }
        })
        .await
        .expect("a snapshot should follow a resize");

        let ServerMsg::PaneSnapshot {
            pane: id,
            rows,
            cols,
            cells,
            ..
        } = msg else {
            unreachable!()
        };
        assert_eq!(id, PaneId(6));
        assert_eq!((rows, cols), (40, 120));
        assert_eq!(cells.len(), 40);
        let _ = pane.kill();
    }

    #[tokio::test]
    async fn a_pane_runs_in_the_checkouts_directory() {
        let dir = tempfile::tempdir().unwrap();
        // The randomly-named leaf is the marker: the child can only print it
        // if it inherited the checkout's directory as its cwd.
        let leaf = dir.path().file_name().unwrap().to_string_lossy().to_string();

        let pane = PaneRuntime::spawn(
            PaneId(8),
            dir.path(),
            Spawn::Program {
                program: if cfg!(windows) { "cd".to_string() } else { "pwd".to_string() },
                args: Vec::new(),
                env: Vec::new(),
            },
            |_| {},
        )
        .unwrap();
        wait_for(&pane, |g| grid_contains(g, &leaf)).await;
    }

    #[tokio::test]
    async fn a_nonexistent_program_does_not_take_the_daemon_down() {
        // Windows routes through `cmd /C`, which itself starts fine and then
        // reports the failure; unix fails at spawn. Either way the daemon
        // must survive — the pane just ends up dead.
        let r = PaneRuntime::spawn(
            PaneId(9),
            &std::env::temp_dir(),
            Spawn::Program {
                program: "argus-definitely-not-a-real-program".to_string(),
                args: Vec::new(),
                env: Vec::new(),
            },
            |_| {},
        );
        if let Ok(pane) = r {
            let mut rx = pane.subscribe();
            let closed = tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    if let Ok(ServerMsg::PaneClosed { code, .. }) = rx.recv().await {
                        return code;
                    }
                }
            })
            .await
            .expect("a bad program should end the pane, not hang it");
            assert_ne!(closed, Some(0), "a missing program is not a success");
        }
    }

    // --- pure vt100 conversion ---------------------------------------------

    #[test]
    fn an_absent_vt100_cell_becomes_a_blank() {
        let c = cell_from_vt100(None);
        assert_eq!(c.ch, " ");
        assert_eq!(c.fg, Color::Default);
    }

    #[test]
    fn colors_convert_across_all_three_forms() {
        assert_eq!(convert_color(vt100::Color::Default), Color::Default);
        assert_eq!(convert_color(vt100::Color::Idx(9)), Color::Idx(9));
        assert_eq!(convert_color(vt100::Color::Rgb(1, 2, 3)), Color::Rgb(1, 2, 3));
    }

    #[test]
    fn a_parsed_screen_snapshots_to_a_full_rectangular_grid() {
        let mut parser = vt100::Parser::new(4, 10, 0);
        parser.process(b"hi");
        let grid = snapshot_grid(&parser);
        assert_eq!(grid.len(), 4);
        assert!(grid.iter().all(|r| r.len() == 10), "every row is full width");
        assert_eq!(rows_of(&grid)[0], "hi");
        assert_eq!(rows_of(&grid)[1], "", "untouched rows are blank, not missing");
    }

    #[test]
    fn sgr_attributes_survive_the_snapshot() {
        let mut parser = vt100::Parser::new(1, 10, 0);
        parser.process(b"\x1b[1;3;4;7;31mX\x1b[0m");
        let grid = snapshot_grid(&parser);
        let c = &grid[0][0];
        assert_eq!(c.ch, "X");
        assert!(c.bold && c.italic && c.underline && c.reverse, "{c:?}");
        assert_eq!(c.fg, Color::Idx(1), "red");
    }

    #[test]
    fn cursor_movement_and_erase_are_honored_not_appended() {
        let mut parser = vt100::Parser::new(2, 10, 0);
        parser.process(b"abcdef\x1b[H\x1b[Kxy");
        assert_eq!(rows_of(&snapshot_grid(&parser))[0], "xy");
    }

    #[test]
    fn cursor_position_and_visibility_survive_vt_parsing() {
        let mut parser = vt100::Parser::new(4, 10, 0);
        parser.process(b"\x1b[3;5H\x1b[?25l");
        assert_eq!(
            snapshot_cursor(&parser),
            Cursor {
                row: 2,
                col: 4,
                visible: false,
            }
        );
    }

    #[test]
    fn paste_is_delimited_only_when_the_child_requested_it() {
        assert_eq!(paste_bytes(b"one\ntwo", false), b"one\ntwo");
        assert_eq!(
            paste_bytes(b"one\ntwo", true),
            b"\x1b[200~one\ntwo\x1b[201~"
        );
    }
}
