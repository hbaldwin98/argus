//! Putting a larger view up, and reaching the one that is, which is how the keys move it.
//!
//! One is up at a time: opening another closes the one up (FIGURE-2). It is an overlay of the
//! window's host (`kit/overlay`), the whole window less 24 px all round, and a press on the scrim
//! outside it closes it as Escape does. The keys it holds are `keys/layers/figure`'s.

use std::rc::Rc;

use gpui_kit::component::dialog::Dialog;
use gpui_kit::{prelude::*, App, Global, WeakEntity, Window};

use super::larger::{Larger, Move};
use super::Figure;
use crate::kit::overlay::host::{Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::theme::scale::pixels;
use crate::theme::Tokens;

/// The larger view that was put up last. It is only the one up while the host says the overlay is.
struct Up(WeakEntity<Larger>);

impl Global for Up {}

/// Opens `figure` larger, in place of one already up.
pub fn open(figure: Rc<Figure>, window: &mut Window, cx: &mut App) {
    let host = Overlays::get(cx);
    host.close(Overlay::Figure, window, cx);
    let larger = cx.new(|cx| Larger::new(figure, cx));
    cx.set_global(Up(larger.downgrade()));
    host.open(
        Spec::new(Overlay::Figure),
        move |dialog: Dialog, window, cx, _| {
            let tokens = *Tokens::global(cx);
            let inset = pixels(tokens.sp_24, window.rem_size());
            let wide = window.viewport_size().width - inset * 2.0;
            dialog.margin_top(inset).w(wide).p_0().on_ok(|_, _, _| false).child(larger.clone())
        },
        window,
        cx,
    );
}

/// The larger view that is up, if one is.
pub fn up(cx: &App) -> Option<gpui_kit::Entity<Larger>> {
    if !Overlays::get(cx).is_open(Overlay::Figure) {
        return None;
    }
    cx.try_global::<Up>().and_then(|up| up.0.upgrade())
}

/// Moves the larger view that is up; whether one was.
pub fn zoom(by: Move, window: &mut Window, cx: &mut App) -> bool {
    let Some(larger) = up(cx) else { return false };
    larger.update(cx, |larger, cx| larger.apply(by, window, cx));
    true
}
