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

use argus_protocol::{CheckoutId, PaneId, PaneKind, PaneStatus};

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
            || crate::paths::has_windows_drive_prefix(rel_path)
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
                resource_policy: pty::ResourcePolicy::Unrestricted,
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
            resource_policy: pty::ResourcePolicy::Agent,
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
        self.forget_pane_sizes(pane);
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
        self.pane_input(pane)
            .ok_or_else(|| anyhow::anyhow!("no such pane"))?
            .write(bytes)
    }

    pub fn paste_pane(&self, pane: PaneId, text: &str) -> anyhow::Result<()> {
        self.pane_input(pane)
            .ok_or_else(|| anyhow::anyhow!("no such pane"))?
            .paste(text.as_bytes())
    }

    /// A handle to a live pane's keyboard, held without the tree lock so it
    /// can outlast the lookup that found it.
    fn pane_input(&self, pane: PaneId) -> Option<pty::PaneInput> {
        let inner = self.inner.lock().unwrap();
        let pane = find_pane_ref(&inner.projects, pane)?;
        (!matches!(pane.status, PaneStatus::Exited { .. })).then(|| pane.runtime.input())
    }



}
