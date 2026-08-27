//! Everything that changes a repository: branch switches, worktrees,
//! fetch and pull.
//!
//! These are the daemon's only writes to Git, and unlike the read-only
//! observation in `git.rs` they shell out to the `git` executable — a
//! refusal from git is the message the user should see, and reproducing
//! its rules in `git2` calls would only be a second place to get them
//! wrong. Each runs off the daemon lock and reports its own failure.

use std::path::PathBuf;
use std::sync::Arc;

use argus_protocol::CheckoutId;

use super::*;

impl Daemon {
    /// Moves this checkout onto an existing branch. `git` refuses when the
    /// switch would clobber uncommitted work, and that refusal is exactly
    /// what should reach the user, so its stderr is passed through.
    ///
    /// Argus refuses one case git allows: switching a *dirty primary*
    /// checkout (TARGET.md §Repository and checkout model). Git carries
    /// uncommitted changes across a switch whenever they don't conflict,
    /// which quietly moves work you were doing on one branch onto another —
    /// and the primary checkout is the repo the user already had, not one
    /// Argus made. A worktree gives the branch a directory of its own and
    /// leaves that work where it was.
    pub async fn switch_branch(&self, checkout: CheckoutId, branch: &str) -> anyhow::Result<()> {
        let (primary, path) = {
            let inner = self.inner.lock().unwrap();
            let c = find_checkout_ref(&inner.projects, checkout)
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?;
            (c.primary, c.path.clone())
        };
        if primary {
            // Read live rather than trusting the cache: the poll is up to
            // two seconds stale, and this is the check that decides whether
            // uncommitted work is about to move.
            let dirty = crate::git::status(&path).is_some_and(|s| s.dirty);
            if dirty {
                anyhow::bail!(
                    "the primary checkout has uncommitted changes — commit them, or make a worktree for {branch} instead"
                );
            }
        }
        self.git_switch(checkout, &["switch"], branch).await
    }

    /// Creates a branch here and moves onto it, leaving the checkout where
    /// it is — unlike `create_worktree`, which makes a directory for it.
    pub async fn create_branch(&self, checkout: CheckoutId, branch: &str) -> anyhow::Result<()> {
        self.git_switch(checkout, &["switch", "-c"], branch).await
    }

    /// Brings every remote up to date without touching a working tree.
    ///
    /// This is what makes a remote's branches visible as rows, so the tree
    /// is refreshed on the way out rather than waiting for the poll — a
    /// fetch the user asked for that appears to have done nothing until two
    /// seconds later reads as a fetch that failed.
    pub async fn fetch(&self, checkout: CheckoutId) -> anyhow::Result<()> {
        let path = self.checkout_path(checkout)?;
        run_git(&path, &["fetch", "--all", "--prune"]).await?;
        self.refresh_checkout_git(checkout);
        self.refresh_branches();
        self.broadcast_tree();
        Ok(())
    }

    /// Moves one checkout up to its upstream, and only ever by
    /// fast-forward: a merge that needs a decision is not something to make
    /// on the user's behalf from a keypress, and git's refusal says so
    /// better than a guess would.
    pub async fn pull(&self, checkout: CheckoutId) -> anyhow::Result<()> {
        let path = self.checkout_path(checkout)?;
        run_git(&path, &["pull", "--ff-only"]).await?;
        self.refresh_checkout_git(checkout);
        self.refresh_branches();
        self.broadcast_tree();
        Ok(())
    }

    /// Drops a local branch. Run from the checkout that asked, which is
    /// the repository's primary one whenever the row was a branch rather
    /// than a directory.
    ///
    /// Local only: `git branch -d` touches `refs/heads`, never
    /// `refs/remotes`, and nothing here pushes a deletion. Removing a
    /// branch from the panel is not removing it from the remote.
    ///
    /// `-d`, never `-D`. A branch is a name on commits, and the row you
    /// delete it from says nothing about whether those commits are anywhere
    /// else; git already knows, so its refusal is the answer and is passed
    /// back as it stands. The main branch is refused outright — it is the
    /// row the column is anchored on, and nobody means to delete it from
    /// here.
    pub async fn delete_branch(&self, checkout: CheckoutId, branch: &str) -> anyhow::Result<()> {
        let branch = checked_branch_name(branch)?;
        let path = self.checkout_path(checkout)?;
        if crate::git::default_branch(&path).as_deref() == Some(branch.as_str()) {
            anyhow::bail!("{branch} is the repository's main branch");
        }

        let output = crate::command::git()
            .args(["branch", "-d", &branch])
            .current_dir(&path)
            .output()
            .await?;
        if !output.status.success() {
            anyhow::bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
        }

        self.refresh_branches();
        self.broadcast_tree();
        Ok(())
    }

    /// `flags` are everything before the branch name, which this appends
    /// itself once it is validated — so the name git is handed is the same
    /// one that was checked, rather than whatever the caller had spelled
    /// into its own argument list.
    async fn git_switch(
        &self,
        checkout: CheckoutId,
        flags: &[&str],
        branch: &str,
    ) -> anyhow::Result<()> {
        let branch = checked_branch_name(branch)?;
        let path = self.checkout_path(checkout)?;

        let output = crate::command::git()
            .args(flags)
            .arg(&branch)
            .current_dir(&path)
            .output()
            .await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{}", stderr.trim());
        }

        // The checkout's name follows the branch it sits on, the way it did
        // when the worktree was created.
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(c) = find_checkout(&mut inner.projects, checkout) {
                c.name = branch.to_string();
            }
        }
        self.refresh_checkout_git(checkout);
        self.refresh_branches();
        self.broadcast_tree();
        Ok(())
    }

    /// `git worktree add`s a new checkout in `base`'s repository and appends
    /// it to the tree. Placed under `.argus/worktrees/<branch>` beside the
    /// repository's primary checkout (DESIGN.md §4 Level 2), regardless of
    /// which checkout `base` itself is — so worktrees always nest under the
    /// one directory, not under each other.
    ///
    /// A branch that already exists gets a directory for the branch it is;
    /// otherwise the branch is created off `base`'s current HEAD. Giving a
    /// branch a checkout and inventing one are the same request from the
    /// tree's point of view — a row for a branch that has no directory yet —
    /// and refusing the first because the name is taken would only mean
    /// telling the user to say it a different way.
    pub async fn create_worktree(
        self: &Arc<Self>,
        base: CheckoutId,
        branch: String,
    ) -> anyhow::Result<()> {
        let branch = checked_branch_name(&branch)?;

        let context = {
            let inner = self.inner.lock().unwrap();
            worktree_context(&inner.projects, base)
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?
        };
        let dest = worktree_dir(&context.root, &branch)?;
        let exists = crate::git::has_local_branch(&context.base, &branch);

        // A branch that is only on a remote starts from there and tracks
        // it, rather than being invented afresh off this checkout's HEAD —
        // the row said `origin/x`, so the worktree has to be `origin/x`.
        let upstream = (!exists)
            .then(|| crate::git::remote_branch_for(&context.base, &branch))
            .flatten();

        let mut command = crate::command::git();
        command.args(["worktree", "add"]);
        if !exists {
            command.args(["-b", &branch]);
        }
        command.arg(&dest);
        if exists {
            command.arg(&branch);
        } else if let Some(upstream) = &upstream {
            command.arg(upstream);
        }
        let output = command.current_dir(&context.base).output().await?;
        if !output.status.success() {
            anyhow::bail!(
                "git worktree add failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let added = {
            let mut inner = self.inner.lock().unwrap();
            let id = CheckoutId(inner.ids.alloc());
            find_repository(&mut inner.projects, context.repository).map(|r| {
                r.checkouts.push(Checkout {
                    id,
                    name: branch,
                    path: dest.clone(),
                    primary: false,
                    panes: Vec::new(),
                    git: None,
                });
                id
            })
        };
        if let Some(id) = added {
            self.refresh_checkout_git(id);
        }
        self.refresh_branches();
        // Broadcast before the setup commands run: they can take as long as
        // an install takes, and the row is what the user asked for.
        self.broadcast_tree();

        run_setup(&context.setup, &dest).await
    }

    /// Errors rather than `None`, so a stale id reaches the user as text.
    pub fn checkout_path(&self, checkout: CheckoutId) -> anyhow::Result<PathBuf> {
        let inner = self.inner.lock().unwrap();
        find_checkout_ref(&inner.projects, checkout)
            .map(|c| c.path.clone())
            .ok_or_else(|| anyhow::anyhow!("no such checkout"))
    }

    /// Kills every pane in a linked-worktree checkout, `git worktree
    /// remove`s it, deletes its branch (best-effort — a branch left behind
    /// costs nothing), and refuses outright on the primary checkout, which
    /// is the repo the user already had, not Argus's to delete (DESIGN.md
    /// §4 Level 2).
    ///
    /// Ordered so a refusal costs nothing: every check that can be made
    /// while the agents are still running is made first, and the panes die
    /// only once git's own removal is expected to go through. See
    /// `git::removal` for why the panes cannot simply be killed afterwards.
    pub async fn remove_checkout(&self, checkout: CheckoutId) -> anyhow::Result<()> {
        let (path, primary, primary_path, pane_ids) = {
            let inner = self.inner.lock().unwrap();
            let c = find_checkout_ref(&inner.projects, checkout)
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?;
            let (_, _, primary_path) = find_checkout_context(&inner.projects, checkout)
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?;
            (
                c.path.clone(),
                c.primary,
                primary_path,
                c.panes.iter().map(|p| p.id).collect::<Vec<_>>(),
            )
        };
        if primary {
            anyhow::bail!("refusing to remove the primary checkout");
        }

        let stale = match crate::git::removal(&primary_path, &path) {
            crate::git::Removal::Blocked(why) => anyhow::bail!("{why}"),
            crate::git::Removal::Stale => true,
            crate::git::Removal::Ready => false,
        };

        let branch = crate::git::status(&path).and_then(|s| s.branch);

        for pane in pane_ids {
            let _ = self.close_pane(pane);
        }

        if stale {
            // Nothing to delete but the registration, and `worktree remove`
            // refuses a directory it cannot find.
            let _ = crate::command::git()
                .args(["worktree", "prune"])
                .current_dir(&primary_path)
                .output()
                .await;
        } else {
            self.run_worktree_remove(&path, &primary_path).await?;
        }

        if let Some(branch) = branch {
            let _ = crate::command::git()
                .args(["branch", "-D", &branch])
                .current_dir(&primary_path)
                .output()
                .await;
        }

        {
            let mut inner = self.inner.lock().unwrap();
            remove_checkout_entry(&mut inner.projects, checkout);
        }
        self.refresh_branches();
        self.broadcast_tree();
        Ok(())
    }

    /// `git worktree remove --force`, retried briefly.
    ///
    /// Killing a pane asks the OS to end the child; the handles it held on
    /// the worktree directory go away a moment later, and on Windows a
    /// directory a process still has open cannot be deleted. Retrying for
    /// about a second turns that race into a wait instead of a refusal the
    /// user has to reissue — by which point the panes are already gone.
    async fn run_worktree_remove(
        &self,
        path: &std::path::Path,
        primary_path: &std::path::Path,
    ) -> anyhow::Result<()> {
        const ATTEMPTS: usize = 10;
        let mut last = String::new();
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            let output = crate::command::git()
                .args(["worktree", "remove", "--force"])
                .arg(path)
                .current_dir(primary_path)
                .output()
                .await?;
            if output.status.success() {
                return Ok(());
            }
            last = String::from_utf8_lossy(&output.stderr).trim().to_string();
        }
        anyhow::bail!("git worktree remove failed: {last}");
    }
}

/// Trims a user-typed branch name and refuses the two spellings that must
/// never reach a git command line: empty, and anything starting with a
/// dash, which git reads as a flag rather than as a name. Git's own refname
/// rules cover everything else, and its refusal says more than a restatement
/// of them here would.
fn checked_branch_name(raw: &str) -> anyhow::Result<String> {
    let branch = raw.trim();
    if branch.is_empty() {
        anyhow::bail!("branch name can't be empty");
    }
    if branch.starts_with('-') {
        anyhow::bail!("not a valid branch name: {branch}");
    }
    Ok(branch.to_string())
}

/// Runs a project's setup commands in a worktree that was just created,
/// in order, stopping at the first that fails.
///
/// Parsed into arguments rather than handed to a shell, the way an editor
/// command is: the daemon owns no console on Windows, a shell would be a
/// second thing to configure, and the commands here are the user's own
/// words either way. A failure is reported but the worktree is kept — a
/// dependency install that did not work is a thing to fix in a directory
/// that exists, not a reason to throw the branch away.
async fn run_setup(commands: &[String], dir: &std::path::Path) -> anyhow::Result<()> {
    for line in commands {
        let argv = crate::editor::parse_command(line).unwrap_or_default();
        let Some((program, args)) = argv.split_first() else {
            continue;
        };
        let output = crate::command::quiet(program)
            .args(args)
            .current_dir(dir)
            .output()
            .await;
        let failed = match output {
            Ok(output) if output.status.success() => continue,
            Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_string(),
            Err(e) => e.to_string(),
        };
        tracing::warn!(
            "setup command {line:?} failed in {}: {failed}",
            dir.display()
        );
        anyhow::bail!("the worktree is there, but setup command {line:?} failed: {failed}");
    }
    Ok(())
}

/// Where a new worktree for `branch` goes, under `root` — the project's
/// configured worktree root for this repository, or the `.argus/worktrees`
/// directory beside its primary checkout.
///
/// The branch name is a user string that becomes a path here, so the
/// components that would steer it out of that root are refused rather than
/// left to git's refname rules — which run too late, and allow more than a
/// path should. `..` climbs out, and `Path::join` throws the base away
/// entirely when what it joins is rooted (`/tmp/x`, `C:\x`,
/// `\\server\share`), which would put the worktree wherever the name said.
fn worktree_dir(root: &std::path::Path, branch: &str) -> anyhow::Result<PathBuf> {
    // A backslash separates directories on Windows and is an ordinary
    // character in a name everywhere else; refusing it keeps one branch
    // name from meaning two different paths.
    let rooted = branch.contains('\\')
        || std::path::Path::new(branch)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)));
    if rooted {
        anyhow::bail!("a branch name can't be a path: {branch}");
    }
    Ok(root.join(branch))
}
