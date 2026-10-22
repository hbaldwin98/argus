//! What grammar to offer for a file, and downloading it. One Argus knows is
//! pinned: each file is checked against the SHA-256 recorded in `known` and
//! refused if it differs. For any other extension npm is searched for a
//! `tree-sitter-…` package that ships a prebuilt parser and highlight query;
//! nothing vouches for what that finds, so it is only ever installed after
//! the person has read its name and said yes (`install`).
//!
//! The page's `ts:code/grammar/fetch.ts`, the same searches and answers.

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::known::{known_for, Known, Pinned};
use super::net::{Http, FILE_LIMIT, TEXT_LIMIT};
use super::store::Installed;

/// A file an offer would download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub url: String,
    pub size: u64,
    /// Lower-case hex; `None` for a file nothing vouches for.
    pub sha256: Option<String>,
}

impl From<&Pinned> for Remote {
    fn from(pin: &Pinned) -> Self {
        Remote { url: pin.url(), size: pin.size, sha256: Some(pin.sha256.to_owned()) }
    }
}

/// A grammar the editor can offer for a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub name: String,
    pub label: String,
    /// `package@version`.
    pub source: String,
    /// The `tree_sitter_<symbol>` the parser exports, which is not always the
    /// name it is kept under (a package's only parser may be named anything).
    pub symbol: String,
    pub wasm: Remote,
    pub queries: Vec<Remote>,
    /// Argus pinned and hashed it; otherwise npm search found it.
    pub pinned: bool,
    /// Who published a package npm search found.
    pub publisher: Option<String>,
}

impl Offer {
    /// The parser's size and the queries', which is what the person is told it costs.
    pub fn bytes(&self) -> u64 {
        self.wasm.size + self.queries.iter().map(|q| q.size).sum::<u64>()
    }

    fn pinned(known: &Known) -> Offer {
        Offer {
            name: known.name.to_owned(),
            label: known.label.to_owned(),
            source: known.package.to_owned(),
            symbol: symbol_of(known.wasm.file),
            wasm: (&known.wasm).into(),
            queries: known.queries.iter().map(Remote::from).collect(),
            pinned: true,
            publisher: None,
        }
    }
}

/// Extensions whose language npm names differently, and the plain names that
/// are worth listing; any other extension that reads like a name is its own.
const NAMES: [(&str, &str); 33] = [
    ("kt", "kotlin"),
    ("kts", "kotlin"),
    ("yml", "yaml"),
    ("md", "markdown"),
    ("ml", "ocaml"),
    ("mli", "ocaml"),
    ("erl", "erlang"),
    ("hrl", "erlang"),
    ("pl", "perl"),
    ("pm", "perl"),
    ("r", "r"),
    ("sql", "sql"),
    ("tf", "hcl"),
    ("hcl", "hcl"),
    ("nix", "nix"),
    ("dart", "dart"),
    ("svelte", "svelte"),
    ("vue", "vue"),
    ("proto", "proto"),
    ("dockerfile", "dockerfile"),
    ("mk", "make"),
    ("cmake", "cmake"),
    ("gleam", "gleam"),
    ("odin", "odin"),
    ("sol", "solidity"),
    ("v", "v"),
    ("nim", "nim"),
    ("clj", "clojure"),
    ("fs", "fsharp"),
    ("fsx", "fsharp"),
    ("hx", "haxe"),
    ("graphql", "graphql"),
    ("gql", "graphql"),
];

/// What npm calls the language `path` is in, by its extension, or `None` for
/// a file that has none or one that could not be a package's name.
pub fn language_of(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    match name.as_str() {
        "dockerfile" => return Some("dockerfile".to_owned()),
        "makefile" => return Some("make".to_owned()),
        _ => {}
    }
    let dot = name.rfind('.').filter(|dot| *dot > 0)?;
    let ext = &name[dot + 1..];
    if let Some((_, language)) = NAMES.iter().find(|(e, _)| *e == ext) {
        return Some((*language).to_owned());
    }
    // `^[a-z][a-z0-9]{0,11}$`
    let mut chars = ext.chars();
    let name_like = chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && ext.len() <= 12
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    name_like.then(|| ext.to_owned())
}

/// The grammar to offer for `path`: the pinned one, or failing that the first
/// npm package that ships what a grammar needs. Offline, or with npm away,
/// there is nothing to offer this time.
pub fn offer_for(path: &str, http: &dyn Http) -> Option<Offer> {
    if let Some(known) = known_for(path) {
        return Some(Offer::pinned(known));
    }
    search(&language_of(path)?, http)
}

#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    objects: Vec<Found>,
}

#[derive(Deserialize)]
struct Found {
    package: Package,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
    publisher: Option<Publisher>,
}

#[derive(Deserialize)]
struct Publisher {
    username: Option<String>,
}

#[derive(Deserialize)]
struct Listing {
    #[serde(default)]
    files: Vec<Listed>,
}

#[derive(Deserialize)]
struct Listed {
    name: String,
    size: u64,
}

/// Its own name or a grammar collective's, never one that only mentions it.
fn named(package: &str, language: &str) -> bool {
    let own = format!("tree-sitter-{language}");
    match package.strip_prefix('@') {
        Some(scoped) => scoped.split_once('/').is_some_and(|(scope, rest)| {
            !scope.is_empty()
                && scope.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                && rest == own
        }),
        None => package == own,
    }
}

/// The symbol a parser file exports: its stem without `tree-sitter-`, with
/// `-` as `_`, which is how tree-sitter names the function it generates
/// (`tree-sitter-c_sharp.wasm` exports `tree_sitter_c_sharp`).
pub fn symbol_of(file: &str) -> String {
    let name = file.rsplit('/').next().unwrap_or(file);
    let stem = name.strip_suffix(".wasm").unwrap_or(name);
    stem.strip_prefix("tree-sitter-").unwrap_or(stem).replace('-', "_")
}

/// `/<one name>.wasm`: a parser at the package's top level.
fn top_level_parser(name: &str) -> bool {
    name.strip_prefix('/')
        .and_then(|rest| rest.strip_suffix(".wasm"))
        .is_some_and(|stem| !stem.is_empty() && !stem.contains('/'))
}

fn search(language: &str, http: &dyn Http) -> Option<Offer> {
    let url = format!("https://registry.npmjs.org/-/v1/search?text=tree-sitter-{language}&size=10");
    let found: Search = serde_json::from_slice(&http.get(&url, TEXT_LIMIT).ok()?).ok()?;
    // One at a time, in npm's order, stopping at the first that will do.
    for Found { package: p } in found.objects.into_iter().filter(|f| named(&f.package.name, language)) {
        let listing = format!("https://data.jsdelivr.com/v1/packages/npm/{}@{}?structure=flat", p.name, p.version);
        let Ok(body) = http.get(&listing, TEXT_LIMIT) else { continue };
        let Ok(Listing { files }) = serde_json::from_slice(&body) else { continue };
        // The parser named for the language, or the only one: a package with
        // several (YAML's ships a schema's beside it) is not guessed at.
        let parsers: Vec<&Listed> = files.iter().filter(|f| top_level_parser(&f.name)).collect();
        let own = format!("/tree-sitter-{language}.wasm");
        let wasm = parsers.iter().find(|f| f.name == own).copied().or_else(|| match parsers[..] {
            [only] => Some(only),
            _ => None,
        });
        let query = files.iter().find(|f| f.name == "/queries/highlights.scm");
        let (Some(wasm), Some(query)) = (wasm, query) else { continue };
        let base = format!("https://cdn.jsdelivr.net/npm/{}@{}", p.name, p.version);
        let remote = |f: &Listed| Remote { url: format!("{base}{}", f.name), size: f.size, sha256: None };
        return Some(Offer {
            name: language.to_owned(),
            label: language.to_owned(),
            source: format!("{}@{}", p.name, p.version),
            symbol: symbol_of(&wasm.name),
            wasm: remote(wasm),
            queries: vec![remote(query)],
            pinned: false,
            publisher: p.publisher.and_then(|p| p.username).filter(|u| !u.is_empty()),
        });
    }
    None
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

fn get(file: &Remote, limit: u64, http: &dyn Http) -> Result<Vec<u8>, String> {
    let bytes = http.get(&file.url, limit)?;
    match &file.sha256 {
        Some(expected) if sha256_hex(&bytes) != *expected => Err(format!("{} is not the file Argus checked", file.url)),
        _ => Ok(bytes),
    }
}

/// Downloads what `offer` names, each pinned file checked. The queries are
/// read as UTF-8 and joined by newlines, as one query.
pub fn download(offer: &Offer, http: &dyn Http) -> Result<Installed, String> {
    let wasm = get(&offer.wasm, FILE_LIMIT, http)?;
    let mut queries = Vec::new();
    for query in &offer.queries {
        queries.push(String::from_utf8_lossy(&get(query, TEXT_LIMIT, http)?).into_owned());
    }
    Ok(Installed {
        name: offer.name.clone(),
        label: offer.label.clone(),
        source: offer.source.clone(),
        symbol: offer.symbol.clone(),
        wasm,
        query: queries.join("\n"),
    })
}

#[cfg(test)]
mod tests;
