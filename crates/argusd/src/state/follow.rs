//! Which checkout an agent pane belongs to: the one its agent is working
//! in. The row follows the agent — when it says it moved, and when its turn
//! ends in a directory inside another worktree of its repository — because
//! an agent that has to be told to report a move mostly never does.

use std::path::{Path, PathBuf};

use super::*;

impl Daemon {
    /// Moves the pane to the checkout `directory` is inside, in any
    /// repository of its project: `argus-hook checkout`, which an agent runs
    /// from wherever it now works — often a worktree it made a moment ago,
    /// before the poll that would have found it.
    pub(super) fn move_agent_into(&self, pane: PaneId, directory: &Path) -> anyhow::Result<()> {
        self.move_agent_into_with(pane, directory, crate::git::list_worktrees)
    }

    pub(super) fn move_agent_into_with(
        &self,
        pane: PaneId,
        directory: &Path,
        list: impl Fn(&Path) -> Vec<PathBuf>,
    ) -> anyhow::Result<()> {
        let destination = self
            .discovered_checkout_containing(pane, directory, Reach::Project, list)
            .ok_or_else(|| {
                anyhow::anyhow!("{} is not inside a checkout of this project", directory.display())
            })?;
        self.move_agent_to_checkout(pane, &destination)
    }

    /// Follows the pane's own agent into the worktree its turn ended in.
    ///
    /// Only a directory that changed since the last turn moves the row, so a
    /// person who moved it by hand is not overruled by an agent still
    /// sitting where it was. Only the agent's own repository counts: a turn
    /// that ends after reading another repository has not moved there.
    pub(super) fn follow_agent_directory(&self, pane: PaneId, reporter: Option<&str>, directory: &Path) {
        self.follow_agent_directory_with(pane, reporter, directory, crate::git::list_worktrees)
    }

    pub(super) fn follow_agent_directory_with(
        &self,
        pane: PaneId,
        reporter: Option<&str>,
        directory: &Path,
        list: impl Fn(&Path) -> Vec<PathBuf>,
    ) {
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
            self.discovered_checkout_containing(pane, directory, Reach::Repository, list)
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
        list: impl Fn(&Path) -> Vec<PathBuf>,
    ) -> Option<PathBuf> {
        self.reconcile_worktrees_with(list);
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
