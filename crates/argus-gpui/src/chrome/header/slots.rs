//! What a stage's view hands the strip: its words and its tools (SHELL-STAGE-8).
//!
//! The page's `drawHead` asked each stage for its crumb, title and tools in one ladder. Here the
//! stage's own view says them once, when it is made, and the strip asks this registry for the
//! stage that is up, so adding a stage adds no arm to the strip. A stage that offers nothing has
//! the name `StripPage` gives it and no tools.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::chrome::strip::Says;
use argus_desktop_core::connection::page::Stage;
use gpui_kit::{AnyElement, App, Global, Window};

type SaysFn = Rc<dyn Fn(&App) -> Says>;
type Tools = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// What one stage offers; either half may be left out.
#[derive(Clone, Default)]
pub struct Offer {
    says: Option<SaysFn>,
    tools: Option<Tools>,
}

impl Offer {
    /// The words the stage's view puts in the strip, asked for each time the strip is drawn.
    pub fn says(mut self, says: impl Fn(&App) -> Says + 'static) -> Self {
        self.says = Some(Rc::new(says));
        self
    }

    /// The buttons the stage's view puts in the strip's end, built each time the strip is drawn.
    pub fn tools(mut self, tools: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.tools = Some(Rc::new(tools));
        self
    }
}

/// The registry. A handle: clones share one list.
#[derive(Clone, Default)]
pub struct Slots(Rc<RefCell<Vec<(Stage, Offer)>>>);

impl Global for Slots {}

impl Slots {
    pub fn of(cx: &mut App) -> Self {
        cx.default_global::<Slots>().clone()
    }

    /// Reads the registry from a context that may not make it.
    pub fn peek(cx: &App) -> Self {
        cx.try_global::<Slots>().cloned().unwrap_or_default()
    }

    /// Puts `offer` in `stage`'s slot, in place of the one there.
    pub fn offer(&self, stage: Stage, offer: Offer) {
        let mut offers = self.0.borrow_mut();
        offers.retain(|(held, _)| *held != stage);
        offers.push((stage, offer));
    }

    fn of_stage(&self, stage: Stage) -> Option<Offer> {
        self.0.borrow().iter().find(|(held, _)| *held == stage).map(|(_, offer)| offer.clone())
    }

    /// What the view of `stage` says; nothing if it offers none.
    pub fn says(&self, stage: Stage, cx: &App) -> Says {
        self.of_stage(stage).and_then(|offer| offer.says).map(|says| says(cx)).unwrap_or_default()
    }

    /// The tools of `stage`'s view, if it has any.
    pub fn tools(&self, stage: Stage, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let tools = self.of_stage(stage)?.tools?;
        Some(tools(window, cx))
    }
}
