//! Regression coverage for the HTML command-center composition.

mod checkouts;
mod feature;
mod panes;
mod rail;

use super::*;
use crate::app::FeaturePanel;

fn command_center() -> App {
    let mut app = app_with_tree();
    app.command_center = true;
    app
}

#[test]
fn production_shell_matches_the_designs_four_regions() {
    let mut app = command_center();
    let text = lines(&draw_at(&mut app, 120, 30)).join("\n");

    for landmark in [
        "ARGUS",
        "WORKSPACE",
        "ACTIVE WORKSPACE",
        "REPOSITORIES",
        "AGENTS",
    ] {
        assert!(text.contains(landmark), "missing {landmark}:\n{text}");
    }
    assert!(
        !text.contains("└ master"),
        "repositories expand only after a click:\n{text}"
    );
    assert_eq!(app.layout.projects.outer.x, 0);
    assert_eq!(
        app.layout.projects.outer.width,
        crate::ui::command_center::SIDEBAR_WIDTH
    );
    assert!(
        app.layout.content.outer.x >= crate::ui::command_center::SIDEBAR_WIDTH
    );
}

#[test]
fn first_run_replaces_the_shell_when_no_tree_exists() {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    std::mem::forget(rx);
    let mut app = App::new(tx);
    let text = lines(&draw_at(&mut app, 100, 24)).join("\n");

    assert!(text.contains("NO WORKSPACE"), "{text}");
    assert!(text.contains("Point Argus at a directory"), "{text}");
    assert_eq!(app.layout.projects.outer, Rect::default());
}

fn click(app: &mut App, column: u16, row: u16) {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

#[test]
fn the_open_tabs_accent_bar_is_not_cut_by_the_rail_junction() {
    let mut app = command_center();
    app.open_view(View::Feature);
    let rows = lines(&draw_at(&mut app, 120, 30));
    let rule = rows
        .iter()
        .find(|line| line.contains('━'))
        .expect("tab rule");
    let bar: String = rule
        .chars()
        .skip_while(|c| *c != '━')
        .take_while(|c| *c != '─')
        .collect();
    assert!(
        !rule[rule.find('━').unwrap()..]
            .trim_end_matches('─')
            .contains('┬'),
        "{rule}"
    );
    assert!(bar.chars().count() >= 7, "{rule}");
}

#[test]
fn the_shell_leaves_a_row_above_the_tabs_and_below_the_status_band() {
    let mut app = command_center();
    let rows = lines(&draw_at(&mut app, 120, 30));
    assert!(rows[0].trim().is_empty(), "{rows:?}");
    assert!(rows[29].trim().is_empty(), "{rows:?}");
}

#[test]
fn the_workspace_header_and_pane_cards_show_agent_telemetry() {
    let mut app = command_center();
    {
        let pane = &mut app.tree[0].repositories[0].checkouts[0].panes[0];
        pane.telemetry = argus_protocol::AgentTelemetry {
            model: Some("gpt-5-codex".into()),
            context_tokens: Some(41_203),
            context_window: Some(272_000),
            cost_usd: Some(0.5),
            tool: Some("Bash".into()),
            tool_calls: Some(3),
            ..Default::default()
        };
    }
    app.select_pane_location(app.flat_pane_locations()[0]);
    let text = lines(&draw_at(&mut app, 160, 30)).join("\n");
    assert!(
        text.contains("▸ Bash · ctx 41k/272k 15% · $0.50 · gpt-5-codex"),
        "{text}"
    );

    app.open_view(View::Panes);
    let text = lines(&draw_at(&mut app, 160, 30)).join("\n");
    assert!(text.contains("▸ Bash · ctx 41k/272k"), "{text}");
}

#[test]
fn token_counts_read_compactly() {
    use crate::ui::command_center::compact_tokens;
    assert_eq!(compact_tokens(980), "980");
    assert_eq!(compact_tokens(4_250), "4.2k");
    assert_eq!(compact_tokens(41_203), "41k");
    assert_eq!(compact_tokens(1_200_000), "1.2M");
}
