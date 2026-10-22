//! What a key is in a stage's own place: the one routine every stage that hears a place of the
//! keymap and holds the rest looks a key up with (the Git tabs, the Checkouts table, and the Files
//! and Code layers as they arrive).
//!
//! The layer asks its own place first, and then the places `LAYERS` lists beyond it, `Anywhere`
//! among them; a key found in neither is held, so it is not typed into the pane under the stage.
//! What each finding does is the stage's, so a stage that hears all of `Anywhere` runs it and one
//! that hears only its Escape (the review) accepts the one action.

use argus_desktop_core::keys::keymap::{Action, Place};
use argus_desktop_core::keys::page_key::PageKey;

use super::layer::action_for;

/// Where a key was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Looked {
    /// In the layer's own place: the stage's gesture.
    Own(Action),
    /// In one of the places beyond it: the window's own key, which the stage may run.
    Beyond(Action),
    /// In none: held, and typed nowhere.
    Held,
}

/// What `page` is in `own`, else in `beyond`. A place listed first takes a key from one after it.
pub fn look_up(own: Place, beyond: &[Place], page: &PageKey) -> Looked {
    if let Some(action) = action_for(&[own], page) {
        return Looked::Own(action);
    }
    match action_for(beyond, page) {
        Some(action) => Looked::Beyond(action),
        None => Looked::Held,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> PageKey {
        PageKey { key: name.into(), ..PageKey::default() }
    }

    #[test]
    fn a_key_of_the_own_place_is_the_stages_and_one_of_anywhere_is_beyond_it() {
        assert_eq!(look_up(Place::Checkouts, &[Place::Anywhere], &key("j")), Looked::Own(Action::Next));
        assert_eq!(look_up(Place::Checkouts, &[Place::Anywhere], &key("Escape")), Looked::Beyond(Action::CloseLayer));
        assert_eq!(look_up(Place::Checkouts, &[Place::Anywhere], &key("3")), Looked::Beyond(Action::Checkouts));
    }

    #[test]
    fn a_key_in_neither_is_held() {
        assert_eq!(look_up(Place::Checkouts, &[Place::Anywhere], &key("q")), Looked::Held);
        assert_eq!(look_up(Place::Checkouts, &[], &key("3")), Looked::Held, "nothing beyond the place when none is asked");
    }

    #[test]
    fn the_own_place_wins_a_key_both_bind() {
        // `h` is History's in the table and Fold's in the Files place: each stage reads its own.
        assert_eq!(look_up(Place::Checkouts, &[Place::Anywhere], &key("h")), Looked::Own(Action::OpenHistory));
        assert_eq!(look_up(Place::Files, &[Place::Anywhere], &key("h")), Looked::Own(Action::ShutFolder));
    }
}
