//! The timer a view's age is said again on: ages are told in minutes, so a half minute is never wrong
//! by one, and the surfaces that say one (an agent's view, a saved view's `built <ago>`) share it.

use std::time::Duration;

use gpui_kit::{Context, Task};

pub const EVERY: Duration = Duration::from_secs(30);

/// A task that asks `V` for a new draw each [`EVERY`] until `V` is gone; the owner keeps it.
pub fn every<V: 'static>(cx: &mut Context<V>) -> Task<()> {
    cx.spawn(async move |this, cx| loop {
        cx.background_executor().timer(EVERY).await;
        if this.update(cx, |_, cx| cx.notify()).is_err() {
            break;
        }
    })
}
