//! What a press on the title bar does to the frameless window outside Windows: it starts the move.
//!
//! Windows answers a hit test over a `Drag` control area with HTCAPTION, which gives the move,
//! double-click to maximize and snap layouts with no press handled here. The test platform has
//! no window to move (its `start_window_move` is unimplemented and panics), so a test press on the
//! title bar on Linux or macOS moves nothing.

use gpui_kit::Window;

pub fn start_move(window: &Window) {
    if !cfg!(test) {
        window.start_window_move();
    }
}
