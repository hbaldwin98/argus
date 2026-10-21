//! Moving a shell pane to another directory by typing a `cd` into it:
//! which shell it is, whether it waits at its prompt with nothing of its
//! own running, and the keys that move it, in that shell's own syntax.
//!
//! Typed text goes to whatever reads the terminal, so it is only typed to a
//! shell at its prompt: a `cd` typed into a running editor or REPL is input
//! to that program instead.

use std::ffi::OsStr;
use std::path::Path;

use super::*;

/// A shell whose syntax for changing directory Argus knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    Posix,
    Fish,
    /// PowerShell, either edition.
    Pwsh,
    Cmd,
}

/// What a pane's shell is doing, as far as moving it needs to know.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellState {
    AtPrompt(Shell),
    /// Something the shell started is in the foreground, or what is
    /// running could not be told.
    Running,
    /// A shell by this name, whose syntax Argus does not know.
    Unknown(String),
}

impl Shell {
    /// The shell `program` runs, by its file name; the error is that name,
    /// for a shell Argus cannot type a `cd` into.
    pub fn named(program: &OsStr) -> Result<Shell, String> {
        let name = Path::new(program)
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match name.as_str() {
            "bash" | "sh" | "zsh" | "dash" | "ksh" | "mksh" | "ash" => Ok(Shell::Posix),
            "fish" => Ok(Shell::Fish),
            "pwsh" | "powershell" => Ok(Shell::Pwsh),
            "cmd" => Ok(Shell::Cmd),
            _ => Err(name),
        }
    }

    /// The keys that move the shell to `directory`. The line is cleared
    /// first, because the person may have left something half-typed at the
    /// prompt, and `rm -rf ` with the `cd` after it would remove the
    /// destination. `None` for a path this shell could not be given intact.
    pub fn cd(self, directory: &Path) -> Option<Vec<u8>> {
        let path = directory.to_str()?;
        if path.chars().any(char::is_control) {
            return None;
        }
        let line = match self {
            Shell::Posix => format!("cd -- '{}'", path.replace('\'', r"'\''")),
            Shell::Fish => format!("cd '{}'", path.replace('\\', r"\\").replace('\'', r"\'")),
            Shell::Pwsh => format!("Set-Location -LiteralPath '{}'", path.replace('\'', "''")),
            // Quotes cannot be escaped inside cmd's, and it expands
            // `%NAME%` even between them.
            Shell::Cmd if path.contains(['"', '%']) => return None,
            Shell::Cmd => format!("cd /d \"{path}\""),
        };
        Some(format!("{}{line}\r", self.clear_line()).into_bytes())
    }

    /// The keys that empty the prompt's line, wherever the cursor is in it.
    /// PowerShell on Windows edits as the console does, and so does cmd:
    /// End, then Ctrl+Home to delete back to the line's start. Escape would
    /// clear it in one key, but a lone ESC followed by text reaches a
    /// pseudoconsole as Alt with the next letter. The rest edit as Emacs
    /// does: Ctrl+E, then Ctrl+U.
    fn clear_line(self) -> &'static str {
        const CONSOLE: &str = "\x1b[F\x1b[1;5H";
        const EMACS: &str = "\x05\x15";
        match self {
            Shell::Cmd => CONSOLE,
            Shell::Pwsh if cfg!(windows) => CONSOLE,
            Shell::Pwsh | Shell::Posix | Shell::Fish => EMACS,
        }
    }
}

/// The shell the user's default command runs, by the program it names.
pub(super) fn default_shell(command: &CommandBuilder) -> Result<Shell, String> {
    #[cfg(unix)]
    if command.is_default_prog() {
        return Shell::named(OsStr::new(&command.get_shell()));
    }
    match command.get_argv().first() {
        Some(program) => Shell::named(program),
        None => Err(String::new()),
    }
}

/// Whether nothing but the shell itself runs in the pane. On Unix, the
/// shell leads the terminal's foreground process group. On Windows, the
/// pane's job holds its console server and a chain of shells, each started
/// by the one before, where no shell runs the same image as its parent:
/// Git Bash's `bin\bash.exe` is a launcher for `usr\bin\bash.exe`, but a
/// `usr\bin\bash.exe` under another is a fork running a command, whose
/// native program has left the job. A shell nested in the first counts as
/// running, which only refuses a move that might have been safe.
#[cfg(windows)]
pub(super) fn idle(job: &ProcessJob) -> bool {
    let Ok(ids) = job.process_ids() else {
        return false;
    };
    let mut shells = Vec::new();
    for pid in ids {
        let Some((parent, image)) = console_host::parent_and_image_of(pid) else {
            return false;
        };
        if !console_host::is_server(&image) {
            shells.push((pid, parent, image));
        }
    }
    one_chain_of_shells(&shells)
}

/// Whether `processes`, each `(pid, parent, image)`, are shells started one
/// by another from a single root, none under a copy of its own image.
#[cfg(any(windows, test))]
fn one_chain_of_shells(processes: &[(u32, u32, String)]) -> bool {
    let within = |pid: u32| processes.iter().find(|(id, ..)| *id == pid);
    let roots = processes.iter().filter(|(_, parent, _)| within(*parent).is_none()).count();
    roots == 1
        && processes.iter().all(|(pid, parent, image)| {
            let children = processes.iter().filter(|(_, p, _)| p == pid).count();
            let forked = within(*parent).is_some_and(|(.., above)| above.eq_ignore_ascii_case(image));
            Shell::named(OsStr::new(image)).is_ok() && children <= 1 && !forked
        })
}

#[cfg(unix)]
pub(super) fn idle(master: &dyn MasterPty, shell: Option<u32>) -> bool {
    match (master.process_group_leader(), shell) {
        (Some(leader), Some(shell)) => leader as u32 == shell,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(processes: &[(u32, u32, &str)]) -> bool {
        let owned: Vec<_> = processes.iter().map(|&(pid, parent, image)| (pid, parent, image.to_string())).collect();
        one_chain_of_shells(&owned)
    }

    #[test]
    fn a_shell_alone_or_behind_its_launcher_is_one_chain() {
        assert!(chain(&[(10, 1, r"C:\pwsh\pwsh.exe")]));
        assert!(chain(&[(10, 1, r"C:\Git\bin\bash.exe"), (11, 10, r"C:\Git\usr\bin\bash.exe")]));
    }

    #[test]
    fn a_shell_running_a_command_is_not_one_chain_of_shells() {
        // A native program in the job.
        assert!(!chain(&[(10, 1, r"C:\pwsh\pwsh.exe"), (11, 10, r"C:\Windows\ping.exe")]));
        // Git Bash's fork, whose native program left the job.
        assert!(!chain(&[
            (10, 1, r"C:\Git\bin\bash.exe"),
            (11, 10, r"C:\Git\usr\bin\bash.exe"),
            (12, 11, r"C:\Git\usr\bin\bash.exe"),
        ]));
        // Two things running side by side.
        assert!(!chain(&[(10, 1, r"C:\cmd.exe"), (11, 10, r"C:\cmd.exe"), (12, 10, r"C:\sh.exe")]));
        assert!(!chain(&[]));
    }

    #[test]
    fn names_a_shell_by_its_file_name_whatever_its_path_or_case() {
        assert_eq!(Shell::named(OsStr::new(r"C:\Program Files\Git\bin\bash.exe")), Ok(Shell::Posix));
        assert_eq!(Shell::named(OsStr::new("/usr/bin/fish")), Ok(Shell::Fish));
        assert_eq!(Shell::named(OsStr::new("PWSH.EXE")), Ok(Shell::Pwsh));
        assert_eq!(Shell::named(OsStr::new("powershell")), Ok(Shell::Pwsh));
        assert_eq!(Shell::named(OsStr::new("cmd.exe")), Ok(Shell::Cmd));
        assert_eq!(Shell::named(OsStr::new("/usr/bin/nu")), Err("nu".to_string()));
    }

    fn typed(shell: Shell, path: &str) -> Option<String> {
        shell.cd(Path::new(path)).map(|keys| String::from_utf8(keys).unwrap())
    }

    #[test]
    fn clears_the_prompt_before_typing_the_cd_and_ends_with_enter() {
        assert_eq!(typed(Shell::Posix, "/src/a b").unwrap(), "\x05\x15cd -- '/src/a b'\r");
        assert_eq!(typed(Shell::Cmd, r"C:\src\a b").unwrap(), "\x1b[F\x1b[1;5Hcd /d \"C:\\src\\a b\"\r");
    }

    #[test]
    fn quotes_survive_each_shell_s_own_escaping() {
        assert_eq!(typed(Shell::Posix, "/it's").unwrap(), "\x05\x15cd -- '/it'\\''s'\r");
        assert_eq!(typed(Shell::Fish, r"/a\it's").unwrap(), "\x05\x15cd '/a\\\\it\\'s'\r");
        assert!(typed(Shell::Pwsh, "/it's").unwrap().ends_with("Set-Location -LiteralPath '/it''s'\r"));
    }

    #[test]
    fn refuses_a_path_the_shell_could_not_be_given_intact() {
        assert_eq!(typed(Shell::Posix, "/a\nrm -rf ~"), None);
        assert_eq!(typed(Shell::Cmd, r"C:\100%PATH%"), None);
        assert_eq!(typed(Shell::Cmd, "C:\\a\"b"), None);
    }
}
