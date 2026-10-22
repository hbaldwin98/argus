//! What a grammar's language writes by convention that its parser does not say: the token a line
//! comment begins with, and whether its blocks are bracketed by braces. `gc` and `=` ask once a
//! grammar is loaded (EDITOR-VIM-24); the page loads none, so there both change nothing.
//!
//! Keyed by a pinned grammar's `name` (`known`). A grammar found by searching npm is not in the
//! table and has neither: its `gc` and `=` do nothing, like an unbound key.

/// The token a line comment begins with, or none for a language that has no line comment (JSON,
/// CSS, HTML) or one Argus does not know.
pub fn comment_token(name: &str) -> Option<&'static str> {
    Some(match name {
        "rust" | "go" | "javascript" | "typescript" | "tsx" | "c" | "cpp" | "c_sharp" | "java" | "scala" | "zig" | "php" => "//",
        "python" | "bash" | "ruby" | "julia" | "toml" | "elixir" => "#",
        "haskell" | "lua" => "--",
        _ => return None,
    })
}

/// Whether the language's blocks are made by braces, so that a line's indent follows the brackets
/// open above it. A language whose blocks are made by layout or by keywords (Python, Ruby, Lua,
/// Haskell, shell) is not, and `=` leaves it alone.
pub fn braced(name: &str) -> bool {
    matches!(name, "rust" | "go" | "javascript" | "typescript" | "tsx" | "c" | "cpp" | "c_sharp" | "java" | "scala" | "zig" | "php" | "json" | "css")
}

#[cfg(test)]
mod tests {
    use super::super::known::KNOWN;
    use super::*;

    #[test]
    fn a_language_names_its_line_comment() {
        assert_eq!(comment_token("rust"), Some("//"));
        assert_eq!(comment_token("python"), Some("#"));
        assert_eq!(comment_token("lua"), Some("--"));
        assert_eq!(comment_token("json"), None);
        assert_eq!(comment_token("not-a-language"), None);
    }

    #[test]
    fn only_brace_languages_are_indented_by_their_brackets() {
        assert!(braced("rust") && braced("json"));
        assert!(!braced("python") && !braced("ruby") && !braced("html") && !braced("not-a-language"));
    }

    /// A language of the pinned list that has a line comment is not left out of the table by
    /// forgetting it: every one is either named here or has none.
    #[test]
    fn every_pinned_language_is_decided() {
        let none = ["json", "css", "html"];
        for known in KNOWN {
            assert!(comment_token(known.name).is_some() || none.contains(&known.name), "{} has no comment token", known.name);
        }
    }
}
