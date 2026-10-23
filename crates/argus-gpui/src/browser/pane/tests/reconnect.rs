//! A browser across a reconnect, through a live `Owner` and its session (SHELL-BROWSER-18): the new
//! daemon's greeting drops what the last one ran and the cell with it, its replay makes the
//! cell anew, and that cell tells the new daemon its room.

use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, Side};
use argus_desktop_core::settings::remembered::Settings;
use argus_protocol::{
    AgentTelemetry, BrowserPage, CheckoutId, CheckoutInfo, Hello, PaneId, PaneInfo, PaneKind, PaneStatus, ProjectId, ProjectInfo, RepositoryId, RepositoryInfo, ServerMsg,
};
use gpui_kit::TestAppContext;

use super::*;
use crate::connection::owner::Owner;

fn tree(browser: bool) -> ServerMsg {
    let pane = |id: u64, kind, title: &str| PaneInfo {
        id: PaneId(id),
        kind,
        title: title.into(),
        status: PaneStatus::Idle,
        note: None,
        template: None,
        children: Vec::new(),
        background: false,
        telemetry: AgentTelemetry::default(),
        since: None,
        live: false,
    };
    let mut panes = vec![pane(1, PaneKind::Agent, "claude")];
    if browser {
        panes.push(pane(PANE, PaneKind::Browser, "Browser"));
    }
    let checkout = CheckoutInfo { id: CheckoutId(100), name: "main".into(), path: "/src/main".into(), panes, git: None, primary: true };
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

fn replay() -> ServerMsg {
    ServerMsg::BrowserPage(Box::new(BrowserPage {
        pane: PaneId(PANE),
        owner: PaneId(1),
        url: "https://a.test/x".into(),
        title: "A page".into(),
        tabs: Vec::new(),
        shown: None,
        following: true,
        held: false,
        driver: None,
        dialog: None,
        select: None,
        sized: None,
        context: None,
    }))
}

#[gpui_kit::test]
fn a_reconnect_drops_the_cell_the_new_daemon_does_not_replay_and_the_one_it_does_is_told_the_size_again(cx: &mut TestAppContext) {
    let (owner, events) = Owner::new(Settings::default());
    owner.act(|session| session.connected(Some(Hello::this_build()), None, vec![tree(true)]));
    owner.call(Call::Resize { side: Side::Workspace, rows: 30, cols: 100 });
    owner.call(Call::Select { pane: 1 });
    // The page comes after the tree and the screen, as the daemon sends it, so the browser opens beside its agent.
    owner.act(|session| session.receive(replay()));
    let calls: Calls = Rc::default();
    let (sink, calling, asking) = (calls.clone(), owner.clone(), owner.clone());
    let caller = Caller::new(move |call| {
        sink.borrow_mut().push(call.clone());
        calling.call(call);
    })
    .asking_viewport(move || asking.next_browser_viewport());
    let (handle, heard, shell) = open_on(cx, caller, events);
    let (spare, _) = tokio::sync::mpsc::unbounded_channel();
    let opened = Opened { handle, heard, shell, calls, events: spare };
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    // The cell opened beside its agent, and said what room it has.
    let told = |opened: &Opened| opened.calls.borrow().iter().filter(|call| matches!(call, Call::BrowserViewport { .. })).count();
    assert!(has(cx, &opened, "browser-105"), "the browser is a cell beside its agent");
    assert_eq!(told(&opened), 1);
    opened.calls.borrow_mut().clear();

    // A daemon that replays nothing: the cell goes with the browser.
    owner.act(|session| session.connected(Some(Hello::this_build()), None, vec![tree(false)]));
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert!(!has(cx, &opened, "browser-address-105"), "a browser the new daemon does not replay is not kept");

    // One that does: the cell is made again, not kept, so it reports its room to the new daemon.
    owner.act(|session| session.connected(Some(Hello::this_build()), None, vec![tree(true)]));
    owner.act(|session| session.receive(replay()));
    cx.executor().advance_clock(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.executor().advance_clock(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    assert!(has(cx, &opened, "browser-address-105"), "the replayed browser has its cell");
    assert!(told(&opened) >= 1, "the daemon is told the room again: {:?}", asked(&opened));
}
