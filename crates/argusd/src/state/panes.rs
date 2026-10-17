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
        let runtime = self.processes.start(
            id,
            &path,
            pty::Spawn::DefaultShell,
            Box::new(move |code| daemon.mark_pane_exited(id, code)),
        )?;

        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(c) = find_checkout(&mut inner.projects, checkout) {
                c.panes.push(Pane::new(id, PaneKind::Shell, "shell".to_string(), runtime));
            }
        }
        self.broadcast_tree();
        Ok(id)
    }

    /// A shell in the home directory that belongs to no checkout.
    pub fn spawn_console(self: &Arc<Self>) -> anyhow::Result<PaneId> {
        let id = PaneId(self.inner.lock().unwrap().ids.alloc());
        let daemon = self.clone();
        let runtime = self.processes.start(
            id,
            &crate::browse::home_dir(),
            pty::Spawn::DefaultShell,
            Box::new(move |code| daemon.mark_pane_exited(id, code)),
        )?;
        self.inner
            .lock()
            .unwrap()
            .consoles
            .push(Pane::new(id, PaneKind::Shell, "shell".to_string(), runtime));
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
        // inside the checkout. The text is judged by the rule the daemon
        // applied to the view the path came from; a link inside the
        // checkout that leads out of it only shows on disk.
        if !argus_protocol::is_checkout_relative(rel_path)
            || !crate::system::paths::stays_inside(&path, rel_path)
        {
            anyhow::bail!("not a path inside the checkout: {rel_path}");
        }

        let editor = match command.map(str::trim).filter(|c| !c.is_empty()) {
            Some(c) => c.to_string(),
            None => crate::system::editor::resolve(),
        };
        let argv = crate::system::editor::command(&editor, rel_path, line);
        let (program, args) = argv.split_first().expect("never empty");

        // A GUI editor cannot live in a pty whatever the client asked for:
        // it would be a blank pane whose child never speaks, which is
        // indistinguishable from a hung one.
        if external || crate::system::editor::is_gui(&editor) {
            // No pty and no pane: this editor brings its own window, and
            // Argus has nothing to draw for it. Detached so closing the
            // daemon doesn't take the user's editor with it.
            crate::system::command::detached(program)
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
        let runtime = self.processes.start(
            id,
            &path,
            pty::Spawn::Program {
                program: program.clone(),
                args: args.to_vec(),
                env: Vec::new(),
                resource_policy: pty::ResourcePolicy::Unrestricted,
            },
            Box::new(move |code| daemon.mark_pane_exited(id, code)),
        )?;

        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(c) = find_checkout(&mut inner.projects, checkout) {
                let title = rel_path.rsplit('/').next().unwrap_or(rel_path).to_string();
                c.panes.push(Pane::new(id, PaneKind::Editor, title, runtime));
            }
        }
        self.broadcast_tree();
        Ok(id)
    }

    /// Starts a new agent, and with it a new conversation.
    #[cfg(test)]
    pub fn spawn_agent(
        self: &Arc<Self>,
        checkout: CheckoutId,
        template_name: &str,
    ) -> anyhow::Result<PaneId> {
        self.spawn_agent_asking(checkout, template_name, None)
    }

    /// Starts a new agent, and with it a new conversation, opening with
    /// `prompt` when there is one. What the template picker and the desktop
    /// window's composer ask for.
    pub fn spawn_agent_asking(
        self: &Arc<Self>,
        checkout: CheckoutId,
        template_name: &str,
        prompt: Option<&str>,
    ) -> anyhow::Result<PaneId> {
        self.start_agent(checkout, template_name, Start::Fresh, None, prompt)
    }

    pub(super) fn start_agent(
        self: &Arc<Self>,
        checkout: CheckoutId,
        template_name: &str,
        start: Start,
        harness_session_id: Option<String>,
        prompt: Option<&str>,
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
        // Before anything is started or installed: a refused message leaves
        // nothing behind.
        let opening = match prompt {
            Some(prompt) => first_message(&self.harness_for(&template).first_message, prompt)
                .ok_or_else(|| anyhow::anyhow!("{template_name} cannot be started with a message"))?,
            None => Vec::new(),
        };

        let path = {
            let inner = self.inner.lock().unwrap();
            // Not on a restore: it is replaying panes that were already
            // running together, and refusing half of them would take work
            // away rather than protect it.
            if start != Start::Resuming {
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
        if let Some((_, text)) = env.iter_mut().find(|(key, _)| key == argus_protocol::INSTRUCTIONS_VAR) {
            *text = harness.instructions(&path);
        }
        env.retain(|(k, _)| !template.env.contains_key(k));
        env.extend(template.env.clone());
        if let Some(entry) = self.plugin_dir_env(&harness, &env) {
            env.retain(|(k, _)| *k != entry.0);
            env.push(entry);
        }

        let (mut args, resuming) = agent_args(
            rest,
            &harness.resume,
            &harness.resume_id,
            start,
            harness_session_id.as_deref(),
        );
        args.extend(opening);

        let spec = pty::Spawn::Program {
            program: program.clone(),
            args,
            env,
            resource_policy: pty::ResourcePolicy::Agent,
        };

        let daemon = self.clone();
        let runtime = match self.processes.start(
            id,
            &path,
            spec,
            Box::new(move |code| daemon.mark_pane_exited(id, code)),
        ) {
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
            if let Some(c) = find_checkout(&mut inner.projects, checkout) {
                let restore_title_reported = pending.title.is_some();
                let title = pending.title.unwrap_or_else(|| template.name.clone());
                let mut pane = Pane::new(id, PaneKind::Agent, title, runtime);
                pane.restore_status_reported = pending.status.is_some();
                pane.restore_title_reported = restore_title_reported;
                if let Some((status, note)) = pending.status {
                    pane.status = status;
                    pane.note = note;
                }
                pane.template = Some(template.name.clone());
                pane.harness = Some(harness.name.to_string());
                pane.harness_session_id = pending.harness_session_id.or(harness_session_id);
                pane.children = pending.children;
                pane.resumed = (resuming && start == Start::Resuming).then(|| Resumed {
                    checkout,
                    template: template.name.clone(),
                    at: self.clock.now(),
                });
                c.panes.push(pane);
            }
        }
        self.broadcast_tree();
        Ok(id)
    }

    /// Names the folder harness plugin folders are written under. Once, at
    /// startup; a daemon that never names one starts no pane with a plugin.
    pub fn set_plugin_root(&self, root: std::path::PathBuf) {
        let _ = self.plugin_root.set(root);
    }

    /// The variable pointing an agent at its harness's plugin folder, which
    /// is written first, with whatever folders the pane's environment, or
    /// else the daemon's, already named ahead of it. A folder that cannot be
    /// written costs the agent its plugin, not its start.
    pub(super) fn plugin_dir_env(
        &self,
        harness: &crate::harness::Harness,
        env: &[(String, String)],
    ) -> Option<(String, String)> {
        let plugin = harness.plugin_dir.as_ref()?;
        let root = self.plugin_root.get()?;
        let inherited = env
            .iter()
            .find(|(key, _)| key == plugin.var)
            .map(|(_, value)| value.into())
            .or_else(|| std::env::var_os(plugin.var));
        plugin
            .prepare(root, inherited)
            .inspect_err(|error| tracing::warn!("could not write the {} plugin: {error}", harness.name))
            .ok()
    }

    pub fn mark_pane_exited(self: &Arc<Self>, pane: PaneId, code: Option<i32>) {
        if self.extension_console_exited(pane, code) {
            return;
        }
        let (retry, restart) = {
            let mut inner = self.inner.lock().unwrap();
            match find_pane_with_checkout(&mut inner.projects, pane) {
                Some((p, checkout)) => {
                    p.status = PaneStatus::Exited { code };
                    p.status_since = std::time::SystemTime::now();
                    p.note = None;
                    p.children.clear();
                    p.background = false;
                    let restart = p.template.clone().map(|template| (checkout, template));
                    (
                        p.resumed
                            .take()
                            .filter(|r| {
                                nothing_to_resume(code, self.clock.now().saturating_duration_since(r.at))
                            }),
                        restart,
                    )
                }
                // A console is a shell: it ends as its row says, with nothing
                // to resume or restart.
                None => {
                    if let Some(p) = inner.consoles.iter_mut().find(|p| p.id == pane) {
                        p.status = PaneStatus::Exited { code };
                        p.status_since = std::time::SystemTime::now();
                    }
                    (None, None)
                }
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
            self.remove_pane(pane);
            if let Err(e) = self.start_agent(r.checkout, &r.template, Start::Fresh, None, None) {
                tracing::warn!("could not start {} after a failed resume: {e}", r.template);
            }
            return;
        }

        if let Some((checkout, template)) = restart {
            if self.restarts(&template, code, checkout) {
                self.remove_pane(pane);
                if let Err(e) = self.start_agent(checkout, &template, Start::Fresh, None, None) {
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
        let now = self.clock.now();
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
    fn remove_pane(&self, pane: PaneId) {
        let removed = {
            let mut inner = self.inner.lock().unwrap();
            remove_pane_with_checkout(&mut inner.projects, pane).map(|(p, _)| p)
        };
        if let Some(removed) = removed {
            self.retire_pane(removed);
        }
    }

    /// Everything a pane leaves behind outside the tree, let go of once it
    /// has been taken out. Every removal comes through here, so a table
    /// keyed by pane cannot be forgotten by one path and not another. What
    /// the pane holds itself, its inbox among them, ends as it is dropped
    /// here.
    ///
    /// Called without `inner` held: each table has its own lock. The
    /// checkout's managed hooks are not a pane's and stay with the caller;
    /// only `close_pane` takes them out.
    pub(super) fn retire_pane(&self, pane: Pane) {
        self.forget_pane_sizes(pane.id);
        self.drop_views_of(pane.id);
        if let Some(root) = self.paste_root.get() {
            crate::system::pastes::forget(root, pane.id);
        }
        // Best-effort: a pane retired because it exited has nothing left
        // to kill, but whatever it started in its session may still run.
        let _ = pane.runtime.kill();
    }

    /// Kills the pane's process (best-effort — it may already have exited)
    /// and removes it from the tree entirely, so a closed pane actually
    /// disappears instead of lingering as a dead row the user can't clear.
    pub fn close_pane(&self, pane: PaneId) -> anyhow::Result<()> {
        // Refused, not ignored, so the person is told why nothing closed:
        // the extension owns the console's life, and it ends with the view.
        if let Some((extension, view)) = self.extension_console_owner(pane) {
            anyhow::bail!("this console is {extension}'s view {view}; it ends when the view does");
        }
        let (removed, orphaned_checkout, used) = {
            let mut inner = self.inner.lock().unwrap();
            // Closing is what makes a repository quiet, so its time is when
            // it was last used; a replacement in place never comes here.
            let used = repository_path_of_pane(&inner.projects, pane).map(|path| {
                let at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                inner.used.insert(path.clone(), at);
                (path, at)
            });
            let taken = remove_pane_with_checkout(&mut inner.projects, pane);
            // Managed hooks belong to the checkout, not the pane, so they
            // come out only once the last agent there is gone — closing one
            // of two agent panes must not blind the other.
            let orphaned = taken
                .as_ref()
                .map(|(_, path)| path.clone())
                .filter(|path| !checkout_has_agent(&inner.projects, path));
            let removed = match taken {
                Some((p, _)) => Some(p),
                None => {
                    let at = inner.consoles.iter().position(|p| p.id == pane);
                    at.map(|at| inner.consoles.remove(at))
                }
            };
            (removed, orphaned, used)
        };
        let removed = removed.ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        self.retire_pane(removed);
        if let Some((path, at)) = used {
            if let Err(e) = self.store.set_repository_used(&path, at) {
                tracing::warn!("could not record {} as used: {e}", path.display());
            }
        }
        // Only here: a restart or failed resume has an agent about to take
        // the seat, which would write the hooks straight back, and a
        // vanished worktree has no directory to take them out of.
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

    /// Names the folder pasted files are kept under. Once, at startup, and
    /// cleared then: whatever an earlier daemon's panes were pasted, they
    /// read at the time, and their processes do not survive it.
    pub fn set_paste_root(&self, root: std::path::PathBuf) {
        crate::system::pastes::forget_all(&root);
        let _ = self.paste_root.set(root);
    }

    /// Keeps a file from a client's machine where this pane's process can
    /// open it, and pastes the path. The path comes back so a test, or a
    /// caller with somewhere to show it, can say where it went.
    pub fn paste_file(
        &self,
        pane: PaneId,
        name: &str,
        bytes: &[u8],
    ) -> anyhow::Result<std::path::PathBuf> {
        let input = self
            .pane_input(pane)
            .ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        let root = self
            .paste_root
            .get()
            .ok_or_else(|| anyhow::anyhow!("this daemon keeps no pasted files"))?;
        let path = crate::system::pastes::keep(root, pane, name, bytes)?;
        input.paste(crate::system::pastes::dropped(&path).as_bytes())?;
        Ok(path)
    }

    /// A handle to a live pane's keyboard, held without the tree lock so it
    /// can outlast the lookup that found it.
    pub(super) fn pane_input(&self, pane: PaneId) -> Option<Arc<dyn processes::Keyboard>> {
        let inner = self.inner.lock().unwrap();
        let pane = find_any_pane_ref(&inner, pane)?;
        (!matches!(pane.status, PaneStatus::Exited { .. })).then(|| pane.runtime.input())
    }



}
