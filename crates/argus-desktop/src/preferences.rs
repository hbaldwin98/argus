//! The Settings dialog's choices as the window holds them: each change the
//! page makes and each step of the zoom keys, held to what this machine can
//! draw, and whether the page has heard them and the file has kept them
//! since they changed.
//!
//! Every choice here is the window's own and touches no daemon, so a change
//! takes effect as it is made; the page still draws the choices from what
//! this sends back, not from the control it moved.

use crate::backdrop;
use crate::page::{FontFamily, SettingChange, SettingsPage};
use crate::settings::{Backdrop, EditorMode, Preferences, MIN_OPACITY};

/// The zoom keys' steps, in percent: a browser's, so a step feels as it
/// does there, and text at either end is still readable.
const ZOOM_STEPS: [u16; 11] = [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200];

pub struct Chosen {
    now: Preferences,
    /// The page has not heard them since they changed.
    unheard: bool,
    /// The file has not kept them since they changed.
    unsaved: bool,
    fonts: Vec<FontFamily>,
}

impl Chosen {
    pub fn new(mut now: Preferences) -> Chosen {
        // A hand-edited file is held to the same bounds as the slider.
        now.opacity = now.opacity.clamp(MIN_OPACITY, 100);
        now.zoom = now.zoom.clamp(ZOOM_STEPS[0], ZOOM_STEPS[ZOOM_STEPS.len() - 1]);
        now.font = family_name(&now.font);
        now.ui_font = family_name(&now.ui_font);
        if !backdrop::can_draw(now.backdrop) {
            now.backdrop = Backdrop::Clear;
        }
        Chosen { now, unheard: true, unsaved: false, fonts: Vec::new() }
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
            SettingChange::Font { family } => next.font = family_name(&family),
            SettingChange::UiFont { family } => next.ui_font = family_name(&family),
        }
        if next != self.now {
            self.now = next;
            self.unheard = true;
            self.unsaved = true;
        }
        Ok(())
    }

    /// One step larger; a zoom between steps, from a hand-edited file,
    /// goes to the next one up.
    pub fn zoom_in(&mut self) {
        let next = ZOOM_STEPS.iter().copied().find(|&s| s > self.now.zoom);
        self.set_zoom(next.unwrap_or(self.now.zoom));
    }

    pub fn zoom_out(&mut self) {
        let next = ZOOM_STEPS.iter().rev().copied().find(|&s| s < self.now.zoom);
        self.set_zoom(next.unwrap_or(self.now.zoom));
    }

    pub fn reset_zoom(&mut self) {
        self.set_zoom(100);
    }

    /// Unheard by the page, which never draws the zoom: the window does.
    fn set_zoom(&mut self, percent: u16) {
        if percent != self.now.zoom {
            self.now.zoom = percent;
            self.unsaved = true;
        }
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
            fonts: self.fonts.clone(),
        })
    }

    /// The installed fonts have been read: the dialog offers them.
    pub fn found_fonts(&mut self, fonts: Vec<FontFamily>) {
        self.fonts = fonts;
        self.unheard = true;
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

/// A font family as the page can quote it in CSS: the page puts it inside
/// quotes in front of the bundled faces, so a quote, backslash or
/// semicolon could end the name and the declaration with it, and the
/// whole stack would be dropped for the browser's default face.
fn family_name(typed: &str) -> String {
    typed
        .chars()
        .filter(|c| !matches!(c, '"' | '\'' | '\\' | ';' | '{' | '}') && !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
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
    fn the_zoom_steps_as_a_browser_does_and_stops_at_either_end() {
        let mut choices = Chosen::new(Preferences::default());
        choices.zoom_in();
        assert_eq!(choices.get().zoom, 110);
        assert!(choices.take_unsaved(), "kept for the next run");
        for _ in 0..20 {
            choices.zoom_in();
        }
        assert_eq!(choices.get().zoom, 200);
        choices.take_unsaved();
        choices.zoom_in();
        assert!(!choices.take_unsaved(), "no step past the end is a change");
        choices.reset_zoom();
        choices.zoom_out();
        assert_eq!(choices.get().zoom, 90);
        for _ in 0..20 {
            choices.zoom_out();
        }
        assert_eq!(choices.get().zoom, 50);
    }

    #[test]
    fn a_hand_edited_zoom_is_held_to_the_steps_ends_and_steps_onto_them() {
        let choices = Chosen::new(Preferences { zoom: 900, ..Preferences::default() });
        assert_eq!(choices.get().zoom, 200);
        let mut choices = Chosen::new(Preferences { zoom: 103, ..Preferences::default() });
        choices.zoom_in();
        assert_eq!(choices.get().zoom, 110);
        let mut choices = Chosen::new(Preferences { zoom: 103, ..Preferences::default() });
        choices.zoom_out();
        assert_eq!(choices.get().zoom, 100);
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
    fn fonts_found_are_heard_with_the_choices() {
        let mut choices = Chosen::new(Preferences::default());
        choices.take();
        let fonts = vec![FontFamily { name: "Cascadia Mono".into(), mono: true }];
        choices.found_fonts(fonts.clone());
        assert_eq!(choices.take().map(|p| p.fonts), Some(fonts));
    }

    #[test]
    fn a_font_family_keeps_nothing_that_could_end_its_quotes() {
        let mut choices = Chosen::new(Preferences::default());
        choices.change(SettingChange::Font { family: "  JetBrainsMono Nerd Font ".into() }).unwrap();
        assert_eq!(choices.get().font, "JetBrainsMono Nerd Font");
        choices.change(SettingChange::Font { family: "x'; color: red; }\\\"".into() }).unwrap();
        assert_eq!(choices.get().font, "x color: red");
        let choices = Chosen::new(Preferences { font: "Fira\"Code".into(), ..Preferences::default() });
        assert_eq!(choices.get().font, "FiraCode", "a hand-edited file too");
        let mut choices = Chosen::new(Preferences { ui_font: "In;ter".into(), ..Preferences::default() });
        assert_eq!(choices.get().ui_font, "Inter", "the interface's too");
        choices.change(SettingChange::UiFont { family: " Segoe\"UI ".into() }).unwrap();
        assert_eq!(choices.get().ui_font, "SegoeUI");
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
