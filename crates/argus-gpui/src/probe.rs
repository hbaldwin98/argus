//! The scripted probes an agent runs against a daemon without a person: show a pane, open a
//! checkout's review, open its history. Each prints what the stage drew at every step.
//!
//! Every probe reads, or opens and shuts a layer the session closes again; nothing in a checkout
//! or the daemon's store is changed.

use std::time::Duration;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Action;
use argus_desktop_core::keys::page_key::PageKey;
use argus_gpui::connection::caller::Caller;
use argus_gpui::connection::heard::Heard;
use gpui_kit::{App, Entity};

use crate::report;

/// Shows one pane and presses Enter in it once, if it is a plain shell, and
/// prints what the screen did: how an agent checks the live pane against a
/// daemon without a person. An agent pane is shown but never typed into, and
/// no pane is made.
pub fn shell(heard: Entity<Heard>, caller: Caller, pane: u64, cx: &mut App) {
    cx.spawn(async move |cx| {
        let wait = |ms| cx.background_executor().timer(Duration::from_millis(ms));
        wait(1500).await;
        caller.call(Call::Select { pane });
        wait(1500).await;
        let (shell, before) = cx.update(|cx| {
            let heard = heard.read(cx);
            let shell = heard.rail.shown.as_ref().is_some_and(|shown| shown.pane == pane && !shown.agent);
            (shell, report::screen(heard))
        });
        println!("probe: pane {pane} shell={shell} before: {before}");
        if !shell {
            return;
        }
        let enter = PageKey { key: "Enter".into(), code: "Enter".into(), ..PageKey::default() };
        caller.call(Call::Key { key: enter });
        wait(1000).await;
        let after = cx.update(|cx| report::screen(heard.read(cx)));
        println!("probe: after Enter: {after}");
    })
    .detach();
}

/// Opens the review of one checkout, `first` for the rail's first or its id, and
/// prints what the Git stage drew from it, then closes it: how an agent checks
/// the stage against a daemon without a person. Only a review is opened and
/// shut, which reads and changes nothing in the checkout: nothing is staged,
/// picked, commented or discarded.
pub fn git(heard: Entity<Heard>, caller: Caller, which: String, cx: &mut App) {
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
        println!("probe: git checkout {checkout} up={} {}", cx.update(|cx| heard.read(cx).git_up()), cx.update(|cx| report::review(heard.read(cx))));
        caller.call(Call::ReviewAction { action: Action::CloseLayer });
        wait(1000).await;
        println!("probe: git closed up={} {}", cx.update(|cx| heard.read(cx).git_up()), cx.update(|cx| report::review(heard.read(cx))));
    })
    .detach();
}

/// Opens the history of one checkout, `first` for the rail's first or its id,
/// unfolds the newest commit, filters to nothing and clears it, printing what
/// the History tab was sent at each step, then closes it. Every call reads: the
/// list of commits, a commit's files and its diff. Nothing in the checkout or
/// the daemon's store is changed.
pub fn history(heard: Entity<Heard>, caller: Caller, which: String, cx: &mut App) {
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
            let line = cx.update(|cx| format!("up={} {}", heard.read(cx).git_up(), report::history(heard.read(cx))));
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
