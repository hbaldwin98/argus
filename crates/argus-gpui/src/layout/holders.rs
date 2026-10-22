//! Who is holding a resize: the drags that have a pane wait for the size they
//! settle on before it reports one.
//!
//! The page marks its body `resizing` while any sash or gutter is held
//! (SHELL-EDGE-1, CROSS-20), one flag for every drag. Two holders written to
//! one flag clear each other: the second drag to end would lift the hold of
//! the first still in progress, and the pane would report its size mid-drag
//! and resize the child program once a frame. So each drag holds under its own
//! name and the resize is held for as long as any of them is.

use argus_desktop_core::layout::docks::Dock;

/// A drag that sizes something the panes sit in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    /// A dock's sash.
    Dock(Dock),
    /// The rail's edge.
    Rail,
    /// The Feature stage's outline edge, and its brief's.
    Outline,
    Brief,
    /// The Code stage's tree edge and outline edge.
    CodeTree,
    CodeOutline,
    /// A gutter between the cells of the Workspace's split grid.
    Grid,
}

/// The drags holding a resize now; each is in at most once.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Holders(Vec<Holder>);

impl Holders {
    /// Whether any drag holds a resize.
    pub fn any(&self) -> bool {
        !self.0.is_empty()
    }

    /// Whether `holder` is one of them.
    pub fn holds(&self, holder: Holder) -> bool {
        self.0.contains(&holder)
    }

    /// Adds `holder`; whether the set changed.
    pub fn hold(&mut self, holder: Holder) -> bool {
        let new = !self.holds(holder);
        if new {
            self.0.push(holder);
        }
        new
    }

    /// Takes `holder` out, leaving any other; whether the set changed.
    pub fn release(&mut self, holder: Holder) -> bool {
        let before = self.0.len();
        self.0.retain(|held| *held != holder);
        self.0.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_held_until_a_drag_holds_it() {
        let mut holders = Holders::default();
        assert!(!holders.any());
        assert!(holders.hold(Holder::Rail));
        assert!(holders.any() && holders.holds(Holder::Rail));
    }

    #[test]
    fn two_drags_do_not_clear_each_other() {
        let mut holders = Holders::default();
        holders.hold(Holder::Dock(Dock::Right));
        holders.hold(Holder::Grid);
        assert!(holders.release(Holder::Dock(Dock::Right)));
        assert!(holders.any(), "the grid's gutter is still held");
        assert!(holders.release(Holder::Grid));
        assert!(!holders.any());
    }

    #[test]
    fn a_holder_is_in_once_and_releasing_one_that_is_not_there_changes_nothing() {
        let mut holders = Holders::default();
        assert!(holders.hold(Holder::Brief));
        assert!(!holders.hold(Holder::Brief), "held twice is still one");
        assert!(holders.release(Holder::Brief));
        assert!(!holders.any(), "one release lifts it");
        assert!(!holders.release(Holder::Brief));
    }

    #[test]
    fn each_dock_holds_under_its_own_name() {
        let mut holders = Holders::default();
        holders.hold(Holder::Dock(Dock::Left));
        holders.hold(Holder::Dock(Dock::Bottom));
        holders.release(Holder::Dock(Dock::Left));
        assert!(holders.holds(Holder::Dock(Dock::Bottom)));
    }
}
