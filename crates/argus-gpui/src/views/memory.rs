//! What the window alone remembers about the blocks of a view: the tab being read, a fold opened,
//! the rows ticked or opened and the filter typed in a table, what is half typed in a form, and
//! where a log was left.
//!
//! The daemon holds a view and sends it whole whenever its agent shows it again; none of this is in
//! it, and none of it may be lost to it, or a tab or a half-typed form would reset each time an
//! agent republishes (the page's module-level maps, `blocks.ts`, `details.ts`, `form.ts`,
//! `table/memory.ts`). So it is kept apart from the snapshot, by the path of the block it belongs
//! to (`pane/id.2.0`, never an index into the views), and never let go: the page's maps are never pruned
//! either, and a view list that is empty while the daemon is reconnecting must not wipe them. It is
//! shared by handle: a block changes it and repaints itself, and nothing else is told.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

/// The one store, shared by whoever draws a block and by `Heard`, which lets a view's go with it.
pub type Shared = Rc<RefCell<Memory>>;

/// What is typed in a form field but not sent: its text as typed, or a toggle's state. Core reads
/// what it means (`views::blocks::form`), so the one type is core's.
pub type Draft = argus_desktop_core::views::blocks::form::Typed;

/// Where a log was left: a log is a tail, so one nobody scrolled up in follows the newest line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogPlace {
    pub top: f32,
    pub tail: bool,
}

#[derive(Debug, Default)]
pub struct Memory {
    tabs: HashMap<String, usize>,
    folds: HashMap<String, bool>,
    selected: HashMap<String, BTreeSet<String>>,
    opened: HashMap<String, BTreeSet<String>>,
    filters: HashMap<String, String>,
    drafts: HashMap<String, Draft>,
    sent: HashMap<String, u32>,
    logs: HashMap<String, LogPlace>,
    asking: HashSet<String>,
}

/// The path an agent's view is drawn under, which every block's path begins with.
pub fn view_path(pane: u64, id: &str) -> String {
    format!("{pane}/{id}")
}

/// The path of the child `index` of the block at `path`.
pub fn child(path: &str, index: impl std::fmt::Display) -> String {
    format!("{path}.{index}")
}

fn trimmed(map: &mut HashMap<String, BTreeSet<String>>, path: &str, keys: &[String]) -> BTreeSet<String> {
    let set = map.entry(path.to_owned()).or_default();
    set.retain(|key| keys.contains(key));
    set.clone()
}

fn toggled(map: &mut HashMap<String, BTreeSet<String>>, path: &str, key: &str) {
    let set = map.entry(path.to_owned()).or_default();
    if !set.remove(key) {
        set.insert(key.to_owned());
    }
}

impl Memory {
    pub fn shared() -> Shared {
        Rc::default()
    }

    /// The tab chosen in the tabs block at `path`, held to the tabs there are now; the first when none
    /// was chosen. A view that came again with fewer tabs shows its last, not nothing.
    pub fn tab(&self, path: &str, count: usize) -> usize {
        self.tabs.get(path).copied().unwrap_or(0).min(count.saturating_sub(1))
    }

    pub fn choose_tab(&mut self, path: &str, tab: usize) {
        self.tabs.insert(path.to_owned(), tab);
    }

    /// Whether the fold at `path` is open: what the person last chose, else `default`, which is only
    /// how the agent says it starts.
    pub fn open(&self, path: &str, default: bool) -> bool {
        self.folds.get(path).copied().unwrap_or(default)
    }

    pub fn set_open(&mut self, path: &str, open: bool) {
        self.folds.insert(path.to_owned(), open);
    }

    /// The row keys ticked in the table at `path`, trimmed to the keys it has now.
    pub fn selection(&mut self, path: &str, keys: &[String]) -> BTreeSet<String> {
        trimmed(&mut self.selected, path, keys)
    }

    pub fn toggle_selected(&mut self, path: &str, key: &str) {
        toggled(&mut self.selected, path, key);
    }

    /// Ticks exactly `keys`, which is how a radio and select-all choose.
    pub fn set_selection(&mut self, path: &str, keys: impl IntoIterator<Item = String>) {
        self.selected.insert(path.to_owned(), keys.into_iter().collect());
    }

    /// The row keys opened to their details in the table at `path`, trimmed to the keys it has now.
    pub fn open_rows(&mut self, path: &str, keys: &[String]) -> BTreeSet<String> {
        trimmed(&mut self.opened, path, keys)
    }

    pub fn toggle_row(&mut self, path: &str, key: &str) {
        toggled(&mut self.opened, path, key);
    }

    pub fn filter(&self, path: &str) -> &str {
        self.filters.get(path).map_or("", String::as_str)
    }

    /// What was typed in the filter; nothing typed is nothing kept.
    pub fn set_filter(&mut self, path: &str, text: &str) {
        if text.is_empty() {
            self.filters.remove(path);
        } else {
            self.filters.insert(path.to_owned(), text.to_owned());
        }
    }

    pub fn draft(&self, path: &str) -> Option<&Draft> {
        self.drafts.get(path)
    }

    pub fn set_draft(&mut self, path: &str, draft: Draft) {
        self.drafts.insert(path.to_owned(), draft);
    }

    /// Forgets every draft under `prefix`: a form that was sent starts clean. It counts the sending,
    /// which is how the boxes a form's fields were typed in are told apart from the ones after.
    pub fn clear_drafts(&mut self, prefix: &str) {
        self.drafts.retain(|path, _| !path.starts_with(prefix));
        *self.sent.entry(prefix.to_owned()).or_default() += 1;
    }

    /// How many times the form under `prefix` was sent. A page redraws a sent form with fresh boxes;
    /// the boxes kept here are named with this, so a sent form gets fresh ones as well.
    pub fn sent(&self, prefix: &str) -> u32 {
        self.sent.get(prefix).copied().unwrap_or(0)
    }

    pub fn log(&self, path: &str) -> Option<LogPlace> {
        self.logs.get(path).copied()
    }

    pub fn set_log(&mut self, path: &str, place: LogPlace) {
        self.logs.insert(path.to_owned(), place);
    }

    /// Whether the confirmation of the action under `key` (`path|action id`) is open: a question
    /// the person is answering must not vanish when the agent shows the view again.
    pub fn asking(&self, key: &str) -> bool {
        self.asking.contains(key)
    }

    pub fn set_asking(&mut self, key: &str, open: bool) {
        if open {
            self.asking.insert(key.to_owned());
        } else {
            self.asking.remove(key);
        }
    }
}

#[cfg(test)]
mod tests;
