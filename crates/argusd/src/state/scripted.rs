//! A daemon whose pane processes and clock the test drives: a process
//! starts and runs until the test says it exits, with the code it says,
//! and time moves only when the test moves it. For the tests about what
//! the daemon does with an exit or a duration, which a real process makes
//! a race against the test. Its Git reads can be scripted too, and go to
//! the disk until they are.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use argus_protocol::{PaneId, ServerMsg};
use tokio::sync::broadcast;

use super::clock::Clock;
use super::git_reads::ScriptedGit;
use super::processes::{Keyboard, OnExit, Process, Processes};
use super::Daemon;
use crate::config::ConfigFile;
use crate::pty::{self, PaneSubscription};

/// A daemon over [`ScriptedProcesses`], [`ScriptedGit`] and a
/// [`ManualClock`]. Derefs to the daemon, so a test reads the same as one
/// over real processes.
pub(crate) struct ScriptedDaemon {
    daemon: Arc<Daemon>,
    pub processes: Arc<ScriptedProcesses>,
    pub(in crate::state) git: Arc<ScriptedGit>,
    pub clock: Arc<ManualClock>,
}

impl ScriptedDaemon {
    pub fn new(config: ConfigFile) -> Self {
        let store = crate::store::Store::in_memory()
            .expect("an in-memory runtime store needs nothing that can fail");
        Self::with_store(config, store)
    }

    pub fn with_store(config: ConfigFile, store: crate::store::Store) -> Self {
        let processes = Arc::new(ScriptedProcesses::default());
        let git = Arc::new(ScriptedGit::default());
        let clock = Arc::new(ManualClock::new());
        let daemon = Daemon::with_ports(
            config,
            store,
            git.clone(),
            processes.clone(),
            clock.clone(),
        );
        Self {
            daemon,
            processes,
            git,
            clock,
        }
    }

    /// Ends the pane's process with `code`, as its pty would.
    pub fn exit(&self, pane: PaneId, code: Option<i32>) {
        self.processes.exit(pane, code);
    }

    pub fn advance(&self, by: Duration) {
        self.clock.advance(by);
    }
}

impl std::ops::Deref for ScriptedDaemon {
    type Target = Arc<Daemon>;

    fn deref(&self) -> &Arc<Daemon> {
        &self.daemon
    }
}

/// What a scripted pane was started as.
pub(crate) struct Started {
    pub cwd: PathBuf,
    /// `None` for the user's shell.
    pub program: Option<String>,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

struct Script {
    started: Started,
    /// Taken by the one exit a process gets.
    on_exit: Option<OnExit>,
    written: Arc<Mutex<Vec<u8>>>,
    killed: Arc<AtomicBool>,
}

#[derive(Default)]
pub(crate) struct ScriptedProcesses {
    panes: Mutex<HashMap<PaneId, Script>>,
}

impl ScriptedProcesses {
    /// Runs the pane's exit callback, outside the lock: the daemon may
    /// start a replacement from inside it.
    pub fn exit(&self, pane: PaneId, code: Option<i32>) {
        let on_exit = self
            .panes
            .lock()
            .unwrap()
            .get_mut(&pane)
            .and_then(|script| script.on_exit.take());
        if let Some(on_exit) = on_exit {
            on_exit(code);
        }
    }

    pub fn started(&self, pane: PaneId) -> Option<Started> {
        let panes = self.panes.lock().unwrap();
        panes.get(&pane).map(|script| Started {
            cwd: script.started.cwd.clone(),
            program: script.started.program.clone(),
            args: script.started.args.clone(),
            env: script.started.env.clone(),
        })
    }

    /// Everything typed or pasted into the pane, as bytes.
    pub fn written(&self, pane: PaneId) -> Vec<u8> {
        let panes = self.panes.lock().unwrap();
        panes
            .get(&pane)
            .map(|script| script.written.lock().unwrap().clone())
            .unwrap_or_default()
    }

    pub fn killed(&self, pane: PaneId) -> bool {
        let panes = self.panes.lock().unwrap();
        panes
            .get(&pane)
            .is_some_and(|script| script.killed.load(Ordering::Relaxed))
    }
}

impl Processes for ScriptedProcesses {
    fn start(
        &self,
        id: PaneId,
        cwd: &std::path::Path,
        spec: pty::Spawn,
        on_exit: OnExit,
    ) -> anyhow::Result<Box<dyn Process>> {
        let (program, args, env) = match spec {
            pty::Spawn::DefaultShell => (None, Vec::new(), Vec::new()),
            pty::Spawn::Program {
                program, args, env, ..
            } => (Some(program), args, env),
        };
        let written = Arc::new(Mutex::new(Vec::new()));
        let killed = Arc::new(AtomicBool::new(false));
        self.panes.lock().unwrap().insert(
            id,
            Script {
                started: Started {
                    cwd: cwd.to_path_buf(),
                    program,
                    args,
                    env,
                },
                on_exit: Some(on_exit),
                written: written.clone(),
                killed: killed.clone(),
            },
        );
        let (damage_tx, _) = broadcast::channel(16);
        Ok(Box::new(ScriptedProcess {
            size: Mutex::new((24, 80)),
            keyboard: Arc::new(ScriptedKeyboard(written)),
            killed,
            damage_tx,
        }))
    }
}

struct ScriptedProcess {
    size: Mutex<(u16, u16)>,
    keyboard: Arc<ScriptedKeyboard>,
    killed: Arc<AtomicBool>,
    damage_tx: broadcast::Sender<ServerMsg>,
}

impl Process for ScriptedProcess {
    fn input(&self) -> Arc<dyn Keyboard> {
        self.keyboard.clone()
    }

    fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()> {
        *self.size.lock().unwrap() = (rows, cols);
        Ok(())
    }

    /// Recorded, not exited: what a killed pane announces lands on a tree
    /// it has already left, and the test decides whether to say it.
    fn kill(&self) -> anyhow::Result<()> {
        self.killed.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// A process that has printed nothing holds no history to scroll.
    fn scrollback(&self, _offset: usize, top: Option<u64>) -> pty::Scrolled {
        pty::Scrolled {
            cells: Vec::new(),
            offset: 0,
            depth: 0,
            top: top.unwrap_or(0),
        }
    }

    fn snapshot_and_subscribe(&self) -> PaneSubscription {
        let (rows, cols) = *self.size.lock().unwrap();
        (
            rows,
            cols,
            vec![vec![argus_protocol::Cell::default(); cols as usize]; rows as usize],
            argus_protocol::Cursor::default(),
            argus_protocol::MouseTracking::default(),
            false,
            argus_protocol::KeyboardMode::default(),
            self.damage_tx.subscribe(),
        )
    }

    fn broadcast_snapshot(&self, _pane: PaneId) {}
}

struct ScriptedKeyboard(Arc<Mutex<Vec<u8>>>);

impl Keyboard for ScriptedKeyboard {
    fn write(&self, bytes: &[u8]) -> anyhow::Result<()> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(())
    }

    fn paste(&self, bytes: &[u8]) -> anyhow::Result<()> {
        self.write(bytes)
    }

    /// A scripted program has enabled focus reporting, so the report is
    /// always written; the pty's own gate is tested in `pty`.
    fn focus(&self, focused: bool) -> anyhow::Result<()> {
        self.write(if focused { b"\x1b[I" } else { b"\x1b[O" })
    }
}

/// Starts at the moment it is made and moves only by [`Self::advance`].
pub(crate) struct ManualClock {
    start: Instant,
    offset: Mutex<Duration>,
}

impl ManualClock {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            offset: Mutex::new(Duration::ZERO),
        }
    }

    pub fn advance(&self, by: Duration) {
        *self.offset.lock().unwrap() += by;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.start + *self.offset.lock().unwrap()
    }
}
