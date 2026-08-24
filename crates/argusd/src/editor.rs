//! Opening a file in the user's own editor, as a pane (DESIGN.md §6, §9 M4).

use std::path::Path;

/// `$VISUAL`, then `$EDITOR`, then a platform default.
pub fn resolve() -> String {
    for var in ["VISUAL", "EDITOR"] {
        if let Some(v) = std::env::var_os(var) {
            let v = v.to_string_lossy().trim().to_string();
            if !v.is_empty() {
                return v;
            }
        }
    }
    if cfg!(windows) { "notepad" } else { "vi" }.to_string()
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
    let program = Path::new(&argv[0])
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();

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
