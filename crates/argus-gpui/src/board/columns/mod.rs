//! The board layout: Queued, Active and Done as three lanes and a side stack of Held over
//! Archived, each a column of cards (`board` of `feature_stage.ts`).
//!
//! Rows arrive in Rust's board order and are only filtered by group here. A card is dragged by
//! the kit's one drag helper (`drag`): only Held and Archived take a drop, and the card itself
//! moves when a pushed board says it did. Columns keep a floor width, so a narrow window scrolls
//! the lanes sideways, and a fade and a chip name what is cut off (`cue`). The scroll of the lanes
//! and of each column is held here, so a snapshot pushed while a person reads does not move them,
//! which the page, rebuilding its stage on every push, does not manage.

pub mod card;
pub mod cue;
pub mod drag;
pub mod lane;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use argus_desktop_core::board::index_words::FeatureColumn;
use argus_desktop_core::connection::page::Features;
use gpui_kit::component::h_flex;
use gpui_kit::{anchored, deferred, div, prelude::*, Bounds, Context, Entity, FocusHandle, Pixels, Point, ScrollHandle, TestSupportExt as _, Window};

use super::ctx::Ctx;
use super::menus::Menu;
use crate::kit::drag::{self as helper, Drag};
use crate::theme::scale::{pixels, rem, zoom};
use crate::theme::Tokens;

/// The five columns in the order the page draws them, which is the order a cut-off chip names them.
pub const DRAWN: [FeatureColumn; 5] = [FeatureColumn::Queued, FeatureColumn::Active, FeatureColumn::Done, FeatureColumn::Held, FeatureColumn::Archived];

/// The boxes a view measured as it laid them out, by what each is of.
pub type Boxes<K> = Rc<RefCell<Vec<(K, Bounds<Pixels>)>>>;

/// What a press on a card holds: which feature, until it is a drag or a click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Press {
    pub slug: String,
}

/// Where the card was picked up inside its box, and how wide the box was: the ghost keeps both.
#[derive(Debug, Clone, PartialEq)]
pub struct Grab {
    pub offset: Point<Pixels>,
    pub width: Pixels,
}

pub struct Lanes {
    ctx: Ctx,
    menu: Entity<Menu>,
    /// Where the peeked card is, for the Move menu the `M` key opens beside it.
    anchor: Rc<Cell<Bounds<Pixels>>>,
    /// The lanes' sideways scroll, and each column's own scroll down its cards.
    scroll: ScrollHandle,
    columns_scroll: [ScrollHandle; 5],
    /// The lanes' box and each column's, as last laid out, for the cut-off chip and the drop hit test.
    view: Rc<Cell<Bounds<Pixels>>>,
    boxes: Boxes<FeatureColumn>,
    /// Each card's box, measured as it is drawn: a drag begins from the box as it is then.
    cards: Boxes<String>,
    drag: Drag<Press>,
    grab: Option<Grab>,
    /// The column under the pointer while a card is dragged.
    over: Option<FeatureColumn>,
    /// Where the keys rest while a drag holds them, so Escape reaches it.
    keys: FocusHandle,
}

impl Lanes {
    pub fn new(ctx: Ctx, menu: Entity<Menu>, anchor: Rc<Cell<Bounds<Pixels>>>, cx: &mut Context<Self>) -> Self {
        Self {
            ctx,
            menu,
            anchor,
            scroll: ScrollHandle::new(),
            columns_scroll: Default::default(),
            view: Rc::default(),
            boxes: Rc::default(),
            cards: Rc::default(),
            drag: Drag::new(),
            grab: None,
            over: None,
            keys: cx.focus_handle(),
        }
    }

    /// Scrolls the lanes so `column` clears the fade, as the chip does.
    pub fn reveal(&mut self, column: FeatureColumn, scroll: &ScrollHandle, view: &Bounds<Pixels>) {
        let Some(next) = self.boxes.borrow().iter().find(|(held, _)| *held == column).map(|(_, bounds)| *bounds) else { return };
        let at = scroll.offset();
        scroll.set_offset(gpui_kit::point(at.x - cue::clear(next.right(), view.right(), view.size.width), at.y));
    }

    fn scroll_of(&self, column: FeatureColumn) -> &ScrollHandle {
        &self.columns_scroll[DRAWN.iter().position(|c| *c == column).unwrap_or(0)]
    }

    /// The column whose box holds `at`: the folded Archived row counts as the Archived column.
    pub fn column_at(&self, at: Point<Pixels>) -> Option<FeatureColumn> {
        self.boxes.borrow().iter().find(|(_, bounds)| bounds.contains(&at)).map(|(column, _)| *column)
    }
}

impl Render for Lanes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(features) = self.ctx.features(cx) else { return div().into_any_element() };
        let features: Rc<Features> = features;
        let lifted = self.drag.is_dragging().then(|| self.drag.payload().cloned()).flatten();
        let (this, rem_size) = (cx.entity(), window.rem_size());
        let padding = pixels(tokens.sp_16 * 2.0, rem_size);
        let now = cue::of(&self.view.get(), &self.boxes.borrow(), &self.scroll, zoom(rem_size), padding);
        let lanes = h_flex()
            .id("feature-lanes")
            .test_support()
            .flex_1()
            .min_w_0()
            .h_full()
            .items_stretch()
            .gap(rem(tokens.sp_12))
            .px(rem(tokens.sp_16))
            .py(rem(tokens.sp_12))
            .bg(tokens.bg_sunken)
            .overflow_x_scroll()
            .track_scroll(&self.scroll)
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
            .child(crate::kit::area::measure(self.view.clone()))
            .child(self.lane(FeatureColumn::Queued, &features, lifted.as_ref(), &tokens, cx))
            .child(self.lane(FeatureColumn::Active, &features, lifted.as_ref(), &tokens, cx))
            .child(self.lane(FeatureColumn::Done, &features, lifted.as_ref(), &tokens, cx))
            .child(self.side(&features, lifted.as_ref(), &tokens, cx));
        let ghost = self.ghost(&features, &tokens);
        div()
            .id("feature-board")
            .test_support()
            .aria_label("Board")
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(lanes)
            .child(cue::fade(&now, &tokens))
            .children(cue::chip(&now, self.scroll.clone(), self.view.get(), &tokens, cx))
            .children(ghost)
            .child(cue::stale(self.view.clone(), self.boxes.clone(), self.scroll.clone(), zoom(rem_size), padding, now, this))
            // The drag takes the keys with this, so Escape cancels it (BOARD-DRAG-13).
            .child(
                div()
                    .id("feature-board-keys")
                    .size_0()
                    .track_focus(&self.keys)
                    .on_key_down(cx.listener(|this, event: &gpui_kit::KeyDownEvent, window, cx| helper::escape(this, event, window, cx))),
            )
            .child(helper::watch(cx.weak_entity()))
            .into_any_element()
    }
}

impl Lanes {
    /// The copy of the card that follows the pointer while one is dragged, where it was picked up.
    fn ghost(&self, features: &Features, tokens: &Tokens) -> Option<gpui_kit::AnyElement> {
        let (press, grab, at) = (self.drag.payload()?, self.grab.as_ref()?, self.drag.at()?);
        if !self.drag.is_dragging() {
            return None;
        }
        let row = features.rows.iter().find(|row| row.slug == press.slug)?;
        Some(deferred(anchored().position(at - grab.offset).child(card::card(row, card::Dress::Lifted, tokens).w(grab.width))).into_any_element())
    }
}
