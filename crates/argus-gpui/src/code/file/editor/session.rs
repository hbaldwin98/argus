//! One file open in the editor, as the window holds it: the kit's editor state once the daemon has
//! sent the text, the rope it last held (what is typed is found against it), the burst of typing
//! and the timers that write, read again, retry and unlight. The sync, the status and what to ask
//! of the daemon are core's `Live`, kept beside it in `Lives`.

use argus_desktop_core::code::grammar::slot::Slot;
use argus_desktop_core::code::pace::Typing;
use argus_desktop_core::code::steps::Mark;
use argus_desktop_core::code::vim::Vim;
use gpui_kit::component::input::{EditorState, RangeDecorationCollection, Rope, TextDecorationCollection};
use gpui_kit::{Entity, Subscription, Task};

use crate::keys::field::Registered;

/// A file by checkout and path, as the daemon and `Lives` name it.
pub type Key = (u64, String);

/// The collections of marks the editor draws over its text.
pub(super) struct Marks {
    /// Lines this version added or changed: a bar the line's height in the gutter.
    pub added: RangeDecorationCollection,
    /// Lines with lines removed before them: a short bar at the top of the line.
    pub removed: RangeDecorationCollection,
    /// What an agent just wrote, lit for a moment.
    pub lit: RangeDecorationCollection,
    /// What the marks were drawn from, so a page that says the same is not drawn again.
    pub said: String,
}

/// What Vim draws over the text: the cursor's block and the selection.
pub(super) struct Look {
    pub block: RangeDecorationCollection,
    pub selected: RangeDecorationCollection,
}

pub(super) struct Session {
    /// `None` until the daemon answers the open: the view is made then, not before.
    pub state: Option<Entity<EditorState>>,
    /// The daemon let the file go: what is typed stays on screen and nothing more is written. The
    /// styled editor sets the state's own flag on every draw, so the flag is kept here too.
    pub read_only: bool,
    /// The text as the editor last said it was, which a change is found against.
    pub snapshot: Rope,
    /// The line Rust last asked the editor to go to: the editor's own moves are never reported, so
    /// Rust's line is gone to only when it changes.
    pub at: u32,
    /// A line to bring to the middle of the view once the editor has been laid out and knows how
    /// tall it is.
    pub centre: Option<usize>,
    pub typing: Typing,
    pub marks: Option<Marks>,
    /// The marks Rust last said, drawn once the view exists and again as they change.
    pub wanted: Vec<Mark>,
    /// A write waits on this pause.
    pub quiet: Option<Task<()>>,
    /// The file is read again for its change marks when this ends.
    pub reread: Option<Task<()>>,
    /// A refused write is tried again when this ends.
    pub retry: Option<Task<()>>,
    /// The agent's change stays lit until this ends.
    pub unlight: Option<Task<()>>,
    /// Vim's text, cursor and undo for this file, made on its first key while Vim is on.
    pub vim: Option<Vim>,
    pub look: Option<Look>,
    /// The person's clashing text, marked while the sync holds a clash.
    pub clashes: Option<TextDecorationCollection>,
    /// What the agent wrote at each clash, with the line it is on.
    pub theirs: Vec<(usize, String)>,
    /// What the bar says of this file's colours: an offer, an install, a failure.
    pub grammar: Slot,
    /// The language the editor was told to colour with.
    pub language: Option<String>,
    /// The editor's place in the key walk, and the hear of what it types.
    pub keys: Option<Registered>,
    pub typing_heard: Option<Subscription>,
}

impl Session {
    pub fn new(at: u32) -> Self {
        Self {
            state: None,
            read_only: false,
            snapshot: Rope::new(),
            at,
            centre: None,
            typing: Typing::default(),
            marks: None,
            wanted: Vec::new(),
            quiet: None,
            reread: None,
            retry: None,
            unlight: None,
            vim: None,
            look: None,
            clashes: None,
            theirs: Vec::new(),
            grammar: Slot::None,
            language: None,
            keys: None,
            typing_heard: None,
        }
    }
}
