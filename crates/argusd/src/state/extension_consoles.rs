//! The consoles extensions' views run: starting them, keeping them across a
//! republish, and ending them with the view or the extension.
//!
//! A console view's process is the daemon's, not the extension's: the person
//! types into its pane as into any console, so it lives in a pane list every
//! pty lookup reads. It is kept apart from `Inner::consoles` because it is
//! not one of the person's tabs and is not brought back by a restart: the
//! extension publishes its view again when it starts, and that starts it.
//!
//! Lock order is the registry, then `inner`. A start holds only `inner`, so a
//! process that exits at once finds its pane; an exit takes the two one after
//! the other, never together.

use std::path::{Path, PathBuf};

use argus_protocol::{ConsoleEnd, ExtConsole, ExtEvent, ExtScope, ExtView};

use super::*;

/// A pane running an extension's console view.
pub(super) struct HeldConsole {
    extension: String,
    view: String,
    pane: Pane,
}

impl HeldConsole {
    pub(super) fn pane(&self) -> &Pane {
        &self.pane
    }

    fn ended(&self) -> Option<ConsoleEnd> {
        match self.pane.status {
            PaneStatus::Exited { code } => Some(ConsoleEnd { code }),
            _ => None,
        }
    }
}

impl Daemon {
    /// Starts what each console view in `views` asks for, or keeps what
    /// already runs it, and writes the pane into the view. The panes it
    /// started are returned, for [`Self::abandon_consoles`] should the
    /// publish fail afterwards; the ones the views no longer name are ended
    /// only by [`Self::settle_consoles`], once the publish is certain.
    ///
    /// Refused as `views[i].console.cwd` when the directory is not there,
    /// with whatever it had started already taken back.
    pub(super) fn start_consoles(
        self: &Arc<Self>,
        name: &str,
        before: &[ExtView],
        views: &mut [ExtView],
    ) -> anyhow::Result<Vec<PaneId>> {
        let mut started = Vec::new();
        for (i, view) in views.iter_mut().enumerate() {
            let (scope, title, view_name) = (view.scope, view.title.clone(), view.name.clone());
            let Some(console) = view.console.as_mut() else { continue };
            let kept = before
                .iter()
                .find(|b| b.name == view_name)
                .and_then(|b| b.console.as_ref())
                .filter(|b| b.same_run(console) && b.ended.is_none())
                .and_then(|b| b.pane)
                .filter(|pane| self.console_running(name, &view_name, *pane));
            // What the extension sent in the daemon's fields is not read.
            console.pane = kept;
            console.ended = None;
            if kept.is_some() {
                continue;
            }
            let at = format!("views[{i}].console");
            let begun = self.console_dir(scope, console.cwd.as_deref(), &format!("{at}.cwd")).and_then(|dir| {
                self.start_console(name, &view_name, &title, console, &dir).map_err(|e| anyhow::anyhow!("{at}.command: {e}"))
            });
            match begun {
                Ok(pane) => {
                    console.pane = Some(pane);
                    started.push(pane);
                }
                Err(error) => {
                    self.abandon_consoles(&started);
                    return Err(error);
                }
            }
        }
        Ok(started)
    }

    /// Starts the process and files its pane in one hold of `inner`: a
    /// command that is over before this returns still finds its pane to
    /// mark exited.
    fn start_console(
        self: &Arc<Self>,
        name: &str,
        view: &str,
        title: &str,
        console: &ExtConsole,
        dir: &Path,
    ) -> anyhow::Result<PaneId> {
        let (program, args) = console.command.split_first().expect("checked to name a program");
        let mut inner = self.inner.lock().unwrap();
        let id = PaneId(inner.ids.alloc());
        let daemon = self.clone();
        let runtime = self
            .processes
            .start(
                id,
                dir,
                pty::Spawn::Program {
                    program: program.clone(),
                    args: args.to_vec(),
                    env: console.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                    // A dev server or a build starts as many processes as it
                    // likes. The agent policy's cap on them is for a CLI that
                    // has run away, not for what the person asked to see.
                    resource_policy: pty::ResourcePolicy::Unrestricted,
                },
                Box::new(move |code| daemon.mark_pane_exited(id, code)),
            )
            .map_err(|e| anyhow::anyhow!("could not start {program}: {e}"))?;
        let pane = Pane::new(id, PaneKind::Shell, title.to_string(), runtime);
        inner.extension_consoles.push(HeldConsole { extension: name.to_string(), view: view.to_string(), pane });
        Ok(id)
    }

    fn console_running(&self, name: &str, view: &str, pane: PaneId) -> bool {
        let inner = self.inner.lock().unwrap();
        let held = inner.extension_consoles.iter().find(|h| h.pane.id == pane);
        held.is_some_and(|h| h.extension == name && h.view == view && h.ended().is_none())
    }

    /// Where a console of a view in `scope` runs: `cwd` under the scope's
    /// root, which must be a directory and stay inside the root once links
    /// are followed. The path returned is the one named, not the resolved
    /// one, which on Windows carries a prefix a program may not take.
    fn console_dir(&self, scope: ExtScope, cwd: Option<&str>, at: &str) -> anyhow::Result<PathBuf> {
        let root = match scope {
            ExtScope::Global => crate::browse::home_dir(),
            ExtScope::Checkout(_) | ExtScope::Repository(_) => {
                let inner = self.inner.lock().unwrap();
                let found = match scope {
                    ExtScope::Checkout(id) => find_checkout_ref(&inner.projects, id).map(|c| c.path.clone()),
                    ExtScope::Repository(id) => {
                        let mut repositories = inner.projects.iter().flat_map(|p| p.repositories.iter());
                        repositories.find(|r| r.id == id).and_then(|r| r.primary_path()).map(Path::to_path_buf)
                    }
                    ExtScope::Global => None,
                };
                found.ok_or_else(|| anyhow::anyhow!("{at}: the view's checkout or repository is not known"))?
            }
        };
        let dir = match cwd {
            Some(cwd) => root.join(cwd),
            None => root.clone(),
        };
        let resolved = dir.canonicalize().map_err(|_| anyhow::anyhow!("{at}: {} is not a directory", dir.display()))?;
        anyhow::ensure!(resolved.is_dir(), "{at}: {} is not a directory", dir.display());
        let inside = root.canonicalize().is_ok_and(|root| resolved.starts_with(root));
        anyhow::ensure!(inside, "{at}: {} is outside the scope's root", dir.display());
        Ok(dir)
    }

    /// Ends panes a publish started and then did not use.
    pub(super) fn abandon_consoles(&self, panes: &[PaneId]) {
        self.end_consoles(|held| panes.contains(&held.pane.id));
    }

    /// Ends every console `name` runs, for an extension that is gone.
    pub(super) fn end_consoles_of(&self, name: &str) {
        self.end_consoles(|held| held.extension == name);
    }

    fn end_consoles(&self, ends: impl Fn(&HeldConsole) -> bool) {
        let gone = {
            let mut inner = self.inner.lock().unwrap();
            let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut inner.extension_consoles).into_iter().partition(|h| ends(h));
            inner.extension_consoles = kept;
            gone
        };
        for held in gone {
            self.retire_pane(held.pane);
        }
    }

    /// Makes the panes of `name` the ones `views` name, and reads into each
    /// view how its console stands. A command that ended before the publish
    /// landed found no view to record it in, so its ending is told to the
    /// extension here, on `events`, unless `old` already says it ended. Returns the panes no view names, to be
    /// retired by a caller that no longer holds the registry.
    pub(super) fn settle_consoles(
        &self,
        name: &str,
        old: &[ExtView],
        views: &mut [ExtView],
        events: Option<&tokio::sync::mpsc::UnboundedSender<ExtEvent>>,
    ) -> Vec<Pane> {
        let mut inner = self.inner.lock().unwrap();
        for view in views.iter_mut() {
            let Some(console) = view.console.as_mut() else { continue };
            let held = inner.extension_consoles.iter().find(|h| Some(h.pane.id) == console.pane);
            let ended = held.and_then(HeldConsole::ended);
            let told = old.iter().any(|o| o.console.as_ref().is_some_and(|c| c.pane == console.pane && c.ended.is_some()));
            if let (Some(end), Some(events), false) = (ended, events, told) {
                let _ = events.send(ExtEvent::ConsoleEnded { view: view.name.clone(), code: end.code });
            }
            console.ended = ended;
        }
        let named: Vec<PaneId> = views.iter().filter_map(|v| v.console.as_ref()?.pane).collect();
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut inner.extension_consoles)
            .into_iter()
            .partition(|h| h.extension == name && !named.contains(&h.pane.id));
        inner.extension_consoles = kept;
        gone.into_iter().map(|h| h.pane).collect()
    }

    /// Whether `pane` is an extension's console, and if so records that its
    /// process ended: on its view, which clients are told, and to the
    /// extension. The pane stays, its last screen with it, until the view is
    /// published again or goes.
    pub(super) fn extension_console_exited(&self, pane: PaneId, code: Option<i32>) -> bool {
        let (extension, view) = {
            let mut inner = self.inner.lock().unwrap();
            let Some(held) = inner.extension_consoles.iter_mut().find(|h| h.pane.id == pane) else {
                return false;
            };
            held.pane.status = PaneStatus::Exited { code };
            held.pane.status_since = std::time::SystemTime::now();
            (held.extension.clone(), held.view.clone())
        };
        self.record_console_end(&extension, &view, pane, code);
        true
    }

    /// Whose console `pane` is, and which of the extension's views it is.
    pub(super) fn extension_console_owner(&self, pane: PaneId) -> Option<(String, String)> {
        let inner = self.inner.lock().unwrap();
        let held = inner.extension_consoles.iter().find(|h| h.pane.id == pane)?;
        Some((held.extension.clone(), held.view.clone()))
    }

    /// The panes of `name`'s consoles, for a test.
    #[cfg(test)]
    pub(super) fn console_panes(&self, name: &str) -> Vec<PaneId> {
        let inner = self.inner.lock().unwrap();
        inner.extension_consoles.iter().filter(|h| h.extension == name).map(|h| h.pane.id).collect()
    }
}
