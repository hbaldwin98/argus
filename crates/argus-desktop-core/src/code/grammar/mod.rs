//! The tree-sitter grammars the Code stage's editor colours with: the ones Argus knows, finding and downloading one, keeping it, and running it.

pub mod fetch;
pub mod install;
pub mod known;
pub mod load;
pub mod net;
pub mod say;
pub mod slot;
pub mod store;
