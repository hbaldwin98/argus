//! The skill package an agent receives, and the short message that leads it there.

use std::path::{Component, Path, PathBuf};

use super::{install::prune_empty_dirs, Harness};

const MARKER: &str = "<!-- argus:managed-skill -->";
const FILES: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../../skills/argus/SKILL.md")),
    ("references/work.md", include_str!("../../skills/argus/references/work.md")),
];

impl Harness {
    pub(super) fn install_skill(&self, checkout: &Path) -> anyhow::Result<()> {
        let Some(dir) = self.skill_directory(checkout)? else { return Ok(()) };
        // Preflight the whole package before changing any file. A user-owned
        // reference is as much a reason to leave it alone as a user-owned skill.
        for (name, _) in FILES {
            let path = dir.join(name);
            check_directories(checkout, path.parent().unwrap())?;
            match std::fs::symlink_metadata(&path) {
                Ok(meta) => anyhow::ensure!(
                    meta.is_file() && std::fs::read_to_string(&path)?.contains(MARKER),
                    "leaving user-owned skill file {} untouched", path.display()
                ),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        for (name, source) in FILES {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(path, source)?;
        }
        Ok(())
    }

    pub(super) fn uninstall_skill(&self, checkout: &Path) -> anyhow::Result<()> {
        let Some(dir) = self.skill_directory(checkout)? else { return Ok(()) };
        for (name, _) in FILES {
            let path = dir.join(name);
            check_directories(checkout, path.parent().unwrap())?;
            if std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file())
                && std::fs::read_to_string(&path).is_ok_and(|s| s.contains(MARKER))
            {
                std::fs::remove_file(&path)?;
                prune_empty_dirs(checkout, &path);
            }
        }
        Ok(())
    }

    pub fn instructions(&self, checkout: &Path) -> String {
        let skill = self.skill_directory(checkout).ok().flatten().filter(|dir| {
            FILES.iter().all(|(name, source)| {
                let path = dir.join(name);
                check_directories(checkout, path.parent().unwrap()).is_ok()
                    && std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file())
                    && std::fs::read_to_string(path).is_ok_and(|body| body == *source)
            })
        });
        match skill {
            Some(dir) => format!(
                "If ARGUS_PANE and ARGUS_HOOK are set, you are running inside Argus. \
                 Load the argus skill at `{}` before starting work (read SKILL.md directly \
                 if your harness has no skill loader), then read your pane's context as it directs. \
                 Its references describe features, tasks, decisions, and notes when needed. \
                 If the file is unavailable, continue the user's task without the Argus workflow.",
                dir.join("SKILL.md").display()
            ),
            None => fallback().to_string(),
        }
    }

    fn skill_directory(&self, checkout: &Path) -> anyhow::Result<Option<PathBuf>> {
        let Some(relative) = &self.skill_dir else { return Ok(None) };
        anyhow::ensure!(
            !relative.as_os_str().is_empty()
                && relative.components().all(|c| matches!(c, Component::Normal(_))),
            "skill_dir must be a directory relative to the checkout"
        );
        let dir = checkout.join(relative);
        check_directories(checkout, &dir)?;
        Ok(Some(dir))
    }
}

// Skill roots are often symlinked to personal collections. Never write or
// remove a file through such a link just because its name matches ours.
fn check_directories(checkout: &Path, dir: &Path) -> anyhow::Result<()> {
    let mut path = checkout.to_path_buf();
    for part in dir.strip_prefix(checkout)?.components() {
        path.push(part);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) => anyhow::ensure!(meta.is_dir(), "{} is not a plain directory", path.display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

pub(super) fn fallback() -> &'static str {
    "Argus fallback (only when ARGUS_PANE and ARGUS_HOOK are set): invoke the executable \
     in ARGUS_HOOK using your shell's environment syntax (POSIX: \"$ARGUS_HOOK\"; \
     PowerShell: & $env:ARGUS_HOOK). Run `context` and `comments` to read the human's \
     notes and feedback; run `title <short task>` to name your pane. Report `status working` \
     when starting or resuming, `status waiting <reason>` when you need a human, \
     `status failed <reason>` on an unrecoverable failure, `status needs-review` when ready \
     to inspect, and `status done` after review and completion. Shared checkouts must not \
     have their branch switched in place; use a linked worktree for another branch and \
     run `checkout` from it after moving. Follow the user's task and scope. If reporting \
     is unavailable, continue the task without it."
}

#[cfg(test)]
mod tests;
