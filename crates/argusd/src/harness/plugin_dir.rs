//! A plugin folder a harness loads from wherever an environment variable
//! says, rather than from the checkout: Claude Code's mods. Argus writes it
//! into a folder of its own and points each of the harness's panes at it,
//! so nothing lands in the checkout and nothing is left there to sweep.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The plugin a harness is started with, shipped inside the daemon.
///
/// Built in only, like [`super::Plugin`]: a program is not something a
/// `[[harness]]` block can describe.
#[derive(Debug, Clone)]
pub struct PluginDir {
    /// The variable listing the folders the harness loads, joined as the
    /// platform joins paths.
    pub var: &'static str,
    /// The folder's name under Argus's own.
    pub name: &'static str,
    /// Its files, by path relative to the folder.
    pub files: &'static [(&'static str, &'static str)],
}

/// Claude Code's mod: the pane's inbox and its reply as it streams, from
/// inside the session (`claude-mod/hooks/register.ts`).
pub fn claude_mod() -> PluginDir {
    PluginDir {
        var: "CLAUDE_CODE_PLUGIN_DIRS",
        name: "claude-mod",
        files: &[
            (
                ".claude-plugin/plugin.json",
                include_str!("claude-mod/.claude-plugin/plugin.json"),
            ),
            ("hooks/hooks.json", include_str!("claude-mod/hooks/hooks.json")),
            ("hooks/register.ts", include_str!("claude-mod/hooks/register.ts")),
        ],
    }
}

impl PluginDir {
    /// Writes the folder under `root`, and returns the variable a pane is
    /// started with: the folders `inherited` already names, then this one.
    ///
    /// A file already as shipped is not written again, since the harness
    /// watches the folder and reloads the plugin in every running session
    /// when one changes.
    pub fn prepare(&self, root: &Path, inherited: Option<OsString>) -> std::io::Result<(String, String)> {
        let dir = root.join(self.name);
        for (relative, source) in self.files {
            let path = dir.join(relative);
            if std::fs::read_to_string(&path).is_ok_and(|on_disk| on_disk == *source) {
                continue;
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, source)?;
        }

        let mut dirs: Vec<PathBuf> = inherited
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_default();
        dirs.retain(|named| !named.as_os_str().is_empty() && *named != dir);
        dirs.push(dir);
        let joined = std::env::join_paths(dirs).map_err(std::io::Error::other)?;
        Ok((self.var.to_string(), joined.to_string_lossy().into_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_is_written_whole_and_named_after_what_was_already_there() {
        let root = tempfile::tempdir().unwrap();
        let theirs = std::env::join_paths(["/opt/their-mods"]).unwrap();

        let (var, value) = claude_mod().prepare(root.path(), Some(theirs)).unwrap();

        let dir = root.path().join("claude-mod");
        assert_eq!(var, "CLAUDE_CODE_PLUGIN_DIRS");
        assert_eq!(
            std::env::split_paths(&value).collect::<Vec<_>>(),
            vec![PathBuf::from("/opt/their-mods"), dir.clone()]
        );
        for (relative, source) in claude_mod().files {
            assert_eq!(std::fs::read_to_string(dir.join(relative)).unwrap(), *source);
        }
    }

    #[test]
    fn a_folder_already_as_shipped_is_left_alone() {
        // Claude Code reloads a mod in every running session when its
        // folder changes, so a pane starting must not touch the others'.
        let root = tempfile::tempdir().unwrap();
        claude_mod().prepare(root.path(), None).unwrap();
        let module = root.path().join("claude-mod/hooks/register.ts");
        let before = std::fs::metadata(&module).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));

        let (_, value) = claude_mod().prepare(root.path(), None).unwrap();

        assert_eq!(std::fs::metadata(&module).unwrap().modified().unwrap(), before);
        assert_eq!(std::env::split_paths(&value).count(), 1, "named once: {value}");
    }
}
