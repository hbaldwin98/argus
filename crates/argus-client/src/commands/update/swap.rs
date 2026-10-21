//! Replacing the installed executables with a release's, all of them or
//! none: a failure part way puts back the ones already moved aside.
//!
//! Each old executable is renamed aside rather than overwritten, because
//! Windows refuses to write over or delete a running `.exe` but lets it be
//! renamed; the daemon, the desktop window and this very `argus` are
//! running. What cannot be deleted yet is deleted by the next update.

use std::path::{Path, PathBuf};

use super::archive::{Executable, EXECUTABLES};

/// Writes `executables` into `dir` in place of what is there.
pub fn install(dir: &Path, executables: &[Executable]) -> anyhow::Result<()> {
    remove_left_behind(dir);
    let staged = stage(dir, executables)?;
    let mut moved: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    for (executable, new) in executables.iter().zip(&staged) {
        let installed = dir.join(&executable.file);
        let swapped = set_aside(&installed).and_then(|aside| {
            moved.push((installed.clone(), aside));
            std::fs::rename(new, &installed).map_err(anyhow::Error::from)
        });
        if let Err(error) = swapped {
            roll_back(&moved);
            for new in &staged {
                let _ = std::fs::remove_file(new);
            }
            return Err(error.context(format!("could not install {}", installed.display())));
        }
    }
    // A copy still running stays until the next update removes it.
    for aside in moved.iter().filter_map(|(_, aside)| aside.as_ref()) {
        let _ = std::fs::remove_file(aside);
    }
    Ok(())
}

/// Each new executable written beside the old one, under a name no
/// release ships, so the swap is a rename within one folder.
fn stage(dir: &Path, executables: &[Executable]) -> anyhow::Result<Vec<PathBuf>> {
    let mut staged = Vec::new();
    for executable in executables {
        let new = dir.join(format!(".{}.new", executable.file));
        let written = std::fs::write(&new, &executable.bytes).and_then(|()| executable_mode(&new));
        if let Err(error) = written {
            for path in staged.iter().chain([&new]) {
                let _ = std::fs::remove_file(path);
            }
            anyhow::bail!(
                "could not write to {}: {error}. Run `argus update` as a user who may change it",
                dir.display()
            );
        }
        staged.push(new);
    }
    Ok(staged)
}

#[cfg(unix)]
fn executable_mode(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn executable_mode(_: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Moves `installed` aside, to `NAME.old` or, when an older one there is
/// still running, the first free `NAME.N.old`. `None` when nothing was
/// installed by that name: a build without the desktop window gains it.
fn set_aside(installed: &Path) -> anyhow::Result<Option<PathBuf>> {
    if !installed.exists() {
        return Ok(None);
    }
    let name = installed.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let free = (0..100)
        .map(|n| match n {
            0 => installed.with_file_name(format!("{name}.old")),
            n => installed.with_file_name(format!("{name}.{n}.old")),
        })
        .find(|aside| !aside.exists() || std::fs::remove_file(aside).is_ok())
        .ok_or_else(|| anyhow::anyhow!("{name} has too many old copies still running beside it"))?;
    std::fs::rename(installed, &free)?;
    Ok(Some(free))
}

fn roll_back(moved: &[(PathBuf, Option<PathBuf>)]) {
    for (installed, aside) in moved.iter().rev() {
        match aside {
            Some(aside) => {
                let _ = std::fs::rename(aside, installed);
            }
            None => {
                let _ = std::fs::remove_file(installed);
            }
        }
    }
}

/// What earlier updates set aside and could not delete then, because it
/// was running; only Argus's own names are touched.
fn remove_left_behind(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let ours = EXECUTABLES.iter().any(|executable| {
            let file = format!("{executable}{}", std::env::consts::EXE_SUFFIX);
            name.strip_prefix(&file).is_some_and(|rest| {
                rest == ".old"
                    || rest
                        .strip_prefix('.')
                        .and_then(|rest| rest.strip_suffix(".old"))
                        .is_some_and(|n| n.parse::<u32>().is_ok())
            })
        });
        if ours {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> String {
        format!("{name}{}", std::env::consts::EXE_SUFFIX)
    }

    fn release(text: &str) -> Vec<Executable> {
        EXECUTABLES.iter().map(|name| Executable { file: file(name), bytes: text.as_bytes().to_vec() }).collect()
    }

    fn read(dir: &Path, name: &str) -> String {
        std::fs::read_to_string(dir.join(file(name))).unwrap()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn every_executable_is_replaced_and_nothing_is_left_beside_them() {
        let dir = tempfile::tempdir().unwrap();
        for name in EXECUTABLES {
            std::fs::write(dir.path().join(file(name)), "old").unwrap();
        }
        std::fs::write(dir.path().join("README.md"), "kept").unwrap();

        install(dir.path(), &release("new")).unwrap();

        for name in EXECUTABLES {
            assert_eq!(read(dir.path(), name), "new");
        }
        let mut expected: Vec<String> = EXECUTABLES.iter().map(|name| file(name)).collect();
        expected.push("README.md".into());
        expected.sort();
        assert_eq!(names(dir.path()), expected);
    }

    #[test]
    fn an_executable_not_installed_before_is_added() {
        // A source build installed without the desktop window.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(file("argus")), "old").unwrap();
        install(dir.path(), &release("new")).unwrap();
        assert_eq!(read(dir.path(), "argus-desktop"), "new");
    }

    #[test]
    fn a_failed_swap_puts_back_what_it_had_moved() {
        let dir = tempfile::tempdir().unwrap();
        for name in EXECUTABLES {
            std::fs::write(dir.path().join(file(name)), "old").unwrap();
        }
        // The last executable stages into `.sub`, which exists, and then
        // renames into `sub`, which does not: the others are swapped by
        // the time it fails.
        let last = EXECUTABLES[EXECUTABLES.len() - 1];
        std::fs::create_dir(dir.path().join(".sub")).unwrap();
        let mut executables = release("new");
        executables.last_mut().unwrap().file = format!("sub/{}", file(last));

        let refused = install(dir.path(), &executables).unwrap_err();

        assert!(format!("{refused:#}").contains("could not install"), "{refused:#}");
        for name in EXECUTABLES {
            assert_eq!(read(dir.path(), name), "old", "{name}");
        }
        assert!(names(dir.path()).iter().all(|name| !name.ends_with(".new") && !name.ends_with(".old")));
        assert!(names(&dir.path().join(".sub")).is_empty(), "its staged copy is gone too");
    }

    #[test]
    fn copies_an_earlier_update_left_behind_are_removed_and_only_those() {
        let dir = tempfile::tempdir().unwrap();
        for left in [format!("{}.old", file("argusd")), format!("{}.3.old", file("argus"))] {
            std::fs::write(dir.path().join(left), "left").unwrap();
        }
        std::fs::write(dir.path().join("notes.old"), "someone else's").unwrap();
        std::fs::write(dir.path().join(format!("{}.x.old", file("argus"))), "not ours").unwrap();

        remove_left_behind(dir.path());

        assert_eq!(names(dir.path()), [format!("{}.x.old", file("argus")), "notes.old".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn an_installed_executable_can_be_run() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &release("new")).unwrap();
        let mode = std::fs::metadata(dir.path().join("argus")).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
    }
}
