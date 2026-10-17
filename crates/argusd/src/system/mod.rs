//! What the daemon asks of the machine it runs on.

pub mod command;
#[cfg(windows)]
pub mod ctrl_c;
pub mod daemon_lock;
#[cfg(windows)]
pub mod dll_search;
pub mod editor;
pub mod logging;
pub mod pastes;
pub mod paths;
