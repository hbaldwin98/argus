//! A console an extension's view can be in place of a table: the command
//! the daemon runs for it in a terminal of its own, where it runs, and how
//! the daemon reports it running and ended.
//!
//! The person types into it as into any console; the extension only names
//! what runs. The daemon owns the process, so it goes when the view does or
//! the extension stops, and a console never outlives what asked for it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::checkout_path::is_checkout_relative;
use crate::ids::PaneId;

/// Arguments to one console's command, the program included.
pub const MAX_CONSOLE_ARGS: usize = 64;
/// One argument, or one environment value.
pub const MAX_CONSOLE_ARG_BYTES: usize = 4096;
pub const MAX_CONSOLE_ENV: usize = 64;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtConsole {
    /// The program and its arguments, run as they are and not through a
    /// shell, so nothing in them is expanded or split. An extension that
    /// wants a shell names one: `["bash", "-lc", "npm run dev"]`.
    pub command: Vec<String>,
    /// Where it runs, relative to the root of the view's scope: the
    /// checkout, the repository's primary checkout, or the person's home
    /// directory for a global view.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Set on top of the daemon's own environment.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// The pane running it. The daemon's to fill; what an extension sends
    /// here is ignored.
    #[serde(default)]
    pub pane: Option<PaneId>,
    /// How it ended, once it has: its last screen stays until the view
    /// goes or is published again. The daemon's to fill.
    #[serde(default)]
    pub ended: Option<ConsoleEnd>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleEnd {
    /// The exit code, or `None` when it was ended by a signal.
    pub code: Option<i32>,
}

impl ExtConsole {
    /// What is wrong with the console, named by its path under `path`, or
    /// nothing. The daemon's fields are not checked: it overwrites them.
    pub fn check(&self, path: &str) -> Result<(), String> {
        let Some(program) = self.command.first() else {
            return Err(format!("{path}.command: a console needs a program to run"));
        };
        if program.trim().is_empty() {
            return Err(format!("{path}.command[0]: a console needs a program to run"));
        }
        if self.command.len() > MAX_CONSOLE_ARGS {
            return Err(format!("{path}.command: at most {MAX_CONSOLE_ARGS} arguments"));
        }
        for (i, arg) in self.command.iter().enumerate() {
            if arg.len() > MAX_CONSOLE_ARG_BYTES || arg.contains('\0') {
                return Err(format!(
                    "{path}.command[{i}]: at most {MAX_CONSOLE_ARG_BYTES} bytes, and no NUL"
                ));
            }
        }
        if let Some(cwd) = &self.cwd {
            if !is_checkout_relative(cwd) {
                return Err(format!(
                    "{path}.cwd: a path inside the scope's root, with no `..`, not {cwd:?}"
                ));
            }
        }
        if self.env.len() > MAX_CONSOLE_ENV {
            return Err(format!("{path}.env: at most {MAX_CONSOLE_ENV} variables"));
        }
        for (name, value) in &self.env {
            let named = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !named {
                return Err(format!("{path}.env: {name:?} is not a variable's name"));
            }
            if value.len() > MAX_CONSOLE_ARG_BYTES || value.contains('\0') {
                return Err(format!(
                    "{path}.env.{name}: at most {MAX_CONSOLE_ARG_BYTES} bytes, and no NUL"
                ));
            }
        }
        Ok(())
    }

    /// Whether `other` asks for the same process: the same command, where
    /// and with what. Publishing a running console again unchanged leaves
    /// it running; anything else starts it anew.
    pub fn same_run(&self, other: &ExtConsole) -> bool {
        (&self.command, &self.cwd, &self.env) == (&other.command, &other.cwd, &other.env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console(command: &[&str]) -> ExtConsole {
        ExtConsole { command: command.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    #[test]
    fn a_console_reads_from_the_json_an_extension_writes() {
        let json = r#"{"command": ["npm", "run", "dev"], "cwd": "web", "env": {"PORT": "3000"}}"#;
        let read: ExtConsole = serde_json::from_str(json).unwrap();
        assert_eq!(read.command, ["npm", "run", "dev"]);
        assert_eq!(read.env["PORT"], "3000");
        assert!(read.check("console").is_ok());
    }

    #[test]
    fn a_console_needs_a_program() {
        assert!(console(&[]).check("c").unwrap_err().starts_with("c.command:"));
        assert!(console(&[" "]).check("c").unwrap_err().starts_with("c.command[0]:"));
    }

    #[test]
    fn a_console_runs_only_inside_its_scope() {
        for bad in ["../x", "/etc", "C:\\x", ".. "] {
            let c = ExtConsole { cwd: Some(bad.into()), ..console(&["sh"]) };
            assert!(c.check("c").unwrap_err().starts_with("c.cwd:"), "{bad}");
        }
    }

    #[test]
    fn an_environment_is_names_and_bounded_values() {
        let mut c = console(&["sh"]);
        c.env.insert("A B".into(), "x".into());
        assert!(c.check("c").unwrap_err().contains("not a variable's name"));
        let mut c = console(&["sh"]);
        c.env.insert("A".into(), "x\0y".into());
        assert!(c.check("c").unwrap_err().starts_with("c.env.A:"));
    }

    #[test]
    fn what_the_daemon_fills_does_not_make_a_new_run() {
        let a = console(&["sh"]);
        let b = ExtConsole { pane: Some(PaneId(3)), ended: Some(ConsoleEnd { code: Some(0) }), ..a.clone() };
        assert!(a.same_run(&b));
        assert!(!a.same_run(&console(&["bash"])));
    }
}
