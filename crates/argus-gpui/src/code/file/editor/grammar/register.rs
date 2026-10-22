//! A grammar that loaded, put where the kit's highlighter looks for languages: its query with the
//! captures the kit would not colour renamed (`theme/syntax`), and a parser factory that hands each
//! parse a fresh parser. The factory runs on every background parse and every injection, so it
//! holds the loaded grammar and never reads the file or compiles the module again.

use std::sync::Arc;

use argus_desktop_core::code::grammar::load::Loaded;
use gpui_kit::component::highlighter::{GrammarConfig, LanguageRegistry};
use tree_sitter::{Language, Parser};

use crate::theme::syntax;

/// What makes a parser: one for each parse, since a parser holds its own store.
pub type Make = Arc<dyn Fn() -> anyhow::Result<(Parser, Language)> + Send + Sync>;

/// The language `name` with `query` as its highlights, parsed by what `make` makes. An editor
/// already showing the name does not see it until it is told the language again.
pub fn language(name: &str, language: Language, query: &str, make: Make) {
    let registry = LanguageRegistry::singleton();
    registry.register(name, &GrammarConfig::new(name, language, Vec::new(), &syntax::query(query), "", ""));
    registry.register_parser_factory(name, make);
}

/// An installed grammar made the kit's language of its name.
pub fn loaded(grammar: &Arc<Loaded>) {
    let held = grammar.clone();
    language(
        &grammar.name,
        grammar.language.clone(),
        &grammar.source,
        Arc::new(move || {
            let parser = held.parser().map_err(anyhow::Error::msg)?;
            Ok((parser, held.language.clone()))
        }),
    );
}

#[cfg(test)]
mod tests;
