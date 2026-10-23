//! A panel in a cell of the grid: what it draws, and how it is carried in, moved and taken out.

use argus_desktop_core::browser::{BrowsersPage, PageBrowser};
use argus_desktop_core::layout::docks::{DocksPage, Panel, PanelKind, PresentPanel};
use argus_desktop_core::panes::shown::workspace_grid::Occupant;
use gpui_kit::{px, TestAppContext};

use super::*;

fn browser(pane: u64, agent: &str) -> PageBrowser {
    PageBrowser {
        pane,
        owner: 1,
        agent: agent.into(),
        url: format!("https://{agent}.test/"),
        title: String::new(),
        tabs: Vec::new(),
        shown: Some("t1".into()),
        following: true,
        held: false,
        driver: None,
        cursor: argus_protocol::BrowserCursor::Default,
        dialog: None,
        select: None,
        sized: None,
        context: None,
    }
}

/// Two agents' browsers open, and the Browsers panel told as one that may have a cell.
fn browsers_open() -> Vec<Event> {
    let present = PresentPanel { panel: Panel::Browsers, kind: PanelKind::Browsers, floats: true, stage: true, closes: false, cell: true };
    vec![
        Event::Browsers(Box::new(BrowsersPage { browsers: vec![browser(105, "trim"), browser(106, "screens")] })),
        Event::Docks(Box::new(DocksPage { present: vec![present], ..DocksPage::default() })),
    ]
}

/// Pane 1 with the Browsers panel split off beside it.
fn beside_the_browsers() -> GridPage {
    GridPage { beside: vec![None], panels: vec![Some(Panel::Browsers)], grid: vec![vec![0, 1]], heights: vec![1000], widths: vec![vec![500, 500]], ..alone() }
}

fn with_browsers(cx: &mut TestAppContext, row: GridPage) -> Opened {
    let opened = opened(cx, row);
    hear(cx, &opened, browsers_open());
    opened.calls.borrow_mut().clear();
    opened
}

#[gpui_kit::test]
fn a_cell_holding_the_browsers_draws_every_browser_under_a_header_that_takes_them_out(cx: &mut TestAppContext) {
    let opened = with_browsers(cx, beside_the_browsers());
    assert!(has(cx, &opened, "browsers-grid-panel"), "the panel's own body, as its dock draws it");
    assert!(has(cx, &opened, "browser-tile-105") && has(cx, &opened, "browser-tile-106"));
    let (cell, tiles) = (bounds(cx, &opened, "cell-1"), bounds(cx, &opened, "browsers-grid-panel"));
    assert!(cell.contains(&tiles.center()), "drawn in the cell, {tiles:?} in {cell:?}");
    assert!(!has(cx, &opened, "cell-1-status"), "a panel has no status to show");
    let close = centre(cx, &opened, "cell-close-1");
    down(cx, &opened, close);
    up(cx, &opened, close);
    assert_eq!(asked(&opened), ["CloseBeside { side: Beside(0) }"]);
}

#[gpui_kit::test]
fn the_browsers_item_carried_from_the_rail_onto_a_cell_splits_them_off_there(cx: &mut TestAppContext) {
    let opened = with_browsers(cx, alone());
    let cell = bounds(cx, &opened, "cell-0");
    let near_bottom = point(cell.center().x, cell.origin.y + cell.size.height - px(60.0));
    let from = centre(cx, &opened, "nav-browsers");
    down(cx, &opened, from);
    drag_to(cx, &opened, near_bottom);
    assert_eq!(label(cx, &opened, "drop-zone").as_deref(), Some("Place below"), "lit as a pane's drop is");
    up(cx, &opened, near_bottom);
    assert_eq!(asked(&opened), ["Docks { gesture: Grid { panel: Browsers, onto: Pane(1), zone: Bottom } }"]);
}

#[gpui_kit::test]
fn the_browsers_item_is_carried_only_while_a_browser_is_open(cx: &mut TestAppContext) {
    let opened = opened(cx, alone());
    let target = centre(cx, &opened, "cell-0");
    let from = centre(cx, &opened, "nav-browsers");
    down(cx, &opened, from);
    drag_to(cx, &opened, target);
    assert!(!has(cx, &opened, "drag-ghost"));
    up(cx, &opened, target);
    assert!(asked(&opened).iter().all(|call| !call.starts_with("Docks")), "{:?}", asked(&opened));
}

#[gpui_kit::test]
fn a_panels_header_carries_its_cell_onto_another_as_a_panes_does(cx: &mut TestAppContext) {
    let opened = with_browsers(cx, beside_the_browsers());
    let cell = bounds(cx, &opened, "cell-0");
    let near_left = point(cell.origin.x + px(10.0), cell.center().y);
    let from = centre(cx, &opened, "cell-head-1");
    down(cx, &opened, from);
    drag_to(cx, &opened, near_left);
    assert_eq!(label(cx, &opened, "drag-ghost").as_deref(), Some("Browsers"));
    up(cx, &opened, near_left);
    let asked_for = Call::MovePane { pane: Occupant::Panel(Panel::Browsers), onto: Occupant::Pane(1), zone: argus_desktop_core::panes::shown::workspace_grid::Zone::Left };
    assert_eq!(asked(&opened), [format!("{asked_for:?}")]);
}
