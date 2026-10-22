//! The ex commands there are and how a name is matched to one: the library's
//! table of names and the shortest each may be written, and the rule that
//! a typed name is the command whose short name begins it and whose full
//! name begins with all of it. `:wri` is `:write`; `:wq`, which no short
//! name begins, is no command.

/// A command: its full name, and the short name it may be written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub name: &'static str,
    pub short: &'static str,
    /// Running it leaves `:` as it was, for the commands that only show.
    pub quiet: bool,
}

const fn e(name: &'static str, short: &'static str) -> Entry {
    Entry { name, short, quiet: false }
}

pub const TABLE: &[Entry] = &[
    e("colorscheme", "colo"),
    e("map", "map"),
    e("imap", "im"),
    e("nmap", "nm"),
    e("vmap", "vm"),
    e("omap", "om"),
    e("noremap", "no"),
    e("nnoremap", "nn"),
    e("vnoremap", "vn"),
    e("inoremap", "ino"),
    e("onoremap", "ono"),
    e("unmap", "unmap"),
    e("mapclear", "mapc"),
    e("nmapclear", "nmapc"),
    e("vmapclear", "vmapc"),
    e("imapclear", "imapc"),
    e("omapclear", "omapc"),
    e("write", "w"),
    e("undo", "u"),
    e("redo", "red"),
    e("set", "se"),
    e("setlocal", "setl"),
    e("setglobal", "setg"),
    e("sort", "sor"),
    e("substitute", "s"),
    e("startinsert", "start"),
    e("nohlsearch", "noh"),
    e("yank", "y"),
    e("put", "pu"),
    e("delmarks", "delm"),
    Entry { name: "marks", short: "marks", quiet: true },
    Entry { name: "registers", short: "reg", quiet: true },
    e("vglobal", "v"),
    e("delete", "d"),
    e("join", "j"),
    e("normal", "norm"),
    e("global", "g"),
    e("version", "ve"),
];

/// The command a typed name stands for.
pub fn find(typed: &str) -> Option<Entry> {
    for i in (1..=typed.len()).rev() {
        if !typed.is_char_boundary(i) {
            continue;
        }
        let prefix = &typed[..i];
        if let Some(entry) = TABLE.iter().find(|e| e.short == prefix) {
            if entry.name.starts_with(typed) {
                return Some(*entry);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;
