//! The peek panel beside the list: the feature under the cursor in a few lines, and the three
//! things to do with it (`peekPanel` of `feature_stage.ts`).
//!
//! Rust builds every line (`Peek`), so this draws what it says and sends what is pressed. With
//! nothing peeked it says so. `Open` opens the feature, `Go to its pane` goes to its agent's, and
//! the `...` opens the Move menu under it, with the `Comes after...` item only this opening
//! offers. The linked tickets (`ticketLinks`) are the tracker step's, and are wired after it.

pub mod lines;

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, Features};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Bounds, Entity, Pixels, ScrollHandle, SharedString, TestSupportExt as _};
use lines::{section, Tone};

use super::cells::meta;
use super::ctx::Ctx;
use super::menus::Menu;
use crate::kit::area;
use crate::kit::button::{button, off, Look};
use crate::kit::icon_button::{icon_button, Node};
use crate::kit::icons::Glyph;
use crate::kit::pill::{standing_status, status_pill};
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::scale::Scale;
use crate::theme::Tokens;

/// The panel's frame: a share of the stage between a floor and a ceiling, scrolling by itself.
///
/// The page's `.peek` is content-box, so its padding comes on top of the clamped `34%`: the whole
/// panel is the clamp of the stage's width plus both sides of padding, which is what `stage` (the
/// stage's width in CSS pixels, 0 before it is measured) gives it here.
pub fn width(stage: f32, tokens: &Scale) -> f32 {
    (stage * tokens.fx_peek_share).clamp(tokens.fx_peek_min, tokens.fx_peek_max) + 2.0 * tokens.sp_20
}

fn frame(scroll: &ScrollHandle, stage: f32, tokens: &Tokens) -> Node {
    v_flex()
        .id("feature-peek")
        .test_support()
        .w(rem(width(stage, tokens)))
        .flex_shrink_0()
        .h_full()
        .overflow_y_scroll()
        .track_scroll(scroll)
        .gap(rem(tokens.sp_16))
        .px(rem(tokens.sp_20))
        .py(rem(tokens.sp_16))
}

pub fn panel(features: &Features, ctx: &Ctx, menu: &Entity<Menu>, scroll: &ScrollHandle, more: Rc<Cell<Bounds<Pixels>>>, stage: f32, tokens: &Tokens) -> impl IntoElement {
    let Some(peek) = &features.peek else {
        return frame(scroll, stage, tokens).aria_label("Peek").child(meta("Choose a feature to peek at it.", tokens)).into_any_element();
    };
    let listed = features.rows.iter().any(|row| row.slug == peek.slug);
    let (open, go, opening, going) = (ctx.clone(), ctx.clone(), peek.slug.clone(), peek.pane);
    let (at, menu, slug) = (more.clone(), menu.clone(), peek.slug.clone());
    frame(scroll, stage, tokens)
        .aria_label(SharedString::from(format!("Peek at #{}", peek.number)))
        .child(
            v_flex()
                .child(
                    h_flex()
                        .items_center()
                        .gap(rem(tokens.sp_8))
                        .typed(tokens.type_label)
                        .child(div().text_color(tokens.accent).child(SharedString::from(format!("#{}", peek.number))))
                        .child(status_pill("peek-standing", peek.standing, standing_status(peek.standing), tokens))
                        .child(div().flex_1())
                        .children(peek.branch.clone().map(|branch| {
                            div().typed(tokens.type_small).text_color(tokens.fg_faint).weighted(tokens.weight_regular).child(SharedString::from(format!("from {branch}")))
                        })),
                )
                .child(div().id("peek-title").test_support().mt(rem(tokens.sp_4)).typed(tokens.type_title).text_color(tokens.fg).child(SharedString::from(peek.title.clone())))
                .children(peek.held.clone().map(|reason| div().text_color(tokens.attention).child(SharedString::from(format!("held: \u{201c}{reason}\u{201d}")))))
                .when(!peek.brief.is_empty(), |head| head.child(div().mt(rem(tokens.sp_6)).text_color(tokens.fg_muted).child(SharedString::from(peek.brief.clone())))),
        )
        .children(section("Needs you", &peek.needs_you, "?", &Tone::needs_you(tokens), tokens))
        .children(section("In flight", &peek.in_flight, "\u{25d0}", &Tone::in_flight(tokens), tokens))
        .children(section("Next up", peek.next_up.as_slice(), "\u{25cb}", &Tone::next_up(tokens), tokens))
        .children(section("Last decided", peek.last_decided.as_slice(), "\u{2713}", &Tone::last_decided(tokens), tokens))
        .child(div().flex_1())
        .child(
            h_flex()
                .gap(rem(tokens.sp_8))
                .items_center()
                .child(button("peek-open", "Open", Look::Primary, false, tokens).child("Open").on_click(move |_, _, cx| open.send(Call::OpenFeature { slug: Some(opening.clone()) }, cx)))
                .child(match going {
                    Some(pane) => button("peek-go", "Go to its pane", Look::Plain, false, tokens).child("Go to its pane").on_click(move |_, _, cx| go.send(Call::GoToPane { pane }, cx)).into_any_element(),
                    None => off("peek-go", "Go to its pane", false, tokens).child("Go to its pane").into_any_element(),
                })
                .child(div().flex_1())
                .when(listed, |actions| {
                    actions.child(
                        icon_button("peek-more", "More: hold, archive, order", false, tokens)
                            .relative()
                            .tooltip(tip("Hold, archive, or order it after another"))
                            .on_click(move |_, window, cx| {
                                let button = at.get();
                                // Under the button, at its left edge, as the page places it.
                                let under = gpui_kit::point(button.left(), button.bottom());
                                menu.update(cx, |menu, cx| menu.open_move(&slug, under, true, window, cx));
                            })
                            .child(area::measure(more.clone()))
                            .child(Glyph::More.at(tokens.icon_sm)),
                    )
                }),
        )
        .into_any_element()
}

#[cfg(test)]
mod width_tests {
    use super::width;
    use crate::theme::scale::Scale;

    /// BOARD-PEEK: `.peek` is content-box, so its padding is on top of `clamp(280px, 34%, 420px)`.
    #[test]
    fn the_peek_is_the_clamped_share_of_the_stage_plus_its_padding() {
        let t = Scale::root();
        let pad = 2.0 * t.sp_20;
        assert_eq!(width(1152.0, &t), 1152.0 * t.fx_peek_share + pad);
        assert_eq!(width(100.0, &t), t.fx_peek_min + pad);
        assert_eq!(width(5000.0, &t), t.fx_peek_max + pad);
    }
}
