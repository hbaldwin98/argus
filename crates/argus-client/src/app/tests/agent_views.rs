//! The views agents showed: asked for once the daemon can send them, held
//! as it sends them, and changed only when it says so.

use super::*;
use argus_protocol::{AgentView, Hello, ShownView};

fn shown(pane: u64, id: &str, title: &str) -> ShownView {
    ShownView {
        pane: PaneId(pane),
        at: 1_700_000_000,
        subject: None,
        view: AgentView {
            id: id.into(),
            title: title.into(),
            blocks: Vec::new(),
            ..AgentView::default()
        },
    }
}

fn watches(sent: &[ClientMsg]) -> usize {
    sent.iter().filter(|m| matches!(m, ClientMsg::WatchViews)).count()
}

fn titles(h: &Harness) -> Vec<String> {
    h.app.agent_views.all().iter().map(|v| v.view.title.clone()).collect()
}

#[test]
fn views_are_asked_for_when_the_daemon_can_send_them() {
    let mut h = Harness::new();
    h.app.greeted(Some(&Hello::this_build()));
    assert_eq!(watches(&h.sent()), 1);
}

#[test]
fn a_daemon_that_predates_views_is_never_asked() {
    let mut h = Harness::new();
    h.app.greeted(Some(&Hello { capabilities: Vec::new(), ..Hello::this_build() }));
    assert_eq!(watches(&h.sent()), 0);
}

#[test]
fn views_are_held_newest_first_and_a_shown_again_one_replaces_itself() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::ViewShown(Box::new(shown(100, "a", "first"))));
    h.app.on_server_msg(ServerMsg::ViewShown(Box::new(shown(101, "b", "second"))));
    assert_eq!(titles(&h), ["second", "first"]);

    h.app.on_server_msg(ServerMsg::ViewShown(Box::new(shown(100, "a", "first, read again"))));
    assert_eq!(titles(&h), ["first, read again", "second"]);
}

#[test]
fn a_reconnect_replays_the_views_rather_than_doubling_them() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::ViewShown(Box::new(shown(100, "a", "first"))));
    h.app.greeted(Some(&Hello::this_build()));
    assert!(titles(&h).is_empty());
}

#[test]
fn closing_and_refreshing_are_requests_that_change_nothing_until_the_daemon_answers() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::ViewShown(Box::new(shown(100, "a", "first"))));
    h.app.open_view(View::AgentViews);
    h.sent();

    h.keys("r");
    h.keys("x");
    let sent = h.sent();
    assert!(sent.iter().any(
        |m| matches!(m, ClientMsg::RefreshView { pane, id } if *pane == PaneId(100) && id == "a")
    ));
    assert!(sent.iter().any(
        |m| matches!(m, ClientMsg::CloseView { pane, id } if *pane == PaneId(100) && id == "a")
    ));
    assert_eq!(titles(&h), ["first"], "the daemon has not said it closed");

    h.app.on_server_msg(ServerMsg::ViewClosed { pane: PaneId(100), id: "a".into() });
    assert!(titles(&h).is_empty());
}

#[test]
fn n_and_p_move_between_views_and_stop_at_the_ends() {
    let mut h = Harness::new();
    for (pane, id) in [(100, "a"), (101, "b"), (102, "c")] {
        h.app.on_server_msg(ServerMsg::ViewShown(Box::new(shown(pane, id, id))));
    }
    h.app.open_view(View::AgentViews);
    assert_eq!(h.app.agent_views.opened().unwrap().view.id, "c", "the newest is open");

    h.keys("n");
    assert_eq!(h.app.agent_views.opened().unwrap().view.id, "b");
    h.keys("nn");
    assert_eq!(h.app.agent_views.opened().unwrap().view.id, "a");
    h.keys("pppp");
    assert_eq!(h.app.agent_views.opened().unwrap().view.id, "c");
}

#[test]
fn a_newer_view_does_not_change_which_one_is_open() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::ViewShown(Box::new(shown(100, "a", "a"))));
    h.app.on_server_msg(ServerMsg::ViewShown(Box::new(shown(101, "b", "b"))));
    h.app.open_view(View::AgentViews);
    h.keys("n");
    assert_eq!(h.app.agent_views.opened().unwrap().view.id, "a");

    h.app.on_server_msg(ServerMsg::ViewShown(Box::new(shown(102, "c", "c"))));
    assert_eq!(h.app.agent_views.opened().unwrap().view.id, "a");
}

#[test]
fn closing_the_open_view_opens_its_neighbour() {
    let mut h = Harness::new();
    for (pane, id) in [(100, "a"), (101, "b"), (102, "c")] {
        h.app.on_server_msg(ServerMsg::ViewShown(Box::new(shown(pane, id, id))));
    }
    h.app.open_view(View::AgentViews);
    h.keys("n");
    h.app.on_server_msg(ServerMsg::ViewClosed { pane: PaneId(101), id: "b".into() });
    assert_eq!(h.app.agent_views.opened().unwrap().view.id, "a");
}

#[test]
fn the_digit_four_opens_the_views_stage_and_keeps_the_rail() {
    let mut h = Harness::new();
    h.keys("4");
    assert_eq!(h.app.view, View::AgentViews);
    h.keys("q");
    assert_eq!(h.app.view, View::Workspace);
}
