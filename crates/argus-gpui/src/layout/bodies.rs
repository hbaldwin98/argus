//! Which view draws a panel's body, and which a stage that is a page: handed in once, by the
//! surface that owns the content, and asked wherever the content may be put.
//!
//! A panel's body is one element whether the panel is docked, floating or the stage (the page
//! moves one node), so a dock, a float frame and the stage each ask this for the same view and none
//! names a surface. A panel nobody offered a view for draws `panel::body`, the stand-in.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::connection::page::Stage;
use argus_desktop_core::layout::docks::Panel;
use gpui_kit::{AnyView, App, Global};

type Matches = Rc<dyn Fn(&Panel) -> bool>;

#[derive(Clone, Default)]
pub struct Contents {
    panels: Rc<RefCell<Vec<(Matches, AnyView)>>>,
    stages: Rc<RefCell<Vec<(Stage, AnyView)>>>,
}

impl Global for Contents {}

impl Contents {
    pub fn of(cx: &mut App) -> Self {
        cx.default_global::<Contents>().clone()
    }

    /// Reads the registry from a context that may not make it.
    pub fn peek(cx: &App) -> Self {
        cx.try_global::<Contents>().cloned().unwrap_or_default()
    }

    /// `view` draws every panel `matches` says yes to.
    pub fn offer_panel(&self, matches: impl Fn(&Panel) -> bool + 'static, view: AnyView) {
        self.panels.borrow_mut().push((Rc::new(matches), view));
    }

    /// `view` draws `stage` when it is a page of its own.
    pub fn offer_stage(&self, stage: Stage, view: AnyView) {
        let mut stages = self.stages.borrow_mut();
        stages.retain(|(held, _)| *held != stage);
        stages.push((stage, view));
    }

    /// The view that draws `panel`, if one was offered.
    pub fn panel(&self, panel: &Panel) -> Option<AnyView> {
        self.panels.borrow().iter().find(|(matches, _)| matches(panel)).map(|(_, view)| view.clone())
    }

    /// The view that draws `stage`, if one was offered.
    pub fn stage(&self, stage: Stage) -> Option<AnyView> {
        self.stages.borrow().iter().find(|(held, _)| *held == stage).map(|(_, view)| view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{div, prelude::*, Context, TestAppContext, Window};

    struct Body;

    impl Render for Body {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[gpui_kit::test]
    fn a_panel_and_a_stage_are_drawn_by_the_view_offered_for_them_and_none_else(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let (views, page) = (cx.new(|_| Body), cx.new(|_| Body));
            let contents = Contents::of(cx);
            contents.offer_panel(|panel| *panel == Panel::Views, views.clone().into());
            contents.offer_stage(Stage::View, page.clone().into());
            assert_eq!(contents.panel(&Panel::Views).map(|body| body.entity_id()), Some(views.entity_id()));
            assert!(contents.panel(&Panel::Console).is_none(), "the console is not offered here");
            assert_eq!(contents.stage(Stage::View).map(|body| body.entity_id()), Some(page.entity_id()));
            assert!(contents.stage(Stage::Code).is_none());
            // A second offer for a stage replaces the first.
            let other = cx.new(|_| Body);
            contents.offer_stage(Stage::View, other.clone().into());
            assert_eq!(contents.stage(Stage::View).map(|body| body.entity_id()), Some(other.entity_id()));
        });
    }
}
