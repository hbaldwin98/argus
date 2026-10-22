//! The Argus window on GPUI: startup only.
//!
//! The tokio runtime is built before the GPUI app, since GPUI's executors
//! are not tokio, and the daemon loop runs on it; what it hears reaches the
//! UI thread through `connection::owner`'s channel. What is here is the
//! runtime, the window, and running until it closes.
//!
//! Set `ARGUS_GPUI_QUIT_MS` to have it print what it heard after that long
//! and exit, which is how an agent runs it unattended. `ARGUS_GPUI_PROBE_SHELL`
//! shows one pane, `ARGUS_GPUI_PROBE_GIT` opens a checkout's review, reads
//! what the Git stage was sent and closes it again, and
//! `ARGUS_GPUI_PROBE_HISTORY` does the same for its history, unfolding the
//! newest commit and filtering to nothing on the way.


// A console window behind the GUI is noise outside a debug build.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

use std::time::Duration;

use argus_desktop_core::connection::daemon;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Action;
use argus_desktop_core::keys::page_key::PageKey;
use argus_desktop_core::settings::remembered;
use argus_desktop_core::system::logging;
use argus_gpui::chrome::shell::Shell;
use argus_gpui::connection::caller::Caller;
use argus_gpui::connection::heard::Heard;
use argus_gpui::connection::owner::Owner;
use argus_gpui::window;
use gpui_kit::{App, AppContext as _, Entity};

fn main() {
    logging::init();
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            tracing::error!("could not start the runtime: {err}");
            std::process::exit(1);
        }
    };

    let mut saved = remembered::load();
    // Only where none was chosen: the person's own choice outlasts the
    // environment's.
    if saved.host.is_none() {
        saved.host = std::env::var("ARGUS_DESKTOP_HOST").ok().filter(|host| !host.trim().is_empty());
    }
    let (owner, events) = Owner::new(saved);
    let daemon = runtime.spawn(daemon::run(owner.clone()));
    let frames = runtime.spawn(owner.clone().frames());

    let quit_after: Option<u64> = std::env::var("ARGUS_GPUI_QUIT_MS").ok().and_then(|ms| ms.parse().ok());
    let calls = owner.clone();
    window::application().run(move |cx: &mut App| {
        window::init(cx);
        // The loops are the window's: they end with it, and so does the app.
        window::quit_when_closed(cx, move || {
            daemon.abort();
            frames.abort();
        })
        .detach();
        let heard = cx.new(|_| Heard::new());
        heard.update(cx, |heard, cx| heard.listen(events, cx));
        let caller = Caller::new(move |call| calls.call(call));
        let probing = caller.clone();
        let options = window::options(cx);
        let shell = heard.clone();
        gpui_kit::open_window(options, cx, move |window, cx| {
            window::quit_instead_of_closing(window, cx);
            cx.new(|cx| Shell::new(shell, caller, window, cx))
        })
        .expect("the window opens");
        cx.activate(true);
        if let Some(pane) = std::env::var("ARGUS_GPUI_PROBE_SHELL").ok().and_then(|pane| pane.parse().ok()) {
            probe(heard.clone(), probing.clone(), pane, cx);
        }
        if let Ok(which) = std::env::var("ARGUS_GPUI_PROBE_GIT") {
            probe_git(heard.clone(), probing.clone(), which, cx);
        }
        if let Ok(which) = std::env::var("ARGUS_GPUI_PROBE_HISTORY") {
            probe_history(heard.clone(), probing.clone(), which, cx);
        }
        if let Some(ms) = quit_after {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_millis(ms)).await;
                cx.update(|cx| {
                    println!("{}", report(heard.read(cx)));
                    cx.quit();
                })
            })
            .detach();
        }
    });
    // Dropping the tasks drops the bridge, which is killed with them.
    runtime.shutdown_timeout(Duration::from_secs(1));
}

/// Shows one pane and presses Enter in it once, if it is a plain shell, and
/// prints what the screen did: how an agent checks the live pane against a
/// daemon without a person. An agent pane is shown but never typed into, and
/// no pane is made.
fn probe(heard: Entity<Heard>, caller: Caller, pane: u64, cx: &mut App) {
    cx.spawn(async move |cx| {
        let wait = |ms| cx.background_executor().timer(Duration::from_millis(ms));
        wait(1500).await;
        caller.call(Call::Select { pane });
        wait(1500).await;
        let (shell, before) = cx.update(|cx| {
            let heard = heard.read(cx);
            let shell = heard.rail.shown.as_ref().is_some_and(|shown| shown.pane == pane && !shown.agent);
            (shell, screen(heard))
        });
        println!("probe: pane {pane} shell={shell} before: {before}");
        if !shell {
            return;
        }
        let enter = PageKey { key: "Enter".into(), code: "Enter".into(), ..PageKey::default() };
        caller.call(Call::Key { key: enter });
        wait(1000).await;
        let after = cx.update(|cx| screen(heard.read(cx)));
        println!("probe: after Enter: {after}");
    })
    .detach();
}

/// Opens the review of one checkout, `first` for the rail's first or its id, and
/// prints what the Git stage drew from it, then closes it: how an agent checks
/// the stage against a daemon without a person. Only a review is opened and
/// shut, which reads and changes nothing in the checkout: nothing is staged,
/// picked, commented or discarded.
fn probe_git(heard: Entity<Heard>, caller: Caller, which: String, cx: &mut App) {
    cx.spawn(async move |cx| {
        let wait = |ms| cx.background_executor().timer(Duration::from_millis(ms));
        wait(1500).await;
        let checkout = cx.update(|cx| {
            let heard = heard.read(cx);
            let first = heard.rail.project.iter().flat_map(|p| &p.repositories).flat_map(|r| &r.checkouts).map(|c| c.id).next();
            which.parse().ok().or(first)
        });
        let Some(checkout) = checkout else {
            println!("probe: git no checkout to open");
            return;
        };
        caller.call(Call::OpenReview { checkout });
        wait(2500).await;
        println!("probe: git checkout {checkout} up={} {}", cx.update(|cx| heard.read(cx).git_up()), cx.update(|cx| review(heard.read(cx))));
        caller.call(Call::ReviewAction { action: Action::CloseLayer });
        wait(1000).await;
        println!("probe: git closed up={} {}", cx.update(|cx| heard.read(cx).git_up()), cx.update(|cx| review(heard.read(cx))));
    })
    .detach();
}

/// Opens the history of one checkout, `first` for the rail's first or its id,
/// unfolds the newest commit, filters to nothing and clears it, printing what
/// the History tab was sent at each step, then closes it. Every call reads: the
/// list of commits, a commit's files and its diff. Nothing in the checkout or
/// the daemon's store is changed.
fn probe_history(heard: Entity<Heard>, caller: Caller, which: String, cx: &mut App) {
    cx.spawn(async move |cx| {
        let wait = |ms| cx.background_executor().timer(Duration::from_millis(ms));
        wait(1500).await;
        let checkout = cx.update(|cx| {
            let heard = heard.read(cx);
            let first = heard.rail.project.iter().flat_map(|p| &p.repositories).flat_map(|r| &r.checkouts).map(|c| c.id).next();
            which.parse().ok().or(first)
        });
        let Some(checkout) = checkout else {
            println!("probe: history no checkout to open");
            return;
        };
        let said = |step: &str, cx: &gpui_kit::AsyncApp| {
            let line = cx.update(|cx| format!("up={} {}", heard.read(cx).git_up(), history(heard.read(cx))));
            println!("probe: history {step} {line}");
        };
        caller.call(Call::OpenHistory { checkout });
        wait(2500).await;
        said("opened", cx);
        caller.call(Call::HistoryAction { action: Action::Unfold, row: None });
        wait(2500).await;
        said("unfolded", cx);
        caller.call(Call::FilterHistory { filter: "zzzzqqqq".into() });
        wait(1000).await;
        said("filtered", cx);
        caller.call(Call::FilterHistory { filter: String::new() });
        wait(1000).await;
        said("cleared", cx);
        caller.call(Call::HistoryAction { action: Action::CloseLayer, row: None });
        wait(1000).await;
        said("closed", cx);
    })
    .detach();
}

/// The history as the Git stage is sent it, on one line.
fn history(heard: &Heard) -> String {
    let Some(page) = &heard.history else {
        return "history=none".to_owned();
    };
    format!(
        "history={} branch={} rows={} commits={} files={} on={:?} loading={} empty={:?} filter={:?} path={:?} diff_rows={} diff_loading={} diff_empty={:?}",
        page.name,
        page.branch,
        page.rows.len(),
        page.rows.iter().filter(|row| row.kind == "commit").count(),
        page.rows.iter().filter(|row| row.kind == "file").count(),
        page.rows.iter().position(|row| row.on),
        page.loading,
        page.empty,
        page.filter,
        page.path,
        page.diff.len(),
        page.diff_loading,
        page.diff_empty,
    )
}

/// The review as the Git stage is sent it, on one line.
fn review(heard: &Heard) -> String {
    let Some(page) = &heard.review else {
        return "review=none".to_owned();
    };
    format!(
        "review={} base={} files={} rows={} file={:?} split={} loading={} empty={:?} +{} -{} unstaged={:?} staged={:?} comment={} jump={}",
        page.name,
        page.base,
        page.files.len(),
        page.rows.len(),
        page.file,
        page.split,
        page.loading,
        page.empty,
        page.added,
        page.removed,
        page.unstaged,
        page.staged,
        page.comment.is_some(),
        page.jump.is_some(),
    )
}

/// What the window heard, on one line.
fn report(heard: &Heard) -> String {
    let rail = &heard.rail;
    let repositories = rail.project.as_ref().map_or(0, |p| p.repositories.len());
    let checkouts = rail.project.iter().flat_map(|p| &p.repositories).map(|r| r.checkouts.len()).sum::<usize>();
    let listed: Vec<String> = rail
        .project
        .iter()
        .flat_map(|p| &p.repositories)
        .flat_map(|r| &r.checkouts)
        .flat_map(|c| &c.panes)
        .map(|pane| format!("{}:{}:{}", pane.id, if pane.agent { "agent" } else { "shell" }, pane.status))
        .collect();
    let panes = listed.len();
    format!(
        "argus-gpui heard: connection={:?} loaded={} projects={} project={} repositories={} checkouts={} panes={} [{}] shown={} docks=[{}] console=[{}] git_up={} {} screen={}",
        heard.connection(),
        rail.loaded,
        rail.projects.len(),
        rail.project.as_ref().map_or("-", |p| p.name.as_str()),
        repositories,
        checkouts,
        panes,
        listed.join(" "),
        rail.shown.as_ref().map_or("-", |s| s.title.as_str()),
        docks(&heard.docks),
        console(&heard.console),
        heard.git_up(),
        review(heard),
        screen(heard),
    )
}

/// The docks as they are drawn: each one's size and tabs, the shown one marked.
fn docks(docks: &argus_desktop_core::layout::docks::DocksPage) -> String {
    [("left", &docks.left), ("right", &docks.right), ("bottom", &docks.bottom)]
        .into_iter()
        .map(|(name, page)| {
            let tabs: Vec<String> = page
                .tabs
                .iter()
                .map(|tab| format!("{}{}", if page.shown.as_ref() == Some(&tab.panel) { "*" } else { "" }, tab.title))
                .collect();
            let open = if page.shown.is_some() { page.size.to_string() } else { "closed".to_owned() };
            format!("{name} {open} {}", tabs.join("|"))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The console as it is drawn: where it is, a tab per console (the ones on a
/// side marked), and which consoles the sides show and type in.
fn console(page: &argus_desktop_core::panes::console::ConsolePage) -> String {
    let tabs: Vec<String> =
        page.tabs.iter().map(|tab| format!("{}:{}:{}{}", tab.pane, tab.title, tab.status, if tab.on { "*" } else { "" })).collect();
    let pane = |pane: Option<u64>| pane.map_or("-".to_owned(), |pane| pane.to_string());
    format!(
        "{:?} tabs={} [{}] first={} second={} typing={} spawn={}",
        page.place,
        tabs.len(),
        tabs.join(" "),
        pane(page.showing),
        pane(page.beside),
        pane(page.typing),
        page.can_spawn
    )
}

/// The pane's screen on one line: its grid, and the first row with text on it.
fn screen(heard: &Heard) -> String {
    let screen = &heard.screen;
    let Some(pane) = screen.pane() else {
        return "none".to_owned();
    };
    let (rows, cols) = screen.size();
    let first: String = (0..rows)
        .map(|row| screen.row_text(row))
        .find(|text| !text.is_empty())
        .unwrap_or_default()
        .chars()
        .take(40)
        .collect();
    let cursor = screen.cursor();
    format!("pane {pane} {rows}x{cols} cursor=({},{}) first={first:?}", cursor.row, cursor.col)
}
