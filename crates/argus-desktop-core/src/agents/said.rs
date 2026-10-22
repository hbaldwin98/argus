//! How the recent-sessions dialog and the transcript reader put a session in words: a row's meta
//! line, the Ended group's summary, the filter menu's choices, the `started` chip, the reader's
//! meta line and match count, and the verb a tool call is told with.
//!
//! The page words these in `agents/sessions_dialog.ts` and `agents/transcript.ts`; a window with no
//! page needs the same words, and two copies would drift, so they are written once here for the
//! GPUI window and tested against the page's wording (SESSIONS-3, -4, -9, -15; TRANSCRIPT-2, -3, -9).

use crate::agents::sessions_dialog::{SessionPreview, SessionRowView, SessionsPage};
use crate::connection::page::{Call, ReaderPage};
use crate::system::clock::Clock;

/// `claude on main · 31 turns`: what ran where, as a sentence; a removed worktree is named as one.
pub fn row_meta(row: &SessionRowView) -> String {
    let turns = row.turns.map(turns_words);
    let place = if row.removed { format!("{}, since removed", row.checkout) } else { row.checkout.clone() };
    let mut parts = vec![format!("{} on {place}", row.harness)];
    parts.extend(turns);
    parts.join(" \u{b7} ")
}

/// `31 turns`, `1 turn`.
pub fn turns_words(turns: u32) -> String {
    format!("{turns} turn{}", if turns == 1 { "" } else { "s" })
}

/// The row's tooltip when it ran in a worktree since removed.
pub fn row_tip(row: &SessionRowView) -> Option<String> {
    row.removed.then(|| format!("ran in {}, a worktree since removed", row.checkout))
}

/// Whether the Ended group is drawn as the folding row: a search or filter asks for what is in the
/// group, and a listing with nothing live has no row to rest the cursor on but these, so neither
/// folds (SESSIONS-3).
pub fn folds(page: &SessionsPage) -> bool {
    page.rows.iter().any(|row| row.live) && page.search.trim().is_empty() && page.harness.is_none() && page.checkout.is_none()
}

/// `4 · newest 20m ago`: what the Ended row says after its name; the newest is left out when
/// unknown.
pub fn ended_summary(page: &SessionsPage) -> String {
    let newest = if page.ended_newest.is_empty() || page.ended_newest == "\u{2014}" { String::new() } else { format!(" \u{b7} newest {} ago", page.ended_newest) };
    format!("{}{newest}", page.ended_count)
}

/// One line of the Filter menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterItem {
    Heading(&'static str),
    Rule,
    /// A choice, and the two filters it sets (`None` is all): the one changed, the other kept.
    Choice { label: String, checked: bool, harness: Option<String>, checkout: Option<String> },
}

impl FilterItem {
    /// What choosing it sends.
    pub fn call(&self) -> Option<Call> {
        match self {
            Self::Choice { harness, checkout, .. } => Some(Call::FilterSessions { harness: harness.clone(), checkout: checkout.clone() }),
            _ => None,
        }
    }
}

/// What the Filter button says after its name: the active filters joined.
pub fn filter_active(page: &SessionsPage) -> Option<String> {
    let active: Vec<&str> = [page.harness.as_deref(), page.checkout.as_deref()].into_iter().flatten().collect();
    (!active.is_empty()).then(|| active.join(" \u{b7} "))
}

/// The Filter menu: a Harness section, a rule, a Checkout section; the current value is checked.
pub fn filter_items(page: &SessionsPage) -> Vec<FilterItem> {
    let section = |heading: &'static str, all: &str, options: &[String], chosen: &Option<String>, set: &dyn Fn(Option<String>) -> (Option<String>, Option<String>)| {
        let item = |label: &str, value: Option<String>| {
            let checked = *chosen == value;
            let (harness, checkout) = set(value);
            FilterItem::Choice { label: label.to_owned(), checked, harness, checkout }
        };
        let mut items = vec![FilterItem::Heading(heading), item(all, None)];
        items.extend(options.iter().map(|option| item(option, Some(option.clone()))));
        items
    };
    let (harness, checkout) = (page.harness.clone(), page.checkout.clone());
    let mut items = section("Harness", "Every harness", &page.harnesses, &page.harness, &|value| (value, checkout.clone()));
    items.push(FilterItem::Rule);
    items.extend(section("Checkout", "Every checkout", &page.checkouts, &page.checkout, &|value| (harness.clone(), value)));
    items
}

/// `started Oct 19, 7:02 AM`, or none without a start; an instant that does not parse is said as
/// the harness wrote it (SESSIONS-15).
pub fn started(preview: &SessionPreview, clock: &Clock) -> Option<String> {
    let at = preview.started.as_deref()?;
    Some(format!("started {}", match clock.zoned(at) {
        Some(zoned) => {
            let hour = zoned.hour();
            let (twelve, ampm) = match hour {
                0 => (12, "AM"),
                1..=11 => (hour, "AM"),
                12 => (12, "PM"),
                _ => (hour - 12, "PM"),
            };
            format!("{}, {twelve}:{:02} {ampm}", crate::system::clock::short(zoned.date()), zoned.minute())
        }
        None => at.to_owned(),
    }))
}

/// `3 earlier entries not shown`; the reader says `not kept: the newest are read` instead.
pub fn earlier(count: u32, reader: bool) -> String {
    let entries = if count == 1 { "entry" } else { "entries" };
    format!("{count} earlier {entries} {}", if reader { "not kept: the newest are read" } else { "not shown" })
}

/// The reader's meta line under its title, in parts: the repository, the checkout (always, so
/// drawn even when empty), the time span and the turns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderMeta {
    pub repository: String,
    pub checkout: String,
    pub span: Option<String>,
    pub turns: Option<String>,
}

pub fn reader_meta(reader: &ReaderPage, clock: &Clock) -> ReaderMeta {
    let session = &reader.session;
    ReaderMeta {
        repository: reader.repository.clone(),
        checkout: session.checkout.clone(),
        // A start that cannot be read leaves no span at all, the end included (`Clock::span`).
        span: clock.span(session.started.as_deref(), reader.ended.as_deref()),
        turns: reader.turns.map(turns_words),
    }
}

/// What sits beside the find field: nothing for an empty find, `no match`, or `2 of 5`.
pub fn find_count(reader: &ReaderPage) -> Option<String> {
    if reader.find.trim().is_empty() {
        return None;
    }
    if reader.matches.is_empty() {
        return Some("no match".to_owned());
    }
    let at = reader.found.and_then(|found| reader.matches.iter().position(|&m| m == found)).map_or(0, |at| at + 1);
    Some(format!("{at} of {}", reader.matches.len()))
}

/// The icon a tool call carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolIcon {
    Read,
    Search,
    Edit,
    Run,
}

/// A tool call told as prose with a small icon (`Read`, `Edited`, `Ran`, `Searched for`). A tool
/// not listed keeps its own name and no icon: a wrong verb would misreport what the agent did.
/// Harnesses name the same act differently (`Bash`, `shell`).
pub fn tool_verb(tool: &str) -> Option<(ToolIcon, &'static str)> {
    Some(match tool.to_lowercase().as_str() {
        "read" | "view" => (ToolIcon::Read, "Read"),
        "grep" | "glob" | "search" => (ToolIcon::Search, "Searched for"),
        "write" => (ToolIcon::Edit, "Wrote"),
        "edit" | "multiedit" | "notebookedit" | "apply_patch" => (ToolIcon::Edit, "Edited"),
        "bash" | "shell" | "exec_command" => (ToolIcon::Run, "Ran"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
