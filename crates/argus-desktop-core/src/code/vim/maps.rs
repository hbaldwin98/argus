//! The mappings `:map` and its kin make: keys that stand for other keys in
//! a mode, and ex names that stand for another command or for keys. They
//! are shared by every editor and hold until `:unmap` or `:mapclear`. A
//! mapping with no mode applies in normal, visual and operator-pending mode,
//! never in insert mode, as the library has it. Reading a key against them
//! is `lookup`; running one is the editor's.

use super::key::{parse, Key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    OperatorPending,
}

/// The mode a mapping was made for; none is every mode but insert.
pub type Context = Option<Mode>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    pub id: usize,
    pub lhs: Vec<Key>,
    pub rhs: Vec<Key>,
    pub context: Context,
    /// The keys it stands for are read as the editor's own, not mapped again.
    pub noremap: bool,
}

/// A mapping of an ex command name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExMap {
    /// `:map :a :b` runs another command.
    Command(String),
    /// `:map :a keys` types keys.
    Keys(String),
}

#[derive(Debug, Default, Clone)]
pub struct Maps {
    /// Newest first, so a later mapping wins.
    list: Vec<Map>,
    ex: Vec<(String, ExMap)>,
    next_id: usize,
}

/// What a run of keys is to the mappings of a mode.
#[derive(Debug, PartialEq, Eq)]
pub enum Hit<'a> {
    /// Exactly a mapping's keys.
    Full(&'a Map),
    /// The start of one or more.
    Partial,
    None,
}

impl Maps {
    fn applies(map: &Map, mode: Mode) -> bool {
        match map.context {
            Some(m) => m == mode,
            None => mode != Mode::Insert,
        }
    }

    /// What `keys` are to the mappings of `mode`.
    pub fn lookup(&self, keys: &[Key], mode: Mode) -> Hit<'_> {
        let mut partial = false;
        for map in self.list.iter().filter(|m| Maps::applies(m, mode)) {
            if map.lhs == keys {
                return Hit::Full(map);
            }
            if map.lhs.starts_with(keys) {
                partial = true;
            }
        }
        if partial {
            Hit::Partial
        } else {
            Hit::None
        }
    }

    /// Makes a mapping from `lhs` to `rhs`, as notation. An ex name (`:a`) is
    /// a command mapping and takes no mode.
    pub fn add(&mut self, lhs: &str, rhs: &str, context: Context, noremap: bool) -> Result<(), String> {
        if lhs != ":" && lhs.starts_with(':') {
            if context.is_some() {
                return Err("Mode not supported for ex mappings".into());
            }
            let name = lhs[1..].to_owned();
            let to = if rhs != ":" && rhs.starts_with(':') { ExMap::Command(rhs[1..].to_owned()) } else { ExMap::Keys(rhs.to_owned()) };
            self.ex.retain(|(n, _)| *n != name);
            self.ex.push((name, to));
            return Ok(());
        }
        let id = self.next_id;
        self.next_id += 1;
        self.list.insert(0, Map { id, lhs: parse(lhs), rhs: parse(rhs), context, noremap });
        Ok(())
    }

    /// Removes the mapping of `lhs` in `context`; whether there was one.
    pub fn remove(&mut self, lhs: &str, context: Context) -> Result<bool, String> {
        if lhs != ":" && lhs.starts_with(':') {
            if context.is_some() {
                return Err("Mode not supported for ex mappings".into());
            }
            let before = self.ex.len();
            self.ex.retain(|(n, _)| n != &lhs[1..]);
            return Ok(self.ex.len() != before);
        }
        let keys = parse(lhs);
        match self.list.iter().position(|m| m.lhs == keys && m.context == context) {
            Some(i) => {
                self.list.remove(i);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// `:mapclear`: every mapping, or only those of `mode` — a mapping with
    /// no mode is kept for the others.
    pub fn clear(&mut self, mode: Option<Mode>) {
        let Some(mode) = mode else {
            self.list.clear();
            return;
        };
        let mut kept = Vec::new();
        for map in self.list.drain(..) {
            match map.context {
                Some(m) if m != mode => kept.push(map),
                Some(_) => {}
                None => {
                    for other in [Mode::Normal, Mode::Insert, Mode::Visual] {
                        if other != mode {
                            kept.push(Map { context: Some(other), ..map.clone() });
                        }
                    }
                }
            }
        }
        self.list = kept;
    }

    /// What a command name is mapped to.
    pub fn ex(&self, name: &str) -> Option<&ExMap> {
        self.ex.iter().find(|(n, _)| n == name).map(|(_, m)| m)
    }
}

#[cfg(test)]
mod tests;
