//! Which view fills the stage while the Workspace would: the Git stage while
//! it is up, else the Workspace's live pane.
//!
//! The session has no Git stage (`review::entry`), so the choice is the
//! window's, made from what the session says is open. The pane is kept, not
//! dropped, behind it: its screen and its subscription are the session's.

use gpui_kit::{div, prelude::*, AnyView, Context, Entity, Subscription, Window};

use crate::connection::heard::Heard;

pub struct StageView {
    heard: Entity<Heard>,
    pane: AnyView,
    git: AnyView,
    _observing: Subscription,
}

impl StageView {
    pub fn new(heard: Entity<Heard>, pane: AnyView, git: AnyView, cx: &mut Context<Self>) -> Self {
        let observing = cx.observe(&heard, |_, _, cx| cx.notify());
        Self { heard, pane, git, _observing: observing }
    }
}

impl Render for StageView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = if self.heard.read(cx).git_up() { self.git.clone() } else { self.pane.clone() };
        div().size_full().child(shown)
    }
}
