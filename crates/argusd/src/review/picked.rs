//! What one file's index entry becomes when some of the lines of its diff
//! are taken into the index or given back, read off the same diff the
//! review showed. Writes only the blob; the index is `git`'s to write.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;
use argus_protocol::{LineKind, PickedLine, ReviewBase};

/// What the index should hold for the file: a mode and a blob, or nothing
/// once the last of a deleted file's lines is taken.
#[derive(Debug, PartialEq, Eq)]
pub struct Picked {
    pub entry: Option<(u32, git2::Oid)>,
}

/// The file at `path` with `lines` of the diff on side `from` moved across:
/// into the index from the working tree, or out of it back to `HEAD`.
pub fn pick(repo_path: &Path, from: ReviewBase, path: &str, lines: &[PickedLine]) -> anyhow::Result<Picked> {
    if lines.is_empty() {
        anyhow::bail!("no lines of {path} were picked");
    }
    let repo = git2::Repository::open(repo_path)
        .with_context(|| format!("could not open Git repository at {}", repo_path.display()))?;
    let index = repo.index().context("could not read Git index")?;
    let indexed = index.get_path(Path::new(path), 0);
    let content = |oid: git2::Oid| -> anyhow::Result<Vec<u8>> {
        Ok(repo.find_blob(oid).with_context(|| format!("could not read {path}"))?.content().to_vec())
    };
    // `gone`: the side the lines move to has no such file, so taking every
    // line leaves no entry rather than an empty one.
    let (old, new, mode, gone) = match from {
        ReviewBase::Unstaged => {
            let workdir = repo.workdir().context("a bare repository has no working tree")?;
            let full = workdir.join(path);
            let on_disk = full.symlink_metadata().is_ok();
            let new = if on_disk {
                // Through git's clean filters, as the review captured it.
                content(repo.blob_path(&full).with_context(|| format!("could not read {path}"))?)?
            } else {
                Vec::new()
            };
            let old = indexed.as_ref().map(|e| content(e.id)).transpose()?.unwrap_or_default();
            let mode = indexed.as_ref().map_or_else(|| new_file_mode(&full), |e| e.mode);
            (old, new, mode, !on_disk)
        }
        ReviewBase::Staged => {
            let Some(entry) = indexed.as_ref() else {
                anyhow::bail!("{path} is not in the index");
            };
            let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            let committed = head.as_ref().and_then(|t| t.get_path(Path::new(path)).ok());
            let old = committed.as_ref().map(|e| content(e.id())).transpose()?.unwrap_or_default();
            (old, content(entry.id)?, entry.mode, committed.is_none())
        }
        ReviewBase::Commit => anyhow::bail!("a commit's lines cannot be staged"),
    };
    if mode & 0o170000 != 0o100000 {
        anyhow::bail!("{path} is not a plain file; stage it whole");
    }
    if is_binary(&old) || is_binary(&new) {
        anyhow::bail!("{path} is binary; stage it whole");
    }
    let merged = merge(&old, &new, from == ReviewBase::Staged, path, lines)?;
    let entry = if merged.is_empty() && gone {
        None
    } else {
        Some((mode, repo.blob(&merged).context("could not write the staged content")?))
    };
    Ok(Picked { entry })
}

/// `old` with the picked changes towards `new` made, or with every change
/// but the picked ones made when `reverse` — a pick off the staged side
/// names what to give back. Lines between hunks come from `old` as they
/// are.
fn merge(old: &[u8], new: &[u8], reverse: bool, path: &str, lines: &[PickedLine]) -> anyhow::Result<Vec<u8>> {
    let moved = || anyhow::anyhow!("{path} changed since it was shown; look again");
    let mut wanted: HashMap<(bool, u32), (&str, bool)> = HashMap::new();
    for line in lines {
        let added = match line.kind {
            LineKind::Added => true,
            LineKind::Removed => false,
            LineKind::Context => anyhow::bail!("an unchanged line of {path} cannot be staged"),
        };
        wanted.insert((added, line.lineno), (line.text.as_str(), false));
    }
    let mut opts = git2::DiffOptions::new();
    opts.context_lines(3);
    let patch = git2::Patch::from_buffers(old, None, new, None, Some(&mut opts))
        .with_context(|| format!("could not diff {path}"))?;
    let old_lines: Vec<&[u8]> = old.split_inclusive(|&b| b == b'\n').collect();
    let mut out: Vec<u8> = Vec::with_capacity(old.len().max(new.len()));
    let mut next = 0usize;
    let push = |out: &mut Vec<u8>, bytes: &[u8]| {
        // A kept line that ended the file without a newline is no longer
        // last once anything follows it.
        if out.last().is_some_and(|&b| b != b'\n') {
            out.push(b'\n');
        }
        out.extend_from_slice(bytes);
    };
    for h in 0..patch.num_hunks() {
        let (hunk, count) = patch.hunk(h)?;
        let before = if hunk.old_lines() > 0 { hunk.old_start().saturating_sub(1) } else { hunk.old_start() };
        while next < before as usize && next < old_lines.len() {
            push(&mut out, old_lines[next]);
            next += 1;
        }
        for l in 0..count {
            let line = patch.line_in_hunk(h, l)?;
            let (added, lineno) = match line.origin() {
                ' ' => {
                    push(&mut out, line.content());
                    next += 1;
                    continue;
                }
                '+' => (true, line.new_lineno()),
                '-' => (false, line.old_lineno()),
                _ => continue,
            };
            let lineno = lineno.ok_or_else(moved)?;
            let picked = match wanted.get_mut(&(added, lineno)) {
                Some((text, seen)) => {
                    let shown = String::from_utf8_lossy(line.content());
                    if shown.trim_end_matches(['\n', '\r']) != *text {
                        return Err(moved());
                    }
                    *seen = true;
                    true
                }
                None => false,
            };
            if !added {
                next += 1;
            }
            if (picked != reverse) == added {
                push(&mut out, line.content());
            }
        }
    }
    for rest in old_lines.iter().skip(next) {
        push(&mut out, rest);
    }
    if wanted.values().any(|(_, seen)| !seen) {
        return Err(moved());
    }
    Ok(out)
}

/// Git's own test: a NUL in the first eight thousand bytes.
fn is_binary(content: &[u8]) -> bool {
    content.iter().take(8000).any(|&b| b == 0)
}

fn new_file_mode(_full: &Path) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if _full.metadata().is_ok_and(|m| m.permissions().mode() & 0o111 != 0) {
            return 0o100755;
        }
    }
    0o100644
}

#[cfg(test)]
mod tests;
