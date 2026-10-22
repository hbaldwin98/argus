//! A file's grammar, ready to parse with: the WebAssembly runtime started
//! once, each language compiled and its query checked once, from what is
//! installed. The page's `ts:code/grammar/load.ts` on tree-sitter's native
//! runtime, which runs the same parser files through wasmtime.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use tree_sitter::wasmtime::Engine;
use tree_sitter::{Language, Parser, Query, WasmStore};

use super::store::{key_for, Installed, Store};

/// A grammar that loaded: its language compiled and its query accepted by it.
pub struct Loaded {
    pub name: String,
    pub label: String,
    pub language: Language,
    pub query: Query,
    /// The query as text, for a highlighter that compiles its own.
    pub source: String,
}

/// One engine for every grammar: a module compiled in it can be instantiated
/// in any store made from it, which is how every parser gets a store of its own.
fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(Engine::default)
}

/// Compiles the parser and its query. A grammar that fails either is an
/// error here, so it is never kept (EDITOR-GRAM-8).
pub fn load(grammar: &Installed) -> Result<Loaded, String> {
    let mut store = WasmStore::new(engine()).map_err(|e| e.to_string())?;
    // The symbol the parser exports, not the name it is kept under.
    let language = store.load_language(&grammar.symbol, &grammar.wasm).map_err(|e| e.to_string())?;
    let query = Query::new(&language, &grammar.query).map_err(|e| e.to_string())?;
    Ok(Loaded { name: grammar.name.clone(), label: grammar.label.clone(), language, query, source: grammar.query.clone() })
}

impl Loaded {
    /// A parser for this grammar. Each has a store of its own, because a
    /// parser runs the grammar's scanner in the store it holds and two
    /// threads cannot share one; the compiled module is shared.
    pub fn parser(&self) -> Result<Parser, String> {
        let mut parser = Parser::new();
        parser.set_wasm_store(WasmStore::new(engine()).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        parser.set_language(&self.language).map_err(|e| e.to_string())?;
        Ok(parser)
    }
}

/// What the window has installed and loaded, by the name a file's grammar is
/// kept under.
pub struct Loader {
    store: Store,
    /// The ones loaded, for an editor to colour with in its first paint.
    done: Mutex<HashMap<String, Arc<Loaded>>>,
}

impl Loader {
    pub fn new(store: Store) -> Self {
        Loader { store, done: Mutex::new(HashMap::new()) }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The grammar installed for `path`, loaded now if it is not yet: `None`
    /// when there is none, the error when one is installed and will not load.
    pub fn installed_for(&self, path: &str) -> Result<Option<Arc<Loaded>>, String> {
        let Some(name) = key_for(path) else { return Ok(None) };
        if let Some(done) = self.loaded(&name) {
            return Ok(Some(done));
        }
        let Some(installed) = self.store.installed(&name) else { return Ok(None) };
        load(&installed).map(|loaded| Some(self.adopt(loaded)))
    }

    /// The grammar for `path` if it is loaded already, without waiting.
    pub fn loaded_for(&self, path: &str) -> Option<Arc<Loaded>> {
        self.loaded(&key_for(path)?)
    }

    fn loaded(&self, name: &str) -> Option<Arc<Loaded>> {
        self.done.lock().ok()?.get(name).cloned()
    }

    /// Holds `loaded` as the one for its name.
    pub fn adopt(&self, loaded: Loaded) -> Arc<Loaded> {
        let loaded = Arc::new(loaded);
        if let Ok(mut done) = self.done.lock() {
            done.insert(loaded.name.clone(), loaded.clone());
        }
        loaded
    }
}

#[cfg(test)]
mod tests;
