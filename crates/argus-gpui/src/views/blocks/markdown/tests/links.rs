//! What a click on a link asks for: Rust opens it, and only a web address ever got this far.

use gpui_kit::TestAppContext;

use super::*;

fn opened_urls(opened: &Opened) -> Vec<String> {
    opened.calls.borrow().iter().filter_map(|call| if let Call::OpenUrl { url } = call { Some(url.clone()) } else { None }).collect()
}

#[gpui_kit::test]
fn a_click_on_a_link_asks_rust_to_open_its_address(cx: &mut TestAppContext) {
    let opened = open(cx, "see [the docs](https://example.com/docs) now");
    click_in(cx, &opened, 0, 6);
    assert_eq!(opened_urls(&opened), ["https://example.com/docs"]);
}

#[gpui_kit::test]
fn a_click_beside_the_link_asks_for_nothing(cx: &mut TestAppContext) {
    let opened = open(cx, "see [the docs](https://example.com/docs) now");
    click_in(cx, &opened, 0, 1);
    click_in(cx, &opened, 0, 14);
    assert!(opened.calls.borrow().is_empty(), "{:?}", opened.calls.borrow());
}

#[gpui_kit::test]
fn a_bare_address_is_a_link_and_so_is_an_image_but_a_script_is_neither(cx: &mut TestAppContext) {
    let opened = open(cx, "https://a.b/x ![pic](https://c.d/p.png) [bad](javascript:alert(1))");
    click_in(cx, &opened, 0, 3);
    click_in(cx, &opened, 0, 15);
    let last = cx.update(|cx| opened.view.read(cx).leaves[0].styled.text.len());
    click_in(cx, &opened, 0, last - 1);
    assert_eq!(opened_urls(&opened), ["https://a.b/x", "https://c.d/p.png"]);
}

#[gpui_kit::test]
fn dragging_across_a_link_selects_it_and_does_not_open_it(cx: &mut TestAppContext) {
    let opened = open(cx, "see [the docs](https://example.com/docs) now");
    let (from, to) = (at(cx, &opened, 0, 2), at(cx, &opened, 0, 10));
    drag(cx, &opened, from, to);
    assert_eq!(selected(cx, &opened).as_deref(), Some("e the do"));
    assert!(opened_urls(&opened).is_empty());
}

fn hover_at(cx: &mut TestAppContext, opened: &Opened, place: gpui_kit::Point<gpui_kit::Pixels>) {
    use gpui_kit::test::TestEventExt as _;
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(place, None, cx)).unwrap();
    settle(cx, opened);
}

fn hovered(cx: &mut TestAppContext, opened: &Opened) -> Option<(usize, usize)> {
    cx.update(|cx| opened.view.read(cx).hover.map(|hover| (hover.leaf, hover.link)))
}

#[gpui_kit::test]
fn the_pointer_over_a_link_underlines_it_and_names_its_address_and_off_it_does_neither(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;
    let opened = open(cx, "see [the docs](https://example.com/docs) now");
    let place = at(cx, &opened, 0, 1);
    hover_at(cx, &opened, place);
    assert_eq!(hovered(cx, &opened), None);
    let shown = |cx: &mut TestAppContext| cx.update_window(opened.handle, |_, window, _| window.try_find("md-address".to_owned()).is_some()).unwrap();
    assert!(!shown(cx));
    let place = at(cx, &opened, 0, 6);
    hover_at(cx, &opened, place);
    assert_eq!(hovered(cx, &opened), Some((0, 0)));
    assert!(shown(cx), "the address a click will open is shown beside the pointer");
    let place = at(cx, &opened, 0, 14);
    hover_at(cx, &opened, place);
    assert_eq!(hovered(cx, &opened), None);
    assert!(!shown(cx));
}
