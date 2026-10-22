//! A feature's ticket as a chip: `<Tracker> <key>` with the key in the reference colour, a dot when an
//! agent showed a view of the ticket, and a press that opens that view or else the ticket in the
//! browser (BOARD-TICKETS-1, -2).
//!
//! The page places it in the feature index's peek alone (`ticketLinks`); an open feature's header has
//! a Tickets button instead, which is not a chip. `ticket_chip(id, &chip, &caller, &tokens)`. Which view is of the ticket, what a hover says and
//! what a press asks for are core's (`board/tracker/tickets`); this draws them. A press does not
//! reach the row under the chip, which would peek it.

use argus_desktop_core::board::tracker::tickets::TicketChip;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, ElementId, MouseButton, Role, SharedString, TestSupportExt as _};

use crate::connection::caller::Caller;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.track`: the chip for `chip`, named `id`; its accessible name is the ticket, and the dot says
/// `a view of it waits`.
pub fn ticket_chip(id: impl Into<ElementId>, chip: &TicketChip, caller: &Caller, tokens: &Tokens) -> impl IntoElement {
    let (fill, text) = (tokens.fill, tokens.fg);
    let viewed = chip.view.is_some();
    let name = SharedString::from(format!("{} {}", chip.link.tracker, chip.link.key));
    let (call, asking) = (chip.press(), caller.clone());
    h_flex()
        .id(id)
        .role(Role::Button)
        .test_support()
        .aria_label(name)
        .flex_shrink_0()
        .h(rem(tokens.icon_btn))
        .px(rem(tokens.sp_8))
        .gap(rem(tokens.sp_6))
        .items_center()
        .border_1()
        .border_color(tokens.line_strong)
        .rounded(rem(tokens.r_sm))
        .text_size(rem(tokens.fs_sm))
        .text_color(if viewed { tokens.fg } else { tokens.fg_muted })
        .cursor_pointer()
        .hover(move |style| style.bg(fill).text_color(text))
        .tooltip(tip(chip.tooltip()))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            asking.call(call.clone());
        })
        .child(SharedString::from(chip.link.tracker.clone()))
        .child(div().font_family(SharedString::from(tokens.font_mono)).weighted(tokens.weight_semibold).text_color(tokens.ref_).child(SharedString::from(chip.link.key.clone())))
        .when(viewed, |chip| {
            chip.child(div().id("ticket-view-dot").test_support().aria_label("a view of it waits").size(rem(tokens.dot_sm)).flex_shrink_0().rounded_full().bg(tokens.attention))
        })
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use argus_desktop_core::board::tracker::tickets::TicketView;
    use argus_desktop_core::connection::page::Link;
    use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
    use gpui_kit::{px, size, AnyWindowHandle, Bounds, Context, Render, TestAppContext, Window, WindowBounds, WindowOptions};

    use super::*;

    type Calls = Rc<RefCell<Vec<String>>>;

    /// A row that peeks when pressed, with the chip in it, as the index and the open feature place it.
    struct Host {
        chip: TicketChip,
        caller: Caller,
        row: Rc<Cell<u32>>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let tokens = *Tokens::global(cx);
            let row = self.row.clone();
            div().id("row").size_full().on_click(move |_, _, _| row.set(row.get() + 1)).child(ticket_chip("chip", &self.chip, &self.caller, &tokens))
        }
    }

    fn chip(view: Option<TicketView>) -> TicketChip {
        TicketChip { link: Link { tracker: "Linear".into(), key: "ENG-88".into(), url: "https://linear.app/x/issue/ENG-88".into(), role: "the work".into() }, view }
    }

    fn open(cx: &mut TestAppContext, chip: TicketChip) -> (AnyWindowHandle, Calls, Rc<Cell<u32>>) {
        cx.update(crate::window::init);
        let (calls, row): (Calls, Rc<Cell<u32>>) = Default::default();
        let (sink, peeks) = (calls.clone(), row.clone());
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions { window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(400.0), px(200.0))))), ..Default::default() },
                cx,
                move |_, cx| cx.new(|_| Host { chip, caller: Caller::new(move |call| sink.borrow_mut().push(format!("{call:?}"))), row: peeks }),
            )
            .expect("open test window")
        });
        cx.run_until_parked();
        (handle, calls, row)
    }

    fn press(cx: &mut TestAppContext, handle: AnyWindowHandle, label: &str) {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let at = window.find_by_label(label).bounds().center();
            window.pointer_move(at, None, cx);
            window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
            window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
        })
        .unwrap();
        cx.run_until_parked();
    }

    /// BOARD-TICKETS-1: a ticket with no view opens in the browser, and the press does not peek the row.
    #[gpui_kit::test]
    fn a_chip_opens_the_ticket_and_does_not_reach_the_row_under_it(cx: &mut TestAppContext) {
        let (handle, calls, row) = open(cx, chip(None));
        press(cx, handle, "Linear ENG-88");
        assert_eq!(*calls.borrow(), ["OpenUrl { url: \"https://linear.app/x/issue/ENG-88\" }"]);
        assert_eq!(row.get(), 0, "the row was not pressed");
        let dot = cx.update_window(handle, |_, window, _| window.try_find_by_label("a view of it waits").is_some()).unwrap();
        assert!(!dot, "no view, no dot");
    }

    /// BOARD-TICKETS-1: a ticket an agent showed a view of opens that view and carries the dot.
    #[gpui_kit::test]
    fn a_chip_with_a_view_opens_it_and_carries_the_dot(cx: &mut TestAppContext) {
        let view = TicketView { pane: 7, id: "plan".into(), title: "Plan".into(), from: "pane #32".into() };
        let (handle, calls, _) = open(cx, chip(Some(view)));
        let dot = cx.update_window(handle, |_, window, _| window.try_find_by_label("a view of it waits").is_some()).unwrap();
        assert!(dot);
        press(cx, handle, "Linear ENG-88");
        assert_eq!(*calls.borrow(), ["OpenView { pane: 7, id: \"plan\" }"]);
    }
}
