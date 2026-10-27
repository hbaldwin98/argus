//! A review of working changes listed before it is read, and one file of it
//! read when the viewer comes to it. A large checkout's changes are
//! thousands of files; the full review (`diff`) writes each one to the
//! object database and makes every patch before it can answer, where the
//! listing reads only what Git already keeps — the index, and the working
//! tree's stat — and writes nothing.
//!
//! It counts each file's added and removed lines while it has time, reading
//! the files without writing them, so a list of a few hundred files shows
//! real counts before any is read; past [`COUNT_BUDGET`] the rest go
//! uncounted until they are read.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Context;
use argus_protocol::{is_checkout_relative, ChangeKind, FileDiff, LineCounts, ReviewBase};

use crate::review::diff::{
    blob_id_spans, capture_blob, hang_spans, render_patch, slashed, wanted_sides, GeneratedReview, BINARY_NOTE, MAX_DIFF_BYTES, TOO_LARGE_NOTE,
};

/// Past this many pairs of a file gone and a file new, no rename is looked
/// for, as `git diff` gives up past its `diff.renameLimit` squared: scoring
/// every pair reads them all, which on a checkout with tens of thousands of
/// untracked files was most of the listing's time.
const RENAME_PAIRS: usize = 1000 * 1000;

/// How long the listing spends counting lines: the listing is what a large
/// checkout's review waits on, so counting stops here and the rest of the
/// files go uncounted.
const COUNT_BUDGET: Duration = Duration::from_millis(400);

/// Every file a side of the working changes holds, each `pending`: its path,
/// how it changed, and a rename's old path, as `git status` would say them.
pub fn list(path: &Path, base: ReviewBase) -> anyhow::Result<GeneratedReview> {
    list_within(path, base, RENAME_PAIRS, COUNT_BUDGET)
}

fn list_within(path: &Path, base: ReviewBase, rename_pairs: usize, counting: Duration) -> anyhow::Result<GeneratedReview> {
    let repo = git2::Repository::open(path).with_context(|| format!("could not open Git repository at {}", path.display()))?;
    let mut opts = git2::DiffOptions::new();
    // No context: only the changed lines are counted.
    opts.ignore_submodules(true).context_lines(0);
    let mut diff = match base {
        ReviewBase::Staged => {
            let head = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
            repo.diff_tree_to_index(head.as_ref(), None, Some(&mut opts))?
        }
        ReviewBase::Unstaged => {
            // An untracked file's content is read only when it is counted.
            opts.include_untracked(true).recurse_untracked_dirs(true).include_typechange(true).show_untracked_content(true);
            repo.diff_index_to_workdir(None, Some(&mut opts))?
        }
        ReviewBase::Commit => anyhow::bail!("a commit is not listed lazily"),
    };
    // A file moved without `git mv` is a deletion and an untracked file,
    // which the full review pairs as a rename; so does this.
    let count = |wanted: &[git2::Delta]| diff.deltas().filter(|d| wanted.contains(&d.status())).count();
    let gone = count(&[git2::Delta::Deleted]);
    let new = count(&[git2::Delta::Added, git2::Delta::Untracked]);
    if gone.saturating_mul(new) <= rename_pairs {
        let mut find = git2::DiffFindOptions::new();
        find.renames(true).for_untracked(true);
        diff.find_similar(Some(&mut find)).context("could not detect review renames")?;
    }
    let started = Instant::now();
    let mut files = Vec::new();
    for (i, delta) in diff.deltas().enumerate() {
        let Some(mut file) = listed(&delta) else { continue };
        if started.elapsed() < counting {
            file.counted = counted(&diff, i, &delta);
        }
        files.push(file);
    }
    Ok(GeneratedReview { files, commit: None })
}

/// The lines delta `i` adds and removes; none for a file over the size a
/// review reads, which its read says is too large rather than counting.
fn counted(diff: &git2::Diff<'_>, i: usize, delta: &git2::DiffDelta) -> Option<LineCounts> {
    let size = delta.old_file().size().max(delta.new_file().size());
    if size as i64 > MAX_DIFF_BYTES {
        return None;
    }
    let patch = git2::Patch::from_diff(diff, i).ok()??;
    let (_, added, removed) = patch.line_stats().ok()?;
    Some(LineCounts { added: added as u32, removed: removed as u32 })
}

/// One delta as a file not yet read.
fn listed(delta: &git2::DiffDelta) -> Option<FileDiff> {
    let path = delta.new_file().path().or_else(|| delta.old_file().path()).map(slashed)?;
    let kind = match delta.status() {
        git2::Delta::Untracked => ChangeKind::Untracked,
        git2::Delta::Added => ChangeKind::Added,
        git2::Delta::Deleted => ChangeKind::Deleted,
        git2::Delta::Renamed => ChangeKind::Renamed,
        git2::Delta::Modified | git2::Delta::Typechange => ChangeKind::Modified,
        // Copies are not looked for; what is left is no change at all.
        _ => return None,
    };
    Some(FileDiff {
        old_path: (kind == ChangeKind::Renamed).then(|| delta.old_file().path().map(slashed)).flatten(),
        path,
        kind,
        hunks: Vec::new(),
        note: None,
        new_blob: None,
        new_lines: None,
        pending: true,
        counted: None,
    })
}

/// One listed file read: its hunks on `base`'s side as the index and the
/// working tree hold it now, highlighted, with the new side's blob and length
/// for the folds. The new side of an unstaged file is written to the object
/// database, as the full review writes it, so a fold reads what was diffed.
pub fn read(path: &Path, base: ReviewBase, file: &str, old_path: Option<&str>, kind: ChangeKind) -> anyhow::Result<FileDiff> {
    // The paths are the client's: one outside the checkout would be read
    // from wherever it named, and git2 panics on one that is not relative.
    if let Some(outside) = [Some(file), old_path].into_iter().flatten().find(|p| !is_checkout_relative(p)) {
        anyhow::bail!("{outside} is not a path in the checkout");
    }
    let repo = git2::Repository::open(path).with_context(|| format!("could not open Git repository at {}", path.display()))?;
    let index = repo.index().context("could not read Git index")?;
    let from = old_path.filter(|_| kind == ChangeKind::Renamed).unwrap_or(file);
    let indexed = |at: &str| index.get_path(Path::new(at), 0).map(|entry| entry.id);
    let (old, new) = match base {
        ReviewBase::Staged => {
            let head = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
            let old = head.and_then(|tree| tree.get_path(Path::new(from)).ok()).map(|entry| entry.id());
            (old, indexed(file))
        }
        ReviewBase::Unstaged => {
            let full = repo.workdir().context("bare repositories cannot be reviewed")?.join(file);
            let new = if full.is_file() || full.is_symlink() {
                Some(capture_blob(&repo, &full).with_context(|| format!("could not capture {}", full.display()))?.0)
            } else {
                None
            };
            (indexed(from), new)
        }
        ReviewBase::Commit => anyhow::bail!("a commit's files come with its review"),
    };
    let mut read = FileDiff {
        old_path: (kind == ChangeKind::Renamed).then(|| from.to_owned()),
        path: file.to_owned(),
        kind,
        hunks: Vec::new(),
        note: None,
        new_blob: new.map(|oid| oid.to_string()),
        new_lines: None,
        pending: false,
        counted: None,
    };
    let blob = |oid: Option<git2::Oid>| -> anyhow::Result<git2::Blob<'_>> {
        match oid {
            Some(oid) => repo.find_blob(oid).with_context(|| format!("could not read {file} as listed")),
            None => repo.find_blob(repo.blob(b"")?).context("could not read an empty blob"),
        }
    };
    let (old_blob, new_blob) = (blob(old)?, blob(new)?);
    if old_blob.size().max(new_blob.size()) as i64 > MAX_DIFF_BYTES {
        read.note = Some(TOO_LARGE_NOTE.to_string());
        return Ok(read);
    }
    if old_blob.is_binary() || new_blob.is_binary() {
        read.note = Some(BINARY_NOTE.to_string());
        return Ok(read);
    }
    let mut opts = git2::DiffOptions::new();
    opts.context_lines(3);
    let patch = git2::Patch::from_blobs(&old_blob, Some(Path::new(from)), &new_blob, Some(Path::new(file)), Some(&mut opts))?;
    render_patch(&patch, &mut read, 0)?;
    if read.note.is_none() && !read.hunks.is_empty() {
        let (wants_old, wants_new) = wanted_sides(&read);
        let old_spans = old.filter(|_| wants_old).and_then(|oid| blob_id_spans(&repo, oid, file));
        let new_spans = new.filter(|_| wants_new).and_then(|oid| blob_id_spans(&repo, oid, file));
        hang_spans(&mut read, old_spans.as_ref(), new_spans.as_ref());
        if new.is_some() {
            let content = new_blob.content();
            let breaks = content.iter().filter(|&&b| b == b'\n').count();
            let unterminated = content.last().is_some_and(|&b| b != b'\n');
            read.new_lines = Some((breaks + usize::from(unterminated)) as u32);
        }
    }
    Ok(read)
}

#[cfg(test)]
mod tests;
