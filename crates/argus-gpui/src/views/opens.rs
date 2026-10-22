//! The small arrow beside an element of a view that opens a saved view (the page's `opensMark`), for
//! the elements labelled by plain text: a section, a card, a stat, a tree node, a checklist item.
//!
//! A press asks Rust to open the target, as the next column of the saved panel the press was in or
//! as a panel of its own; this never decides where. A target the window does not hold is drawn dimmed
//! and struck through, and is still pressed, since the view may be saved later.

use argus_desktop_core::connection::page::Call;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{prelude::*, AnyElement, Role, SharedString};

use super::blocks::pen::Pen;
use super::ctx::ViewCtx;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;

/// What a press of the mark of `id` in the view `ctx` is in asks for.
pub fn open_call(id: &str, ctx: &ViewCtx) -> Call {
    Call::OpenSavedView { id: id.to_owned(), from: ctx.saved.clone() }
}

/// `.vopen`: an arrow in the reference colour at the end of the element's label, lighting to the
/// fill under the pointer. Nothing when the element opens none. `key` names it for a test.
pub fn opens_mark(opens: Option<&str>, key: &str, ctx: &ViewCtx, pen: &Pen) -> Option<AnyElement> {
    let id = opens?;
    let tokens = pen.draw.tokens;
    let held = pen.draw.saved_title(id);
    let name = held.unwrap_or(id).to_owned();
    let (caller, asked, fill) = (pen.draw.caller.clone(), open_call(id, ctx), tokens.fill);
    Some(
        gpui_kit::div()
            .id(SharedString::from(format!("{key}-opens")))
            .role(Role::Button)
            .test_support()
            .aria_label(format!("Open the saved view {name}"))
            .ml(rem(tokens.sp_6))
            .px(rem(tokens.sp_4))
            .rounded(rem(tokens.r_xs))
            .text_color(tokens.ref_)
            .text_size(rem(tokens.fs_xs))
            .weighted(tokens.weight_semibold)
            .cursor_pointer()
            .hover(move |style| style.bg(fill))
            .when(held.is_none(), |mark| mark.opacity(tokens.opacity_dim).line_through())
            .tooltip(tip(if held.is_some() { format!("Open {name}") } else { format!("No saved view {id} yet") }))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                caller.call(asked.clone());
            })
            .child("\u{2197}")
            .into_any_element(),
    )
}
