//! The leader and the pane's chords, end to end: keys through the walk into a real session, and the
//! events it answers with back to a surface (audit item 7). Asserted by what the window ends up
//! showing, not by the calls it made.

use std::sync::Arc;

use argus_desktop_core::connection::page::{Call, Stage};
use argus_desktop_core::settings::remembered::Settings;
use argus_protocol::{
    AgentTelemetry, CheckoutId, CheckoutInfo, PaneId, PaneInfo, PaneKind, PaneStatus, ProjectId, ProjectInfo, RepositoryId, RepositoryInfo,
    ServerMsg,
};
use gpui_kit::TestAppContext;

use super::*;
use crate::connection::owner::Owner;

/// One project, one checkout, one shell: the pane the keys are typed into.
fn tree() -> ServerMsg {
    let pane = PaneInfo {
        id: PaneId(2),
        kind: PaneKind::Shell,
        title: "zsh".into(),
        status: PaneStatus::Idle,
        note: None,
        template: None,
        children: Vec::new(),
        background: false,
        telemetry: AgentTelemetry::default(),
        since: None,
        live: false,
    };
    let checkout = CheckoutInfo { id: CheckoutId(100), name: "main".into(), path: "/src/main".into(), panes: vec![pane], git: None, primary: true };
    let repository = RepositoryInfo {
        id: RepositoryId(10),
        name: "argus".into(),
        checkouts: vec![checkout],
        branches: Vec::new(),
        default_branch: None,
        remote_branches: Vec::new(),
        unmerged: Default::default(),
        last_used: None,
    };
    ServerMsg::Tree(vec![ProjectInfo { id: ProjectId(1), name: "src".into(), root: None, repositories: vec![repository] }])
}

/// A window over a real session showing a shell, as the app builds it; the keys are the pane's.
fn open_session(cx: &mut TestAppContext) -> (Opened, Arc<Owner>) {
    let (owner, events) = Owner::new(Settings::default());
    owner.act(|session| session.receive(tree()));
    owner.call(Call::Select { pane: 2 });
    let calling = owner.clone();
    let (handle, heard) = open_on(cx, Caller::new(move |call| calling.call(call)), events);
    let (spare, _) = tokio::sync::mpsc::unbounded_channel();
    (Opened { handle, heard, calls: Rc::default(), events: spare }, owner)
}

/// SHELL-KEYS-5, 9: the leader, then a key the session binds to the sheet, the palette or the
/// settings, comes back as `Run` and the window opens it.
#[gpui_kit::test]
fn the_leader_then_a_key_opens_the_sheet_the_palette_or_the_settings(cx: &mut TestAppContext) {
    for (key, kind) in [("?", Overlay::Sheet), ("k", Overlay::Palette), (",", Overlay::Settings)] {
        let (opened, _owner) = open_session(cx);
        press(cx, &opened, "ctrl-space");
        if key == "?" {
            question_mark(cx, &opened);
        } else {
            press(cx, &opened, key);
        }
        assert_eq!(overlays(cx), [kind], "leader then {key}");
        press(cx, &opened, "escape");
    }
}

/// SHELL-KEYS-5: a key nothing binds after the leader is dropped, and opens nothing.
#[gpui_kit::test]
fn a_key_nothing_binds_after_the_leader_opens_nothing(cx: &mut TestAppContext) {
    let (opened, _owner) = open_session(cx);
    press(cx, &opened, "ctrl-space");
    press(cx, &opened, "q");
    assert!(overlays(cx).is_empty());
}

/// SHELL-KEYS-6: a stage digit after the leader lands: the session answers with the stage's tree,
/// and the rail lights it.
#[gpui_kit::test]
fn the_leader_then_a_digit_changes_the_stage_the_rail_lights(cx: &mut TestAppContext) {
    let (opened, owner) = open_session(cx);
    owner.call(Call::ShowFeatures { open: false });
    cx.run_until_parked();
    press(cx, &opened, "ctrl-space");
    press(cx, &opened, "3");
    cx.run_until_parked();
    let stage = cx.update(|cx| opened.heard.read(cx).rail.stage);
    assert_eq!(stage, Stage::Checkouts);
}
