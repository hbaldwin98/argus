//! Whether every key a mode takes is somewhere to be read: in `?`, and on
//! the bar too unless its keymap kept it to `?` on purpose.

use super::*;
use crate::app::{Help, Lens, View};
use std::collections::BTreeSet;

/// Each mode whose keys come from its keymap, in every state that changes
/// what its bar says.
fn modes() -> Vec<(&'static str, Vec<App>)> {
    let rail = [
        Focus::Projects,
        Focus::Repositories,
        Focus::Checkouts,
        Focus::Panes,
    ]
    .map(|focus| {
        let mut app = app_with_tree();
        // A repository with nothing running, so the rail has a fold to
        // offer `z` beside.
        app.tree[0].repositories.push(repository(
            9,
            "satellite",
            vec![checkout(20, "main", true, vec![])],
        ));
        app.focus = focus;
        app
    });
    // A console's row, which offers what a pane of no checkout can do.
    let mut on_console = app_with_tree();
    on_console.on_server_msg(argus_protocol::ServerMsg::Consoles(vec![pane_info(
        900,
        PaneKind::Shell,
        "console",
        PaneStatus::Idle,
    )]));
    on_console.choose_rail_row(crate::app::RailRow::Console(0));
    let rail: Vec<App> = rail.into_iter().chain([on_console]).collect();
    let stage = |view: View| {
        let mut app = app_with_tree();
        app.open_view(view);
        app
    };
    // The index, each lens of an open feature, and the detail column.
    let reading = |lens: Lens, in_detail: bool| {
        let mut app = app_reading_a_feature();
        app.lens = lens;
        app.in_detail = in_detail;
        app
    };
    let mut index = app_reading_a_feature();
    index.feature_open = false;
    let feature = [
        index,
        reading(Lens::Work, false),
        reading(Lens::Work, true),
        reading(Lens::Record, false),
        reading(Lens::Flows, false),
    ];
    let from_history = app_with_commit_review();
    let overlay = |overlay: Overlay| {
        let mut app = app_with_tree();
        app.overlay = Some(overlay);
        app
    };

    vec![
        ("the rail", rail),
        ("the checkouts stage", vec![stage(View::Checkouts)]),
        ("the views stage", vec![stage(View::AgentViews)]),
        ("the feature stage", feature.into()),
        (
            "a diff",
            vec![app_with_review(), app_with_review_split(true), from_history],
        ),
        ("history", vec![app_with_history()]),
        ("a brief", vec![app_with_a_brief("The pty.")]),
        ("settings", vec![overlay(Overlay::Settings { sel: 0 })]),
        ("a sequence diagram", vec![overlay(Overlay::SequenceDiagram)]),
    ]
}

/// The keys a bar offers: the first word of each hint, `j/k` counting as
/// both of its keys.
fn offered(bar: &str) -> BTreeSet<String> {
    bar.split("  ")
        .filter_map(|hint| hint.split_whitespace().next())
        .flat_map(|word| match word {
            "/" => vec![word.to_string()],
            _ => word.split('/').map(str::to_string).collect(),
        })
        .collect()
}

/// A key as a bar spells it.
fn spelled(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Tab => "tab".to_string(),
        other => format!("{other:?}"),
    }
}

#[test]
fn every_key_a_mode_takes_is_in_its_help_and_on_its_bar_unless_kept_to_help() {
    let mut unread = Vec::new();
    for (mode, states) in modes() {
        let keymap = states[0].mode().keymap();
        let mut on_bar = BTreeSet::new();
        let mut help = String::new();
        for mut app in states {
            // Wide enough that every bar draws its widest tier.
            on_bar.extend(offered(&bar(&draw_at(&mut app, 400, 30))));
            app.help = Some(Help::default());
            help.push_str(&lines(&draw_at(&mut app, 200, 100)).join("\n"));
        }

        for binding in keymap.bindings() {
            let keys: Vec<String> = binding.codes().map(spelled).collect();
            if keys.is_empty() || !binding.in_help() {
                continue;
            }
            if !help.contains(binding.help()) {
                unread.push(format!("{mode}: {keys:?} is not in `?`"));
            }
            if binding.on_bar() && !keys.iter().any(|key| on_bar.contains(key)) {
                unread.push(format!(
                    "{mode}: {keys:?} ({}) is not on the bar, and not kept to `?`",
                    binding.help()
                ));
            }
        }
    }
    assert!(unread.is_empty(), "{unread:#?}");
}
