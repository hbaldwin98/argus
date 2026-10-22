//! Dragging a card from one column to another, by the kit's one drag helper.
//!
//! The press is armed on the card and becomes a drag after the decided 5 px; Escape cancels it with
//! nothing sent; the release that ends a drag presses nothing, so a drag never peeks or opens
//! (BOARD-DRAG-8, -13). What a drop asks is `index_words::drop_verdict`'s: a hold asks why in a
//! dialog, an archive or a refusal is `Call::DropFeature`, which Rust answers. The card itself moves
//! only when a pushed board says it did.

use argus_desktop_core::board::index_words::{drop_verdict, FeatureColumn, Landing};
use argus_desktop_core::connection::page::Call;
use gpui_kit::{Context, FocusHandle, Window};

use super::{Lanes, Press};
use crate::board::menus::run::run_move;
use crate::kit::drag::{Drag, Dragger, Step};

impl Dragger for Lanes {
    type Payload = Press;

    fn drag(&self) -> &Drag<Press> {
        &self.drag
    }

    fn drag_mut(&mut self) -> &mut Drag<Press> {
        &mut self.drag
    }

    fn keys(&self) -> &FocusHandle {
        &self.keys
    }

    fn dragged(&mut self, step: Step<Press>, window: &mut Window, cx: &mut Context<Self>) {
        match step {
            Step::Began(_) | Step::Moved(_) => self.over = self.drag.at().and_then(|at| self.column_at(at)),
            Step::Dropped(press, at) => {
                let on = self.column_at(at);
                (self.over, self.grab) = (None, None);
                self.landed(&press.slug, on, window, cx);
            }
            Step::Cancelled(_) | Step::Clicked(_) => (self.over, self.grab) = (None, None),
        }
    }
}

impl Lanes {
    /// A card dropped on `on`, if it was dropped on a column at all (outside every column is a
    /// cancel, with no notice).
    fn landed(&mut self, slug: &str, on: Option<FeatureColumn>, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(on), Some(row)) = (on, self.ctx.row(slug, cx)) else { return };
        match drop_verdict(&row, on) {
            Landing::Nothing => {}
            Landing::Hold => run_move(&self.ctx, &row, "held", window, cx),
            Landing::Archive | Landing::Refuse(_) => self.ctx.send(Call::DropFeature { slug: slug.to_owned(), on }, cx),
        }
    }
}
