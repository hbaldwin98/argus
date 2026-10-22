//! What an action of the Anywhere place does, and the Escape ladder: the page's `run`
//! (`keys/keymap.ts`).
//!
//! The session runs the window's own commands (`Call::Command`), so a stage switch, the console's
//! keys and the composer are one call; what the window itself does is open the palette, the sheet
//! and the settings over the stage (`over`), and put away the first thing Escape finds. The
//! actions of a stage's own keys (a table's rows, a feature's lenses) arrive with their stages,
//! and are refused here until then, so a key without a home does nothing.

use argus_desktop_core::keys::keymap::{Action, Layer, LAYERS};
use gpui_kit::{App, Window};

use super::focus;
use super::layer::Layers;
use super::over::{overlay_of, run_over};

/// Runs `action`, saying whether there was anything for it to do.
pub fn run(action: Action, window: &mut Window, cx: &mut App) -> bool {
    if action == Action::CloseLayer {
        return escape(window, cx);
    }
    let layers = Layers::of(cx);
    // A menu or a question over the stage holds the keys until it closes.
    if layers.is_up(Layer::Menu, window, cx) {
        return false;
    }
    if overlay_of(action).is_some() {
        return run_over(action, window, cx);
    }
    layers.command(action)
}

/// Escape: puts away the first thing in the layers' order, whether its layer is up or not.
pub fn escape(window: &mut Window, cx: &mut App) -> bool {
    let layers = Layers::of(cx);
    focus::escape_from(LAYERS, |layer| layers.handler(layer).is_some_and(|handler| handler.escape(window, cx)))
}

#[cfg(test)]
mod tests;
