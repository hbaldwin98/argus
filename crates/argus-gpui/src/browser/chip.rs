//! A browser's status drawn: the chip on a tile and the dot on a nav sub-row (SHELL-BROWSER-9).
//!
//! Core says who has the browser (`status::status`) and the words and class for it; this gives the
//! class its colour, as `.bstatus` and `.nav .bdot` do: the agent driving is the accent, the person
//! driving is `ok`, the person holding it is `attention`, and a browser nobody drives has a faint
//! dot and no chip.

use argus_desktop_core::browser::status::Status;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Hsla, Role, SharedString, TestSupportExt as _};

use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The colour the class gives the chip and the dot; none is faint.
pub fn colour(status: Option<Status>, tokens: &Tokens) -> Hsla {
    match status {
        Some(Status::Agent) => tokens.accent,
        Some(Status::Person) => tokens.ok,
        Some(Status::Held) => tokens.attention,
        None => tokens.fg_faint,
    }
}

/// `.bdot`: the filled mark alone.
pub fn dot(id: impl Into<SharedString>, status: Option<Status>, tokens: &Tokens) -> impl IntoElement {
    div().id(id.into()).flex_none().size(rem(tokens.dot_md)).rounded_full().bg(colour(status, tokens))
}

/// `.bstatus`: the dot and the words in a pill, or nothing when nobody has the browser.
pub fn chip(id: impl Into<SharedString>, status: Option<Status>, tokens: &Tokens) -> Option<impl IntoElement> {
    let status = status?;
    let (id, ink) = (id.into(), colour(Some(status), tokens));
    Some(
        h_flex()
            .id(id)
            .test_support()
            .role(Role::Status)
            .flex_none()
            .items_center()
            .gap(rem(tokens.sp_4))
            .px(rem(tokens.sp_8))
            .rounded(rem(tokens.r_pill))
            .border_1()
            .border_color(tokens.line_strong)
            .text_size(rem(tokens.fs_xs))
            .text_color(ink)
            .child(div().flex_none().size(rem(tokens.dot_md)).rounded_full().bg(ink))
            .child(status.word()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::settings::remembered::Theme;

    /// The page's style.css: `.bstatus.agent` and `.nav .bdot.agent` are the accent, `.person` is
    /// `ok`, `.held` is `attention`, and a dot with no class is `fg-faint` (SHELL-BROWSER-9).
    #[test]
    fn each_status_class_has_its_own_colour_and_nobody_is_faint() {
        let tokens = crate::theme::choice::tokens(Theme::default(), true, 100);
        assert_eq!(colour(Some(Status::Agent), &tokens), tokens.accent);
        assert_eq!(colour(Some(Status::Person), &tokens), tokens.ok);
        assert_eq!(colour(Some(Status::Held), &tokens), tokens.attention);
        assert_eq!(colour(None, &tokens), tokens.fg_faint);
        let all = [tokens.accent, tokens.ok, tokens.attention, tokens.fg_faint];
        for (at, one) in all.iter().enumerate() {
            assert!(all[at + 1..].iter().all(|other| other != one), "two classes share a colour");
        }
    }
}
