//! What shows a carried pane: its name following the pointer, the zone it would land on, and the
//! stage outlined when it would open there.
//!
//! The page lit the zone with `.drop-zone` over the cell and marked the body `dropping-on-stage`;
//! a checkout row as a target is the rail's own to draw (it reads [`Spots::lit`]), since the row
//! is its element. Everything here is drawn over the work area and takes no press.

use argus_desktop_core::panes::shown::workspace_grid::zones::zone_rect;
use argus_desktop_core::panes::shown::workspace_grid::Zone;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{anchored, deferred, div, point, prelude::*, px, AnyElement, Bounds, Pixels, Point, SharedString};

use super::landing::Landing;
use crate::kit::spots::{Spot, Spots};
use crate::theme::Tokens;

/// How far the name sits from the pointer, so it does not hide what is dropped on.
const OFFSET: f32 = 12.0;

/// What the zone of a cell says in words, for the one who cannot see it lit.
pub fn zone_words(zone: Zone) -> &'static str {
    match zone {
        Zone::Centre => "Swap places",
        Zone::Left => "Place on the left",
        Zone::Right => "Place on the right",
        Zone::Top => "Place above",
        Zone::Bottom => "Place below",
    }
}

/// The pane's name beside the pointer.
pub fn ghost(title: &str, at: Point<Pixels>, tokens: &Tokens) -> AnyElement {
    let title = SharedString::from(title.to_owned());
    deferred(
        anchored().position(at + point(px(OFFSET), px(OFFSET))).snap_to_window().child(
            div()
                .id("drag-ghost")
                .test_support()
                .aria_label(title.clone())
                .px_2()
                .py_1()
                .rounded(px(tokens.r_sm))
                .border_1()
                .border_color(tokens.line_strong)
                .bg(tokens.bg_surface)
                .text_sm()
                .text_color(tokens.fg)
                .child(title),
        ),
    )
    .into_any_element()
}

/// The area the landing lights, drawn in a box whose corner is `origin`: the zone of a cell, or the
/// stage. Nothing for a landing that lights nothing here.
pub fn lit(landing: Landing, origin: Point<Pixels>, spots: &Spots, tokens: &Tokens) -> Option<AnyElement> {
    let (id, words, area) = match landing {
        Landing::Cell { zone, bounds, .. } => {
            let rect = zone_rect(zone);
            let lit = Bounds::new(
                point(bounds.origin.x + bounds.size.width * rect.x, bounds.origin.y + bounds.size.height * rect.y),
                gpui_kit::size(bounds.size.width * rect.w, bounds.size.height * rect.h),
            );
            ("drop-zone", zone_words(zone), lit)
        }
        Landing::Stage => ("drop-stage", "Open on the stage", spots.get(Spot::Stage)?),
        Landing::Checkout(_) | Landing::Nowhere => return None,
    };
    let zone = matches!(landing, Landing::Cell { .. });
    Some(
        div()
            .id(id)
            .test_support()
            .aria_label(words)
            .absolute()
            .left(area.origin.x - origin.x)
            .top(area.origin.y - origin.y)
            .w(area.size.width)
            .h(area.size.height)
            .border_2()
            .border_color(tokens.accent)
            .when(zone, |lit| lit.bg(tokens.accent.opacity(tokens.share_landing)))
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_zone_has_words_and_they_differ() {
        let all = [Zone::Centre, Zone::Left, Zone::Right, Zone::Top, Zone::Bottom];
        let words: Vec<_> = all.iter().map(|zone| zone_words(*zone)).collect();
        for (i, a) in words.iter().enumerate() {
            assert!(!a.is_empty());
            assert!(words.iter().skip(i + 1).all(|b| a != b));
        }
    }
}
