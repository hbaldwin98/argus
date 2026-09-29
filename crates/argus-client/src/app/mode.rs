//! Which mode has the keys.
//!
//! Key dispatch, the status bar and the `?` window each used to work this
//! out for themselves, in three different orders, and they disagreed: the
//! bar offered the rail's keys while the directory browser had them, and
//! `?` over a sequence diagram listed the rail's. Now there is one answer,
//! and each of the three matches on it exhaustively, so a mode nobody
//! handles does not compile. What the keys *are* in each mode is its child,
//! [`keymap`].

use super::*;

mod keymap;

/// What the keys go to, in the order they are claimed. The keymap window is
/// not one of these: it opens over whichever of them raised the question,
/// and answers for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A confirmation, or a line of text to be typed.
    Prompt,
    /// The directory browser, up while a project or repository is added.
    DirPicker,
    /// A picker; which one is `App::picker`'s kind.
    Picker,
    /// A branch filter being typed in the Checkouts stage.
    CheckoutFilter,
    /// A floating window.
    Overlay(OverlayMode),
    /// A stage that is not the workspace, which owns its keys outright.
    Stage(View),
    /// Typing into the workspace terminal.
    Pane,
    /// The rail; how deep its cursor is, is `focus`.
    Rail,
}

/// Which floating window has the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    /// A diff, whether reached from the rail or from history.
    Review,
    History,
    Brief,
    Settings,
    SequenceDiagram,
    /// A pane floating over the stage: an editor, or a window on a pane.
    Pane,
}

impl App {
    /// The mode the next key goes to, the keymap window aside.
    pub fn mode(&self) -> Mode {
        if self.prompt.is_some() {
            return Mode::Prompt;
        }
        if self.dir_picker.is_some() {
            return Mode::DirPicker;
        }
        if self.picker.is_some() {
            return Mode::Picker;
        }
        if self.checkout_filtering {
            return Mode::CheckoutFilter;
        }
        if let Some(overlay) = &self.overlay {
            return Mode::Overlay(match overlay {
                Overlay::Review => OverlayMode::Review,
                Overlay::History => OverlayMode::History,
                Overlay::Brief => OverlayMode::Brief,
                Overlay::Settings { .. } => OverlayMode::Settings,
                Overlay::SequenceDiagram => OverlayMode::SequenceDiagram,
                Overlay::Pane { .. } => OverlayMode::Pane,
            });
        }
        match self.focus {
            Focus::View => Mode::Stage(self.view),
            // A review opened with nothing floating is still a review.
            Focus::Review => Mode::Overlay(OverlayMode::Review),
            Focus::PaneContent => Mode::Pane,
            Focus::Projects
            | Focus::Repositories
            | Focus::Checkouts
            | Focus::Panes
            | Focus::Overlay => Mode::Rail,
        }
    }

    /// Whether a printable key would be typed into something rather than
    /// read as a command — which is also whether `?` is a character here.
    pub(super) fn takes_text(&self) -> bool {
        match self.mode() {
            Mode::Prompt => !matches!(self.prompt, Some(Prompt::ConfirmRemove { .. })),
            Mode::DirPicker | Mode::CheckoutFilter | Mode::Pane => true,
            Mode::Picker => self.picker.as_ref().is_some_and(|p| p.kind.is_fuzzy()),
            Mode::Overlay(OverlayMode::Brief) => self
                .brief
                .as_ref()
                .is_some_and(|v| v.mode == BriefMode::Insert),
            Mode::Overlay(OverlayMode::Pane) => true,
            Mode::Stage(View::Feature) => self.line.is_some(),
            Mode::Overlay(_) | Mode::Stage(_) | Mode::Rail => false,
        }
    }
}
