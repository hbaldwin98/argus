//! Read-only git status for a checkout: branch, dirty/clean, ahead/behind
//! upstream, and the repo's current worktree list. See DESIGN.md §4 Level 2
//! and §9 M2. Mutating operations (`worktree add`/`remove`) live next to the
//! daemon state they update, in `state.rs`, not here.

use std::path::{Path, PathBuf};

use orion_protocol::GitStatus;

/// Returns `None` if `path` isn't inside a git repo at all. Any other
/// failure (e.g. a transient lock) also degrades to `None` rather than
/// erroring the whole tree snapshot — status is best-effort.
pub fn status(path: &Path) -> Option<GitStatus> {
    let repo = git2::Repository::open(path).ok()?;

    let head = repo.head().ok();
    let branch = head
        .as_ref()
        .and_then(|h| h.shorthand())
        .filter(|s| *s != "HEAD") // detached HEAD shorthand is literally "HEAD"
        .map(str::to_string);

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).renames_head_to_index(true);
    let changed_files = repo
        .statuses(Some(&mut opts))
        .map(|s| s.iter().count())
        .unwrap_or(0);

    let (ahead, behind) = ahead_behind(&repo, head.as_ref()).unwrap_or((0, 0));

    Some(GitStatus {
        branch,
        dirty: changed_files > 0,
        changed_files,
        ahead,
        behind,
    })
}

/// Every worktree path git currently knows about for the repo at `path`
/// (the primary checkout included, always first). Empty if `path` isn't a
/// git repo or the `git` binary isn't available — callers treat that as
/// "nothing to reconcile", never as "everything was removed".
pub fn list_worktrees(path: &Path) -> Vec<PathBuf> {
    let Ok(output) = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(path)
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_worktree_list(&String::from_utf8_lossy(&output.stdout))
}

/// The `worktree <path>` lines out of `git worktree list --porcelain`, in
/// the order git emits them. Split out from `list_worktrees` so the parsing
/// is testable without a real repo on disk.
fn parse_worktree_list(porcelain: &str) -> Vec<PathBuf> {
    porcelain
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(|p| PathBuf::from(p.trim_end_matches('\r')))
        .collect()
}

fn ahead_behind(repo: &git2::Repository, head: Option<&git2::Reference>) -> Option<(usize, usize)> {
    let head = head?;
    let local_oid = head.target()?;
    let branch_name = head.shorthand()?;
    let branch = repo.find_branch(branch_name, git2::BranchType::Local).ok()?;
    let upstream = branch.upstream().ok()?;
    let upstream_oid = upstream.get().target()?;
    repo.graph_ahead_behind(local_oid, upstream_oid).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_worktree_paths_in_git_order() {
        // Real `git worktree list --porcelain` shape: stanzas separated by
        // blank lines, each led by the `worktree` line, primary first.
        let out = "\
worktree C:/src/orion
HEAD abc123
branch refs/heads/master

worktree C:/src/orion/.orion/worktrees/feature-x
HEAD def456
branch refs/heads/feature-x
";
        assert_eq!(
            parse_worktree_list(out),
            vec![
                PathBuf::from("C:/src/orion"),
                PathBuf::from("C:/src/orion/.orion/worktrees/feature-x"),
            ]
        );
    }

    #[test]
    fn ignores_non_worktree_lines_and_crlf() {
        let out = "worktree /a\r\nHEAD abc\r\nbare\r\n\r\nworktree /b\r\ndetached\r\n";
        assert_eq!(parse_worktree_list(out), vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }

    #[test]
    fn empty_output_yields_no_worktrees() {
        // Callers rely on this meaning "nothing to reconcile" rather than
        // "every worktree was removed" — see `reconcile_worktrees`.
        assert!(parse_worktree_list("").is_empty());
    }

    #[test]
    fn status_of_a_non_repo_is_none() {
        let dir = std::env::temp_dir().join("orion-test-not-a-repo");
        let _ = std::fs::create_dir_all(&dir);
        assert!(status(&dir).is_none());
    }

    #[test]
    fn list_worktrees_of_a_non_repo_is_empty() {
        let dir = std::env::temp_dir().join("orion-test-not-a-repo");
        let _ = std::fs::create_dir_all(&dir);
        assert!(list_worktrees(&dir).is_empty());
    }
}
