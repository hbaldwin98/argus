//! Opening a file in the user's own editor, as a pane (DESIGN.md §6, §9 M4).

use std::path::Path;

/// Editors that draw their own window and cannot live in a pty. Putting
/// one in a pane gives a blank grid and a child that never speaks — which
/// is exactly what a hung editor looks like.
const GUI_EDITORS: &[&str] = &[
    "notepad",
    "notepad++",
    "wordpad",
    "code",
    "code-insiders",
    "codium",
    "cursor",
    "windsurf",
    "zed",
    "subl",
    "sublime_text",
    "atom",
    "gvim",
    "mvim",
    "devenv",
    "idea",
    "rider",
    "pycharm",
    "webstorm",
];

/// Terminal editors worth looking for when nothing is configured, best
/// first. Only reached on a system with no `$VISUAL`/`$EDITOR` at all.
const FALLBACKS: &[&str] = &["nvim", "vim", "hx", "helix", "micro", "nano", "vi"];

/// Whether `editor` brings its own window. Matched on the program name, so
/// a full path or a command with flags still resolves.
pub fn is_gui(editor: &str) -> bool {
    let program = editor.split_whitespace().next().unwrap_or(editor);
    let stem = program_stem(program);
    GUI_EDITORS.contains(&stem.as_str())
}

/// Extracts a program name from either Unix or Windows path syntax,
/// regardless of which platform the daemon itself is running on.
pub(crate) fn program_stem(program: &str) -> String {
    let file = program.rsplit(['/', '\\']).next().unwrap_or(program);
    Path::new(file)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// `$VISUAL`, then `$EDITOR`, then whichever terminal editor is actually
/// installed.
///
/// The last step matters most on Windows, which ships no terminal editor:
/// the old fallback was `notepad`, and notepad in a pty is a blank pane
/// with no way to tell it has failed. Falling back to a name that isn't
/// installed at least fails loudly.
pub fn resolve() -> String {
    for var in ["VISUAL", "EDITOR"] {
        if let Some(v) = std::env::var_os(var) {
            let v = v.to_string_lossy().trim().to_string();
            if !v.is_empty() {
                return v;
            }
        }
    }
    FALLBACKS
        .iter()
        .find(|e| on_path(e))
        .map(|e| e.to_string())
        // Nothing installed. On Windows notepad always is, and as a GUI
        // editor it opens in its own window rather than a dead pane.
        .unwrap_or_else(|| if cfg!(windows) { "notepad" } else { "vi" }.to_string())
}

/// Whether `program` is runnable, honouring `PATHEXT` on Windows — where a
/// bare name matches `nvim.exe`, `nvim.cmd`, and friends.
fn on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(|e| e.to_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&path).any(|dir| {
        exts.iter()
            .any(|ext| dir.join(format!("{program}{ext}")).is_file())
    })
}

/// The argv to open `path` at `line`. `editor` may carry its own flags
/// (`"code -w"`), which are kept ahead of the file.
///
/// Line-number syntax differs per editor and there is no probing it, so
/// unknown editors just get the path — off by a jump beats failing to open.
pub fn command(editor: &str, path: &str, line: Option<u32>) -> Vec<String> {
    let mut argv: Vec<String> = editor.split_whitespace().map(str::to_string).collect();
    if argv.is_empty() {
        argv.push("vi".to_string());
    }
    let program = program_stem(&argv[0]);

    match (line, program.as_str()) {
        (Some(n), "vi" | "vim" | "nvim" | "nano" | "emacs" | "emacsclient" | "gedit" | "kak") => {
            argv.push(format!("+{n}"));
            argv.push(path.to_string());
        }
        (Some(n), "hx" | "helix" | "subl" | "sublime_text") => {
            argv.push(format!("{path}:{n}"));
        }
        (Some(n), "code" | "codium" | "cursor") => {
            argv.push("--goto".to_string());
            argv.push(format!("{path}:{n}"));
        }
        _ => argv.push(path.to_string()),
    }
    argv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vi_family_takes_a_plus_line_argument() {
        assert_eq!(
            command("nvim", "src/a.rs", Some(42)),
            ["nvim", "+42", "src/a.rs"]
        );
        assert_eq!(command("vim", "a.rs", Some(1)), ["vim", "+1", "a.rs"]);
    }

    #[test]
    fn helix_and_sublime_take_the_line_on_the_path() {
        assert_eq!(command("hx", "src/a.rs", Some(9)), ["hx", "src/a.rs:9"]);
    }

    #[test]
    fn vscode_needs_its_goto_flag() {
        assert_eq!(
            command("code", "src/a.rs", Some(9)),
            ["code", "--goto", "src/a.rs:9"]
        );
    }

    #[test]
    fn an_unknown_editor_still_opens_the_file() {
        // Off by a jump beats refusing to open it at all.
        assert_eq!(command("ed", "a.rs", Some(9)), ["ed", "a.rs"]);
    }

    #[test]
    fn with_no_line_every_editor_just_gets_the_path() {
        assert_eq!(command("nvim", "a.rs", None), ["nvim", "a.rs"]);
        assert_eq!(command("code", "a.rs", None), ["code", "a.rs"]);
    }

    #[test]
    fn flags_in_the_editor_string_are_kept_ahead_of_the_file() {
        assert_eq!(
            command("code -w", "a.rs", Some(3)),
            ["code", "-w", "--goto", "a.rs:3"]
        );
    }

    #[test]
    fn a_full_path_to_an_editor_is_still_recognised() {
        assert_eq!(
            command("/usr/bin/nvim", "a.rs", Some(3)),
            ["/usr/bin/nvim", "+3", "a.rs"]
        );
        assert_eq!(
            command(r"C:\Program Files\Neovim\nvim.exe", "a.rs", Some(3)),
            [r"C:\Program", "Files\\Neovim\\nvim.exe", "a.rs"],
            "a space in the path defeats the split, as it would in a shell"
        );
    }

    #[test]
    fn an_empty_editor_setting_falls_back_rather_than_spawning_nothing() {
        assert_eq!(command("", "a.rs", None), ["vi", "a.rs"]);
    }

    #[test]
    fn gui_editors_are_recognised_however_they_are_written() {
        assert!(is_gui("notepad"));
        assert!(is_gui("Notepad.exe"));
        assert!(is_gui(r"C:\Windows\System32\notepad.exe"));
        assert!(is_gui("code -w"));
        assert!(is_gui("/usr/bin/subl"));
    }

    #[test]
    fn terminal_editors_are_not_mistaken_for_gui_ones() {
        for e in ["nvim", "vim", "hx", "nano", "emacs", "/usr/bin/nvim", "vim -u NONE"] {
            assert!(!is_gui(e), "{e} belongs in a pane");
        }
    }

    #[test]
    fn resolve_prefers_visual_then_editor() {
        // Serialised by nothing: this is the only test touching these vars.
        std::env::set_var("VISUAL", "hx");
        std::env::set_var("EDITOR", "nvim");
        assert_eq!(resolve(), "hx");
        std::env::remove_var("VISUAL");
        assert_eq!(resolve(), "nvim");
        std::env::remove_var("EDITOR");
        assert!(!resolve().is_empty());
    }
}
