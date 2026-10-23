//! A pane carried by its header or from the rail: what is carried, and the handle any view
//! presses it through. A panel that may have a cell (`Panel::cells`) is carried to one too: by its
//! cell's header, or as a dock panel the work area follows, whose landing on a cell is asked here.
//!
//! The drag itself is the work area's (`layout/work`, one `Dragger` for every payload, so the
//! 5 px threshold and Escape are not written twice); the cell headers are the split grid's and the
//! rail's rows are the rail's, and neither is a child of it. Each calls [`Carrier::press`] with
//! what its press picked up. Where the pane lands is [`landing`], what shows it is [`preview`].

pub mod landing;
pub mod preview;

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::layout::docks::Panel;
use argus_desktop_core::panes::shown::workspace_grid::Occupant;
use gpui_kit::{App, Global, Pixels, Point, WeakEntity};

use super::work::{Held, WorkView};

/// What a press picked up.
#[derive(Debug, Clone, PartialEq)]
pub enum Carried {
    /// A cell's header: what cell `slot` holds, which is never its own target.
    Cell { slot: u8, held: Occupant, title: String },
    /// A pane's row in the rail. `checkout` is where it is now, which makes the other checkouts'
    /// rows targets; none for a pane that cannot move between checkouts.
    Rail { pane: u64, title: String, checkout: Option<u64> },
    /// A panel carried as a dock panel is, asked where it would land on the grid.
    Panel { panel: Panel, title: String },
}

impl Carried {
    pub fn title(&self) -> &str {
        match self {
            Carried::Cell { title, .. } | Carried::Rail { title, .. } | Carried::Panel { title, .. } => title,
        }
    }

    /// What the carried thing is in a cell, which a cell holding it already does not take again.
    fn occupant(&self) -> Occupant {
        match self {
            Carried::Cell { held, .. } => held.clone(),
            Carried::Rail { pane, .. } => Occupant::Pane(*pane),
            Carried::Panel { panel, .. } => Occupant::Panel(panel.clone()),
        }
    }
}

/// The work area that follows a carried pane, which it says on being made. A handle: clones
/// share one slot, so a view built before the work area still reaches it.
#[derive(Clone, Default)]
pub struct Carrier(Rc<RefCell<Option<WeakEntity<WorkView>>>>);

impl Global for Carrier {}

impl Carrier {
    pub fn of(cx: &mut App) -> Self {
        cx.default_global::<Carrier>().clone()
    }

    /// The work area that follows the drags from now on.
    pub fn serve(&self, work: WeakEntity<WorkView>) {
        *self.0.borrow_mut() = Some(work);
    }

    /// Follows a press of `carried` at `at`: a drag after the threshold, nothing before it.
    pub fn press(&self, at: Point<Pixels>, carried: Carried, cx: &mut App) {
        let Some(work) = self.0.borrow().clone() else { return };
        work.update(cx, |work, cx| work.press(at, Held::Pane(carried), cx)).ok();
    }

    /// Follows a press of the panel `panel` named `title` at `at`: carried to a dock it docks, over the
    /// stage it lands as its kind does when `stage` says it can. A press that never travels is the
    /// row's own click.
    pub fn press_panel(&self, at: Point<Pixels>, panel: Panel, title: String, stage: bool, cx: &mut App) {
        let Some(work) = self.0.borrow().clone() else { return };
        work.update(cx, |work, cx| work.press(at, Held::Panel { panel, title, stage, click: false, review: None }, cx)).ok();
    }

    /// Follows a press of Git in the rail with no review open: carried as the changes' panel, whose
    /// drop opens `checkout`'s review first. A press that never travels is Git's own click.
    pub fn press_git(&self, at: Point<Pixels>, checkout: u64, cx: &mut App) {
        let Some(work) = self.0.borrow().clone() else { return };
        let held = Held::Panel { panel: Panel::Changes, title: "Changes".into(), stage: true, click: false, review: Some(checkout) };
        work.update(cx, |work, cx| work.press(at, held, cx)).ok();
    }
}
