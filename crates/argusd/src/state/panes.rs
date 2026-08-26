//! Panes: starting them, ending them, sizing them, and everything an agent
//! running in one can say about itself.
//!
//! Shells, agents and editors are one primitive here — a pty and a row —
//! and they differ only in what gets spawned and who is allowed to rename
//! it. The status reporting lives with them rather than with the receiver
//! in `hook.rs` because deciding *whether* a report is allowed to land is
//! about the pane's own identity: which session owns the row, which ones
//! are merely running inside it, and whether the process is still alive.

use std::sync::Arc;
use std::time::Duration;

use argus_protocol::{CheckoutId, PaneId, PaneKind, PaneStatus, MAX_DELEGATE_TASK_BYTES};

use super::*;

impl Daemon {
    pub fn spawn_shell(self: &Arc<Self>, checkout: CheckoutId) -> anyhow::Result<PaneId> {
        let path = {
            let inner = self.inner.lock().unwrap();
            find_checkout_ref(&inner.projects, checkout)
                .map(|c| c.path.clone())
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?
        };

        let id = {
            let mut inner = self.inner.lock().unwrap();
            PaneId(inner.ids.alloc())
        };

        let daemon = self.clone();
        let runtime = PaneRuntime::spawn(id, &path, pty::Spawn::DefaultShell, move |code| {
            daemon.mark_pane_exited(id, code);
        })?;

        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(c) = find_checkout(&mut inner.projects, checkout) {
                c.panes.push(Pane {
                    id,
                    kind: PaneKind::Shell,
                    title: "shell".to_string(),
                    status: PaneStatus::Idle,
                    note: None,
                    template: None,
                    harness: None,
                    children: Vec::new(),
                    restore_status_reported: false,
                    restore_title_reported: false,
                    harness_session_id: None,
                    resumed: None,
                    runtime,
                });
            }
        }
        self.broadcast_tree();
        Ok(id)
    }

    /// Opens `rel_path` (repo-relative) in the user's editor as a pane.
    pub fn spawn_editor(
        self: &Arc<Self>,
        checkout: CheckoutId,
        rel_path: &str,
        line: Option<u32>,
        external: bool,
        command: Option<&str>,
    ) -> anyhow::Result<PaneId> {
        let path = self.checkout_path(checkout)?;
        // Rejected here rather than trusted: `path` is spawned into a
        // command line, and a client is not the authority on what is
        // inside the checkout. A leading separator is not `is_absolute` on
        // Windows, so it is checked by hand rather than left to the platform.
        if rel_path.is_empty()
            || rel_path.starts_with(['/', '\\'])
            || std::path::Path::new(rel_path).is_absolute()
            || has_windows_drive_prefix(rel_path)
            || rel_path.split(['/', '\\']).any(|c| c == "..")
        {
            anyhow::bail!("not a path inside the checkout: {rel_path}");
        }

        let editor = match command.map(str::trim).filter(|c| !c.is_empty()) {
            Some(c) => c.to_string(),
            None => crate::editor::resolve(),
        };
        let argv = crate::editor::command(&editor, rel_path, line);
        let (program, args) = argv.split_first().expect("never empty");

        // A GUI editor cannot live in a pty whatever the client asked for:
        // it would be a blank pane whose child never speaks, which is
        // indistinguishable from a hung one.
        if external || crate::editor::is_gui(&editor) {
            // No pty and no pane: this editor brings its own window, and
            // Argus has nothing to draw for it. Detached so closing the
            // daemon doesn't take the user's editor with it.
            crate::command::detached(program)
                .args(args)
                .current_dir(&path)
                .spawn()
                .map_err(|e| anyhow::anyhow!("could not start {program}: {e}"))?;
            return Ok(PaneId(0));
        }

        let id = {
            let mut inner = self.inner.lock().unwrap();
            PaneId(inner.ids.alloc())
        };
        let daemon = self.clone();
        let runtime = PaneRuntime::spawn(
            id,
            &path,
            pty::Spawn::Program {
                program: program.clone(),
                args: args.to_vec(),
                env: Vec::new(),
            },
            move |code| daemon.mark_pane_exited(id, code),
        )?;

        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(c) = find_checkout(&mut inner.projects, checkout) {
                c.panes.push(Pane {
                    id,
                    kind: PaneKind::Editor,
                    title: rel_path.rsplit('/').next().unwrap_or(rel_path).to_string(),
                    status: PaneStatus::Idle,
                    note: None,
                    template: None,
                    harness: None,
                    children: Vec::new(),
                    restore_status_reported: false,
                    restore_title_reported: false,
                    harness_session_id: None,
                    resumed: None,
                    runtime,
                });
            }
        }
        self.broadcast_tree();
        Ok(id)
    }

    /// Starts a new agent, and with it a new conversation. What the user
    /// gets from the template picker.
    pub fn spawn_agent(
        self: &Arc<Self>,
        checkout: CheckoutId,
        template_name: &str,
    ) -> anyhow::Result<PaneId> {
        self.start_agent(checkout, template_name, Start::Fresh, None)
    }

    /// Opens an independent agent pane beside `source_id` and queues one
    /// bounded, single-line task into its terminal.
    pub fn delegate_agent(
        self: &Arc<Self>,
        source_id: PaneId,
        template_name: Option<&str>,
        task: &str,
    ) -> anyhow::Result<PaneId> {
        const MAX_LIVE_AGENTS: usize = 4;

        let task = task.split_whitespace().collect::<Vec<_>>().join(" ");
        if task.is_empty() {
            anyhow::bail!("delegation requires a task");
        }
        if task.len() > MAX_DELEGATE_TASK_BYTES {
            anyhow::bail!("delegation task exceeds {MAX_DELEGATE_TASK_BYTES} bytes");
        }

        let _delegation = self.delegation.lock().unwrap();

        let (checkout_id, inherited_template) = {
            let inner = self.inner.lock().unwrap();
            let mut source = None;
            'projects: for project in &inner.projects {
                for repository in &project.repositories {
                    for checkout in &repository.checkouts {
                        if let Some(pane) = checkout.panes.iter().find(|pane| pane.id == source_id) {
                            source = Some((checkout, pane));
                            break 'projects;
                        }
                    }
                }
            }
            let (checkout, pane) = source.ok_or_else(|| anyhow::anyhow!("no such source pane"))?;
            if pane.kind != PaneKind::Agent || matches!(pane.status, PaneStatus::Exited { .. }) {
                anyhow::bail!("delegation source must be a live agent pane");
            }
            let live_agents = checkout
                .panes
                .iter()
                .filter(|pane| {
                    pane.kind == PaneKind::Agent
                        && !matches!(pane.status, PaneStatus::Exited { .. })
                })
                .count();
            if live_agents >= MAX_LIVE_AGENTS {
                anyhow::bail!("this checkout already has {MAX_LIVE_AGENTS} live agents");
            }
            (checkout.id, pane.template.clone())
        };

        let template_name = template_name
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .or(inherited_template)
            .ok_or_else(|| anyhow::anyhow!("source pane has no agent template"))?;
        let pane = self.spawn_agent(checkout_id, &template_name)?;
        if let Err(error) = self
            .paste_pane(pane, &task)
            .and_then(|_| self.write_pane(pane, b"\r"))
        {
            let _ = self.close_pane(pane);
            return Err(error.context("could not deliver delegated task"));
        }
        Ok(pane)
    }

    pub(super) fn start_agent(
        self: &Arc<Self>,
        checkout: CheckoutId,
        template_name: &str,
        start: Start,
        harness_session_id: Option<String>,
    ) -> anyhow::Result<PaneId> {
        let template = self
            .templates
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.name == template_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no such agent template: {template_name}"))?;
        let Some((program, rest)) = template.cmd.split_first() else {
            anyhow::bail!("agent template {template_name} has an empty cmd");
        };

        let path = {
            let inner = self.inner.lock().unwrap();
            // Only on a fresh start: a restore is replaying panes that were
            // already running together, and refusing half of them would
            // take work away rather than protect it.
            if start == Start::Fresh {
                if let Some(running) = exclusive_conflict(&inner.projects, checkout) {
                    anyhow::bail!(
                        "{running} is already working here, and this project allows one agent per checkout — make a worktree for another"
                    );
                }
            }
            find_checkout_ref(&inner.projects, checkout)
                .map(|c| c.path.clone())
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?
        };

        let id = {
            let mut inner = self.inner.lock().unwrap();
            PaneId(inner.ids.alloc())
        };
        self.starting_agents
            .lock()
            .unwrap()
            .insert(id, PendingStart::default());

        // Must land before the process starts: a harness reads its hook
        // config at its own startup, not on later file changes.
        let harness = self.harness_for(&template);
        let port = self.hook_port.load(std::sync::atomic::Ordering::Relaxed);
        if port != 0 {
            if let Err(e) = harness.install(&path, id, port, &self.hook_token) {
                tracing::warn!(
                    "failed to install {} hooks in {}: {e}",
                    harness.name,
                    path.display()
                );
            }
        }

        // The template's own env wins: a user who set one of these by hand
        // meant it.
        let mut env = crate::harness::env(id, port, &self.hook_token);
        env.retain(|(k, _)| !template.env.contains_key(k));
        env.extend(template.env.clone());

        let (args, resuming) = agent_args(
            rest,
            &harness.resume,
            &harness.resume_id,
            start,
            harness_session_id.as_deref(),
        );

        let spec = pty::Spawn::Program {
            program: program.clone(),
            args,
            env,
        };

        let daemon = self.clone();
        let runtime = match PaneRuntime::spawn(id, &path, spec, move |code| {
            daemon.mark_pane_exited(id, code);
        }) {
            Ok(runtime) => runtime,
            Err(error) => {
                self.starting_agents.lock().unwrap().remove(&id);
                return Err(error);
            }
        };

        {
            // Lock in this order everywhere that spans the pre-spawn mailbox
            // and pane tree, so an arriving hook cannot slip between them.
            let mut starting = self.starting_agents.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            let pending = starting.remove(&id).unwrap_or_default();
            let restore_status_reported = pending.status.is_some();
            let restore_title_reported = pending.title.is_some();
            if let Some(c) = find_checkout(&mut inner.projects, checkout) {
                c.panes.push(Pane {
                    id,
                    kind: PaneKind::Agent,
                    title: pending.title.unwrap_or_else(|| template.name.clone()),
                    status: pending
                        .status
                        .as_ref()
                        .map_or(PaneStatus::Idle, |(status, _)| *status),
                    note: pending.status.and_then(|(_, note)| note),
                    template: Some(template.name.clone()),
                    harness: Some(harness.name.to_string()),
                    children: pending.children,
                    restore_status_reported,
                    restore_title_reported,
                    harness_session_id: pending.harness_session_id.or(harness_session_id),
                    resumed: resuming.then(|| Resumed {
                        checkout,
                        template: template.name.clone(),
                        at: std::time::Instant::now(),
                    }),
                    runtime,
                });
            }
        }
        self.broadcast_tree();
        Ok(id)
    }

    pub fn mark_pane_exited(self: &Arc<Self>, pane: PaneId, code: Option<i32>) {
        let (retry, restart) = {
            let mut inner = self.inner.lock().unwrap();
            match find_pane_with_checkout(&mut inner.projects, pane) {
                Some((p, checkout)) => {
                    p.status = PaneStatus::Exited { code };
                    p.note = None;
                    p.children.clear();
                    let restart = p.template.clone().map(|template| (checkout, template));
                    (
                        p.resumed
                            .take()
                            .filter(|r| nothing_to_resume(code, r.at.elapsed())),
                        restart,
                    )
                }
                None => (None, None),
            }
        };

        if let Some(r) = retry {
            // The CLI has just told us there was no conversation to
            // continue. Take the dead row out rather than leaving it beside
            // its replacement, and give the user the agent they had.
            tracing::info!(
                "{} had nothing to resume in this checkout; starting it fresh",
                r.template
            );
            let _ = self.remove_pane(pane);
            if let Err(e) = self.start_agent(r.checkout, &r.template, Start::Fresh, None) {
                tracing::warn!("could not start {} after a failed resume: {e}", r.template);
            }
            return;
        }

        if let Some((checkout, template)) = restart {
            if self.restarts(&template, code, checkout) {
                let _ = self.remove_pane(pane);
                if let Err(e) = self.start_agent(checkout, &template, Start::Fresh, None) {
                    tracing::warn!("could not restart {template}: {e}");
                }
                return;
            }
        }

        self.broadcast_tree();
    }

    /// Whether an agent that just exited should be started again: its
    /// template's policy says so, and it is not looping.
    ///
    /// A CLI that dies immediately on every start would otherwise be
    /// restarted forever, spending the machine on a row nobody can read. A
    /// burst of restarts close together therefore gives up and leaves the
    /// exited row, which is the thing that tells the operator what happened.
    fn restarts(&self, template: &str, code: Option<i32>, checkout: CheckoutId) -> bool {
        /// How many times a template may restart in one checkout inside the
        /// window before Argus stops trying.
        const LIMIT: u32 = 3;
        /// Restarts further apart than this are somebody's long-running
        /// agent ending normally, not a loop.
        const WINDOW: Duration = Duration::from_secs(60);

        let wanted = self
            .templates
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.name == template)
            .map(|t| t.restart)
            .unwrap_or_default();
        let restart = match wanted {
            crate::config::Restart::Never => false,
            crate::config::Restart::OnFailure => code != Some(0),
            crate::config::Restart::Always => true,
        };
        if !restart {
            return false;
        }

        let mut attempts = self.restart_attempts.lock().unwrap();
        let key = (checkout, template.to_string());
        let now = std::time::Instant::now();
        let attempt = attempts.entry(key).or_insert((0, now));
        if now.duration_since(attempt.1) > WINDOW {
            *attempt = (0, now);
        }
        attempt.0 += 1;
        if attempt.0 > LIMIT {
            tracing::warn!(
                "{template} exited {} times in a row; leaving the row for you to read",
                attempt.0
            );
            return false;
        }
        true
    }

    /// Drops a pane from the tree without touching the checkout's managed
    /// hooks — for a pane being replaced in place, where an agent is about
    /// to take its seat and would only have to write them back.
    fn remove_pane(&self, pane: PaneId) -> Option<Pane> {
        let mut inner = self.inner.lock().unwrap();
        remove_pane_with_checkout(&mut inner.projects, pane).map(|(p, _)| p)
    }

    /// Kills the pane's process (best-effort — it may already have exited)
    /// and removes it from the tree entirely, so a closed pane actually
    /// disappears instead of lingering as a dead row the user can't clear.
    pub fn close_pane(&self, pane: PaneId) -> anyhow::Result<()> {
        let (removed, orphaned_checkout) = {
            let mut inner = self.inner.lock().unwrap();
            let taken = remove_pane_with_checkout(&mut inner.projects, pane);
            // Managed hooks belong to the checkout, not the pane, so they
            // come out only once the last agent there is gone — closing one
            // of two agent panes must not blind the other.
            let orphaned = taken
                .as_ref()
                .map(|(_, path)| path.clone())
                .filter(|path| !checkout_has_agent(&inner.projects, path));
            (taken.map(|(p, _)| p), orphaned)
        };
        let removed = removed.ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        {
            let mut viewers = self.viewers.lock().unwrap();
            viewers.wanted.remove(&pane);
            viewers.applied.remove(&pane);
        }
        let _ = removed.runtime.kill();
        if let Some(path) = orphaned_checkout {
            for h in &self.harnesses {
                if let Err(e) = h.uninstall(&path) {
                    tracing::warn!(
                        "failed to clear {} hooks in {}: {e}",
                        h.name,
                        path.display()
                    );
                }
            }
        }
        self.broadcast_tree();
        Ok(())
    }

    pub fn write_pane(&self, pane: PaneId, bytes: &[u8]) -> anyhow::Result<()> {
        let input = {
            let inner = self.inner.lock().unwrap();
            let pane = find_pane_ref(&inner.projects, pane)
                .ok_or_else(|| anyhow::anyhow!("no such pane"))?;
            pane.runtime.input()
        };
        input.write(bytes)
    }

    pub fn paste_pane(&self, pane: PaneId, text: &str) -> anyhow::Result<()> {
        let input = {
            let inner = self.inner.lock().unwrap();
            let pane = find_pane_ref(&inner.projects, pane)
                .ok_or_else(|| anyhow::anyhow!("no such pane"))?;
            pane.runtime.input()
        };
        input.paste(text.as_bytes())
    }

    /// Hands out the identity a connection uses to claim pane sizes.
    pub fn new_viewer(&self) -> ViewerId {
        ViewerId(
            self.next_viewer
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        )
    }

    /// Records one client's size for a pane and reconciles the pty against
    /// every client showing it.
    pub fn resize_pane(
        &self,
        viewer: ViewerId,
        pane: PaneId,
        rows: u16,
        cols: u16,
    ) -> anyhow::Result<()> {
        {
            let inner = self.inner.lock().unwrap();
            if find_pane_ref(&inner.projects, pane).is_none() {
                anyhow::bail!("no such pane");
            }
        }
        self.viewers
            .lock()
            .unwrap()
            .wanted
            .entry(pane)
            .or_default()
            .insert(viewer, (rows, cols));
        self.reconcile_size(pane)
    }

    /// Drops one client's claim on a pane it has stopped showing, letting
    /// the pane grow back to what the remaining clients can display.
    pub fn release_pane_size(&self, viewer: ViewerId, pane: PaneId) {
        {
            let mut viewers = self.viewers.lock().unwrap();
            let Some(wanted) = viewers.wanted.get_mut(&pane) else {
                return;
            };
            if wanted.remove(&viewer).is_none() {
                return;
            }
            if wanted.is_empty() {
                viewers.wanted.remove(&pane);
            }
        }
        let _ = self.reconcile_size(pane);
    }

    /// Drops every claim a disconnecting client held.
    pub fn release_viewer(&self, viewer: ViewerId) {
        let touched: Vec<PaneId> = {
            let viewers = self.viewers.lock().unwrap();
            viewers
                .wanted
                .iter()
                .filter(|(_, wanted)| wanted.contains_key(&viewer))
                .map(|(pane, _)| *pane)
                .collect()
        };
        for pane in touched {
            self.release_pane_size(viewer, pane);
        }
    }

    fn reconcile_size(&self, pane: PaneId) -> anyhow::Result<()> {
        let Some((rows, cols)) = self.viewers.lock().unwrap().pending(pane) else {
            return Ok(());
        };
        let inner = self.inner.lock().unwrap();
        let p =
            find_pane_ref(&inner.projects, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        p.runtime.resize(rows, cols)?;
        // Only once it took. Recording a size the pty rejected would make
        // every later request agreeing with it a no-op.
        self.viewers.lock().unwrap().applied.insert(pane, (rows, cols));
        // A subscribed client's cached grid is only ever sized by whatever
        // snapshot it last received; incremental Damage can't grow it.
        // Push a fresh full snapshot at the new size so growing a pane
        // (very common — new panes start at a hardcoded default far
        // smaller than most terminal heights) doesn't leave the newly
        // exposed area permanently blank.
        p.runtime.broadcast_snapshot(pane);
        Ok(())
    }

    pub fn subscribe_pane(&self, pane: PaneId) -> anyhow::Result<PaneSubscription> {
        let inner = self.inner.lock().unwrap();
        let p =
            find_pane_ref(&inner.projects, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        Ok(p.runtime.snapshot_and_subscribe())
    }

    /// Applies a hook-reported status, unless the pane has already exited —
    /// a hook firing after the process died (e.g. `Stop` racing a crash) is
    /// stale and shouldn't resurrect a dead pane's row.
    /// Applies a hook report to whichever agent sent it.
    ///
    /// Every report a harness makes carries the session it came from, so a
    /// CLI spawned inside a pane — which inherits the pane's hook URL and
    /// token and cannot be stopped from calling home — lands in that pane's
    /// child list instead of overwriting the row. The agent Argus started
    /// stays the authority on what the pane says.
    pub(super) fn report_pane_status(
        &self,
        pane: PaneId,
        reporter: Option<&str>,
        status: PaneStatus,
        note: Option<String>,
    ) {
        match self.child_of(pane, reporter) {
            Some(session) => self.set_child_status(pane, &session, status, note),
            None => self.set_pane_hook_status(pane, status, note),
        }
    }

    pub(super) fn report_pane_title(&self, pane: PaneId, reporter: Option<&str>, title: &str) {
        match self.child_of(pane, reporter) {
            Some(session) => self.set_child_label(pane, &session, title),
            None => self.set_pane_title(pane, title),
        }
    }

    /// The reporting session, when it is not the one that owns the pane.
    /// A report with no session at all is the pane's own: only a harness
    /// event carries one, and `argus-hook status` typed by hand has none.
    pub(super) fn child_of(&self, pane: PaneId, reporter: Option<&str>) -> Option<String> {
        let reporter = reporter?;
        // Match insertion's lock order so ownership cannot change between
        // checking the pre-spawn mailbox and checking the pane tree.
        let starting = self.starting_agents.lock().unwrap();
        let inner = self.inner.lock().unwrap();
        let owner = find_pane_ref(&inner.projects, pane)
            .and_then(|pane| pane.harness_session_id.as_deref())
            .or_else(|| {
                starting
                    .get(&pane)
                    .and_then(|pending| pending.harness_session_id.as_deref())
            })?;
        (owner != reporter).then(|| reporter.to_string())
    }

    fn with_child(&self, pane: PaneId, session: &str, edit: impl FnOnce(&mut ChildAgent)) {
        {
            let mut starting = self.starting_agents.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            let children = match find_pane(&mut inner.projects, pane) {
                Some(p) if matches!(p.status, PaneStatus::Exited { .. }) => return,
                Some(p) => &mut p.children,
                None => match starting.get_mut(&pane) {
                    Some(pending) => &mut pending.children,
                    None => return,
                },
            };
            match children.iter_mut().find(|c| c.session_id == session) {
                Some(child) => {
                    child.at = std::time::Instant::now();
                    edit(child)
                }
                None => {
                    let mut child = ChildAgent {
                        session_id: session.to_string(),
                        label: None,
                        status: PaneStatus::Working,
                        note: None,
                        at: std::time::Instant::now(),
                    };
                    edit(&mut child);
                    children.push(child);
                    if children.len() > MAX_CHILDREN {
                        children.remove(0);
                    }
                }
            }
            // A child that has gone idle is no longer something running
            // under this row, so it stops being listed under it.
            children
                .retain(|c| !matches!(c.status, PaneStatus::Idle | PaneStatus::Exited { .. }));
        }
        self.broadcast_tree();
    }

    fn set_child_status(
        &self,
        pane: PaneId,
        session: &str,
        status: PaneStatus,
        note: Option<String>,
    ) {
        let note = note.map(|n| clean_title(&n)).filter(|n| !n.is_empty());
        self.with_child(pane, session, |child| {
            child.status = status;
            child.note = note;
        });
    }

    fn set_child_label(&self, pane: PaneId, session: &str, title: &str) {
        let label = clean_title(title);
        if label.is_empty() {
            return;
        }
        self.with_child(pane, session, |child| child.label = Some(label));
    }

    /// Forgets children that have gone quiet. A child says so when it
    /// finishes and is cleared with its parent's turn either way, so this
    /// only catches the one that was killed, crashed, or lost its harness
    /// mid-turn — otherwise its row would sit there claiming to be working
    /// for as long as the parent kept going.
    pub(super) fn drop_silent_children(&self) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let mut changed = false;
            for p in all_panes(&mut inner.projects) {
                let before = p.children.len();
                p.children.retain(|c| c.at.elapsed() < CHILD_SILENCE);
                changed |= p.children.len() != before;
            }
            changed
        };
        if changed {
            self.broadcast_tree();
        }
    }

    pub(super) fn set_pane_hook_status(&self, pane: PaneId, status: PaneStatus, note: Option<String>) {
        let changed = {
            let mut starting = self.starting_agents.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) if !is_stale_report(p.status, status) => {
                    if self.restoring.load(std::sync::atomic::Ordering::Relaxed) {
                        p.restore_status_reported = true;
                    }
                    // A note explains one state; the report that leaves that
                    // state takes it away with it, so a stale "waiting for
                    // the db password" can't sit under a working row.
                    let note = note.map(|n| clean_title(&n)).filter(|n| !n.is_empty());
                    let mut changed = p.status != status || p.note != note;
                    p.status = status;
                    p.note = note;
                    // The turn that spawned them is over, so anything still
                    // listed under this row has finished without saying so.
                    // A background agent outliving the turn is not lost by
                    // this: its next report lists it again.
                    if status == PaneStatus::Idle && !p.children.is_empty() {
                        p.children.clear();
                        changed = true;
                    }
                    changed
                }
                Some(_) => false,
                None => match starting.get_mut(&pane) {
                    Some(pending)
                        if !is_stale_report(
                            pending
                                .status
                                .as_ref()
                                .map_or(PaneStatus::Idle, |(status, _)| *status),
                            status,
                        ) =>
                    {
                        let note = note.map(|n| clean_title(&n)).filter(|n| !n.is_empty());
                        let mut changed = pending.status.as_ref() != Some(&(status, note.clone()));
                        pending.status = Some((status, note));
                        if status == PaneStatus::Idle && !pending.children.is_empty() {
                            pending.children.clear();
                            changed = true;
                        }
                        changed
                    }
                    _ => false,
                },
            }
        };
        if changed {
            self.broadcast_tree();
        }
    }

    /// Renames a pane at the agent's own request (`argus-hook title ...`).
    ///
    /// A column of four rows all reading "claude" says nothing about which
    /// one is worth looking at; the agent knows what it is doing, so it is
    /// the one asked. Ignored for a pane that has exited — a rename racing
    /// a crash shouldn't relabel a dead row.
    pub(super) fn set_pane_title(&self, pane: PaneId, title: &str) {
        let title = clean_title(title);
        if title.is_empty() {
            return;
        }
        let changed = {
            let mut starting = self.starting_agents.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) if !matches!(p.status, PaneStatus::Exited { .. }) => {
                    if self.restoring.load(std::sync::atomic::Ordering::Relaxed) {
                        p.restore_title_reported = true;
                    }
                    if p.title != title {
                        p.title = title;
                        true
                    } else {
                        false
                    }
                }
                Some(_) => false,
                None => match starting.get_mut(&pane) {
                    Some(pending) if pending.title.as_deref() != Some(&title) => {
                        pending.title = Some(title);
                        true
                    }
                    _ => false,
                },
            }
        };
        if changed {
            self.broadcast_tree();
        }
    }

    /// Records the conversation Argus would resume this pane with.
    ///
    /// A claim from a session that is not the pane's current owner is only
    /// honoured while the pane is not working: the pane's own agent starting
    /// over (`/clear`, a resume) is idle at that moment, whereas a CLI
    /// spawned from inside a turn arrives mid-work. The latter is listed as
    /// a child instead, which is what keeps a nested agent from stealing the
    /// identity the row resumes from.
    pub(super) fn set_pane_session_id(&self, pane: PaneId, raw: &str) {
        let Some(session_id) = valid_session_id(raw) else {
            return;
        };
        if self.child_of(pane, Some(&session_id)).is_some() {
            let working = {
                let starting = self.starting_agents.lock().unwrap();
                let inner = self.inner.lock().unwrap();
                find_pane_ref(&inner.projects, pane)
                    .map(|p| p.status)
                    .or_else(|| {
                        starting
                            .get(&pane)
                            .and_then(|pending| pending.status.as_ref().map(|(status, _)| *status))
                    })
                    == Some(PaneStatus::Working)
            };
            if working {
                self.with_child(pane, &session_id, |_| {});
                return;
            }
        }
        let changed = {
            let mut starting = self.starting_agents.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p)
                    if !matches!(p.status, PaneStatus::Exited { .. })
                        && p.harness_session_id.as_deref() != Some(&session_id) =>
                {
                    p.harness_session_id = Some(session_id);
                    p.children.clear();
                    true
                }
                Some(_) => false,
                None => match starting.get_mut(&pane) {
                    Some(pending) if pending.harness_session_id.as_deref() != Some(&session_id) => {
                        pending.harness_session_id = Some(session_id);
                        pending.children.clear();
                        true
                    }
                    _ => false,
                },
            }
        };
        if changed {
            self.broadcast_tree();
        }
    }

    /// Moves a live agent row to the known checkout it has started working
    /// in. The PTY stays intact; this changes Argus's affiliation, not the
    /// child process's working directory. The reporting command runs in the
    /// destination directory, which is the evidence that the agent moved.
    pub(super) fn move_agent_to_checkout(
        &self,
        pane: PaneId,
        destination: &std::path::Path,
    ) -> anyhow::Result<()> {
        let (source_path, target_path, template, source_has_agent) = {
            let mut inner = self.inner.lock().unwrap();
            let (project_index, source_repository_index, source_index, pane_index) = inner
                .projects
                .iter()
                .enumerate()
                .find_map(|(project_index, project)| {
                    project.repositories.iter().enumerate().find_map(
                        |(repository_index, repository)| {
                            repository.checkouts.iter().enumerate().find_map(
                                |(checkout_index, checkout)| {
                                    checkout
                                        .panes
                                        .iter()
                                        .position(|candidate| candidate.id == pane)
                                        .map(|pane_index| {
                                            (
                                                project_index,
                                                repository_index,
                                                checkout_index,
                                                pane_index,
                                            )
                                        })
                                },
                            )
                        },
                    )
                })
                .ok_or_else(|| anyhow::anyhow!("no such pane"))?;

            let project = &mut inner.projects[project_index];
            let (target_repository_index, target_index) = project
                .repositories
                .iter()
                .enumerate()
                .find_map(|(repository_index, repository)| {
                    repository
                        .checkouts
                        .iter()
                        .position(|checkout| same_path(&checkout.path, destination))
                        .map(|checkout_index| (repository_index, checkout_index))
                })
                .ok_or_else(|| anyhow::anyhow!("destination is not a checkout in this project"))?;
            if source_repository_index == target_repository_index && source_index == target_index {
                return Ok(());
            }

            let moving = &project.repositories[source_repository_index].checkouts[source_index]
                .panes[pane_index];
            if moving.kind != PaneKind::Agent {
                anyhow::bail!("only agent panes can change checkout affiliation");
            }
            if matches!(moving.status, PaneStatus::Exited { .. }) {
                anyhow::bail!("an exited pane cannot change checkout affiliation");
            }

            let source_path = project.repositories[source_repository_index].checkouts[source_index]
                .path
                .clone();
            let target_path = project.repositories[target_repository_index].checkouts[target_index]
                .path
                .clone();
            let moving = project.repositories[source_repository_index].checkouts[source_index]
                .panes
                .remove(pane_index);
            let template = moving.template.clone();
            project.repositories[target_repository_index].checkouts[target_index]
                .panes
                .push(moving);
            let source_has_agent = project.repositories[source_repository_index].checkouts
                [source_index]
                .panes
                .iter()
                .any(|candidate| candidate.kind == PaneKind::Agent);
            (source_path, target_path, template, source_has_agent)
        };

        if !source_has_agent {
            for harness in &self.harnesses {
                if let Err(error) = harness.uninstall(&source_path) {
                    tracing::warn!(
                        "failed to clear {} hooks in {}: {error}",
                        harness.name,
                        source_path.display()
                    );
                }
            }
        }

        let found = template.as_deref().and_then(|name| {
            self.templates
                .lock()
                .unwrap()
                .iter()
                .find(|template| template.name == name)
                .cloned()
        });
        if let Some(template) = found.as_ref() {
            let harness = self.harness_for(template);
            let port = self.hook_port.load(std::sync::atomic::Ordering::Relaxed);
            if port != 0 {
                if let Err(error) = harness.install(&target_path, pane, port, &self.hook_token) {
                    tracing::warn!(
                        "failed to install {} hooks in {}: {error}",
                        harness.name,
                        target_path.display()
                    );
                }
            }
        }

        self.broadcast_tree();
        Ok(())
    }
}
