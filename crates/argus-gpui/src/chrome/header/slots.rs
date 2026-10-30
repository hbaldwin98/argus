//! What a stage's view hands the strip: its words, its tabs and its tools (SHELL-STAGE-8).
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

use super::tabs::Tabs;

type SaysFn = Rc<dyn Fn(&App) -> Says>;
type Tools = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type TabsFn = Rc<dyn Fn(&mut Window, &mut App) -> Option<Tabs>>;

/// Whose slot an offer is in. Git is no `Stage` of core's (it draws where the Workspace's pane
/// is), but it has a bar of its own to fill.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Stage(Stage),
    Git,
}

impl From<Stage> for Slot {
    fn from(stage: Stage) -> Self {
        Slot::Stage(stage)
    }
}

/// What one stage offers; either half may be left out.
#[derive(Clone, Default)]
pub struct Offer {
    says: Option<SaysFn>,
    tools: Option<Tools>,
    tabs: Option<TabsFn>,
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

    /// The tabs the stage's view puts right after the breadcrumb, built each time the strip is
    /// drawn; none while it has none to show.
    pub fn tabs(mut self, tabs: impl Fn(&mut Window, &mut App) -> Option<Tabs> + 'static) -> Self {
        self.tabs = Some(Rc::new(tabs));
        self
    }
}

/// The registry. A handle: clones share one list.
#[derive(Clone, Default)]
pub struct Slots(Rc<RefCell<Vec<(Slot, Offer)>>>);

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
    pub fn offer(&self, slot: impl Into<Slot>, offer: Offer) {
        let slot = slot.into();
        let mut offers = self.0.borrow_mut();
        offers.retain(|(held, _)| *held != slot);
        offers.push((slot, offer));
    }

    fn of_slot(&self, slot: Slot) -> Option<Offer> {
        self.0.borrow().iter().find(|(held, _)| *held == slot).map(|(_, offer)| offer.clone())
    }

    /// What the view of `slot` says; nothing if it offers none.
    pub fn says(&self, slot: impl Into<Slot>, cx: &App) -> Says {
        self.of_slot(slot.into()).and_then(|offer| offer.says).map(|says| says(cx)).unwrap_or_default()
    }

    /// The tools of `slot`'s view, if it has any.
    pub fn tools(&self, slot: impl Into<Slot>, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let tools = self.of_slot(slot.into())?.tools?;
        Some(tools(window, cx))
    }

    /// The tabs of `slot`'s view, if it has any showing.
    pub fn tabs(&self, slot: impl Into<Slot>, window: &mut Window, cx: &mut App) -> Option<Tabs> {
        let tabs = self.of_slot(slot.into())?.tabs?;
        tabs(window, cx)
    }
}
