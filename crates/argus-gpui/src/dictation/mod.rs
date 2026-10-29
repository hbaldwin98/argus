//! Dictation in the window: the panel over the pane, the strip's chip, the keys, and the engine behind them.

pub mod chip;
#[cfg(feature = "dictation")]
pub mod engine;
pub mod keys;
pub mod level;
pub mod panel;
