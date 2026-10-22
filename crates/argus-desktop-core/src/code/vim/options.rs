//! The options `:set` reads and writes. The library registers five and
//! Argus adds none (EDITOR-VIM-14): `filetype` (`ft`), `textwidth` (`tw`),
//! `langmap` (`lmap`), `pcre` and `insertModeEscKeysTimeout`. `filetype`
//! and `textwidth` belong to one editor; the others are set for all of
//! them, or for one editor by `:setlocal`. Anything else is unknown, as
//! `ignorecase` and `tabstop` are.

use std::collections::HashMap;
use std::fmt;

/// What an option holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Number(f64),
    Text(String),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{b}"),
            Value::Number(n) => write!(f, "{n}"),
            Value::Text(t) => write!(f, "{t}"),
        }
    }
}

/// What `:set` was given to set an option to.
#[derive(Debug, Clone, PartialEq)]
pub enum Given {
    /// A bare name: a boolean is set.
    Unset,
    /// `no` before the name.
    Off,
    /// `name=text`
    Text(String),
}

/// Which copy of an option a `:set` is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// `:set`: the editor's own copy where it has one, and the shared one.
    Both,
    /// `:setlocal`
    Local,
    /// `:setglobal`
    Global,
}

/// The options every editor shares.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub pcre: bool,
    pub esc_timeout: f64,
    pub langmap: String,
    /// `langmap` read: what a typed character stands for in normal and
    /// visual mode.
    pub keymap: HashMap<char, char>,
}

impl Default for Options {
    fn default() -> Options {
        Options { pcre: true, esc_timeout: 200.0, langmap: String::new(), keymap: HashMap::new() }
    }
}

/// An editor's own options.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Locals {
    pub textwidth: Option<f64>,
    pub filetype: Option<String>,
    pub pcre: Option<bool>,
    pub esc_timeout: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bool,
    Number,
    Text,
}

/// An option's name, as `:set` may spell it, and its kind.
fn named(name: &str) -> Option<(&'static str, Kind)> {
    Some(match name {
        "filetype" | "ft" => ("filetype", Kind::Text),
        "textwidth" | "tw" => ("textwidth", Kind::Number),
        "langmap" | "lmap" => ("langmap", Kind::Text),
        "pcre" => ("pcre", Kind::Bool),
        "insertModeEscKeysTimeout" => ("insertModeEscKeysTimeout", Kind::Number),
        _ => return None,
    })
}

/// The characters of `list` with a backslash taken off the one it escapes.
fn unescaped_chars(list: &str) -> Vec<char> {
    let mut out = Vec::new();
    let mut chars = list.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Splits `s` at the commas (or semicolons) that are not escaped.
fn split_unescaped(s: &str, at: char) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            parts.last_mut().expect("one part").push(c);
            if let Some(n) = chars.next() {
                parts.last_mut().expect("one part").push(n);
            }
        } else if c == at {
            parts.push(String::new());
        } else {
            parts.last_mut().expect("one part").push(c);
        }
    }
    parts
}

/// `langmap` as Vim reads it: parts separated by commas, each a list of
/// pairs (`aA`) or `from;to` lists (`abc;ABC`); a malformed part is skipped.
pub fn parse_langmap(list: &str) -> HashMap<char, char> {
    let mut map = HashMap::new();
    for part in split_unescaped(list, ',') {
        if part.is_empty() {
            continue;
        }
        let halves = split_unescaped(&part, ';');
        if halves.len() == 2 {
            let (from, to) = (unescaped_chars(&halves[0]), unescaped_chars(&halves[1]));
            if from.len() == to.len() {
                map.extend(from.into_iter().zip(to));
            }
        } else if halves.len() == 1 {
            let pairs = unescaped_chars(&part);
            if pairs.len().is_multiple_of(2) {
                map.extend(pairs.chunks(2).map(|p| (p[0], p[1])));
            }
        }
    }
    map
}

/// Whether the option is a boolean, for `:set name` and `:set noname`.
pub fn is_boolean(name: &str) -> bool {
    named(name).is_some_and(|(_, k)| k == Kind::Bool)
}

impl Options {
    /// The option's value for an editor with these `locals`.
    pub fn get(&self, name: &str, scope: Scope, locals: &Locals) -> Result<Option<Value>, String> {
        let Some((name, _)) = named(name) else { return Err(format!("Unknown option: {name}")) };
        let local = scope != Scope::Global;
        let global = scope != Scope::Local;
        Ok(match name {
            "filetype" => local.then(|| Value::Text(locals.filetype.clone().unwrap_or_default())),
            "textwidth" => local.then(|| Value::Number(locals.textwidth.unwrap_or(80.0))),
            "langmap" => global.then(|| Value::Text(self.langmap.clone())),
            "pcre" => local
                .then_some(locals.pcre)
                .flatten()
                .or(global.then_some(self.pcre))
                .map(Value::Bool),
            _ => local
                .then_some(locals.esc_timeout)
                .flatten()
                .or(global.then_some(self.esc_timeout))
                .map(Value::Number),
        })
    }

    /// Sets the option from what `:set` was given. A boolean takes none or
    /// off, and any text is an invalid argument; a number takes a number.
    pub fn set(&mut self, name: &str, given: Given, scope: Scope, locals: &mut Locals) -> Result<(), String> {
        let Some((canon, kind)) = named(name) else { return Err(format!("Unknown option: {name}")) };
        let local = scope != Scope::Global;
        let global = scope != Scope::Local;
        let text = match &given {
            Given::Text(t) => Some(t.as_str()),
            _ => None,
        };
        let bad = || format!("Invalid argument: {name}={}", text.unwrap_or_default());
        match canon {
            "filetype" => {
                if local {
                    locals.filetype = Some(text.unwrap_or_default().to_owned());
                }
            }
            "textwidth" => {
                if local {
                    let width = match &given {
                        Given::Off => 0.0,
                        _ => text.and_then(|v| v.trim().parse::<f64>().ok()).ok_or_else(bad)?.round(),
                    };
                    if width > 1.0 {
                        locals.textwidth = Some(width);
                    }
                }
            }
            "langmap" => {
                if global {
                    self.langmap = text.unwrap_or_default().to_owned();
                    self.keymap = parse_langmap(&self.langmap);
                }
            }
            "pcre" => {
                let on = match given {
                    Given::Unset => true,
                    Given::Off => false,
                    Given::Text(ref t) if t.is_empty() => true,
                    Given::Text(_) => return Err(bad()),
                };
                if global {
                    self.pcre = on;
                }
                if local {
                    locals.pcre = Some(on);
                }
            }
            _ => {
                debug_assert_eq!(kind, Kind::Number);
                let n = match &given {
                    Given::Off => 0.0,
                    _ => text.and_then(|v| v.trim().parse::<f64>().ok()).ok_or_else(bad)?,
                };
                if global {
                    self.esc_timeout = n;
                }
                if local {
                    locals.esc_timeout = Some(n);
                }
            }
        }
        Ok(())
    }

    /// The number of characters a row of text may hold before `gq` wraps it.
    pub fn text_width(locals: &Locals) -> usize {
        locals.textwidth.map_or(80, |w| w as usize)
    }
}

#[cfg(test)]
mod tests;
