//! The Views launcher at the strip's end: `Views`, and how many views agents have shown
//! (`#views-tab`, `drawViews`).
//!
//! The page keeps it while the views panel is out, lit, so it is the way to close a drawer too;
//! it is gone when no agent has shown a view. Its press is `toggle_views`. The window does not
//! hear the agents' views yet (audit item 80), so the shell draws it with none until it does;
//! where the launcher lives once the strip has no room is an open question (`audit.md` 5, 14),
//! and it stays where the page has it.

use argus_desktop_core::connection::page::Call;
use gpui_kit::{div, prelude::*, px, SharedString};

use crate::connection::caller::Caller;
use crate::kit::button::{button, Look};
use crate::theme::Tokens;

/// What the launcher says: nothing at all with no view to open.
pub fn words(waiting: usize) -> Option<String> {
    (waiting > 0).then(|| format!("{waiting} {} agents showed", if waiting == 1 { "view" } else { "views" }))
}

/// The launcher for `waiting` views, `out` when the panel is out. Pressing it sends nothing while
/// `held`: the daemon is gone.
pub fn launcher(waiting: usize, out: bool, held: bool, caller: &Caller, tokens: &Tokens) -> Option<impl IntoElement> {
    let tip = words(waiting)?;
    let caller = caller.clone();
    let count = div()
        .min_w(px(tokens.sp_20))
        .px(px(tokens.sp_6))
        .rounded(px(tokens.r_pill))
        .bg(tokens.fill)
        .text_color(tokens.fg_muted)
        .text_size(px(tokens.fs_xs))
        .line_height(px(tokens.sp_20))
        .text_center()
        .child(SharedString::from(waiting.to_string()));
    Some(
        button("views-tab", tip, Look::Plain, true, tokens)
            // The page says aria-pressed; the kit re-exports no toggled state, so it is the selected one.
            .aria_selected(out)
            .when(out, |button| button.bg(tokens.bg_selected).border_color(tokens.line_strong))
            .gap(px(tokens.sp_6))
            .child("Views")
            .child(count)
            .on_click(move |_, _, _| {
                if !held {
                    caller.call(Call::ToggleViews);
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
