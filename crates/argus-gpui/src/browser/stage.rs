//! The Browsers stage (SHELL-STAGE-16): every browser as a tile, in the order Rust lists them (the
//! order the agents were started in), or the words for none open. Drawn where the Workspace's pane
//! is while the session's stage is Browsers (`chrome/stage`).
//!
//! Core says which browsers there are (`BrowsersPage`); the pictures are `Heard`'s, decoded once
//! and shown by one [`PictureView`] a browser, kept while the browser lasts so a tile's picture
//! repaints alone. The grid fits as many columns of at least `browser_tile_min` as the stage holds,
//! as `repeat(auto-fill, minmax(360px, 1fr))` does, so the stage measures itself and asks to be
//! drawn again when the number of columns changes. The strip's words (`Browsers`, `Every agent`)
//! are core's.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::{canvas, div, prelude::*, Bounds, Context, Entity, Pixels, Role, TestSupportExt as _, Window};

use super::empty;
use super::picture::PictureView;
use super::tile::tile;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::theme::scale::{css, rem};
use crate::theme::Tokens;

pub struct BrowsersStage {
    heard: Entity<Heard>,
    caller: Caller,
    /// One view of the picture for each browser listed, dropped with the browser.
    pictures: BTreeMap<u64, Entity<PictureView>>,
    /// The area the stage was laid out in, in the window's pixels.
    area: Rc<Cell<Bounds<Pixels>>>,
}

/// How many columns of tiles `width` css pixels holds: as many as fit at the least tile width with
/// the grid's padding and gaps, and always one.
pub fn columns(width: f32, tokens: &Tokens) -> usize {
    let fit = (width - 2.0 * tokens.sp_12 + tokens.sp_12) / (tokens.browser_tile_min + tokens.sp_12);
    (fit.floor() as usize).max(1)
}

impl BrowsersStage {
    pub fn new(heard: Entity<Heard>, caller: Caller, cx: &mut Context<Self>) -> Self {
        cx.observe(&heard, |_, _, cx| cx.notify()).detach();
        Self { heard, caller, pictures: BTreeMap::new(), area: Rc::default() }
    }
}

impl Render for BrowsersStage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let (page, held) = {
            let heard = self.heard.read(cx);
            (heard.browsers.clone(), heard.held())
        };
        // A browser that closed lets its view go; one that opened gets its own.
        self.pictures.retain(|pane, _| page.browsers.iter().any(|browser| browser.pane == *pane));
        for browser in &page.browsers {
            let heard = self.heard.clone();
            self.pictures.entry(browser.pane).or_insert_with(|| cx.new(|cx| PictureView::new(heard, browser.pane, tokens.r_md - 1.0, cx)));
        }
        let root = div().id("browsers").test_support().role(Role::Group).relative().size_full().bg(tokens.bg_sunken);
        if page.browsers.is_empty() {
            return root.child(empty::stage(&tokens)).into_any_element();
        }
        let rem_size = window.rem_size();
        let measured = self.area.get().size.width;
        let width = css(if measured > Pixels::ZERO { measured } else { window.viewport_size().width }, rem_size);
        // Measured as it is laid out, so a stage that changes its columns is drawn again at once.
        let (area, me, then) = (self.area.clone(), cx.entity_id(), tokens);
        let measure = canvas(
            move |bounds, window, cx| {
                let before = columns(css(area.get().size.width, window.rem_size()), &then);
                area.set(bounds);
                if columns(css(bounds.size.width, window.rem_size()), &then) != before {
                    cx.notify(me);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let tiles = page.browsers.iter().map(|browser| tile(browser, self.pictures[&browser.pane].clone().into(), &self.caller, held, &tokens));
        root.child(measure)
            .child(
                div()
                    .id("browsers-grid")
                    .test_support()
                    .size_full()
                    .overflow_y_scroll()
                    .grid()
                    .grid_cols(columns(width, &tokens) as u16)
                    .content_start()
                    .gap(rem(tokens.sp_12))
                    .p(rem(tokens.sp_12))
                    .children(tiles),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::themes::ledger;

    #[test]
    fn the_columns_are_as_many_as_fit_at_the_least_tile_width_and_never_none() {
        let tokens = ledger::tokens();
        // padding 12 + gaps of 12 + tiles of 360.
        assert_eq!(columns(0.0, &tokens), 1);
        assert_eq!(columns(384.0, &tokens), 1);
        assert_eq!(columns(755.9, &tokens), 1);
        assert_eq!(columns(756.0, &tokens), 2);
        assert_eq!(columns(1128.0, &tokens), 3);
        assert_eq!(columns(1127.0, &tokens), 2);
    }
}
