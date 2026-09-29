//! Whether every key a mode takes is somewhere to be read: in `?`, and on
//! the bar too unless its keymap kept it to `?` on purpose.

use super::*;
use crate::app::{FeaturePanel, Help, View};
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
        app.focus = focus;
        app
    });
    let stage = |view: View| {
        let mut app = app_with_tree();
        app.open_view(view);
        app
    };
    let feature = [
        FeaturePanel::Features,
        FeaturePanel::Tasks,
        FeaturePanel::Diagrams,
        FeaturePanel::Decisions,
    ]
    .map(|panel| {
        let mut app = stage(View::Feature);
        app.panel = panel;
        app
    });
    let mut from_history = app_with_review();
    from_history.review.as_mut().unwrap().review.commit = Some(argus_protocol::CommitInfo {
        oid: "a".repeat(40),
        short: "aaaaaaa".to_string(),
        summary: "Wake a pane's pump on the byte".to_string(),
        author: "hunt".to_string(),
        time: 0,
    });
    from_history.history = app_with_history().history;
    let overlay = |overlay: Overlay| {
        let mut app = app_with_tree();
        app.overlay = Some(overlay);
        app
    };

    vec![
        ("the rail", rail.into()),
        ("the panes stage", vec![stage(View::Panes)]),
        ("the checkouts stage", vec![stage(View::Checkouts)]),
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
