//! Which checkout a pane belongs to: for an agent, the one it is working
//! in. The row follows the agent — when it says it moved, and when its turn
//! ends in a directory inside another worktree of its repository — because
//! an agent that has to be told to report a move mostly never does. A live
//! pane whose worktree is removed outside Argus belongs to the primary
//! checkout from then on.

use std::path::{Path, PathBuf};

use argus_protocol::Report;

use super::*;

impl Daemon {
    /// Moves the pane to the checkout `directory` is inside, in any
    /// repository of its project: `argus-hook checkout`, which an agent runs
    /// from wherever it now works — often a worktree it made a moment ago,
    /// before the poll that would have found it.
    ///
    /// A CLI started inside the pane is not its owner, and its ask is
    /// dropped rather than refused: the row follows the agent Argus started
    /// in it, and the child has nothing to correct.
    pub(super) fn move_agent_into(
        &self,
        pane: PaneId,
        reporter: Option<&str>,
        directory: &Path,
    ) -> anyhow::Result<()> {
        if self.child_of(pane, reporter).is_some() {
            return Ok(());
        }
        let destination = self
            .discovered_checkout_containing(pane, directory, Reach::Project)
            .ok_or_else(|| {
                anyhow::anyhow!("{} is not inside a checkout of this project", directory.display())
            })?;
        self.move_agent_to_checkout(pane, &destination)
    }

    /// Follows the pane's own agent into the worktree a status report says
    /// it is in, when that report is the end of a turn.
    ///
    /// A turn's end, not its tool calls: an agent that steps into another
    /// worktree to look at something has not moved there. Only a directory
    /// that changed since the last turn moves the row, so a person who moved
    /// it by hand is not overruled by an agent still sitting where it was.
    /// Only the agent's own repository counts: a turn that ends after
    /// reading another repository has not moved there.
    pub(super) fn follow_status_report(
        &self,
        pane: PaneId,
        reporter: Option<&str>,
        report: Report,
        directory: Option<&Path>,
    ) {
        let (Report::Idle, Some(directory)) = (report, directory) else {
            return;
        };
        if self.child_of(pane, reporter).is_some() {
            return;
        }
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) if p.directory.as_deref() != Some(directory) => {
                    p.directory = Some(directory.to_path_buf());
                    true
                }
                _ => false,
            }
        };
        if !changed {
            return;
        }
        let Some(destination) =
            self.discovered_checkout_containing(pane, directory, Reach::Repository)
        else {
            return;
        };
        if let Err(error) = self.move_agent_to_checkout(pane, &destination) {
            tracing::warn!("pane {} could not follow its agent: {error}", pane.0);
        }
    }

    /// The checkout `directory` is in, after looking at the repositories'
    /// worktrees again. Always, not only when no known checkout holds it:
    /// a worktree made under `.argus/worktrees` is inside the primary
    /// checkout, which would otherwise answer for it.
    fn discovered_checkout_containing(
        &self,
        pane: PaneId,
        directory: &Path,
        reach: Reach,
    ) -> Option<PathBuf> {
        self.reconcile_worktrees();
        let inner = self.inner.lock().unwrap();
        checkout_containing(&inner.projects, pane, directory, reach)
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

        self.install_hooks_for(pane, template.as_deref(), &target_path);
        self.broadcast_tree();
        Ok(())
    }

    /// Writes the managed hooks of the pane's template into the checkout it
    /// now belongs to, so its next report reaches the daemon from there.
    fn install_hooks_for(&self, pane: PaneId, template: Option<&str>, target: &Path) {
        let found = template.and_then(|name| {
            self.templates
                .lock()
                .unwrap()
                .iter()
                .find(|template| template.name == name)
                .cloned()
        });
        let Some(template) = found else {
            return;
        };
        let harness = self.harness_for(&template);
        let port = self.hook_port.load(std::sync::atomic::Ordering::Relaxed);
        if port == 0 {
            return;
        }
        if let Err(error) = harness.install(target, pane, port, &self.hook_token) {
            tracing::warn!(
                "failed to install {} hooks in {}: {error}",
                harness.name,
                target.display()
            );
        }
    }

    /// Gives the agents [`adopt_orphans`] moved the hooks of their new
    /// checkout. Nothing is taken out of the old one: it is gone.
    pub(super) fn settle_adopted(&self, adopted: Vec<Adopted>) {
        for agent in adopted {
            self.install_hooks_for(agent.pane, agent.template.as_deref(), &agent.home);
        }
    }
}

/// An agent pane [`adopt_orphans`] moved, and where to.
pub(super) struct Adopted {
    pane: PaneId,
    template: Option<String>,
    home: PathBuf,
}

/// Takes the panes of a linked worktree Git no longer lists: a pane whose
/// process still runs moves to the repository's primary checkout, and one
/// that has exited is handed back to be retired.
///
/// Removing the worktree is the usual last step of finishing a branch, and
/// the agent that finished it is usually standing in it — often it ran the
/// `git worktree remove` itself. Killing its pane for that ended the
/// session mid-command. The process keeps running in a directory that no
/// longer exists, so the primary checkout is not where it works, only the
/// nearest row that still is a checkout: the person can see it, read what
/// it said, and close it when they choose. Shells move as well, for the
/// same reason.
pub(super) fn adopt_orphans(repository: &mut Repository, gone: Checkout) -> (Vec<Pane>, Vec<Adopted>) {
    let Some(home) = repository.checkouts.iter_mut().find(|c| c.primary) else {
        return (gone.panes, Vec::new());
    };
    let mut retired = Vec::new();
    let mut adopted = Vec::new();
    for pane in gone.panes {
        if matches!(pane.status, PaneStatus::Exited { .. }) {
            retired.push(pane);
            continue;
        }
        if pane.kind == PaneKind::Agent {
            adopted.push(Adopted {
                pane: pane.id,
                template: pane.template.clone(),
                home: home.path.clone(),
            });
        }
        home.panes.push(pane);
    }
    (retired, adopted)
}

/// How far from its own checkout a pane may be moved.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    Repository,
    Project,
}

/// The innermost checkout holding `directory` — innermost because Argus
/// puts worktrees inside the primary checkout, under `.argus/worktrees`.
fn checkout_containing(
    projects: &[Project],
    pane: PaneId,
    directory: &Path,
    reach: Reach,
) -> Option<PathBuf> {
    let holds = |r: &Repository| r.checkouts.iter().any(|c| c.panes.iter().any(|p| p.id == pane));
    let project = projects.iter().find(|p| p.repositories.iter().any(holds))?;
    let directory = canonical(directory);
    project
        .repositories
        .iter()
        .filter(|r| reach == Reach::Project || holds(r))
        .flat_map(|r| r.checkouts.iter())
        .map(|c| (c, canonical(&c.path)))
        .filter(|(_, path)| directory.starts_with(path))
        .max_by_key(|(_, path)| path.components().count())
        .map(|(c, _)| c.path.clone())
}

/// A path as the disk spells it, so a directory reported through a symlink
/// or in another case on Windows still lands in its checkout. One that does
/// not exist is compared as written.
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
