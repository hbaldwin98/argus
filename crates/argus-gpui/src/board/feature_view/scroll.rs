//! Where each list of an open feature is scrolled, and when the cursor's row is brought into view
//! (BOARD-WORK-21): only when the cursor moved.
//!
//! Every pushed tree redraws the feature, and pulling the row back each time would undo the person's
//! own scrolling. The page kept each box's offset by its class names and restored it on the successor;
//! the handles here are held by the view, so the offsets survive a redraw by being kept. What is left to
//! decide is when to scroll, by the cursor's identity: the feature, the lens and the row.

use argus_desktop_core::connection::page::{FeatureView, Lens};
use gpui_kit::ScrollHandle;

use crate::board::lenses::{flows, record, work};

/// The lists' scroll handles: the outline, the Record lens's list and the Flows lens's.
#[derive(Default, Clone)]
pub struct Scrolls {
    pub outline: ScrollHandle,
    pub record: ScrollHandle,
    pub flows: ScrollHandle,
    /// The box the inline diagram scrolls sideways in, and the expanded diagram's canvas.
    pub holder: ScrollHandle,
    pub canvas: ScrollHandle,
}

/// Which list a row is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Outline,
    Record,
    Flows,
}

/// The row the cursor is on, as the list it is in, which child of it, and the identity that says
/// whether the cursor moved: `<feature>/<lens>/<row>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lit {
    pub identity: String,
    pub which: Which,
    pub child: usize,
}

pub fn lit(view: &FeatureView) -> Option<Lit> {
    let number = view.number;
    match view.lens {
        Lens::Work => {
            let at = view.rows.iter().position(|row| row.on)?;
            Some(Lit { identity: format!("{number}/work/{}", view.rows[at].label), which: Which::Outline, child: work::outline::lit_child(view)? })
        }
        Lens::Record => {
            let rows = &view.record.as_ref()?.rows;
            let at = rows.iter().position(|row| row.on)?;
            Some(Lit { identity: format!("{number}/record/{}", rows[at].label), which: Which::Record, child: record::lit_child(view)? })
        }
        Lens::Flows => {
            let flow = view.flows.as_ref()?.flow.as_ref()?;
            let step = flow.step.as_ref()?;
            Some(Lit { identity: format!("{number}/flows/{}/{}", flow.id, step.number), which: Which::Flows, child: flows::lit_child(view)? })
        }
    }
}

impl Scrolls {
    /// Brings the row to the top of what shows if it is out of it; the handle of its list.
    pub fn bring(&self, lit: &Lit) {
        let handle = match lit.which {
            Which::Outline => &self.outline,
            Which::Record => &self.record,
            Which::Flows => &self.flows,
        };
        handle.scroll_to_item(lit.child);
    }
}

#[cfg(test)]
mod tests;
