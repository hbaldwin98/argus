//! The shells that belong to no checkout: where they sit on the rail, what
//! picking one shows, and asking the daemon for one.

use super::*;
use argus_protocol::{Created, Hello};

/// A harness whose daemon keeps consoles and runs `ids`, with the cursor
/// on the fixture's last pane — the row above the consoles.
fn with_consoles(ids: &[u64]) -> Harness {
    let mut h = Harness::new();
    h.app.greeted(Some(&Hello::this_build()));
    h.app.on_server_msg(ServerMsg::Consoles(consoles(ids)));
    h.keys("lllj");
    h.sent();
    h
}

fn consoles(ids: &[u64]) -> Vec<PaneInfo> {
    ids.iter().map(|&id| pane(id, &format!("console {id}"))).collect()
}

#[test]
fn consoles_are_the_rails_last_rows() {
    let h = with_consoles(&[900, 901]);
    let rows = h.app.rail_rows();
    assert_eq!(rows[rows.len() - 2..], [RailRow::Console(0), RailRow::Console(1)]);
}

#[test]
fn walking_past_the_last_pane_puts_a_console_on_the_stage() {
    let mut h = with_consoles(&[900]);
    h.key(KeyCode::Char('j'));

    assert_eq!(h.app.rail_cursor(), Some(RailRow::Console(0)));
    assert_eq!(h.app.column_pane(), Some(PaneId(900)));
    assert!(
        h.sent().iter().any(|m| matches!(m, ClientMsg::Subscribe { pane: PaneId(900) })),
        "the stage streams the console"
    );
    assert!(h.app.current_checkout().is_none(), "a console belongs to no checkout");

    h.key(KeyCode::Char('k'));
    assert_eq!(h.app.column_pane(), Some(PaneId(101)), "and back up is the tree again");
}

#[test]
fn the_daemon_is_told_a_console_is_shown_under_no_checkout() {
    let mut h = with_consoles(&[900]);
    h.key(KeyCode::Char('j'));
    let view = h.app.view();
    assert_eq!((view.pane, view.checkout), (Some(PaneId(900)), None));
}

#[test]
fn enter_on_a_console_types_into_it() {
    let mut h = with_consoles(&[900]);
    h.keys("jl");
    assert_eq!(h.app.focus, Focus::PaneContent);
    h.sent();

    h.key(KeyCode::Char('z'));
    let sent = h.sent();
    assert!(
        matches!(&sent[..], [ClientMsg::Input { pane: PaneId(900), bytes }] if bytes == b"z"),
        "{sent:?}"
    );
}

#[test]
fn a_console_hangs_under_nothing_so_h_stays_on_it() {
    let mut h = with_consoles(&[900]);
    h.key(KeyCode::Char('j'));
    h.key(KeyCode::Char('h'));
    assert_eq!(h.app.rail_cursor(), Some(RailRow::Console(0)));
}

#[test]
fn c_asks_for_a_console_and_shows_it_only_once_the_daemon_lists_it() {
    let mut h = with_consoles(&[]);
    h.key(KeyCode::Char('c'));
    let request = h.sent().pop().expect("a request");
    let ClientMsg::SpawnConsole { request_id } = request else {
        panic!("{request:?}");
    };
    assert_ne!(request_id, 0, "it asks to hear what was made");

    h.app.on_server_msg(ServerMsg::Created { request_id, created: Some(Created::Pane(PaneId(900))) });
    assert!(h.app.consoles.is_empty(), "nothing predicted");
    assert_eq!(h.app.column_pane(), Some(PaneId(101)), "the stage waits for the list");

    h.app.on_server_msg(ServerMsg::Consoles(consoles(&[900])));
    assert_eq!(h.app.column_pane(), Some(PaneId(900)));
    assert_eq!(h.app.focus, Focus::PaneContent, "the keys go to the new shell");
}

#[test]
fn s_on_a_console_starts_another_console() {
    let mut h = with_consoles(&[900]);
    h.key(KeyCode::Char('j'));
    h.sent();
    h.key(KeyCode::Char('s'));
    assert!(matches!(h.sent()[..], [ClientMsg::SpawnConsole { .. }]));
}

#[test]
fn a_daemon_without_consoles_is_asked_for_none_and_c_says_why() {
    let mut h = Harness::new();
    h.app.greeted(Some(&Hello { capabilities: Vec::new(), ..Hello::this_build() }));
    h.sent();

    h.key(KeyCode::Char('c'));

    assert!(h.sent().is_empty());
    assert!(h.app.status_alert);
    assert!(h.app.status.contains("predates consoles"), "{}", h.app.status);
    assert!(!h.app.rail_rows().iter().any(|row| matches!(row, RailRow::Console(_))));
}

#[test]
fn x_on_a_console_asks_the_daemon_to_close_it_and_the_row_stays_until_it_has() {
    let mut h = with_consoles(&[900]);
    h.key(KeyCode::Char('j'));
    h.key(KeyCode::Char('x'));
    assert!(h.sent().iter().any(|m| matches!(m, ClientMsg::Kill { pane: PaneId(900) })));
    assert_eq!(h.app.consoles.len(), 1, "closed when the daemon says so");
}

#[test]
fn a_console_that_ends_gives_its_row_to_the_next_and_the_keys_to_the_rail() {
    let mut h = with_consoles(&[900, 901]);
    h.keys("jl");

    h.app.on_server_msg(ServerMsg::Consoles(consoles(&[901])));

    assert_eq!(h.app.column_pane(), Some(PaneId(901)));
    assert_eq!(h.app.focus, Focus::Panes, "typing never goes on into a shell nobody chose");
}

#[test]
fn the_last_console_ending_puts_the_cursor_back_on_the_tree() {
    let mut h = with_consoles(&[900]);
    h.key(KeyCode::Char('j'));

    h.app.on_server_msg(ServerMsg::Consoles(Vec::new()));

    assert_eq!(h.app.rail_cursor(), Some(RailRow::Pane(h.app.pane_location().unwrap())));
    assert_eq!(h.app.column_pane(), Some(PaneId(101)));
}

#[test]
fn on_a_console_what_needs_a_checkout_sends_nothing() {
    let mut h = with_consoles(&[900]);
    h.key(KeyCode::Char('j'));
    h.sent();
    for key in ['R', 'H', 'F', 'P', 'b', 'f'] {
        h.key(KeyCode::Char(key));
    }
    assert!(h.sent().is_empty());

    h.key(KeyCode::Char('a'));
    assert!(h.app.picker.is_none());
    assert!(h.app.status.contains("console"), "{}", h.app.status);
}

#[test]
fn another_view_reads_the_trees_checkout_and_coming_back_shows_the_console() {
    let mut h = with_consoles(&[900]);
    h.key(KeyCode::Char('j'));
    h.key(KeyCode::Char('2'));
    assert_eq!(h.app.current_checkout().map(|c| c.id), Some(CheckoutId(10)));

    h.key(KeyCode::Char('1'));
    assert_eq!(h.app.column_pane(), Some(PaneId(900)));
}

#[test]
fn a_restarted_daemon_without_consoles_takes_them_off_the_rail() {
    let mut h = with_consoles(&[900]);
    h.app.greeted(None);
    assert!(h.app.consoles.is_empty());
    assert!(!h.app.consoles_offered);
}
