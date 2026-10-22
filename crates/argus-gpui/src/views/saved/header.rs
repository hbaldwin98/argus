//! A saved column's own header: the title row when there are several columns, and the meta row with
//! the scope, when it was built, the view's buttons, Refresh, Mark stale, Details and the `...`
//! menu (CHECKOUTS-CODE-VIEWS-SAVED-5, -6, -9).
//!
//! Every press goes to the daemon and the column changes only when it says: a refresh waits until
//! the view is saved again, a stale mark until the daemon lists it stale, a delete until the view is
//! gone. What the text says is core's (`views::column`).

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::Panel;
use argus_desktop_core::views::column::{built, details};
use argus_desktop_core::views::saved::PageSaved;
use argus_protocol::Chip;
use argus_protocol::Tone;
use gpui_kit::component::h_flex;
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, ClickEvent, MouseButton, Role, SharedString};

use super::delete::{self, Reach};
use super::details as fold;
use crate::kit::button::{button, off, Look};
use crate::kit::icon_button::icon_button;
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::actions::{action_button, PressWith};
use crate::views::blocks::pen::Pen;
use crate::views::chip::chip;
use crate::views::ctx::ViewCtx;

/// Whose header it is.
pub struct Head<'a> {
    pub panel: &'a Panel,
    pub index: usize,
    pub view: &'a PageSaved,
    /// The only column: it is named and closed by its host (the dock's tab, the panel's head or the
    /// float's title bar), so it has no title row.
    pub lone: bool,
    /// The panel draws a head of its own, which then carries the stale chip.
    pub headed: bool,
    /// The daemon is gone: a press sends nothing.
    pub held: bool,
}

fn stale() -> Chip {
    Chip { label: "stale".into(), tone: Tone::Warn }
}

/// The close button of the column at `index`: "Close this view" for the first, "Close this column"
/// for the others.
pub fn close_button(panel: &Panel, index: usize, pen: &Pen, held: bool) -> AnyElement {
    let tokens = pen.draw.tokens;
    let words = if index == 0 { "Close this view" } else { "Close this column" };
    let (caller, panel_id) = (pen.draw.caller.clone(), panel.clone());
    icon_button(SharedString::from(format!("saved-close-{panel}-{index}")), words, false, tokens)
        .tooltip(tip(words))
        .on_click(move |_, _, _| {
            if !held {
                caller.call(Call::CloseSavedColumn { panel: panel_id.clone(), column: index })
            }
        })
        .child(Glyph::Close.at(tokens.icon_sm))
        .into_any_element()
}

fn title_row(head: &Head, pen: &Pen) -> AnyElement {
    let (tokens, view) = (pen.draw.tokens, head.view);
    let (panel, index, held) = (head.panel, head.index, head.held);
    let id = format!("saved-title-{panel}-{index}");
    let mine = panel.clone();
    let middle = {
        let (caller, mine) = (pen.draw.caller.clone(), mine.clone());
        move |event: &ClickEvent, _: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            if event.is_middle_click() {
                cx.stop_propagation();
                if !held {
                    caller.call(Call::CloseSavedColumn { panel: mine.clone(), column: index });
                }
            }
        }
    };
    let beyond_first = (index > 0).then(|| {
        let (float, pop) = (pen.draw.caller.clone(), pen.draw.caller.clone());
        let (float_panel, pop_panel) = (mine.clone(), mine.clone());
        h_flex()
            .gap(rem(tokens.sp_8))
            .child(
                icon_button(SharedString::from(format!("{id}-float")), "Open as a dialog", false, tokens)
                    .tooltip(tip("Open as a dialog"))
                    .on_click(move |_, _, _| {
                        if !held {
                            float.call(Call::FloatSavedColumn { panel: float_panel.clone(), column: index })
                        }
                    })
                    .child(Glyph::Box.at(tokens.icon_sm)),
            )
            .child(
                icon_button(SharedString::from(format!("{id}-pop")), "Pop out into a panel of its own", false, tokens)
                    .tooltip(tip("Pop out into a panel of its own"))
                    .on_click(move |_, _, _| {
                        if !held {
                            pop.call(Call::PopOutSavedColumn { panel: pop_panel.clone(), column: index })
                        }
                    })
                    .child(Glyph::Expand.at(tokens.icon_sm)),
            )
    });
    h_flex()
        .id(SharedString::from(id))
        .test_support()
        .gap(rem(tokens.sp_8))
        .items_center()
        // A middle press is the row's alone, as a tab's is: it must not autoscroll.
        .on_mouse_down(MouseButton::Middle, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .on_aux_click(middle)
        .child(div().flex_1().min_w_0().truncate().typed(tokens.type_heading).child(SharedString::from(view.view.title.clone())))
        .children(view.view.status.as_ref().map(|status| chip(status, tokens)))
        .children(view.stale.then(|| chip(&stale(), tokens)))
        .children(beyond_first)
        .child(close_button(panel, index, pen, held))
        .into_any_element()
}

fn meta_row(head: &Head, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let (tokens, view) = (pen.draw.tokens, head.view);
    let (panel, index, held) = (head.panel, head.index, head.held);
    let id = format!("saved-meta-row-{panel}-{index}");
    let built = built(pen.draw.clock, view);
    let name = panel.to_string();
    let open_key = fold::key(&name, index, &view.id);
    let open = pen.memory.borrow().open(&open_key, false);
    let reach = Reach { memory: pen.memory.clone(), repaint: pen.repaint };
    let delete_key = delete::key(&name, index, &view.id);
    let asking = pen.memory.borrow().asking(&delete_key);

    let refresh_id = view.id.clone();
    let refresh = if view.refreshing {
        off(SharedString::from(format!("{id}-refresh")), "Refresh", true, tokens).tooltip(tip("Ask the agent that saved it to build it again")).child("Refresh").into_any_element()
    } else {
        let caller = pen.draw.caller.clone();
        button(SharedString::from(format!("{id}-refresh")), "Refresh", Look::Plain, true, tokens)
            .tooltip(tip("Ask the agent that saved it to build it again"))
            .child("Refresh")
            .on_click(move |_, _, _| {
                if !held {
                    caller.call(Call::RefreshSavedView { id: refresh_id.clone() })
                }
            })
            .into_any_element()
    };
    let stale_id = view.id.clone();
    let mark = if view.stale {
        off(SharedString::from(format!("{id}-stale")), "Mark stale", true, tokens).tooltip(tip("Say it is out of date")).child("Mark stale").into_any_element()
    } else {
        let caller = pen.draw.caller.clone();
        button(SharedString::from(format!("{id}-stale")), "Mark stale", Look::Plain, true, tokens)
            .tooltip(tip("Say it is out of date"))
            .child("Mark stale")
            .on_click(move |_, _, _| {
                if !held {
                    caller.call(Call::MarkSavedViewStale { id: stale_id.clone() })
                }
            })
            .into_any_element()
    };
    let toggle = {
        let (memory, key, repaint) = (pen.memory.clone(), open_key.clone(), pen.repaint);
        button(SharedString::from(format!("{id}-details")), "Details", Look::Plain, true, tokens)
            .aria_expanded(open)
            .tooltip(tip("What it was built from"))
            .child("Details")
            .on_click(move |_, _, cx| {
                memory.borrow_mut().set_open(&key, !open);
                cx.notify(repaint);
            })
    };
    let own: Vec<AnyElement> = view.view.actions.iter().map(|action| action_button(action, ctx, std::rc::Rc::new(PressWith::default), false, pen)).collect();
    let question = asking.then(|| delete::question(&id, &view.id, delete_key.clone(), reach.clone(), pen.draw.caller.clone(), held, tokens));
    let more = delete::more_button(format!("{id}-more"), &view.view.title, delete_key, reach, held, tokens, pen.cx);
    let meta = |text: SharedString| div().text_size(rem(tokens.fs_xs)).text_color(tokens.fg_faint).child(text);
    let status = (head.lone).then(|| view.view.status.as_ref().map(|status| chip(status, tokens))).flatten();
    let stale_chip = (head.lone && view.stale && !head.headed).then(|| chip(&stale(), tokens));
    h_flex()
        .id(SharedString::from(id))
        .role(Role::Group)
        .test_support()
        .aria_label("About this view")
        .gap(rem(tokens.sp_8))
        .flex_wrap()
        .items_center()
        .children(status)
        .children(stale_chip)
        .child(meta(SharedString::from(view.scope.label.clone())))
        .child(meta(SharedString::from(built.said.clone())).id(SharedString::from(format!("saved-built-{panel}-{index}"))).tooltip(tip(built.exact)))
        .child(h_flex().flex_shrink_0().gap(rem(tokens.sp_8)).items_center().ml_auto().children(own).child(refresh).child(mark).children(question).child(toggle).child(more))
        .into_any_element()
}

/// The column's header: the title row (not for a lone view), the meta row and, while Details is open,
/// the details. A stale column carries a bar of the attention colour down the header's left edge.
pub fn header(head: &Head, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let open = pen.memory.borrow().open(&fold::key(&head.panel.to_string(), head.index, &head.view.id), false);
    let more = open.then(|| fold::list(&details(pen.draw.clock, head.view), &head.view.id, &pen.draw));
    let title = (!head.lone).then(|| title_row(head, pen));
    let meta = meta_row(head, ctx, pen);
    v_flex()
        .id(SharedString::from(format!("saved-colhead-{}-{}", head.panel, head.index)))
        .test_support()
        .relative()
        .flex_shrink_0()
        .gap(rem(tokens.sp_6))
        .px(rem(tokens.sp_16))
        .py(rem(tokens.sp_12))
        .border_b_1()
        .border_color(tokens.line)
        .children(head.view.stale.then(|| div().absolute().left_0().top_0().bottom_0().w(rem(tokens.mark_bar_w)).bg(tokens.attention)))
        .children(title)
        .child(meta)
        .children(more)
        .into_any_element()
}
