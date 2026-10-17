//! Turning the terminal events this client receives into the bytes a pty
//! child expects to read.
//!
//! Separate from `app::input` and `app::mouse`, which decide *whether* an
//! event goes to a pane at all. By the time anything reaches here that
//! decision is made, and the only question left is what a child reading
//! its pty should see.

mod keys;
mod mouse;

pub use keys::{encode_key, is_leader};
pub use mouse::encode_mouse;
