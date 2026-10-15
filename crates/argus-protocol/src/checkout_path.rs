//! Whether a path an agent wrote stays inside the checkout it is read
//! against.
//!
//! A path in a view is the agent's, and a click on it opens a file in the
//! person's editor, so it is checked by the daemon that holds the view and
//! again by the client that opens it. Both ask here, so the two cannot
//! disagree about what escapes.

/// Whether `path` names something inside a checkout: relative, with no `..`
/// and no component Windows would reduce to one. Text only; nothing is read
/// from disk, so a symlink inside the checkout is still the opener's to
/// resolve.
///
/// Windows drops trailing dots and spaces from a component, so `".. "` and
/// `"..."` both open the parent directory there; those are refused on every
/// platform, as is anything with a control character.
pub fn is_checkout_relative(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    if path.chars().any(char::is_control) {
        return false;
    }
    let mut chars = path.chars();
    if let (Some(first), Some(':')) = (chars.next(), chars.next()) {
        if first.is_ascii_alphabetic() {
            return false;
        }
    }
    let mut named = false;
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            _ if part.chars().all(|c| c == '.') => return false,
            _ if part.ends_with(['.', ' ']) => return false,
            _ => named = true,
        }
    }
    named
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_below_the_root_is_relative() {
        for path in ["src/main.rs", "a", "./src/lib.rs", "dir\\file.txt", "a//b", ".github/ci.yml", "v1.2/x"] {
            assert!(is_checkout_relative(path), "{path}");
        }
    }

    #[test]
    fn nothing_that_is_empty_or_absolute_is_relative() {
        for path in ["", ".", "./", "/etc/passwd", "\\windows", "\\\\server\\share\\f", "//host/share", "C:\\x", "c:/x", "d:x"] {
            assert!(!is_checkout_relative(path), "{path:?}");
        }
    }

    #[test]
    fn a_parent_component_anywhere_is_refused() {
        for path in ["..", "../x", "a/../b", "a/..", "a\\..\\b", "a/b/../../.."] {
            assert!(!is_checkout_relative(path), "{path:?}");
        }
    }

    #[test]
    fn a_component_windows_reduces_to_the_parent_is_refused() {
        for path in [".. ", "a/.. /b", "...", "a/..../b", "a/.. ", "a/b.", "a/b /c", "x\\... \\y"] {
            assert!(!is_checkout_relative(path), "{path:?}");
        }
    }

    #[test]
    fn a_control_character_is_refused() {
        assert!(!is_checkout_relative("a\nb"));
        assert!(!is_checkout_relative("a\u{0}b"));
    }
}
