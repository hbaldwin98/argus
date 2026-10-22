//! Where a feature's tickets are drawn as chips (BOARD-TICKETS-1): in the index's peek, as the page's
//! `ticketLinks` places them, and nowhere else in the index. An open feature's header has its Tickets
//! button instead.

use argus_desktop_core::connection::page::{Event, Link, PageView, ViewsPage};
use gpui_kit::TestAppContext;

use super::*;
use crate::keys::root::tests::{click_label, has_label};

fn linked(mut features: Features) -> Features {
    let link = Link { tracker: "Linear".into(), key: "ENG-88".into(), url: "https://linear.app/x/issue/ENG-88".into(), role: "the work".into() };
    for row in &mut features.rows {
        row.peeked = row.slug == "trim";
    }
    features.peek.as_mut().expect("the list peeks the first row").links = vec![link];
    features
}

fn showing(pane: u64, id: &str) -> Event {
    let subject = Link { tracker: "Linear".into(), key: "ENG-88".into(), url: "https://linear.app/x/issue/ENG-88".into(), role: String::new() };
    let view = PageView {
        pane,
        from: "claude".into(),
        at: 0,
        refreshing: false,
        pressed: Vec::new(),
        subject: Some(subject),
        view: argus_protocol::AgentView { id: id.into(), title: "Plan".into(), ..Default::default() },
        diagrams: Vec::new(),
        changed: Vec::new(),
        unseen: false,
    };
    Event::Views(Box::new(ViewsPage { views: vec![view], ..ViewsPage::default() }))
}

#[gpui_kit::test]
fn the_peek_carries_a_chip_for_a_linked_ticket_and_none_without_one(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    assert!(!has_label(cx, &opened, "Linear ENG-88") && !present(cx, &opened, "peek-tickets"), "no link, no chip");
    hear(cx, &opened, vec![heard(linked(list()))]);
    assert!(has_label(cx, &opened, "Linear ENG-88"));
    drain(&opened);
    click_label(cx, &opened, "Linear ENG-88");
    assert_eq!(sent(&opened), ["OpenUrl { url: \"https://linear.app/x/issue/ENG-88\" }"], "and pressing it does not open the feature");
}

#[gpui_kit::test]
fn a_view_an_agent_showed_of_the_ticket_makes_its_chip_open_the_view(cx: &mut TestAppContext) {
    let opened = on(cx, linked(list()));
    hear(cx, &opened, vec![showing(7, "plan")]);
    assert!(has_label(cx, &opened, "a view of it waits"));
    drain(&opened);
    click_label(cx, &opened, "Linear ENG-88");
    assert_eq!(sent(&opened), ["OpenView { pane: 7, id: \"plan\" }"]);
}
