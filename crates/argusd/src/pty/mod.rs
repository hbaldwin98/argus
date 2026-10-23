//! One PTY-backed pane: spawns a child process attached to a pty, mirrors its
//! output into a terminal emulator on a coalesced ~60Hz tick, and broadcasts
//! the changed cell runs. See ARCHITECTURE.md, "Panes and terminals".

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

#[cfg(any(windows, test))]
use std::ffi::OsString;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};

use argus_protocol::{
    damage, grid_runs, Cell, Color, CompactString, Cursor, CursorShape, KeyboardMode, MouseEncoding, MouseMode,
    MouseTracking, PaneId, ServerMsg, BLANK,
};
#[cfg(unix)]
use portable_pty::ChildKiller;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tokio::sync::broadcast;

mod cd;
#[cfg(windows)]
mod console_host;
mod job;
mod pace;
mod vt;

use job::*;
use pace::FRAME_INTERVAL;
#[cfg(windows)]
pub(crate) use job::ProcessJob;
pub use cd::{Shell, ShellState};
pub use vt::Scrolled;
use vt::*;

const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;
/// The largest a pane is made, far past any screen: a size is a client's to
/// ask, and the grid is allocated whole, so 65535 by 65535 would ask the
/// daemon for billions of cells and end it.
pub const MAX_ROWS: u16 = 1_000;
pub const MAX_COLS: u16 = 2_000;
const SCROLLBACK_LINES: usize = 4000;
const OUTPUT_QUEUE_CHUNKS: usize = 256;
const MAX_CHUNKS_PER_FRAME: usize = 64;
/// The most a read from the pty hands the pump at once.
const READ_CHUNK: usize = 8192;
/// How long the output pump keeps draining a dead child's remaining output
/// before announcing the exit. Short-lived commands routinely exit before
/// any of their output has been drained.
const EXIT_FLUSH_GRACE: Duration = Duration::from_millis(500);
const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";
const PTY_TERM: &str = "xterm-256color";
#[cfg(windows)]
const WINDOWS_SHELL_FALLBACKS: &[&str] = &["pwsh", "powershell", "cmd.exe"];

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
            Self::DefaultShell => {
                let mut command = default_shell_command();
                strip_argus_hook_identity(&mut command);
                (command, ResourcePolicy::Unrestricted)
            }
            Self::Program {
                program,
                args,
                env,
                resource_policy,
            } => {
                let mut command = program_command(&program, &args);
                // Before the pane's own: an agent is handed a hook identity
                // of its own, which may equal what the daemon inherited.
                strip_argus_hook_identity(&mut command);
                for (key, value) in env {
                    command.env(key, value);
                }
                (command, resource_policy)
            }
        }
    }
}

fn default_shell_command() -> CommandBuilder {
    #[cfg(windows)]
    {
        // portable-pty's default Windows program is always ComSpec, which is
        // normally cmd.exe. Keep the builder's merged environment, but select
        // an interactive shell explicitly so PowerShell installations win.
        let mut command = CommandBuilder::new("cmd.exe");
        let shell = select_shell(
            command.get_env("SHELL"),
            WINDOWS_SHELL_FALLBACKS,
            |candidate| {
                executable_on_path(
                    candidate,
                    command.get_env("PATH"),
                    command.get_env("PATHEXT"),
                )
            },
        );
        command.get_argv_mut()[0] = shell;
        command
    }

    #[cfg(not(windows))]
    {
        CommandBuilder::new_default_prog()
    }
}

#[cfg(any(windows, test))]
fn select_shell(
    configured: Option<&OsStr>,
    fallbacks: &[&str],
    is_available: impl Fn(&OsStr) -> bool,
) -> OsString {
    if let Some(shell) = configured {
        if !shell.is_empty() && is_available(shell) {
            return shell.to_owned();
        }
    }

    fallbacks
        .iter()
        .map(OsStr::new)
        .find(|shell| is_available(shell))
        .map(OsStr::to_owned)
        // The final fallback is deliberately returned even if PATH is
        // incomplete: Windows supplies cmd.exe, and the spawn error is more
        // useful than refusing to create the pane here.
        .unwrap_or_else(|| OsString::from(*fallbacks.last().expect("shell fallbacks")))
}

#[cfg(windows)]
fn executable_on_path(
    program: &OsStr,
    path: Option<&OsStr>,
    pathext: Option<&OsStr>,
) -> bool {
    let extensions: Vec<OsString> = pathext
        .map(|value| value.to_string_lossy().split(';').map(OsString::from).collect())
        .unwrap_or_else(|| {
            [".EXE", ".CMD", ".BAT", ".COM"]
                .into_iter()
                .map(OsString::from)
                .collect()
        });

    let has_path = Path::new(program).is_absolute()
        || program.to_string_lossy().contains(['\\', '/']);
    if has_path {
        return executable_in_directory(Path::new("."), program, &extensions)
            || Path::new(program).is_file();
    }

    if executable_in_directory(Path::new("."), program, &extensions) {
        return true;
    }

    path.into_iter()
        .flat_map(std::env::split_paths)
        .any(|directory| executable_in_directory(&directory, program, &extensions))
}

#[cfg(windows)]
fn executable_in_directory(directory: &Path, program: &OsStr, extensions: &[OsString]) -> bool {
    let candidate = directory.join(program);
    if candidate.is_file() {
        return true;
    }
    if Path::new(program).extension().is_some() {
        return false;
    }

    extensions.iter().any(|extension| {
        let mut candidate = directory.join(program);
        candidate.set_extension(extension.to_string_lossy().trim_start_matches('.'));
        candidate.is_file()
    })
}

pub struct PaneRuntime {
    /// The console server and the child. First, so a runtime dropped
    /// without a kill ends them before the master closes the
    /// pseudoconsole: `ClosePseudoConsole` waits for the server to exit,
    /// and a server with no one left reading its output never does.
    #[cfg(windows)]
    job: ProcessJob,
    master: Box<dyn MasterPty + Send>,
    input: PaneInput,
    vt: Arc<StdMutex<Vt>>,
    /// The child itself belongs to its exit waiter, so ending it goes
    /// through a killer cloned off it before it was handed over.
    #[cfg(unix)]
    killer: Killer,
    /// The session the child leads, for the Unix sweep.
    #[cfg(unix)]
    pid: Option<u32>,
    /// The shell it runs, for the user's shell; `None` for a program.
    shell: Option<Result<Shell, String>>,
    damage_tx: broadcast::Sender<ServerMsg>,
    /// How many times the pump has woken, so a test can tell an idle pane
    /// from one ticking on a timer. Only a Unix test reads it: a ConPTY
    /// child is never silent from the start.
    #[cfg(test)]
    #[cfg_attr(windows, allow(dead_code))]
    pump_wakes: Arc<std::sync::atomic::AtomicUsize>,
}

/// A pane's grid and modes, and the damage stream that continues them.
pub type PaneSubscription = (
    u16,
    u16,
    Vec<Vec<Cell>>,
    Cursor,
    MouseTracking,
    bool,
    KeyboardMode,
    broadcast::Receiver<ServerMsg>,
);

#[derive(Clone)]
pub struct PaneInput {
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
    vt: Arc<StdMutex<Vt>>,
}

impl PaneInput {
    pub fn write(&self, bytes: &[u8]) -> anyhow::Result<()> {
        self.writer.lock().unwrap().write_all(bytes)?;
        Ok(())
    }

    pub fn paste(&self, bytes: &[u8]) -> anyhow::Result<()> {
        let bracketed = self.vt.lock().unwrap().bracketed_paste();
        self.writer
            .lock()
            .unwrap()
            .write_all(&paste_bytes(bytes, bracketed))?;
        Ok(())
    }

    /// Tells the child its pane gained or lost the person's focus, if it
    /// asked to hear of it. A child that did not would read the report as
    /// typed text.
    pub fn focus(&self, focused: bool) -> anyhow::Result<()> {
        let asked = self.vt.lock().unwrap().focus_events();
        if let Some(report) = focus_report(asked, focused) {
            self.writer.lock().unwrap().write_all(report)?;
        }
        Ok(())
    }
}

/// `CSI I` or `CSI O`, for a child that enabled focus reporting.
fn focus_report(asked: bool, focused: bool) -> Option<&'static [u8]> {
    asked.then_some(if focused { b"\x1b[I" } else { b"\x1b[O" })
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
        // Before the console server starts: everything a test pane starts
        // ends with the test binary, whatever this code does.
        #[cfg(all(test, windows))]
        job::hold_test_process();
        let pty_system = native_pty_system();
        let size = PtySize {
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            pixel_width: 0,
            pixel_height: 0,
        };
        #[cfg(windows)]
        let (pair, console_server) =
            console_host::open_with_server(|| pty_system.openpty(size))?;
        #[cfg(not(windows))]
        let pair = pty_system.openpty(size)?;

        let starts_shell = matches!(spec, Spawn::DefaultShell);
        let (mut cmd, resource_policy) = spec.into_command();
        let shell = starts_shell.then(|| cd::default_shell(&cmd));
        set_pty_terminal(&mut cmd);
        // Argus owns the outer Herdr pane. Processes nested in its PTYs must
        // not compete with the client's aggregate lifecycle report for it.
        strip_herdr_context(&mut cmd, std::env::vars_os().map(|(key, _)| key));
        strip_claude_session(&mut cmd, std::env::vars_os().map(|(key, _)| key));
        strip_git_repository(&mut cmd);
        cmd.cwd(cwd);

        #[cfg(windows)]
        let job = job_for(resource_policy)?;
        #[cfg(windows)]
        if let Some(server) = console_server {
            // Before the child starts: the server must not outlive the
            // owner whatever happens next.
            if let Err(error) = job.assign(server.as_raw_handle()) {
                tracing::warn!("this pane's console server is not contained: {error:#}");
            }
        }
        #[cfg(not(windows))]
        let _ = resource_policy;

        // Windows `assign_to_job` takes `&mut child`; Unix never mutates it.
        #[cfg(windows)]
        let mut child = pair.slave.spawn_command(cmd)?;
        #[cfg(not(windows))]
        let child = pair.slave.spawn_command(cmd)?;
        #[cfg(windows)]
        assign_to_job(&job, &mut child)?;
        drop(pair.slave);

        let vt = Arc::new(StdMutex::new(Vt::new(
            DEFAULT_ROWS,
            DEFAULT_COLS,
            SCROLLBACK_LINES,
        )));
        let reader = pair.master.try_clone_reader()?;
        let input = PaneInput {
            writer: Arc::new(StdMutex::new(pair.master.take_writer()?)),
            vt: vt.clone(),
        };
        #[cfg(unix)]
        let killer = StdMutex::new(child.clone_killer());
        #[cfg(unix)]
        let pid = child.process_id();
        let (damage_tx, _) = broadcast::channel::<ServerMsg>(64);
        #[cfg(test)]
        let pump_wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let (byte_tx, mut byte_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(OUTPUT_QUEUE_CHUNKS);
        spawn_output_reader(reader, byte_tx);
        let (exit_tx, mut exit_rx) = tokio::sync::oneshot::channel::<Option<i32>>();
        spawn_exit_waiter(child, exit_tx);

        {
            let vt = vt.clone();
            let damage_tx = damage_tx.clone();
            let replier = input.writer.clone();
            #[cfg(test)]
            let pump_wakes = pump_wakes.clone();
            tokio::spawn(async move {
                // The pump's own copy of the screen, kept current from the
                // emulator's damage rather than rebuilt every frame.
                let mut live: Vec<Vec<Cell>> = Vec::new();
                let mut sent = Sent::default();
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
                    //
                    // Nothing else wakes it but a synchronized update the
                    // child began and has not ended, which is drawn anyway
                    // once overdue: an idle pane sleeps here until its child
                    // speaks or exits.
                    let deadline = vt.lock().unwrap().sync_deadline();
                    let mut exited = None;
                    let first = tokio::select! {
                        chunk = byte_rx.recv(), if !eof => {
                            eof = chunk.is_none();
                            chunk
                        }
                        code = &mut exit_rx => {
                            // A waiter that vanished without a word cannot
                            // say how the child ended.
                            exited = Some(code.unwrap_or(None));
                            None
                        }
                        _ = tokio::time::sleep_until(
                            deadline.unwrap_or_else(std::time::Instant::now).into(),
                        ), if deadline.is_some() => None,
                    };
                    let began = tokio::time::Instant::now();
                    #[cfg(test)]
                    pump_wakes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                    let mut dirty = vt.lock().unwrap().end_overdue_sync();
                    let mut budget = MAX_CHUNKS_PER_FRAME;
                    if let Some(chunk) = first {
                        feed(&mut vt.lock().unwrap(), &chunk, id);
                        dirty = true;
                        budget -= 1;
                    }
                    // Whatever else is already queued rides along on this
                    // frame rather than costing one of its own.
                    for _ in 0..budget {
                        let Ok(chunk) = byte_rx.try_recv() else { break };
                        feed(&mut vt.lock().unwrap(), &chunk, id);
                        dirty = true;
                    }
                    // Sent whether or not anyone is watching the grid: a
                    // copy is a one-off, and there is no later frame that
                    // could carry it instead.
                    let (copied, replies) = {
                        let mut vt = vt.lock().unwrap();
                        (vt.take_copies(), vt.take_replies())
                    };
                    for text in copied {
                        let _ = damage_tx.send(ServerMsg::Clipboard { pane: id, text });
                    }
                    if !replies.is_empty() {
                        // A child gone since it asked is not listening.
                        let _ = replier.lock().unwrap().write_all(&replies);
                    }
                    if dirty && damage_tx.receiver_count() == 0 {
                        // Nobody is watching this pane. The emulator is fed
                        // either way above, so its screen stays current, but
                        // bringing a grid up to date for an audience of none
                        // is what a background agent's output charges the
                        // pane you are actually typing into — these tasks
                        // share the runtime with the connection carrying
                        // your keystrokes. The damage keeps until someone
                        // watches, and forgetting what was sent makes their
                        // first frame a full one.
                        sent = Sent::default();
                    } else if dirty {
                        send_frame(&mut vt.lock().unwrap(), &mut live, &mut sent, &damage_tx, id);
                    }

                    if let Some(code) = exited {
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
                                    vt.lock().unwrap().process(&chunk);
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
                            let mut vt = vt.lock().unwrap();
                            // Whatever it held back for an update it never
                            // got to end is the last thing it drew.
                            vt.end_sync_now();
                            send_frame(&mut vt, &mut live, &mut sent, &damage_tx, id);
                        }

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
                        pace::next_frame(began).await;
                    }
                }
            });
        }

        Ok(PaneRuntime {
            #[cfg(windows)]
            job,
            master: pair.master,
            input,
            vt,
            #[cfg(unix)]
            killer,
            #[cfg(unix)]
            pid,
            shell,
            damage_tx,
            #[cfg(test)]
            pump_wakes,
        })
    }

    /// What its shell is doing; `None` when it runs a program instead.
    pub fn shell(&self) -> Option<ShellState> {
        let shell = match self.shell.as_ref()? {
            Ok(shell) => *shell,
            Err(name) => return Some(ShellState::Unknown(name.clone())),
        };
        #[cfg(windows)]
        let idle = cd::idle(&self.job);
        #[cfg(unix)]
        let idle = cd::idle(self.master.as_ref(), self.pid);
        Some(if idle { ShellState::AtPrompt(shell) } else { ShellState::Running })
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
        self.vt.lock().unwrap().resize(rows, cols);
        Ok(())
    }

    pub fn kill(&self) -> anyhow::Result<()> {
        #[cfg(windows)]
        {
            end_process_tree(&self.job)
        }
        #[cfg(unix)]
        {
            end_process_tree(self.pid, &self.killer)
        }
    }

    #[cfg(test)]
    pub fn full_snapshot(&self) -> (u16, u16, Vec<Vec<Cell>>, Cursor, MouseTracking, bool) {
        let vt = self.vt.lock().unwrap();
        let (rows, cols) = vt.size();
        (
            rows,
            cols,
            vt.grid(),
            vt.cursor(),
            vt.mouse(),
            vt.alternate_screen(),
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
    /// Atomic on purpose: the pump needs the emulator's lock to produce a frame,
    /// so holding it across both halves means no frame can be published
    /// between them. Taken separately, a frame landing in that gap belongs
    /// to neither — it is newer than the snapshot and older than the
    /// receiver — and the cells it carried stay wrong on the subscriber's
    /// grid until something else happens to overwrite them.
    pub fn snapshot_and_subscribe(&self) -> PaneSubscription {
        let vt = self.vt.lock().unwrap();
        let (rows, cols) = vt.size();
        let rx = self.damage_tx.subscribe();
        (
            rows,
            cols,
            vt.grid(),
            vt.cursor(),
            vt.mouse(),
            vt.alternate_screen(),
            vt.keyboard(),
            rx,
        )
    }

    /// Rows sitting `offset` lines above the live screen, or from line
    /// `top` ([`Vt::scrollback`]).
    pub fn scrollback(&self, offset: usize, top: Option<u64>) -> Scrolled {
        self.vt.lock().unwrap().scrollback(offset, top)
    }

    /// Pushes a fresh full-grid snapshot to whoever is currently subscribed.
    /// Used after a resize, since a subscriber's cached grid can only be
    /// grown or shrunk by replacing it wholesale — incremental Damage spans
    /// referencing indices outside its current size are meaningless to it.
    ///
    /// Captured and sent under one hold of the emulator's lock, for the same
    /// reason [`PaneRuntime::snapshot_and_subscribe`] is atomic. The pump
    /// holds that lock across producing a frame *and* publishing it, so a
    /// snapshot taken with the lock released could be overtaken by a Damage
    /// newer than itself. The subscriber would apply the newer spans to a
    /// grid that is still the old size — where [`Grid::apply`] silently
    /// drops everything out of range — and then replace the lot with the
    /// older snapshot. The pump's `prev` has moved on either way, so those
    /// cells are never sent again and the pane keeps drawing text that is
    /// no longer on the screen it came from.
    pub fn broadcast_snapshot(&self, pane: PaneId) {
        publish_snapshot(&self.vt, &self.damage_tx, pane);
    }
}

/// Feeds the child's output to the emulator, logging each switch between
/// the normal and alternate screens with the bytes that made it. A
/// full-screen child that lands back on the normal screen has its frames
/// pile into the history and the wheel scroll that instead of the child;
/// the log is what tells whether the child asked for that or something
/// between it and the emulator did.
fn feed(vt: &mut Vt, chunk: &[u8], pane: PaneId) {
    let was = vt.alternate_screen();
    vt.process(chunk);
    let now = vt.alternate_screen();
    if was != now {
        let (rows, cols) = vt.size();
        let switched = if now { "entered" } else { "left" };
        tracing::info!(
            "pane {pane} {switched} the alternate screen at {rows}x{cols}, after: {}",
            switch_context(chunk)
        );
    }
}

/// The part of `chunk` worth reading after a screen switch: the bytes
/// around the last request that switches screens or resets the terminal,
/// or the tail when the switch came out of a synchronized update an
/// earlier chunk began. Escaped, so the log stays one line of text.
fn switch_context(chunk: &[u8]) -> String {
    const SWITCHES: &[&[u8]] = &[b"\x1b[?1049", b"\x1b[?1047", b"\x1b[?47", b"\x1bc"];
    const BEFORE: usize = 120;
    const AFTER: usize = 40;
    let at = SWITCHES
        .iter()
        .filter_map(|switch| chunk.windows(switch.len()).rposition(|w| w == *switch))
        .max();
    let (start, end) = match at {
        Some(at) => (at.saturating_sub(BEFORE), (at + AFTER).min(chunk.len())),
        None => (chunk.len().saturating_sub(BEFORE + AFTER), chunk.len()),
    };
    chunk[start..end].escape_ascii().to_string()
}

fn set_pty_terminal(command: &mut CommandBuilder) {
    let inherited = command.get_env("TERM").and_then(OsStr::to_str);
    if inherited.is_none() || inherited == Some("dumb") {
        // The daemon can be started without a terminal, but every pane still
        // owns a real PTY and needs a terminal description for full-screen
        // programs to select their normal capabilities.
        command.env("TERM", PTY_TERM);
    }
}

/// The body of [`PaneRuntime::broadcast_snapshot`], over the handles it
/// shares with the pump rather than over the runtime itself — which is not
/// `Sync`, and so cannot be handed to a thread that wants to prove the lock
/// is held for the whole of it.
fn publish_snapshot(
    vt: &Arc<StdMutex<Vt>>,
    damage_tx: &broadcast::Sender<ServerMsg>,
    pane: PaneId,
) {
    let vt = vt.lock().unwrap();
    let (rows, cols) = vt.size();
    let _ = damage_tx.send(ServerMsg::PaneRows {
        pane,
        rows,
        cols,
        runs: grid_runs(&vt.grid()),
        cursor: vt.cursor(),
        mouse: vt.mouse(),
        alternate_screen: vt.alternate_screen(),
        keyboard: vt.keyboard(),
    });
}

/// What the pump last sent its watchers, so the next frame carries only
/// the difference.
#[derive(Default)]
struct Sent {
    /// The grid the watchers hold, moved along by each frame's scroll and
    /// runs exactly as a watcher moves its own.
    grid: Option<Vec<Vec<Cell>>>,
    cursor: Option<Cursor>,
    /// Tracked alongside the grid because a child can turn mouse
    /// reporting on or off without changing a single cell, and a client
    /// that misses the change forwards mouse bytes to a child that will
    /// print them.
    mouse: Option<MouseTracking>,
    alternate_screen: Option<bool>,
    /// Tracked for the reason `mouse` is: an agent CLI pushes its keyboard
    /// flags once at start-up without drawing a cell, and a client that
    /// misses that keeps sending Shift+Enter as a plain Enter.
    keyboard: Option<KeyboardMode>,
}

/// Brings `live` up to the emulator's screen and sends the watchers what
/// they lack of it, when they lack anything.
fn send_frame(
    vt: &mut Vt,
    live: &mut Vec<Vec<Cell>>,
    sent: &mut Sent,
    damage_tx: &broadcast::Sender<ServerMsg>,
    pane: PaneId,
) {
    vt.refresh(live);
    let cursor = vt.cursor();
    let mouse = vt.mouse();
    let alternate_screen = vt.alternate_screen();
    let keyboard = vt.keyboard();
    let (scroll, runs) = damage(sent.grid.as_mut(), live);
    let unchanged = scroll.is_none()
        && runs.is_empty()
        && sent.cursor == Some(cursor)
        && sent.mouse == Some(mouse)
        && sent.alternate_screen == Some(alternate_screen)
        && sent.keyboard == Some(keyboard);
    if unchanged {
        return;
    }
    // After a resize the watchers' grid is replaced by a snapshot anyway;
    // runs laid over one of another shape would not rebuild this one.
    let same_shape = |grid: &Vec<Vec<Cell>>| {
        grid.len() == live.len() && grid.iter().zip(live.iter()).all(|(a, b)| a.len() == b.len())
    };
    match sent.grid.as_mut() {
        Some(grid) if same_shape(grid) => {
            for run in &runs {
                run.apply(grid);
            }
        }
        _ => sent.grid = Some(live.clone()),
    }
    sent.cursor = Some(cursor);
    sent.mouse = Some(mouse);
    sent.alternate_screen = Some(alternate_screen);
    sent.keyboard = Some(keyboard);
    let _ = damage_tx.send(ServerMsg::RowDamage {
        pane,
        scroll,
        runs,
        cursor,
        mouse,
        alternate_screen,
        keyboard,
    });
}

#[cfg(test)]
mod tests;
