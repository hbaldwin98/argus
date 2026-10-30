//! The Views launcher at the strip's end: `Views`, and how many views agents have shown
//! (`#views-tab`, `drawViews`).
//!
//! The page keeps it while the views panel is out, lit, so it is the way to close a drawer too;
//! it is gone when no agent has shown a view. Its press is Rust's (`views::head::launch`): hide the
//! panel while it is out, else open the newest view. Where the launcher lives once the strip has no
//! room is an open question (`audit.md` 5, 14), and it stays where the page has it.

use argus_desktop_core::connection::page::Call;
use gpui_kit::{div, prelude::*, SharedString};

use crate::connection::caller::Caller;
use crate::kit::button::{button, Look};
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the launcher says: nothing at all with no view to open.
pub fn words(waiting: usize) -> Option<String> {
    (waiting > 0).then(|| format!("{waiting} {} agents showed", if waiting == 1 { "view" } else { "views" }))
}

/// The launcher for `waiting` views, `out` when the panel is out. Pressing it sends `press` (what
/// Rust says it does), and nothing while `held`: the daemon is gone.
pub fn launcher(waiting: usize, out: bool, press: Option<Call>, held: bool, caller: &Caller, tokens: &Tokens) -> Option<impl IntoElement> {
    let tip = words(waiting)?;
    let caller = caller.clone();
    let count = div()
        .min_w(rem(tokens.sp_20))
        .px(rem(tokens.sp_6))
        .rounded(rem(tokens.r_pill))
        .bg(tokens.fill)
        .text_color(tokens.fg_muted)
        .text_size(rem(tokens.fs_xs))
        .line_height(rem(tokens.sp_20))
        .text_center()
        .child(SharedString::from(waiting.to_string()));
    Some(
        button("views-tab", tip, Look::Plain, false, tokens)
            // The page says aria-pressed; the kit re-exports no toggled state, so it is the selected one.
            .aria_selected(out)
            .when(out, |button| button.bg(tokens.bg_selected).border_color(tokens.line_strong))
            .h(rem(tokens.dictate_chip_h))
            .rounded_full()
            .gap(rem(tokens.sp_6))
            .border_1()
            .border_color(tokens.line_strong)
            .child("Views")
            .child(count)
            .on_click(move |_, _, _| {
                if let (false, Some(press)) = (held, press.clone()) {
                    caller.call(press);
                }
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_gone_with_no_view_and_counts_one_as_a_view_and_more_as_views() {
        assert_eq!(words(0), None);
        assert_eq!(words(1).as_deref(), Some("1 view agents showed"));
        assert_eq!(words(3).as_deref(), Some("3 views agents showed"));
    }
}
