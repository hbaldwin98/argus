//! The Settings dialog's choices as the window holds them: each change the
//! page makes, held to what this machine can draw, and whether the page has
//! heard them and the file has kept them since they changed.
//!
//! Every choice here is the window's own and touches no daemon, so a change
//! takes effect as it is made; the page still draws the choices from what
//! this sends back, not from the control it moved.

use crate::backdrop;
use crate::page::{SettingChange, SettingsPage};
use crate::settings::{Backdrop, EditorMode, Preferences, MIN_OPACITY};

pub struct Chosen {
    now: Preferences,
    /// The page has not heard them since they changed.
    unheard: bool,
    /// The file has not kept them since they changed.
    unsaved: bool,
}

impl Chosen {
    pub fn new(mut now: Preferences) -> Chosen {
        // A hand-edited file is held to the same bounds as the slider.
        now.opacity = now.opacity.clamp(MIN_OPACITY, 100);
        if !backdrop::can_draw(now.backdrop) {
            now.backdrop = Backdrop::Clear;
        }
        Chosen { now, unheard: true, unsaved: false }
    }

    pub fn get(&self) -> &Preferences {
        &self.now
    }

    /// Makes one change; a backdrop this machine cannot draw is refused
    /// with the reason, which the dialog already shows beside it.
    pub fn change(&mut self, change: SettingChange) -> Result<(), String> {
        let mut next = self.now.clone();
        match change {
            SettingChange::Theme { theme } => next.theme = theme,
            SettingChange::Opacity { percent } => next.opacity = percent.clamp(MIN_OPACITY, 100),
            SettingChange::Backdrop { backdrop } => {
                let offers = backdrop::offered();
                let offer = offers.iter().find(|o| o.backdrop == backdrop);
                match offer {
                    None => return Err(format!("{backdrop:?} cannot be drawn on this system")),
                    Some(offer) => {
                        if let Some(reason) = &offer.unavailable {
                            return Err(reason.clone());
                        }
                    }
                }
                next.backdrop = backdrop;
            }
            SettingChange::Editor { mode } => next.editor = mode,
            SettingChange::EditorCommand { command } => next.editor_command = command,
            SettingChange::Notify { on } => next.notify = on,
            SettingChange::NotifyInBackground { only } => next.notify_in_background = only,
            SettingChange::FeatureLayout { layout } => next.feature_layout = layout,
        }
        if next != self.now {
            self.now = next;
            self.unheard = true;
            self.unsaved = true;
        }
        Ok(())
    }

    /// What an editor opened from the window is asked as: whether it runs
    /// on its own, and the command, `None` leaving the daemon to read
    /// `$VISUAL` and `$EDITOR`.
    pub fn editor(&self) -> (bool, Option<String>) {
        let command = self.now.editor_command.trim();
        (self.now.editor == EditorMode::Separately, (!command.is_empty()).then(|| command.to_string()))
    }

    /// The dialog, when the page has not heard it.
    pub fn take(&mut self) -> Option<SettingsPage> {
        std::mem::take(&mut self.unheard).then(|| SettingsPage {
            chosen: self.now.clone(),
            min_opacity: MIN_OPACITY,
            backdrops: backdrop::offered(),
        })
    }

    /// A page that has just loaded holds nothing: it hears them again.
    pub fn resend(&mut self) {
        self.unheard = true;
    }

    /// Whether they changed since last asked, so they are saved.
    pub fn take_unsaved(&mut self) -> bool {
        std::mem::take(&mut self.unsaved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::Layout;
    use crate::settings::Theme;

    #[test]
    fn a_change_is_heard_and_saved_once() {
        let mut choices = Chosen::new(Preferences::default());
        assert!(choices.take().is_some(), "the page hears them on load");
        assert!(choices.take().is_none());
        choices.change(SettingChange::Theme { theme: Theme::Macchiato }).unwrap();
        assert_eq!(choices.take().map(|p| p.chosen.theme), Some(Theme::Macchiato));
        assert!(choices.take_unsaved());
        assert!(!choices.take_unsaved(), "saved once");
    }

    #[test]
    fn choosing_what_is_already_chosen_is_no_change() {
        let mut choices = Chosen::new(Preferences::default());
        choices.take();
        choices.change(SettingChange::FeatureLayout { layout: Layout::List }).unwrap();
        assert!(choices.take().is_none());
        assert!(!choices.take_unsaved());
    }

    #[test]
    fn the_opacity_never_goes_below_the_least_the_slider_offers() {
        let mut choices = Chosen::new(Preferences { opacity: 0, ..Preferences::default() });
        assert_eq!(choices.get().opacity, MIN_OPACITY, "a hand-edited file too");
        choices.change(SettingChange::Opacity { percent: 250 }).unwrap();
        assert_eq!(choices.get().opacity, 100);
        choices.change(SettingChange::Opacity { percent: 55 }).unwrap();
        assert_eq!(choices.get().opacity, 55);
    }

    #[test]
    fn an_empty_editor_command_leaves_the_daemon_to_the_environment() {
        let mut choices = Chosen::new(Preferences::default());
        assert_eq!(choices.editor(), (false, None));
        choices.change(SettingChange::EditorCommand { command: "  code -w ".into() }).unwrap();
        choices.change(SettingChange::Editor { mode: EditorMode::Separately }).unwrap();
        assert_eq!(choices.editor(), (true, Some("code -w".into())));
        choices.change(SettingChange::Editor { mode: EditorMode::Checkout }).unwrap();
        assert!(!choices.editor().0, "a pane, which the daemon floats");
    }

    #[test]
    fn a_backdrop_this_machine_cannot_draw_is_refused_and_clear_never_is() {
        let mut choices = Chosen::new(Preferences::default());
        for offer in backdrop::offered() {
            let made = choices.change(SettingChange::Backdrop { backdrop: offer.backdrop });
            assert_eq!(made.err(), offer.unavailable);
        }
        assert!(choices.change(SettingChange::Backdrop { backdrop: Backdrop::Clear }).is_ok());
        if cfg!(not(windows)) {
            assert!(choices.change(SettingChange::Backdrop { backdrop: Backdrop::Mica }).is_err());
            assert_eq!(choices.get().backdrop, Backdrop::Clear);
        }
    }
}
