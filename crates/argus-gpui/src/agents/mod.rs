//! The agents the window starts and the sessions they leave: the composer that starts one, the
//! recent-sessions dialog and the transcript reader that opens in place of its list.
//!
//! What each holds and says is `argus-desktop-core`'s (`agents`); these draw it and send the calls
//! the page sends (`page/src/agents`).

pub mod ask;
pub mod search;
pub mod compose;
pub mod install;
pub mod offer;
pub mod sessions;
pub mod transcript;
