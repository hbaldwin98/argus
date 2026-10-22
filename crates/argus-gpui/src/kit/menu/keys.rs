//! The keys a menu answers beyond the kit's: `j` and `k` step the highlight as Down and Up do.
//!
//! The page's `menu` place binds them (`next` and `previous`), so a menu is moved through the way
//! a list is. The kit binds only the arrows, in the context its menu declares.

use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::{App, KeyBinding};

/// The kit's name for the context its menu holds the keys in.
const MENU: &str = "PopupMenu";

/// Binds `j` and `k` in a menu. Called once when the window starts.
pub fn install(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("j", SelectDown, Some(MENU)), KeyBinding::new("k", SelectUp, Some(MENU))]);
}
