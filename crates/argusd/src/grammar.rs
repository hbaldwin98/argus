//! Which tree-sitter grammar reads a file, chosen by its extension. Both
//! highlighting and the outline of a file parse with it, so they agree on
//! what language a file is in.

use tree_sitter::Language;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Grammar {
    Rust,
    TypeScript,
    Tsx,
    Python,
    CSharp,
    Css,
    Yaml,
    Toml,
    Json,
    Markdown,
}

impl Grammar {
    /// Extension alone. Content sniffing and shebang lines would buy a
    /// handful of extensionless files at the cost of reading every blob
    /// twice.
    pub fn for_path(path: &str) -> Option<Grammar> {
        let name = path.rsplit('/').next().unwrap_or(path);
        let (_, ext) = name.rsplit_once('.')?;
        Some(match ext.to_ascii_lowercase().as_str() {
            "rs" => Grammar::Rust,
            "ts" | "mts" | "cts" => Grammar::TypeScript,
            // The TSX grammar is a superset, and plain TypeScript rejects
            // JSX, so JavaScript in any spelling is better served by TSX
            // than by nothing.
            "tsx" | "js" | "jsx" | "mjs" | "cjs" => Grammar::Tsx,
            "py" | "pyi" => Grammar::Python,
            "cs" | "csx" => Grammar::CSharp,
            "css" => Grammar::Css,
            "yaml" | "yml" => Grammar::Yaml,
            "toml" => Grammar::Toml,
            "json" | "jsonc" => Grammar::Json,
            "md" | "markdown" => Grammar::Markdown,
            _ => return None,
        })
    }

    pub fn language(self) -> Language {
        match self {
            Grammar::Rust => tree_sitter_rust::LANGUAGE.into(),
            Grammar::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Grammar::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Grammar::Python => tree_sitter_python::LANGUAGE.into(),
            Grammar::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Grammar::Css => tree_sitter_css::LANGUAGE.into(),
            Grammar::Yaml => tree_sitter_yaml::LANGUAGE.into(),
            Grammar::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            Grammar::Json => tree_sitter_json::LANGUAGE.into(),
            Grammar::Markdown => tree_sitter_md::LANGUAGE.into(),
        }
    }

    /// The name tree-sitter's tooling knows the grammar by.
    pub fn name(self) -> &'static str {
        match self {
            Grammar::Rust => "rust",
            Grammar::TypeScript => "typescript",
            Grammar::Tsx => "tsx",
            Grammar::Python => "python",
            Grammar::CSharp => "c-sharp",
            Grammar::Css => "css",
            Grammar::Yaml => "yaml",
            Grammar::Toml => "toml",
            Grammar::Json => "json",
            Grammar::Markdown => "markdown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grammar_is_chosen_by_extension_in_any_case() {
        assert_eq!(Grammar::for_path("src/a.RS"), Some(Grammar::Rust));
        assert_eq!(Grammar::for_path("page/src/main.mjs"), Some(Grammar::Tsx));
        assert_eq!(Grammar::for_path("Makefile"), None);
        assert_eq!(Grammar::for_path("dir.rs/file"), None, "the file's name, not a directory's");
    }
}
