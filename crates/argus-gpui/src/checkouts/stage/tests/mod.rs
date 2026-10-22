//! The Checkouts stage in the whole window: what the table draws of a snapshot, what each press
//! sends, and what the session's word changes. Rules cited are `gpui/parity/checkouts-code-views.md`'s
//! (`CHECKOUTS-n`).
//!
//! The window is the shell over a caller that collects its calls (`keys::root::tests`), and the
//! session's table is sent as an event, so nothing here asks a real session.

use argus_desktop_core::checkouts::stage::Confirm;
use argus_desktop_core::checkouts::table::{CheckoutDetail, Checkouts, CheckoutsRow, LastCommit, Running};
use argus_desktop_core::connection::page::{Connection, Event, Project, Rail, Repository, Stage, Status};
use gpui_kit::TestAppContext;

use crate::keys::root::tests::{drain, has_label, hear, open, Opened};

mod dialogs;
mod helpers;
mod keys;
mod menus;
mod side;
mod states;
mod table;

pub(crate) use helpers::*;

/// A checkout row called `name` on `branch`, quiet: clean, in step, idle.
pub(crate) fn checkout(id: u64, name: &str, branch: &str) -> CheckoutsRow {
    CheckoutsRow {
        key: format!("c{id}"),
        checkout: Some(id),
        name: Some(name.into()),
        primary: false,
        branch: branch.into(),
        git: None,
        clean: true,
        changed: 0,
        ahead: 0,
        behind: 0,
        detached: false,
        note: None,
        last: None,
        running: Vec::new(),
        shared: None,
        feature: None,
        viewed: false,
    }
}

/// A branch with no directory.
pub(crate) fn branch(name: &str) -> CheckoutsRow {
    CheckoutsRow { key: format!("b{name}"), checkout: None, name: None, branch: name.into(), clean: false, ..checkout(0, "", "") }
}

pub(crate) fn commit(short: &str, summary: &str) -> LastCommit {
    LastCommit { short: short.into(), summary: summary.into(), ago: "40m".into() }
}

pub(crate) fn running(id: u64, title: &str, status: &'static str, agent: bool) -> Running {
    Running { id, title: title.into(), agent, status }
}

/// The table of `argus` with the checkouts `main` (primary) and `desktop`, and nine branches
/// no checkout is on, hidden.
pub(crate) fn table(selected: &str) -> Checkouts {
    let main = CheckoutsRow { primary: true, ..checkout(100, "main", "main") };
    let desktop = CheckoutsRow {
        changed: 6,
        ahead: 3,
        last: Some(commit("9d41c07", "Draw a feature's flows")),
        running: vec![running(1, "claude \u{b7} screenshots", "working", true)],
        feature: Some(42),
        ..checkout(101, "desktop", "desktop-design")
    };
    Checkouts {
        repository: Some("argus".into()),
        checkouts: 2,
        branches: 9,
        main: Some("main".into()),
        rows: vec![main, desktop],
        selected: Some(selected.into()),
        detail: Some(CheckoutDetail {
            name: "desktop".into(),
            path: "C:\\src\\orion\\.argus\\worktrees\\desktop".into(),
            upstream: Some("origin/desktop-design \u{b7} 3 ahead".into()),
            changes: Some("2 staged \u{b7} 4 unstaged files".into()),
            feature_title: Some("Screenshot harness".into()),
        }),
        files_offered: true,
        reveal: Some("Show in Explorer"),
        ..Checkouts::default()
    }
}

pub(crate) fn rail() -> Rail {
    Rail {
        project: Some(Project {
            id: 1,
            name: "orion".into(),
            needs_you: 0,
            repositories: vec![Repository {
                id: 10,
                name: "argus".into(),
                quiet: false,
                divider: false,
                open: true,
                panes: 0,
                status: None,
                used: None,
                checkouts: Vec::new(),
            }],
            running: Vec::new(),
        }),
        repository: Some(10),
        loaded: true,
        stage: Stage::Checkouts,
        templates: vec!["claude".into(), "codex".into()],
        ..Rail::default()
    }
}

pub(crate) fn connected() -> Event {
    Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime: true, host: None })
}

pub(crate) fn heard(table: Checkouts) -> Event {
    Event::Checkouts(Box::new(table))
}

/// A connected window on the Checkouts stage that has heard `table`, its calls cleared.
pub(crate) fn on(cx: &mut TestAppContext, table: Checkouts) -> Opened {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), Event::Rail(Box::new(rail())), heard(table)]);
    drain(&opened);
    opened
}

/// A removal question for `name`, as the session holds it.
pub(crate) fn removing(name: &str) -> Confirm {
    Confirm::Remove { name: name.into(), branch: name.into() }
}

pub(crate) fn drawn(cx: &mut TestAppContext, opened: &Opened, label: &str) -> bool {
    has_label(cx, opened, label)
}
