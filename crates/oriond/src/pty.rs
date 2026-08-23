//! One PTY-backed pane: spawns a child process attached to a pty, mirrors its
//! output into a `vt100` grid on a coalesced ~60Hz tick, and broadcasts the
//! changed cell spans. See DESIGN.md §2 and §8.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use orion_protocol::{diff_grid, Cell, Color, PaneId, ServerMsg};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tokio::sync::broadcast;

const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;
const SCROLLBACK_LINES: usize = 4000;
const FRAME_INTERVAL: Duration = Duration::from_millis(16);

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

#[cfg(unix)]
fn program_command(program: &str, args: &[String]) -> CommandBuilder {
    let mut c = CommandBuilder::new(program);
    c.args(args);
    c
}

pub struct PaneRuntime {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    parser: Arc<StdMutex<vt100::Parser>>,
    child: Arc<StdMutex<Box<dyn Child + Send + Sync>>>,
    damage_tx: broadcast::Sender<ServerMsg>,
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
        cmd.cwd(cwd);

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let parser = Arc::new(StdMutex::new(vt100::Parser::new(
            DEFAULT_ROWS,
            DEFAULT_COLS,
            SCROLLBACK_LINES,
        )));
        let child = Arc::new(StdMutex::new(child));
        let (damage_tx, _) = broadcast::channel::<ServerMsg>(64);

        let (byte_tx, mut byte_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if byte_tx.send(buf[..n].to_vec()).is_err() {
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
                let mut on_exit = Some(on_exit);
                loop {
                    interval.tick().await;

                    let mut dirty = false;
                    while let Ok(chunk) = byte_rx.try_recv() {
                        parser.lock().unwrap().process(&chunk);
                        dirty = true;
                    }
                    if dirty {
                        let cur = snapshot_grid(&parser.lock().unwrap());
                        let spans = diff_grid(prev.as_ref(), &cur);
                        if !spans.is_empty() {
                            let _ = damage_tx.send(ServerMsg::Damage { pane: id, spans });
                        }
                        prev = Some(cur);
                    }

                    let exited = child.lock().unwrap().try_wait().ok().flatten();
                    if let Some(status) = exited {
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
            writer,
            parser,
            child,
            damage_tx,
        })
    }

    pub fn write_input(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.writer.write_all(bytes)?;
        Ok(())
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

    pub fn full_snapshot(&self) -> (u16, u16, Vec<Vec<Cell>>) {
        let parser = self.parser.lock().unwrap();
        let (rows, cols) = parser.screen().size();
        (rows, cols, snapshot_grid(&parser))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerMsg> {
        self.damage_tx.subscribe()
    }

    /// Pushes a fresh full-grid snapshot to whoever is currently subscribed.
    /// Used after a resize, since a subscriber's cached grid can only be
    /// grown or shrunk by replacing it wholesale — incremental Damage spans
    /// referencing indices outside its current size are meaningless to it.
    pub fn broadcast_snapshot(&self, pane: PaneId) {
        let (rows, cols, cells) = self.full_snapshot();
        let _ = self.damage_tx.send(ServerMsg::PaneSnapshot { pane, rows, cols, cells });
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
