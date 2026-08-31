//! One PTY-backed pane: spawns a child process attached to a pty, mirrors its
//! output into a `vt100` grid on a coalesced ~60Hz tick, and broadcasts the
//! changed cell spans. See DESIGN.md §2 and §8.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};

use argus_protocol::{
    diff_grid, Cell, Color, CompactString, Cursor, CursorShape, MouseEncoding, MouseMode,
    MouseTracking, PaneId, ServerMsg, BLANK,
};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tokio::sync::broadcast;

mod job;
mod vt;

use job::*;
use vt::*;

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

#[cfg(windows)]
const AGENT_JOB_MEMORY_BYTES: usize = 8 * 1024 * 1024 * 1024;
#[cfg(windows)]
const AGENT_JOB_PROCESS_LIMIT: u32 = 64;

#[derive(Clone, Copy)]
pub enum ResourcePolicy {
    Unrestricted,
    Agent,
}

/// What to run in a newly-opened pty: the user's shell, or a named program
/// (an agent CLI) with its own args and extra environment variables.
pub enum Spawn {
    DefaultShell,
    Program {
        program: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        resource_policy: ResourcePolicy,
    },
}

impl Spawn {
    fn into_command(self) -> (CommandBuilder, ResourcePolicy) {
        match self {
            Self::DefaultShell => (
                CommandBuilder::new_default_prog(),
                ResourcePolicy::Unrestricted,
            ),
            Self::Program {
                program,
                args,
                env,
                resource_policy,
            } => {
                let mut command = program_command(&program, &args);
                for (key, value) in env {
                    command.env(key, value);
                }
                (command, resource_policy)
            }
        }
    }
}

pub struct PaneRuntime {
    master: Box<dyn MasterPty + Send>,
    input: PaneInput,
    parser: Arc<StdMutex<vt100::Parser>>,
    /// Shared with the pump: the cursor shape lives outside `vt100`, but a
    /// snapshot taken from any thread still has to report it.
    shape: Arc<StdMutex<CursorShapeScanner>>,
    child: Arc<StdMutex<Box<dyn Child + Send + Sync>>>,
    damage_tx: broadcast::Sender<ServerMsg>,
    #[cfg(windows)]
    _job: Option<ProcessJob>,
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

        let (mut cmd, resource_policy) = spec.into_command();
        // Argus owns the outer Herdr pane. Processes nested in its PTYs must
        // not compete with the client's aggregate lifecycle report for it.
        strip_herdr_context(&mut cmd, std::env::vars_os().map(|(key, _)| key));
        cmd.cwd(cwd);

        #[cfg(windows)]
        let job = job_for(resource_policy)?;
        #[cfg(not(windows))]
        let _ = resource_policy;

        // Windows `assign_to_job` takes `&mut child`; Unix never mutates it.
        #[cfg(windows)]
        let mut child = pair.slave.spawn_command(cmd)?;
        #[cfg(not(windows))]
        let child = pair.slave.spawn_command(cmd)?;
        #[cfg(windows)]
        assign_to_job(job.as_ref(), &mut child)?;
        drop(pair.slave);

        let parser = Arc::new(StdMutex::new(vt100::Parser::new(
            DEFAULT_ROWS,
            DEFAULT_COLS,
            SCROLLBACK_LINES,
        )));
        let reader = pair.master.try_clone_reader()?;
        let input = PaneInput {
            writer: Arc::new(StdMutex::new(pair.master.take_writer()?)),
            parser: parser.clone(),
        };
        let child = Arc::new(StdMutex::new(child));
        let shape = Arc::new(StdMutex::new(CursorShapeScanner::default()));
        let (damage_tx, _) = broadcast::channel::<ServerMsg>(64);

        let (byte_tx, mut byte_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(OUTPUT_QUEUE_CHUNKS);
        spawn_output_reader(reader, byte_tx);

        {
            let parser = parser.clone();
            let shape = shape.clone();
            let child = child.clone();
            let damage_tx = damage_tx.clone();
            tokio::spawn(async move {
                let mut prev: Option<Vec<Vec<Cell>>> = None;
                let mut prev_cursor = None;
                // Tracked alongside the grid because a child can turn mouse
                // reporting on or off without changing a single cell, and a
                // client that misses the change forwards mouse bytes to a
                // child that will print them.
                let mut prev_mouse = None;
                let mut prev_alt = None;
                let mut on_exit = Some(on_exit);
                let mut eof = false;
                loop {
                    // Wait for the first byte, not for the next tick. A
                    // tick-first pump makes an echoed keystroke sit out the
                    // remainder of a frame it had no part in starting, and
                    // this pane's frame grid has no relation to the one the
                    // client draws on, so the two waits stack. The rate cap
                    // is at the bottom of the loop instead: it belongs after
                    // a frame has been presented, not before one is begun.
                    let first = if eof {
                        // Nothing more can arrive, but the exit poll below
                        // still has to run.
                        tokio::time::sleep(FRAME_INTERVAL).await;
                        None
                    } else {
                        tokio::select! {
                            chunk = byte_rx.recv() => {
                                eof = chunk.is_none();
                                chunk
                            }
                            // A pane that produces nothing still has to be
                            // watched for its child exiting.
                            _ = tokio::time::sleep(FRAME_INTERVAL) => None,
                        }
                    };

                    let mut dirty = false;
                    let mut budget = MAX_CHUNKS_PER_FRAME;
                    if let Some(chunk) = first {
                        shape.lock().unwrap().feed(&chunk);
                        parser.lock().unwrap().process(&chunk);
                        dirty = true;
                        budget -= 1;
                    }
                    // Whatever else is already queued rides along on this
                    // frame rather than costing one of its own.
                    for _ in 0..budget {
                        let Ok(chunk) = byte_rx.try_recv() else { break };
                        shape.lock().unwrap().feed(&chunk);
                        parser.lock().unwrap().process(&chunk);
                        dirty = true;
                    }
                    if dirty && damage_tx.receiver_count() == 0 {
                        // Nobody is watching this pane. The parser is fed
                        // either way above, so its screen stays current, but
                        // snapshotting and diffing a whole grid for an
                        // audience of none is what a background agent's
                        // output charges the pane you are actually typing
                        // into — these tasks share the runtime with the
                        // connection carrying your keystrokes. Dropping
                        // `prev` makes the next watched frame a full
                        // repaint, which is the only correct diff against a
                        // grid we stopped tracking.
                        prev = None;
                        prev_cursor = None;
                        prev_mouse = None;
                        prev_alt = None;
                    } else if dirty {
                        let shape = shape.lock().unwrap().shape();
                        let parser = parser.lock().unwrap();
                        let cur = snapshot_grid(&parser);
                        let cursor = snapshot_cursor(&parser, shape);
                        let mouse = snapshot_mouse(&parser);
                        let alternate_screen = parser.screen().alternate_screen();
                        let spans = diff_grid(prev.as_ref(), &cur);
                        if !spans.is_empty()
                            || prev_cursor != Some(cursor)
                            || prev_mouse != Some(mouse)
                            || prev_alt != Some(alternate_screen)
                        {
                            let _ = damage_tx.send(ServerMsg::Damage {
                                pane: id,
                                spans,
                                cursor,
                                mouse,
                                alternate_screen,
                            });
                        }
                        prev = Some(cur);
                        prev_cursor = Some(cursor);
                        prev_mouse = Some(mouse);
                        prev_alt = Some(alternate_screen);
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
                                    shape.lock().unwrap().feed(&chunk);
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
                            let shape = shape.lock().unwrap().shape();
                            let parser = parser.lock().unwrap();
                            let cur = snapshot_grid(&parser);
                            let cursor = snapshot_cursor(&parser, shape);
                            let mouse = snapshot_mouse(&parser);
                            let alternate_screen = parser.screen().alternate_screen();
                            let spans = diff_grid(prev.as_ref(), &cur);
                            if !spans.is_empty()
                                || prev_cursor != Some(cursor)
                                || prev_mouse != Some(mouse)
                                || prev_alt != Some(alternate_screen)
                            {
                                let _ = damage_tx.send(ServerMsg::Damage {
                                    pane: id,
                                    spans,
                                    cursor,
                                    mouse,
                                    alternate_screen,
                                });
                            }
                        }

                        let code = Some(status.exit_code() as i32);
                        let _ = damage_tx.send(ServerMsg::PaneClosed { pane: id, code });
                        if let Some(cb) = on_exit.take() {
                            cb(code);
                        }
                        break;
                    }

                    // The rate cap. A sustained stream still coalesces onto
                    // FRAME_INTERVAL; a lone keystroke's echo has already
                    // gone out above without waiting for it.
                    if dirty {
                        tokio::time::sleep(FRAME_INTERVAL).await;
                    }
                }
            });
        }

        Ok(PaneRuntime {
            master: pair.master,
            input,
            parser,
            shape,
            child,
            damage_tx,
            #[cfg(windows)]
            _job: job,
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
        self.parser.lock().unwrap().screen_mut().set_size(rows, cols);
        Ok(())
    }

    pub fn kill(&self) -> anyhow::Result<()> {
        #[cfg(windows)]
        if let Some(job) = &self._job {
            return job.terminate();
        }
        self.child.lock().unwrap().kill()?;
        Ok(())
    }

    pub fn full_snapshot(&self) -> (u16, u16, Vec<Vec<Cell>>, Cursor, MouseTracking, bool) {
        let shape = self.shape.lock().unwrap().shape();
        let parser = self.parser.lock().unwrap();
        let (rows, cols) = parser.screen().size();
        (
            rows,
            cols,
            snapshot_grid(&parser),
            snapshot_cursor(&parser, shape),
            snapshot_mouse(&parser),
            parser.screen().alternate_screen(),
        )
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
    pub fn snapshot_and_subscribe(
        &self,
    ) -> (
        u16,
        u16,
        Vec<Vec<Cell>>,
        Cursor,
        MouseTracking,
        bool,
        broadcast::Receiver<ServerMsg>,
    ) {
        let shape = self.shape.lock().unwrap().shape();
        let parser = self.parser.lock().unwrap();
        let (rows, cols) = parser.screen().size();
        let rx = self.damage_tx.subscribe();
        (
            rows,
            cols,
            snapshot_grid(&parser),
            snapshot_cursor(&parser, shape),
            snapshot_mouse(&parser),
            parser.screen().alternate_screen(),
            rx,
        )
    }

    /// Rows sitting `offset` lines above the live screen, with the offset
    /// actually reached and how deep the buffer goes.
    ///
    /// The parser's offset is moved and put straight back under one hold
    /// of the lock, because it is parser-global: left set, it would drag
    /// every other subscriber's frames back with this client's view. The
    /// alternate screen answers with a depth of zero rather than letting
    /// the shell's history show through underneath a full-screen child.
    pub fn scrollback(&self, offset: usize) -> (Vec<Vec<Cell>>, usize, usize) {
        read_scrollback(&mut self.parser.lock().unwrap(), offset)
    }

    /// Pushes a fresh full-grid snapshot to whoever is currently subscribed.
    /// Used after a resize, since a subscriber's cached grid can only be
    /// grown or shrunk by replacing it wholesale — incremental Damage spans
    /// referencing indices outside its current size are meaningless to it.
    pub fn broadcast_snapshot(&self, pane: PaneId) {
        let (rows, cols, cells, cursor, mouse, alternate_screen) = self.full_snapshot();
        let _ = self.damage_tx.send(ServerMsg::PaneSnapshot {
            pane,
            rows,
            cols,
            cells,
            cursor,
            mouse,
            alternate_screen,
        });
    }
}

#[cfg(test)]
mod tests;
