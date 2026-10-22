//! The pane on screen is brought into view once, not on every redraw.

use super::*;

/// A rail with one repository of `n` shell panes under `main`, the `on`th shown.
fn tall(n: u64, on: u64) -> Event {
    let panes = (1..=n).map(|id| pane(id, false, &format!("shell {id}"), "idle", id == on)).collect();
    let repo = Repository {
        id: 11,
        name: "argus".into(),
        quiet: false,
        divider: false,
        open: true,
        panes: n as usize,
        status: None,
        used: None,
        checkouts: vec![checkout_row(110, "main", true, panes)],
    };
    let mut rail = busy();
    rail.project.as_mut().unwrap().repositories = vec![repo];
    rail.shown = Some(Shown {
        pane: on,
        repository: "argus".into(),
        checkout: "main".into(),
        branch: "main".into(),
        agent: false,
        browser: false,
        title: format!("shell {on}"),
        status: "idle",
        resumed: None,
    });
    Event::Rail(Box::new(rail))
}

fn offset(cx: &mut TestAppContext, opened: &Opened) -> f32 {
    cx.update(|cx| opened.rail.read(cx).scrolled().as_f32())
}

/// The frames the scroll takes to be asked for, made and drawn.
fn settle(cx: &mut TestAppContext, opened: &Opened) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }
}

#[gpui_kit::test]
fn the_shown_pane_below_the_fold_is_scrolled_into_view_once_and_the_list_is_then_left_alone(cx: &mut TestAppContext) {
    // SHELL-RAIL-18.
    let opened = open(cx, vec![status(Connection::Connected), tall(40, 1)]);
    settle(cx, &opened);
    assert_eq!(offset(cx, &opened), 0.0, "the first pane is in view");
    hear(cx, &opened, vec![tall(40, 38)]);
    settle(cx, &opened);
    let moved = offset(cx, &opened);
    assert!(moved < 0.0, "the pane below the fold was brought up: {moved}");
    let row = cx.update_window(opened.handle, |_, window, _| window.find("pane-38").bounds()).unwrap();
    let list = cx.update_window(opened.handle, |_, window, _| window.find("rail-list").bounds()).unwrap();
    // To the pixel: the scroll handle and the list differ by one in how they count the list's edge.
    assert!(row.bottom().as_f32() <= list.bottom().as_f32() + 1.0 && row.top() >= list.top(), "{row:?} is not inside {list:?}");
    // A person reading the rail is not fought: a redraw with the same pane shown leaves it where it is.
    cx.update(|cx| opened.rail.read(cx).scroll_by(40.0));
    hear(cx, &opened, vec![tall(40, 38)]);
    settle(cx, &opened);
    assert_eq!(offset(cx, &opened), moved + 40.0, "the same pane is not scrolled to again");
}

#[gpui_kit::test]
fn another_pane_on_screen_is_scrolled_to_in_its_turn(cx: &mut TestAppContext) {
    let opened = open(cx, vec![status(Connection::Connected), tall(40, 38)]);
    settle(cx, &opened);
    hear(cx, &opened, vec![tall(40, 38)]);
    settle(cx, &opened);
    let low = offset(cx, &opened);
    assert!(low < 0.0, "{low}");
    hear(cx, &opened, vec![tall(40, 1)]);
    settle(cx, &opened);
    let row = cx.update_window(opened.handle, |_, window, _| window.find("pane-1").bounds()).unwrap();
    let list = cx.update_window(opened.handle, |_, window, _| window.find("rail-list").bounds()).unwrap();
    assert!(row.top().as_f32() >= list.top().as_f32() - 1.0 && row.bottom() <= list.bottom(), "the first pane is brought back into view: {row:?} in {list:?}");
}
