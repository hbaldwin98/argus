//! What the combine-projects dialog says: its title and labels, each row's notes, the summary that
//! follows the ticks, and what its button reads.
//!
//! The dialog's state is `combine`; this is its wording, so the page and the GPUI window say the
//! same thing about the same page. Whether a press is allowed is `can_combine`, which the button
//! only shows.

use super::combine_page::{CombinePage, CombineRow};
use super::projects::{counted, tally, Tally, Tone};
use crate::connection::page::ProjectChoice;

pub const TITLE: &str = "Combine projects";
/// The action that opens the dialog, in the switcher and in the add dialog's note.
pub const ACTION: &str = "Combine projects\u{2026}";
pub const INTO_LABEL: &str = "Into";
pub const INTO_NOTE: &str = "keeps its name and settings";
pub const SCANS_A_FOLDER: &str = "scans a folder";
pub const NOTHING_TO_FOLD: &str = "No other project in this workspace.";

/// How one line of the summary reads: a good thing that happens, or a quiet cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Check,
    Dash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub mark: Mark,
    pub words: String,
}

/// The repositories a project holds, as the Into control and each row say it.
pub fn repositories(n: usize) -> String {
    counted(n, "repository", "repositories")
}

/// What a row says of its agents, only when somebody is needed or working.
pub fn status_note(row: &CombineRow) -> Option<Tally> {
    let choice = ProjectChoice { id: row.id, name: row.name.clone(), needs_you: row.needs_you, working: row.working };
    Some(tally(&choice)).filter(|t| t.tone != Tone::Muted)
}

pub fn ticked(page: &CombinePage) -> usize {
    page.rows.iter().filter(|row| row.ticked).count()
}

/// What the ticks come to, line by line; nothing while nothing is ticked.
pub fn summary(page: &CombinePage) -> Vec<Line> {
    let n = ticked(page);
    let Some(into) = page.into.as_ref().filter(|_| n > 0) else {
        return Vec::new();
    };
    let mut lines = vec![Line {
        mark: Mark::Check,
        words: format!("{} will hold {}: {}.", into.name, repositories(page.repositories), page.holds.join(", ")),
    }];
    if page.agents > 0 {
        lines.push(Line {
            mark: Mark::Check,
            words: format!("Running panes move with their repositories and keep running: {}.", counted(page.agents, "agent", "agents")),
        });
    }
    lines.push(Line { mark: Mark::Check, words: "Their feature boards come too; features keep their numbers.".to_owned() });
    let leaving = if n == 1 { "the project row leaves".to_owned() } else { format!("the {n} project rows leave") };
    lines.push(Line {
        mark: Mark::Dash,
        words: format!("Their own worktree and setup settings are dropped; {leaving} the switcher. Nothing on disk changes."),
    });
    lines
}

/// The button: how many go into which.
pub fn button_label(page: &CombinePage) -> String {
    let into = page.into.as_ref().map_or("", |into| into.name.as_str());
    match ticked(page) {
        0 => format!("Combine into {into}"),
        n => format!("Combine {n} into {into}"),
    }
}

pub fn can_combine(page: &CombinePage) -> bool {
    ticked(page) > 0 && !page.combining
}

#[cfg(test)]
mod tests;
