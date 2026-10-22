//! What the Argus desktop window holds and says, with no window in it: the
//! session over the daemon's tree, each surface's state, the events and
//! calls the page and the daemon trade, and the choices remembered between
//! runs. The Tauri window and the GPUI one both stand on it, so every module
//! is `pub` for the GPUI window even where the Tauri binary names only
//! `code`, `connection`, `session`, `settings` and `system`.
//!
//! The model is `session`, the daemon is reached in `connection::daemon`,
//! and `connection::page` is the contract with whatever draws it. Nothing
//! here names a windowing or platform crate; what needs one stays in the
//! binary that draws.

pub mod agents;
pub mod board;
pub mod browser;
pub mod checkouts;
pub mod chrome;
pub mod code;
pub mod connection;
pub mod extensions;
#[cfg(any(test, feature = "scenes"))]
#[cfg_attr(not(test), allow(dead_code))]
mod fixtures;
pub mod keys;
pub mod layout;
pub mod panes;
pub mod review;
// Public under the `scenes` feature so a window other than the Tauri page can
// draw the same scenes, as `argus-gpui`'s screenshot test does.
#[cfg(any(test, feature = "scenes"))]
#[cfg_attr(not(test), allow(dead_code))]
pub mod screens;
pub mod session;
pub mod settings;
pub mod system;
pub mod views;
