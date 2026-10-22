//! A browser's picture drawn in one place: the page's `canvasFor(pane, place)`.
//!
//! It fills the box it is given with the pane's picture, kept in its page's shape (`object-fit:
//! contain`, so a click lands where it was aimed), over the box's own background. The image is
//! `Heard`'s, shared with every other place that draws this pane; this repaints only for a
//! [`Pictured`] of its pane, not for the stage's redraws or another browser's pictures. It also
//! hands the atlas the images `Pictures` has replaced, since only a view has a window to do it
//! with.

use gpui_kit::{div, img, prelude::*, Context, Entity, ObjectFit, Subscription, Window};

use crate::connection::heard::{Heard, Pictured};
use crate::theme::scale::rem;

pub struct PictureView {
    heard: Entity<Heard>,
    pane: u64,
    /// The rounding of the bottom corners, in the page's pixels: GPUI clips to a rectangle, so a picture in a
    /// rounded card rounds itself.
    corners: f32,
    _pictured: Subscription,
}

impl PictureView {
    pub fn new(heard: Entity<Heard>, pane: u64, corners: f32, cx: &mut Context<Self>) -> Self {
        let pictured = cx.subscribe(&heard, move |_, _, Pictured(of): &Pictured, cx| {
            if *of == pane {
                cx.notify();
            }
        });
        Self { heard, pane, corners, _pictured: pictured }
    }
}

impl Render for PictureView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let picture = self.heard.update(cx, |heard, _| heard.pictures.draw(self.pane)).map(|picture| picture.image);
        for old in self.heard.update(cx, |heard, _| heard.pictures.take_retired()) {
            // Only the atlas's copy goes; a failure is a window that has no atlas left to tell.
            let _ = window.drop_image(old);
        }
        div().size_full().children(picture.map(|image| img(image).size_full().rounded_b(rem(self.corners)).object_fit(ObjectFit::Contain)))
    }
}
