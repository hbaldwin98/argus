//! The Feature layer of the key walk: up while the Feature stage's index is the stage, hearing its
//! keys (the `Feature` and `Card` places) and the window's own (`Anywhere`), and holding every other
//! key so none is typed into the pane under the stage (BOARD-KEYS-1..13, `featureAction`).
//!
//! The cursor and what the index lists are Rust's: `j`, `k`, Home, End, `V` and `z` are sent as the
//! action they are, and the index changes when the next snapshot says so. `/` takes the filter
//! field, `N` opens the note a new feature is written as, and the card keys act on the feature
//! under the cursor, answering "not mine" when nothing is peeked or the peeked feature is not
//! among the rows drawn, so the key falls through. While a menu is open it holds the keys: it steps
//! through its items and Enter chooses, and while the hold dialog is up every key is its field's.
//! Escape on the bare index does nothing (BOARD-KEYS-13, an open question in `gpui/audit.md` §5).

use argus_desktop_core::board::capture_note::CaptureGesture;
use argus_desktop_core::connection::page::{Call, Stage};
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{point, px, App, Context, WeakEntity, Window};

use super::FeatureStage;
use crate::board::menus::run::run_move;
use crate::keys::layer::{action_for, Handler, Press, Verdict};
use crate::keys::run;
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;
use crate::theme::scale::pixels;
use crate::theme::Tokens;

pub struct StageKeys {
    pub stage: WeakEntity<FeatureStage>,
}

impl Handler for StageKeys {
    fn open(&self, _: &Window, cx: &App) -> bool {
        self.stage.upgrade().is_some_and(|stage| {
            let heard = stage.read(cx).ctx.heard.read(cx);
            heard.rail.stage == Stage::Feature && heard.features.as_ref().is_none_or(|features| features.open.is_none())
        })
    }

    fn keys(&self, press: &Press, _: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        let Some(stage) = self.stage.upgrade() else { return Verdict::Pass };
        // The hold dialog's field has the keys; whatever else is heard is its own.
        if Overlays::get(cx).is_open(Overlay::Hold) {
            return Verdict::Held;
        }
        if stage.read(cx).menu.read(cx).is_open() {
            return menu_key(&stage, press, window, cx);
        }
        match action_for(&[Place::Feature, Place::Card], press.page) {
            Some(action) => match stage.update(cx, |stage, cx| stage.run(action, window, cx)) {
                true => Verdict::Taken,
                false => Verdict::Pass,
            },
            None => match action_for(&[Place::Anywhere], press.page) {
                Some(action) if run::run(action, window, cx) => Verdict::Taken,
                _ => Verdict::Held,
            },
        }
    }

    /// Escape puts away the Move menu.
    fn escape(&self, _: &mut Window, cx: &mut App) -> bool {
        let Some(stage) = self.stage.upgrade() else { return false };
        let menu = stage.read(cx).menu.clone();
        let open = menu.read(cx).is_open();
        if open {
            menu.update(cx, |menu, cx| menu.close(cx));
        }
        open
    }
}

/// A key while a menu is open: its own, and nothing else of the stage.
fn menu_key(stage: &gpui_kit::Entity<FeatureStage>, press: &Press, window: &mut Window, cx: &mut App) -> Verdict {
    let menu = stage.read(cx).menu.clone();
    match action_for(&[Place::Menu], press.page) {
        Some(Action::Next) => menu.update(cx, |menu, cx| menu.step(1, cx)),
        Some(Action::Previous) => menu.update(cx, |menu, cx| menu.step(-1, cx)),
        _ if press.page.key == "Enter" => menu.update(cx, |menu, cx| menu.choose(window, cx)),
        _ => {
            // Escape and the window's own keys put it away through the ladder; the rest are held.
            return match action_for(&[Place::Anywhere], press.page) {
                Some(action) if run::run(action, window, cx) => Verdict::Taken,
                _ => Verdict::Held,
            };
        }
    }
    Verdict::Taken
}

impl FeatureStage {
    /// A key of the stage's own places, saying whether the stage had anything for it to act on.
    fn run(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match action {
            Action::Next | Action::Previous | Action::First | Action::Last | Action::Layout | Action::Folded => {
                self.ctx.send(Call::FeatureAction { action }, cx);
                return true;
            }
            Action::Filter => {
                self.focus_filter(window, cx);
                return true;
            }
            Action::NewFeature => {
                self.ctx.send(Call::Capture { gesture: CaptureGesture::Open }, cx);
                return true;
            }
            _ => {}
        }
        // The card keys act on the feature under the cursor, and only one the index draws.
        let Some(features) = self.ctx.features(cx) else { return false };
        let Some(peek) = &features.peek else { return false };
        let Some(row) = features.rows.iter().find(|row| row.slug == peek.slug).cloned() else { return false };
        match action {
            Action::Choose => self.ctx.send(Call::OpenFeature { slug: Some(row.slug.clone()) }, cx),
            Action::Move => {
                // Beside the row: its left edge and 40 px in, and its bottom; (200, 200) when it was
                // not measured.
                let at = self.anchor.get();
                let (tokens, rem_size) = (*Tokens::global(cx), window.rem_size());
                let fallback = pixels(tokens.fx_menu_fallback, rem_size);
                let at = if at.size.width > px(0.0) { point(at.left() + pixels(tokens.fx_menu_across, rem_size), at.bottom()) } else { point(fallback, fallback) };
                self.menu.update(cx, |menu, cx| menu.open_move(&row.slug, at, true, window, cx));
            }
            Action::Hold if row.moves.iter().any(|m| m.to == "unhold") => self.ctx.send(Call::FeatureAction { action: Action::Hold }, cx),
            Action::Hold => run_move(&self.ctx, &row, "held", window, cx),
            Action::Archive => self.ctx.send(Call::FeatureAction { action: Action::Archive }, cx),
            Action::GoToPane => {
                if let Some(pane) = peek.pane {
                    self.ctx.send(Call::GoToPane { pane }, cx);
                }
            }
            _ => return false,
        }
        true
    }
}
