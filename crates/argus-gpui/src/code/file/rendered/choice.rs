//! The header's choice between a markdown file read as a document and as its text.
//!
//! Two pills in one track, like the Git stage's Unified and Split: both words are always there and
//! the one on is raised, so the choice is read without being pressed. Pressing one only says which;
//! the file is redrawn by the stage that holds the mode.

use std::rc::Rc;

use gpui_kit::{prelude::*, AnyElement, App, Role, Window};

use super::mode::Mode;
use crate::kit::segment;
use crate::theme::Tokens;

/// What a press on a pill does: the stage keeps the mode.
pub type Choose = Rc<dyn Fn(Mode, &mut Window, &mut App)>;

/// The track with its two pills, `mode` being the one on.
pub fn choice(mode: Mode, choose: &Choose, tokens: &Tokens) -> AnyElement {
    let pill = |id: &'static str, label: &'static str, this: Mode| {
        let choose = choose.clone();
        segment::switch_option(id, Role::RadioButton, mode == this, tokens).child(label).on_click(move |_, window, cx| choose(this, window, cx))
    };
    segment::track("code-read-as", "Read as", Role::RadioGroup, tokens)
        .child(pill("code-view-rendered", "Rendered", Mode::Rendered))
        .child(pill("code-view-source", "Source", Mode::Source))
        .into_any_element()
}
